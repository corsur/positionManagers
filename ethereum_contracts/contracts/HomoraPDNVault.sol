//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";

import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IHomoraAvaxRouter.sol";
import "./interfaces/IHomoraBank.sol";
import "./interfaces/IHomoraOracle.sol";
import "./interfaces/IHomoraSpell.sol";
import "./libraries/VaultLib.sol";

contract HomoraPDNVault is ReentrancyGuard, IStrategyManager {
    struct Position {
        uint256 collShareAmount;
    }

    // --- modifiers ---
    modifier onlyAdmin() {
        require(msg.sender == admin, "unauthorized admin op");
        _;
    }

    modifier onlyApertureManager() {
        require(msg.sender == apertureManager, "unauthorized position mgr op");
        _;
    }

    modifier onlyController() {
        require(isController[msg.sender], "unauthorized controller");
        _;
    }

    // --- constants ---
    uint256 private constant _NO_ID = 0;
    bytes private constant HARVEST_DATA = abi.encodeWithSignature("harvestWMasterChef()");
    bytes4 private constant addLiquiditySig = bytes4(
        keccak256(
            "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
        )
    );
    bytes4 private constant removeLiquiditySig = bytes4(
        keccak256(
            "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
        )
    );

    // --- accounts ---
    address public admin;
    address public apertureManager;
    address public feeCollector;
    mapping(address => bool) public isController;

    // --- config ---
    address public stableToken; // token 0
    address public assetToken; // token 1
    address public spell;
    address public rewardToken;
//    address public collToken; // ERC-1155 collateral token address
//    uint public collId; // ERC-1155 token id corresponding to the underlying ERC-20 LP token
    address public lpToken; // ERC-20 LP token address
    uint256 public leverageLevel; // target leverage
    uint256 public pid; // pool id
    IHomoraBank public homoraBank;
    IHomoraOracle public oracle; // HomoraBank's oracle for determining prices.
    IHomoraAvaxRouter public router;

    uint256 public targetDebtRatio; // target debt ratio * 10000, 92% -> 9200
    uint256 public minDebtRatio; // minimum debt ratio * 10000
    uint256 public maxDebtRatio; // maximum debt ratio * 10000
    uint256 public dnThreshold; // delta deviation percentage * 10000

    uint256 public MAX_OPEN_AMOUNT;
    uint256 public MAX_WITHDRAW_AMOUNT;
    uint256 public WITHDRAW_FEE; // multiplied by 1e4
    uint256 public HARVEST_FEE; // multiplied by 1e4

    ManagementFeeInfo public managementFeeInfo;

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
    // Position id of the PDN vault in HomoraBank. Zero for new position.
    uint256 public homoraBankPosId;
    uint256 public totalCollShareAmount;

    // --- event ---
    event LogDeposit(
        address indexed _from,
        uint256 collSize,
        uint256 collShareAmount
    );
    event LogWithdraw(
        address indexed _to,
        uint256 withdrawShareAmount,
        uint256 stableTokenWithdrawAmount,
        uint256 assetTokenWithdrawAmount,
        uint256 avaxWithdrawAmount
    );
    event LogRebalance(uint256 equityBefore, uint256 equityAfter);
    event LogReinvest(uint256 equityBefore, uint256 equityAfter);

    // --- error ---
    error HomoraPDNVault_PositionIsHealthy();
    error HomoraPDNVault_DeltaNotNeutral();
    error HomoraPDNVault_DebtRatioNotHealthy();
    error Insufficient_Liquidity_Mint();

    constructor(
        address _admin,
        address _apertureManager,
        address _feeCollector,
        address _controller,
        address _stableToken,
        address _assetToken,
        address _homoraBank,
        address _spell,
        address _rewardToken,
        uint256 _pid
    ) {
        admin = _admin;
        apertureManager = _apertureManager;
        feeCollector = _feeCollector;
        isController[_controller] = true;
        stableToken = _stableToken;
        assetToken = _assetToken;
        homoraBank = IHomoraBank(_homoraBank);
        oracle = IHomoraOracle(homoraBank.oracle());
        require(oracle.support(_stableToken), "Oracle doesn't support stable token.");
        require(oracle.support(_assetToken), "Oracle doesn't support asset token.");

        spell = _spell;
        rewardToken = _rewardToken;
        pid = _pid;
        homoraBankPosId = _NO_ID;
        totalCollShareAmount = 0;
        lpToken = IHomoraSpell(spell).pairs(stableToken, assetToken);
        require(lpToken != address(0), "Pair does not match the spell.");
        (,, uint16 liqIncentive) = oracle.tokenFactors(lpToken);
        require(liqIncentive != 0, "Oracle doesn't support lpToken.");
        router = IHomoraAvaxRouter(IHomoraSpell(spell).router());
    }

    /// @dev Set config for delta neutral valut.
    /// @param _leverageLevel: Target leverage
    /// @param _targetDebtRatio: Target debt ratio * 10000
    /// @param _minDebtRatio: Minimum debt ratio * 10000
    /// @param _maxDebtRatio: Maximum debt ratio * 10000
    /// @param _dnThreshold: Delta deviation threshold in percentage * 10000
    /// @param _harvestFee: Fee collected on farming rewards
    /// @param _withdrawFee: Fee collected on user withdrawals
    /// @param _managementFee: Management fee, initially zero
    function initialize(
        uint256 _leverageLevel,
        uint256 _targetDebtRatio,
        uint256 _minDebtRatio,
        uint256 _maxDebtRatio,
        uint256 _dnThreshold,
        uint256 _harvestFee,
        uint256 _withdrawFee,
        uint256 _managementFee
    ) external onlyAdmin {
        setConfig(_leverageLevel, _targetDebtRatio, _minDebtRatio, _maxDebtRatio, _dnThreshold);
        setHarvestFee(_harvestFee);
        setWithdrawFee(_withdrawFee);
        setManagementFee(_managementFee);
    }

    function setControllers(
        address[] calldata controllers,
        bool[] calldata statuses
    ) public onlyAdmin {
        require(controllers.length == statuses.length, 'controllers & statuses length mismatched');
        for (uint i = 0; i < controllers.length; i++) {
            isController[controllers[i]] = statuses[i];
        }
    }

    function setHarvestFee(
        uint256 fee
    ) public onlyAdmin {
        HARVEST_FEE = fee;
    }

    function setWithdrawFee(
        uint256 fee
    ) public onlyAdmin {
        WITHDRAW_FEE = fee;
    }

    function setManagementFee(
        uint256 fee
    ) public onlyAdmin {
        managementFeeInfo.MANAGEMENT_FEE = fee;
    }

    /// @dev Set config for delta neutral valut.
    /// @param targetLeverage: Target leverage
    /// @param targetR: Target debt ratio * 10000
    /// @param minR: Minimum debt ratio * 10000
    /// @param maxR: Maximum debt ratio * 10000
    /// @param dnThr: Delta deviation threshold in percentage * 10000
    function setConfig(
        uint256 targetLeverage,
        uint256 targetR,
        uint256 minR,
        uint256 maxR,
        uint256 dnThr
    ) public onlyAdmin {
        require(targetLeverage >= 2, "Leverage at least 2");
        leverageLevel = targetLeverage;
        require(minR < targetR && targetR < maxR, "Invalid debt ratios");
        targetDebtRatio = targetR;
        minDebtRatio = minR;
        maxDebtRatio = maxR;
        require(0 < dnThr && dnThr < 10000, "Invalid delta threshold");
        dnThreshold = dnThr;
    }

    fallback() external payable {}

    receive() external payable {}

    /// @dev Open a new Aperture position for `recipient`
    /// @param recipient: Owner of the position on Aperture
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity
    function openPosition(
        address recipient,
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount,
            uint256 minEquityETH,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256, uint256));
        depositInternal(
            recipient,
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Increase an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity
    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount,
            uint256 minEquityETH,
            uint256 minReinvestETH
        ) = abi.decode(data, (uint256, uint256, uint256, uint256));
        depositInternal(
            address(0),
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Decrease an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: The recipient, the amount of shares to withdraw and the minimum amount of assets returned
    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            address recipient,
            uint256 shareAmount,
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (address, uint256, uint256, uint256, uint256));
        console.log('amount %d', shareAmount);
        console.log('recipient %s', recipient);
        withdrawInternal(
            position_info,
            recipient,
            shareAmount,
            amtAMin,
            amtBMin,
            minReinvestETH
        );
    }

    /// @dev Close an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Owner of the position on Aperture and the minimum amount of assets returned
    function closePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (
            address recipient,
            uint256 amtAMin,
            uint256 amtBMin,
            uint256 minReinvestETH
        ) = abi.decode(data, (address, uint256, uint256, uint256));
        withdrawInternal(
            position_info,
            recipient,
            getShareAmount(position_info),
            amtAMin,
            amtBMin,
            minReinvestETH
        );
    }

    /// @dev Calculate the params passed to Homora to create PDN position
    /// @param stableTokenDepositAmount: The amount of stable token supplied by user
    /// @param assetTokenDepositAmount: The amount of asset token supplied by user
    function deltaNeutral(
        uint256 stableTokenDepositAmount,
        uint256 assetTokenDepositAmount
    )
        internal
        returns (
            uint256 stableTokenAmount,
            uint256 assetTokenAmount,
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        )
    {
        stableTokenAmount = stableTokenDepositAmount;

        // swap all assetTokens into stableTokens
        if (assetTokenDepositAmount > 0) {
            uint256 amount = _swap(
                assetTokenDepositAmount,
                assetToken,
                stableToken
            );
            // update the stableToken amount
            stableTokenAmount += amount;
        }
        assetTokenAmount = 0;

        // total stableToken leveraged amount
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        uint256 totalAmount = stableTokenAmount * leverageLevel;
        uint256 desiredAmount = totalAmount / 2;
        stableTokenBorrowAmount = desiredAmount - stableTokenAmount;
        assetTokenBorrowAmount = router.quote(
            desiredAmount,
            reserve0,
            reserve1
        );
    }

    /// @dev Internal deposit function
    /// @param recipient: Aperture position owner
    /// @param position_info: Aperture position info
    /// @param stableTokenDepositAmount: Amount of stable token supplied by user
    /// @param assetTokenDepositAmount: Amount of asset token supplied by user
    /// @param minEquityETH: Minimum equity received after adding liquidity
    function depositInternal(
        address recipient,
        PositionInfo memory position_info,
        uint256 stableTokenDepositAmount,
        uint256 assetTokenDepositAmount,
        uint256 minEquityETH,
        uint256 minReinvestETH
    ) internal {
        reinvestInternal(minReinvestETH);

        // Record original position equity before adding liquidity
        uint256[2] memory equities;
        equities[0] = getEquityETHValue();

        // Record the balance state before transfer fund.
        uint256[3] memory balanceArray = [
            IERC20(stableToken).balanceOf(address(this)),
            IERC20(assetToken).balanceOf(address(this)),
            address(this).balance
        ];

        // Transfer user's deposit.
        if (stableTokenDepositAmount > 0)
            IERC20(stableToken).transferFrom(
                msg.sender,
                address(this),
                stableTokenDepositAmount
            );
        if (assetTokenDepositAmount > 0)
            IERC20(assetToken).transferFrom(
                msg.sender,
                address(this),
                assetTokenDepositAmount
            );

        uint256 stableTokenBorrowAmount;
        uint256 assetTokenBorrowAmount;
        (
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            stableTokenBorrowAmount,
            assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenDepositAmount, assetTokenDepositAmount);

//        (uint256 reserve0, uint256 reserve1) = _getReserves();
//        (
//            uint256 _stableTokenBorrowAmount,
//            uint256 _assetTokenBorrowAmount
//        ) = VaultLib.deltaNeutral(
//            stableTokenDepositAmount,
//            assetTokenDepositAmount,
//            reserve0,
//            reserve1,
//            leverageLevel
//        );

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), stableTokenDepositAmount);
        IERC20(assetToken).approve(address(homoraBank), assetTokenDepositAmount);

        bytes memory data = abi.encodeWithSelector(
            addLiquiditySig,
            stableToken,
            assetToken,
            [
                stableTokenDepositAmount,
                assetTokenDepositAmount,
                0,
                stableTokenBorrowAmount,
                assetTokenBorrowAmount,
                0,
                0,
                0
            ],
            pid
        );

        homoraBankPosId = homoraBank.execute(
            homoraBankPosId,
            spell,
            data
        );

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        // Position equity after adding liquidity
        equities[1] = getEquityETHValue();
        // Calculate user share amount.
        uint256 equityChange = equities[1] - equities[0];
        uint256 collShareAmount = equities[0] == 0
            ? equityChange
            : (equityChange * totalCollShareAmount) / equities[0];

        if ((equities[1] - equities[0]) < minEquityETH) {
            revert Insufficient_Liquidity_Mint();
        }

        // Update vault position state.
        totalCollShareAmount += collShareAmount;

        // Update deposit owner's position state.
        positions[position_info.chainId][position_info.positionId]
            .collShareAmount += collShareAmount;

        // Return leftover funds to user.
        IERC20(stableToken).transfer(
            recipient,
            IERC20(stableToken).balanceOf(address(this)) - balanceArray[0]
        );
        IERC20(assetToken).transfer(
            recipient,
            IERC20(assetToken).balanceOf(address(this)) - balanceArray[1]
        );
        payable(recipient).transfer(address(this).balance - balanceArray[2]);

        emit LogDeposit(recipient, equityChange, collShareAmount);
    }

    function withdrawInternal(
        PositionInfo memory position_info,
        address recipient,
        uint256 withdrawShareAmount,
        uint256 amtAMin,
        uint256 amtBMin,
        uint256 minReinvestETH
    ) internal {
        require(withdrawShareAmount > 0, "zero withdrawal amount");
        require(
            withdrawShareAmount <= getShareAmount(position_info),
            "not enough share amount to withdraw"
        );
        console.log('share amount %d', getShareAmount(position_info));

        reinvestInternal(minReinvestETH);

        // Record the balance state before remove liquidity.
        uint256[3] memory balanceArray = [
            IERC20(stableToken).balanceOf(address(this)),
            IERC20(assetToken).balanceOf(address(this)),
            address(this).balance
        ];

        // Calculate collSize to withdraw.
        uint256 collWithdrawSize = (withdrawShareAmount * getCollateralSize()) /
            totalCollShareAmount;

        // Calculate debt to repay in two tokens.
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = currentDebtAmount();

        // Encode removeLiqiduity call.
        bytes memory data = abi.encodeWithSelector(
            removeLiquiditySig,
            stableToken,
            assetToken,
            [
                collWithdrawSize,
                0,
                (stableTokenDebtAmount * withdrawShareAmount) /
                    totalCollShareAmount,
                (assetTokenDebtAmount * withdrawShareAmount) /
                    totalCollShareAmount,
                0,
                0,
                0
            ]
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Calculate token disbursement amount.
        uint256[3] memory withdrawAmounts = [
            // Stable token withdraw amount
            IERC20(stableToken).balanceOf(address(this))
            - balanceArray[0]
            + (balanceArray[0] * withdrawShareAmount) / totalCollShareAmount,
            // Asset token withdraw amount
            IERC20(assetToken).balanceOf(address(this))
            - balanceArray[1]
            + (balanceArray[1] * withdrawShareAmount) / totalCollShareAmount,
            // AVAX withdraw amount
            address(this).balance
            - balanceArray[2]
            + (balanceArray[2] * withdrawShareAmount) / totalCollShareAmount
        ];

        // Transfer fund to user (caller).
        IERC20(stableToken).transfer(recipient, withdrawAmounts[0]);
        IERC20(assetToken).transfer(recipient, withdrawAmounts[1]);
        payable(recipient).transfer(withdrawAmounts[2]);

        // Update position info.
        positions[position_info.chainId][position_info.positionId]
            .collShareAmount -= withdrawShareAmount;
        totalCollShareAmount -= withdrawShareAmount;

        // Emit event.
        emit LogWithdraw(
            recipient,
            withdrawShareAmount,
            withdrawAmounts[0],
            withdrawAmounts[1],
            withdrawAmounts[2]
        );
    }

    /// @dev Check if the farming position is delta neutral
    /// @param pos: Farming position in Homora Bank
    function isDeltaNeutral(
        VaultLib.VaultPosition memory pos
    ) public view returns (bool) {
        // Assume token A is the stable token
        console.log("n_B", pos.amtB, "d_B", pos.debtAmtB);
        console.log("delta", VaultLib.getOffset(pos.amtB, pos.debtAmtB), dnThreshold);
        return (VaultLib.getOffset(pos.amtB, pos.debtAmtB) < dnThreshold);
    }

    /// @dev Check if the debt ratio is healthy
    function isDebtRatioHealthy() public view returns (bool) {
        uint256 debtRatio = getDebtRatio();
        return (minDebtRatio < debtRatio) && (debtRatio < maxDebtRatio);
    }

    function rebalance(
        uint256 maxRebalanceCostETH,
        uint256 minReinvestETH
    ) external onlyController {
        reinvestInternal(minReinvestETH);

        uint256 debtRatio = getDebtRatio();
        console.log("Debt ratio before: ", debtRatio);

        VaultLib.VaultPosition memory pos = getPositionInfo();
        // Check if the position need rebalance
        if (isDeltaNeutral(pos) && isDebtRatioHealthy()) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        // 1. short: amtB < debtAmtB, R > Rt
        if (pos.debtAmtB > pos.amtB) {
            _reBalanceShort(pos);
        }
        // 2. long: amtB > debtAmtB, R < Rt
        else {
            _rebalanceLong(pos);
        }

        console.log("Debt ratio after: ", getDebtRatio());

        pos = getPositionInfo();
        // Check if the rebalance succeeded
        if (!isDeltaNeutral(pos)) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }

        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogRebalance(pos.collateralSize, getCollateralSize());
    }

    /// @param pos: Farming position in Homora Bank
    function _reBalanceShort(
        VaultLib.VaultPosition memory pos
    ) internal {
        (uint256 reserveA, uint256 reserveB) = _getReserves();

        (
            uint256 collWithdrawAmt,
            uint256 amtARepay,
            uint256 amtBRepay
        ) = VaultLib.rebalanceShort(pos, leverageLevel, reserveA, reserveB);

        bytes memory data1 = abi.encodeWithSelector(
            removeLiquiditySig,
            stableToken,
            assetToken,
            [collWithdrawAmt, 0, amtARepay, amtBRepay, 0, 0, 0]
        );

        homoraBank.execute(homoraBankPosId, spell, data1);
    }

    /// @param pos: Farming position in Homora Bank
    function _rebalanceLong(
        VaultLib.VaultPosition memory pos
    ) internal {
        (uint256 reserveA, uint256 reserveB) = _getReserves();
        uint256 amtAReward = IERC20(stableToken).balanceOf(address(this));

        (
            uint256 amtABorrow,
            uint256 amtBBorrow
        ) = VaultLib.rebalanceLong(
                pos,
                leverageLevel,
                reserveA,
                reserveB,
                amtAReward
            );
        IERC20(stableToken).approve(address(homoraBank), VaultLib.MAX_UINT);
        bytes memory data0 = abi.encodeWithSelector(
            addLiquiditySig,
            stableToken,
            assetToken,
            [amtAReward, 0, 0, amtABorrow, amtBBorrow, 0, 0, 0],
            pid
        );
        homoraBank.execute(homoraBankPosId, spell, data0);
        IERC20(stableToken).approve(address(homoraBank), 0);
    }

    function reinvest(
        uint256 minReinvestETH
    ) external onlyController {
        reinvestInternal(minReinvestETH);
    }

    function _harvest() internal {
        homoraBank.execute(homoraBankPosId, spell, HARVEST_DATA);
    }

    function reinvestInternal(
        uint256 minReinvestETH
    ) internal {
        // Position nonexistent
        if (homoraBankPosId == _NO_ID) {
            return;
        }

        uint256 equityBefore = getEquityETHValue();

        // 1. claim rewards
        _harvest();
        _swapReward();

        // 3. swap any AVAX leftover
        uint256 avaxBalance = address(this).balance;
        if (avaxBalance > 0) {
            _swapAVAX(avaxBalance, stableToken);
        }

        // 2. reinvest with the current balance
        uint256 stableTokenBalance = IERC20(stableToken).balanceOf(address(this));
        uint256 assetTokenBalance = IERC20(assetToken).balanceOf(address(this));

        (uint256 reserve0, uint256 reserve1) = _getReserves();
        uint256 liquidity = (((stableTokenBalance * leverageLevel) / 2) *
            IERC20(lpToken).totalSupply()) / reserve0;
        require(liquidity > 0, "Insufficient liquidity minted");

        (
            uint256 stableTokenAmount,
            uint256 assetTokenAmount,
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance);

//        (
//            uint256 stableTokenBorrowAmount,
//            uint256 assetTokenBorrowAmount
//        ) = VaultLib.deltaNeutral(
//            stableTokenBalance,
//            assetTokenBalance,
//            reserve0,
//            reserve1,
//            leverageLevel
//        );

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), VaultLib.MAX_UINT);
        IERC20(assetToken).approve(address(homoraBank), VaultLib.MAX_UINT);

        // Encode the calling function.
        bytes memory data = abi.encodeWithSelector(
            addLiquiditySig,
            stableToken,
            assetToken,
            [
                stableTokenAmount,
                assetTokenAmount,
                0,
                stableTokenBorrowAmount,
                assetTokenBorrowAmount,
                0,
                0,
                0
            ],
            pid
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        uint256 equityAfter = getEquityETHValue();

        require((equityAfter - equityBefore) > minReinvestETH, "Received less equity than expected");

        emit LogReinvest(equityBefore, equityAfter);
    }

    /// @dev Homora position info
    function getPositionInfo()
        internal view
        returns (VaultLib.VaultPosition memory pos)
    {
        pos.collateralSize = getCollateralSize();
        (pos.amtA, pos.amtB) = convertCollateralToTokens(pos.collateralSize);
        (pos.debtAmtA, pos.debtAmtB) = currentDebtAmount();
        return pos;
    }

    /// @notice Swap amount of fromToken into toToken
    function _swap(
        uint256 amount,
        address fromToken,
        address toToken
    )
        internal
        returns (uint256)
    {
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        IERC20(fromToken).approve(address(router), amount);
        uint256[] memory resAmt = router.swapExactTokensForTokens(
            amount,
            0,
            path,
            address(this),
            block.timestamp
        );
        return resAmt[1];
    }

    function _swapAVAX(uint256 amount, address toToken)
        internal
        returns (uint256)
    {
        address fromToken = router.WAVAX();
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        uint256[] memory resAmt = router.swapExactAVAXForTokens{value: amount}(
            0,
            path,
            address(this),
            block.timestamp
        );
        return resAmt[1];
    }

    /// @notice Swap reward tokens into stable tokens
    function _swapReward() internal {
        uint256 rewardAmt = IERC20(rewardToken).balanceOf(address(this));
        if (rewardAmt > 0) {
            _swap(rewardAmt, rewardToken, stableToken);
        }
    }

    /// @notice Get the amount of each of the two tokens in the pool. Stable token first
    function _getReserves()
        internal view
        returns (uint256, uint256)
    {
        return VaultLib.getReserves(lpToken, stableToken);
    }

    /// @dev Query the current debt amount for both tokens. Stable first
    function currentDebtAmount()
        public view
        returns (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        )
    {
        if (homoraBankPosId == _NO_ID) {
            return (0, 0);
        }
        (address[] memory tokens, uint256[] memory debts) = homoraBank.getPositionDebts(homoraBankPosId);
        for (uint256 i = 0; i < tokens.length; i++) {
            if (tokens[i] == stableToken) {
                stableTokenDebtAmount = debts[i];
            } else if (tokens[i] == assetToken) {
                assetTokenDebtAmount = debts[i];
            }
        }
        return (stableTokenDebtAmount, assetTokenDebtAmount);
    }

    /// @dev Query the collateral factor of the LP token on Homora
    function getCollateralFactor() public view returns (uint16 collateralFactor) {
        (, collateralFactor,) = oracle.tokenFactors(lpToken);
    }

    /// @dev Query the borrow factor of the debt token on Homora
    /// @param token: Address of the ERC-20 debt token
    function getBorrowFactor(address token)
        public view
        returns (uint16 borrowFactor)
    {
        (borrowFactor,,) = oracle.tokenFactors(token);
    }

    /// @dev Total position value not weighted by the collateral factor
    function getCollateralETHValue()
        public view
        returns (uint256)
    {
        if (homoraBankPosId == _NO_ID) {
            return 0;
        }
        return homoraBank.getCollateralETHValue(homoraBankPosId) * 10**4 / getCollateralFactor();
    }

    /// @dev Total debt value not weighted by the borrow factors
    function getBorrowETHValue()
        public view
        returns (uint256)
    {
        if (homoraBankPosId == _NO_ID) {
            return 0;
        }
        (uint256 stableTokenDebtAmount, uint256 assetTokenDebtAmount) = currentDebtAmount();
        return oracle.asETHBorrow(stableToken, stableTokenDebtAmount, msg.sender) * 10**4 / getBorrowFactor(stableToken)
        + oracle.asETHBorrow(assetToken, assetTokenDebtAmount, msg.sender) * 10**4 / getBorrowFactor(assetToken);
    }

    /// @dev Net equity value of the PDN position
    function getEquityETHValue()
        public view
        returns (uint256)
    {
        return getCollateralETHValue() - getBorrowETHValue();
    }

    /// @notice Calculate the debt ratio and return the ratio, multiplied by 1e4
    function getDebtRatio()
        public view
        returns (uint256)
    {
        require(homoraBankPosId != _NO_ID, "Invalid Homora Bank position id");
        uint256 collateralValue = homoraBank.getCollateralETHValue(homoraBankPosId);
        uint256 borrowValue = homoraBank.getBorrowETHValue(homoraBankPosId);
        return (borrowValue * 10000) / collateralValue;
    }

//    /// @notice Calculate the real time leverage and return the leverage, multiplied by 1e4
//    function getLeverage() public view returns (uint256) {
//        // 0: stableToken, 1: assetToken
//        (uint256 amount0, uint256 amount1) = convertCollateralToTokens(
//            getCollateralSize()
//        );
//        (uint256 debtAmt0, uint256 debtAmt1) = currentDebtAmount();
//        (uint256 reserve0, uint256 reserve1) = _getReserves();
//
//        uint256 collateralValue = amount0 +
//            (amount1 > 0 ? router.quote(amount1, reserve1, reserve0) : 0);
//        uint256 debtValue = debtAmt0 +
//            (debtAmt1 > 0 ? router.quote(debtAmt1, reserve1, reserve0) : 0);
////        uint256 collateralValue = getCollateralETHValue();
////        uint256 debtValue = getBorrowETHValue();
//
//        return (collateralValue * 10000) / (collateralValue - debtValue);
//    }

    function getCollateralSize()
        public view
        returns (uint256)
    {
        if (homoraBankPosId == _NO_ID) {
            return 0;
        }
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        return collateralSize;
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens. Stable token first
    /// @param collAmount: Amount of LP token
    function convertCollateralToTokens(uint256 collAmount)
        public view
        returns (uint256, uint256)
    {
        return VaultLib.convertCollateralToTokens(lpToken, stableToken, collAmount);
    }

    /// @dev Query a user position's share
    /// @param position_info: Aperture position info
    function getShareAmount(PositionInfo memory position_info)
        public view
        returns (uint256)
    {
        return positions[position_info.chainId][position_info.positionId].collShareAmount;
    }

//    function quote(address token, uint256 amount)
//        public view
//        returns(uint256)
//    {
//        (uint256 reserve0, uint256 reserve1) = _getReserves();
//        if (token == stableToken) {
//            return router.quote(amount, reserve0, reserve1);
//        } else {
//            return router.quote(amount, reserve1, reserve0);
//        }
//    }

//    /// @notice swap function for external tests, swap stableToken into assetToken
//    function swapExternal(address token, uint256 amount0)
//        external
//        returns (uint256 amt)
//    {
//        IERC20(token).transferFrom(msg.sender, address(this), amount0);
//        if (token == stableToken) {
//            amt = _swap(amount0, stableToken, assetToken);
//            IERC20(assetToken).transfer(msg.sender, amt);
//        } else if (token == assetToken) {
//            amt = _swap(amount0, assetToken, stableToken);
//            IERC20(stableToken).transfer(msg.sender, amt);
//        }
//        return amt;
//    }
}

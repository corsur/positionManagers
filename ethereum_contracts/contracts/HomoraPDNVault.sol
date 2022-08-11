//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IHomoraAvaxRouter.sol";
import "./interfaces/IHomoraBank.sol";
import "./interfaces/IHomoraOracle.sol";
import "./interfaces/IHomoraSpell.sol";
import "./libraries/VaultLib.sol";

contract HomoraPDNVault is ReentrancyGuard, IStrategyManager {
    using SafeERC20 for IERC20;

    struct Position {
        uint256 shareAmount;
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
    bytes4 private constant ADD_LIQUIDITY_SIG = bytes4(
        keccak256(
            "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
        )
    );
    bytes4 private constant REMOVE_LIQUIDITY_SIG = bytes4(
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

    uint256 public maxOpen; // Maximum amount allowed in stable to add in one transaction
    uint256 public maxWithdraw; // Maximum amount allowed in stable to withdraw in one transaction
    uint256 public withdrawFee; // multiplied by 1e4
    uint256 public harvestFee; // multiplied by 1e4

    ManagementFeeInfo public managementFeeInfo;

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
    // Position id of the PDN vault in HomoraBank. Zero for new position.
    uint256 public homoraBankPosId;
    uint256 public totalShareAmount;

    // --- event ---
    event LogDeposit(
        uint16 indexed chainId,
        uint128 indexed positionId,
        uint256 equityAmount,
        uint256 shareAmount
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
    error Zero_Withdrawal_Amount();
    error Insufficient_Share_Withdraw();

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
        lpToken = IHomoraSpell(spell).pairs(stableToken, assetToken);
        require(lpToken != address(0), "Pair does not match the spell.");
        (, , uint16 liqIncentive) = oracle.tokenFactors(lpToken);
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
        harvestFee = fee;
    }

    function setWithdrawFee(
        uint256 fee
    ) public onlyAdmin {
        withdrawFee = fee;
    }

    function setManagementFee(
        uint256 fee
    ) public onlyAdmin {
        managementFeeInfo.managementFee = fee;
    }

    /// @dev Set config for delta neutral valut.
    /// @param _leverageLevel: Target leverage
    /// @param _targetDebtRatio: Target debt ratio * 10000
    /// @param _minDebtRatio: Minimum debt ratio * 10000
    /// @param _maxDebtRatio: Maximum debt ratio * 10000
    /// @param _dnThreshold: Delta deviation threshold in percentage * 10000
    function setConfig(
        uint256 _leverageLevel,
        uint256 _targetDebtRatio,
        uint256 _minDebtRatio,
        uint256 _maxDebtRatio,
        uint256 _dnThreshold
    ) public onlyAdmin {
        require(_leverageLevel >= 2, "Leverage at least 2");
        leverageLevel = _leverageLevel;
        require(_minDebtRatio < _targetDebtRatio && _targetDebtRatio < _maxDebtRatio, "Invalid debt ratios");
        targetDebtRatio = _targetDebtRatio;
        minDebtRatio = _minDebtRatio;
        maxDebtRatio = _maxDebtRatio;
        require(0 < _dnThreshold && _dnThreshold < 10000, "Invalid delta threshold");
        dnThreshold = _dnThreshold;
    }

    receive() external payable {}

    /// @dev Open a new Aperture position for `recipient`
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
    function openPosition(
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
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Increase an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
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
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount,
            minEquityETH,
            minReinvestETH
        );
    }

    /// @dev Decrease an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: The recipient, the amount of shares to withdraw and the minimum amount of assets returned, etc
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
    /// @param data: Owner of the position on Aperture and the minimum amount of assets returned, etc
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
            uint256 amount = swap(
                assetTokenDepositAmount,
                assetToken,
                stableToken
            );
            // update the stableToken amount
            stableTokenAmount += amount;
        }
        assetTokenAmount = 0;

        // total stableToken leveraged amount
        (uint256 reserve0, uint256 reserve1) = getReserves();
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
    /// @param position_info: Aperture position info
    /// @param stableTokenDepositAmount: Amount of stable token supplied by user
    /// @param assetTokenDepositAmount: Amount of asset token supplied by user
    /// @param minEquityETH: Minimum equity received after adding liquidity
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function depositInternal(
        PositionInfo memory position_info,
        uint256 stableTokenDepositAmount,
        uint256 assetTokenDepositAmount,
        uint256 minEquityETH,
        uint256 minReinvestETH
    ) internal {
        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            this.rebalance(10, minReinvestETH);
        } else {
            reinvestInternal(minReinvestETH);
        }

        // Record original position equity before adding liquidity
        uint256 equityBefore = getEquityETHValue();

        // Record the balance state before transfer fund.
//        uint256[3] memory balanceArray = [
//            IERC20(stableToken).balanceOf(address(this)),
//            IERC20(assetToken).balanceOf(address(this)),
//            address(this).balance
//        ];

        // Transfer user's deposit.
        if (stableTokenDepositAmount > 0)
            IERC20(stableToken).safeTransferFrom(
                msg.sender,
                address(this),
                stableTokenDepositAmount
            );
        if (assetTokenDepositAmount > 0)
            IERC20(assetToken).safeTransferFrom(
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
            ADD_LIQUIDITY_SIG,
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
        // Calculate user share amount.
        uint256 equityChange = getEquityETHValue() - equityBefore;
        uint256 shareAmount = equityBefore == 0
            ? equityChange
            : (equityChange * totalShareAmount) / equityBefore;

        if (equityChange < minEquityETH) {
            revert Insufficient_Liquidity_Mint();
        }

        // Update vault position state.
        totalShareAmount += shareAmount;

        // Update deposit owner's position state.
        positions[position_info.chainId][position_info.positionId]
            .shareAmount += shareAmount;

        // Return leftover funds to user.
//        IERC20(stableToken).transfer(
//            recipient,
//            IERC20(stableToken).balanceOf(address(this)) - balanceArray[0]
//        );
//        IERC20(assetToken).transfer(
//            recipient,
//            IERC20(assetToken).balanceOf(address(this)) - balanceArray[1]
//        );
//        payable(recipient).transfer(address(this).balance - balanceArray[2]);

        // Check if the PDN position is still healthy
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogDeposit(
            position_info.chainId,
            position_info.positionId,
            equityChange,
            shareAmount
        );
    }

    /// @dev Internal withdraw function
    /// @param position_info: Aperture position info
    /// @param withdrawShareAmount: Amount of user shares to withdraw
    /// @param minStableReceived: Minimum amount of stable token returned to user
    /// @param minAssetReceived: Minimum amount of asset token returned to user
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function withdrawInternal(
        PositionInfo memory position_info,
        address recipient,
        uint256 withdrawShareAmount,
        uint256 minStableReceived,
        uint256 minAssetReceived,
        uint256 minReinvestETH
    ) internal {
        if (withdrawShareAmount == 0) {
            revert Zero_Withdrawal_Amount();
        }
        if (withdrawShareAmount > getShareAmount(position_info)) {
            revert Insufficient_Share_Withdraw();
        }
        // Check if the PDN position need rebalance
        if (!isDeltaNeutral() || !isDebtRatioHealthy()) {
            this.rebalance(10, minReinvestETH);
        } else {
            reinvestInternal(minReinvestETH);
        }

        console.log('share amount %d', getShareAmount(position_info));

        // Record the balance state before remove liquidity.
//        uint256[3] memory balanceArray = [
//            IERC20(stableToken).balanceOf(address(this)),
//            IERC20(assetToken).balanceOf(address(this)),
//            address(this).balance
//        ];

        // Calculate collSize to withdraw.
        uint256 collWithdrawSize = (withdrawShareAmount * getCollateralSize()) /
        totalShareAmount;

        // Calculate debt to repay in two tokens.
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = currentDebtAmount();

        // Encode removeLiqiduity call.
        bytes memory data = abi.encodeWithSelector(
            REMOVE_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [
                collWithdrawSize,
                0,
                (stableTokenDebtAmount * withdrawShareAmount) /
                totalShareAmount,
                (assetTokenDebtAmount * withdrawShareAmount) /
                totalShareAmount,
                0,
                0,
                0
            ]
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Calculate token disbursement amount.
        uint256[3] memory withdrawAmounts = [
            // Stable token withdraw amount
            IERC20(stableToken).balanceOf(address(this)),
            // Asset token withdraw amount
            IERC20(assetToken).balanceOf(address(this)),
            // AVAX withdraw amount
            address(this).balance
        ];
//        uint256[3] memory withdrawAmounts = [
//            // Stable token withdraw amount
//            IERC20(stableToken).balanceOf(address(this))
//            - balanceArray[0]
//            + (balanceArray[0] * withdrawShareAmount) / totalShareAmount,
//            // Asset token withdraw amount
//            IERC20(assetToken).balanceOf(address(this))
//            - balanceArray[1]
//            + (balanceArray[1] * withdrawShareAmount) / totalShareAmount,
//            // AVAX withdraw amount
//            address(this).balance
//            - balanceArray[2]
//            + (balanceArray[2] * withdrawShareAmount) / totalShareAmount
//        ];

        // Transfer fund to user (caller).
        IERC20(stableToken).transfer(recipient, withdrawAmounts[0]);
        IERC20(assetToken).transfer(recipient, withdrawAmounts[1]);
        payable(recipient).transfer(withdrawAmounts[2]);

        // Update position info.
        positions[position_info.chainId][position_info.positionId]
            .shareAmount -= withdrawShareAmount;
        totalShareAmount -= withdrawShareAmount;

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
    function isDeltaNeutral()
        public view
        returns (bool)
    {
        // Assume token A is the stable token
        VaultLib.VaultPosition memory pos = getPositionInfo();
//        console.log("n_B", pos.amtB, "d_B", pos.debtAmtB);
//        console.log("delta", VaultLib.getOffset(pos.amtB, pos.debtAmtB));
        return (VaultLib.getOffset(pos.amtB, pos.debtAmtB) < dnThreshold);
    }

    function isDebtRatioHealthy() public view returns (bool) {
        if (homoraBankPosId == _NO_ID) {
            return true;
        } else {
            uint256 debtRatio = getDebtRatio();
            return (minDebtRatio < debtRatio) && (debtRatio < maxDebtRatio);
        }
    }

    /// @dev Rebalance Homora Bank's farming position if delta is not neutral or debt ratio is not healthy
    /// @param slippage: Slippage on the swap between stable token and asset token, multiplied by 1e4, 0.1% => 10
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function rebalance(
        uint256 slippage,
        uint256 minReinvestETH
    ) external onlyController {
        reinvestInternal(minReinvestETH);

        uint256 debtRatio = getDebtRatio();
        console.log("Debt ratio before: ", debtRatio);

        // Check if the PDN position need rebalance
        if (isDeltaNeutral() && isDebtRatioHealthy()) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        // Equity before rebalance
        uint256 equityBefore = getEquityETHValue();

        VaultLib.VaultPosition memory pos = getPositionInfo();
        // 1. short: amtB < debtAmtB, R > Rt
        if (pos.debtAmtB > pos.amtB) {
            reBalanceShort(pos);
        }
        // 2. long: amtB > debtAmtB, R < Rt
        else {
            rebalanceLong(pos);
        }

        console.log("Debt ratio after: ", getDebtRatio());

        // Check if the rebalance succeeded
        if (!isDeltaNeutral()) {
            revert HomoraPDNVault_DeltaNotNeutral();
        }
        if (!isDebtRatioHealthy()) {
            revert HomoraPDNVault_DebtRatioNotHealthy();
        }

        emit LogRebalance(equityBefore, getEquityETHValue());
    }

    function reBalanceShort(
        VaultLib.VaultPosition memory pos
    ) internal {
        (uint256 reserveA, uint256 reserveB) = getReserves();

        (
            uint256 collWithdrawAmt,
            uint256 amtARepay,
            uint256 amtBRepay
        ) = VaultLib.rebalanceShort(pos, leverageLevel, reserveA, reserveB);

        bytes memory data = abi.encodeWithSelector(
            REMOVE_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [collWithdrawAmt, 0, amtARepay, amtBRepay, 0, 0, 0]
        );

        homoraBank.execute(homoraBankPosId, spell, data);
    }

    function rebalanceLong(
        VaultLib.VaultPosition memory pos
    ) internal {
        (uint256 reserveA, uint256 reserveB) = getReserves();
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
        bytes memory data = abi.encodeWithSelector(
            ADD_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [amtAReward, 0, 0, amtABorrow, amtBBorrow, 0, 0, 0],
            pid
        );
        homoraBank.execute(homoraBankPosId, spell, data);
        IERC20(stableToken).approve(address(homoraBank), 0);
    }

    /// @dev Collect reward tokens and reinvest
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvest(
        uint256 minReinvestETH
    ) external onlyController {
        reinvestInternal(minReinvestETH);
    }

    function harvest() internal {
        homoraBank.execute(homoraBankPosId, spell, HARVEST_DATA);
    }

    /// @dev Internal reinvest function
    /// @param minReinvestETH: Minimum equity received after reinvesting
    function reinvestInternal(
        uint256 minReinvestETH
    ) internal {
        // Position nonexistent
        if (homoraBankPosId == _NO_ID || totalShareAmount == 0) {
            return;
        }

        uint256 equityBefore = getEquityETHValue();

        // 1. Claim rewards
        harvest();
        swapReward();

        // 2. Swap any AVAX leftover
        uint256 avaxBalance = address(this).balance;
        if (avaxBalance > 0) {
            swapAVAX(avaxBalance, stableToken);
        }

        // 3. Reinvest with the current balance
        uint256 stableTokenBalance = IERC20(stableToken).balanceOf(address(this));
        uint256 assetTokenBalance = IERC20(assetToken).balanceOf(address(this));
//        console.log("stableTokenBalance", stableTokenBalance);
//        console.log("assetTokenBalance", assetTokenBalance);

//        (uint256 reserve0, uint256 reserve1) = getReserves();
//        uint256 liquidity = (((stableTokenBalance * leverageLevel) / 2) *
//            IERC20(lpToken).totalSupply()) / reserve0;
//        console.log("liquidity", liquidity);

//        require(liquidity > 0, "Insufficient liquidity minted");

        uint256 stableTokenBorrowAmount;
        uint256 assetTokenBorrowAmount;
        (
            stableTokenBalance,
            assetTokenBalance,
            stableTokenBorrowAmount,
            assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance);
//        console.log("stableTokenBorrowAmount", stableTokenBorrowAmount);
//        console.log("assetTokenBorrowAmount", assetTokenBorrowAmount);

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
            ADD_LIQUIDITY_SIG,
            stableToken,
            assetToken,
            [
                stableTokenBalance,
                assetTokenBalance,
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
//        console.log("equity before reinvest", equityBefore);
//        console.log("equity after reinvest", equityAfter);

        if (equityAfter < equityBefore + minReinvestETH) {
            if (VaultLib.getOffset(equityAfter, equityBefore + minReinvestETH) >= 100) {
                revert Insufficient_Liquidity_Mint();
            }
        }
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

    /// @notice Swap fromToken into toToken
    function swap(
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

    /// @notice Swap native AVAX into toToken
    function swapAVAX(uint256 amount, address toToken)
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
    function swapReward() internal {
        uint256 rewardAmt = IERC20(rewardToken).balanceOf(address(this));
        if (rewardAmt > 0) {
            swap(rewardAmt, rewardToken, stableToken);
        }
    }

    /// @notice Get the amount of each of the two tokens in the pool. Stable token first
    function getReserves()
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
    }

    /// @dev Query the collateral factor of the LP token on Homora, 0.84 => 8400
    function getCollateralFactor() public view returns (uint16 collateralFactor) {
        (, collateralFactor,) = oracle.tokenFactors(lpToken);
//        console.log("collateralFactor", collateralFactor);
    }

    /// @dev Query the borrow factor of the debt token on Homora, 1.04 => 10400
    /// @param token: Address of the ERC-20 debt token
    function getBorrowFactor(address token)
        public view
        returns (uint16 borrowFactor)
    {
        (borrowFactor,,) = oracle.tokenFactors(token);
//        console.log("borrowFactor", borrowFactor);
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
        return positions[position_info.chainId][position_info.positionId].shareAmount;
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
//        IERC20(token).safeTransferFrom(msg.sender, address(this), amount0);
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

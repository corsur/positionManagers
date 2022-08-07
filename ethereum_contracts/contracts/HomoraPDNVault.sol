//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IHomoraAvaxRouter.sol";
import "./interfaces/IHomoraBank.sol";
import "./interfaces/IHomoraOracle.sol";
import "./interfaces/IHomoraSpell.sol";
import "./interfaces/IUniswapPair.sol";
import "./interfaces/IUniswapV2Factory.sol";
import "./libraries/VaultLib.sol";

contract HomoraPDNVault is ERC20, ReentrancyGuard, IStrategyManager {
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

    // --- constants ---
    uint256 private constant _NO_ID = 0;
    uint256 private constant MAX_UINT256 = 2**256 - 1;
    bytes private constant HARVEST_DATA =
        abi.encodeWithSelector(bytes4(keccak256("harvestWMasterChef()")));

    // --- config ---
    address public admin;
    address public apertureManager;
    address public stableToken;
    address public assetToken;
    address public spell;
    address public rewardToken;
    address public lpToken;
    uint256 public leverageLevel;
    uint256 public pid; // pool id
    IHomoraBank public homoraBank;
    IUniswapPair public pair;
    IHomoraAvaxRouter public router;

    uint256 private _targetDebtRatio; // target debt ratio * 10000
    uint256 private _minDebtRatio; // minimum debt ratio * 10000
    uint256 private _maxDebtRatio; // maximum debt ratio * 10000
    uint256 private _dnThreshold; // offset percentage * 10000

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
    uint256 public homoraBankPosId;
    uint256 public totalCollShareAmount;
    bool isDeltaNeutral;
    bool isDebtRatioHealthy;
    VaultLib.VaultPosition pos;

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
    error HomoraPDNVault_DeltaIsNeutral();
    error HomoraPDNVault_DebtRatioIsHealthy();
    error Insufficient_Liquidity_Mint();

    constructor(
        address _admin,
        address _apertureManager,
        string memory _name,
        string memory _symbol,
        address _stableToken,
        address _assetToken,
        uint256 _leverageLevel,
        address _homoraBank,
        address _spell,
        address _rewardToken,
        uint256 _pid
    ) ERC20(_name, _symbol) {
        admin = _admin;
        apertureManager = _apertureManager;
        stableToken = _stableToken;
        assetToken = _assetToken;
        homoraBank = IHomoraBank(_homoraBank);
        require(_leverageLevel >= 2, "Leverage at least 2");
        leverageLevel = _leverageLevel;
        spell = _spell;
        rewardToken = _rewardToken;
        pid = _pid;
        homoraBankPosId = _NO_ID;
        totalCollShareAmount = 0;
        lpToken = IHomoraSpell(spell).pairs(stableToken, assetToken);
        require(lpToken != address(0), "Pair does not match the spell.");
        pair = IUniswapPair(lpToken);
        router = IHomoraAvaxRouter(IHomoraSpell(spell).router());

        // set config values
        _targetDebtRatio = 9200;
        _minDebtRatio = 9100;
        _maxDebtRatio = 9300;
        _dnThreshold = 300;
    }

    fallback() external payable {}

    receive() external payable {}

    function openPosition(
        address recipient,
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount
        ) = abi.decode(data, (uint256, uint256));
        depositInternal(
            recipient,
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount
        );
    }

    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (
            uint256 stableTokenDepositAmount,
            uint256 assetTokenDepositAmount
        ) = abi.decode(data, (uint256, uint256));
        depositInternal(
            address(0),
            position_info,
            stableTokenDepositAmount,
            assetTokenDepositAmount
        );
    }

    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external onlyApertureManager nonReentrant {
        (uint256 amount, address recipient) = abi.decode(
            data,
            (uint256, address)
        );
        // console.log('amount %d:', amount);
        // console.log('recipient %s', recipient);
        withdrawInternal(position_info, amount, recipient);
    }

    function setConfig(
        uint256 targetR,
        uint256 minR,
        uint256 maxR,
        uint256 dnThr
    ) public onlyAdmin {
        require(minR < targetR && targetR < maxR, "Invalid debt ratios");
        _targetDebtRatio = targetR;
        _minDebtRatio = minR;
        _maxDebtRatio = maxR;
        require(0 < dnThr && dnThr < 10000, "Invalid delta threshold");
        _dnThreshold = dnThr;
    }

    function deltaNeutral(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    )
        internal
        returns (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        )
    {
        _stableTokenAmount = _stableTokenDepositAmount;

        // swap all assetTokens into stableTokens
        if (_assetTokenDepositAmount > 0) {
            uint256 amount = _swap(
                _assetTokenDepositAmount,
                assetToken,
                stableToken
            );
            // update the stableToken amount
            _stableTokenAmount += amount;
        }
        _assetTokenAmount = 0;

        // total stableToken leveraged amount
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        uint256 totalAmount = _stableTokenAmount * leverageLevel;
        uint256 desiredAmount = totalAmount / 2;
        _stableTokenBorrowAmount = desiredAmount - _stableTokenAmount;
        _assetTokenBorrowAmount = router.quote(
            desiredAmount,
            reserve0,
            reserve1
        );
    }

    function currentDebtAmount() public view returns (uint256, uint256) {
        (address[] memory tokens, uint256[] memory debts) = homoraBank
            .getPositionDebts(homoraBankPosId);
        uint256 stableTokenDebtAmount = 0;
        uint256 assetTokenDebtAmount = 0;

        for (uint256 i = 0; i < tokens.length; i++) {
            if (tokens[i] == stableToken) {
                stableTokenDebtAmount = debts[i];
            }
            if (tokens[i] == assetToken) {
                assetTokenDebtAmount = debts[i];
            }
        }
        return (stableTokenDebtAmount, assetTokenDebtAmount);
    }

    function depositInternal(
        address recipient,
        PositionInfo memory position_info,
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) internal {
        // Record the balance state before transfer fund.

        uint256[] memory balanceArray = new uint256[](3);
        balanceArray[0] = IERC20(stableToken).balanceOf(address(this));
        balanceArray[1] = IERC20(assetToken).balanceOf(address(this));
        balanceArray[2] = address(this).balance;

        // Transfer user's deposit.
        if (_stableTokenDepositAmount > 0)
            IERC20(stableToken).transferFrom(
                msg.sender,
                address(this),
                _stableTokenDepositAmount
            );
        if (_assetTokenDepositAmount > 0)
            IERC20(assetToken).transferFrom(
                msg.sender,
                address(this),
                _assetTokenDepositAmount
            );

        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount);

        // Record original collateral size.
        uint256 originalCollSize = getCollateralSize();

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), _stableTokenAmount);
        IERC20(assetToken).approve(address(homoraBank), _assetTokenAmount);

        bytes memory data = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
                )
            ),
            stableToken,
            assetToken,
            [
                _stableTokenAmount,
                _assetTokenAmount,
                0,
                _stableTokenBorrowAmount,
                _assetTokenBorrowAmount,
                0,
                0,
                0
            ],
            pid
        );

        homoraBankPosId = IHomoraBank(homoraBank).execute(
            homoraBankPosId,
            spell,
            data
        );

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        // Calculate user share amount.
        uint256 finalCollSize = getCollateralSize();
        uint256 collSize = finalCollSize - originalCollSize;
        uint256 collShareAmount = originalCollSize == 0
            ? collSize
            : (collSize * totalCollShareAmount) / originalCollSize;

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

        emit LogDeposit(recipient, collSize, collShareAmount);
    }

    function withdrawInternal(
        PositionInfo memory position_info,
        uint256 withdrawShareAmount,
        address recipient
    ) internal {
        require(withdrawShareAmount > 0, "zero withdrawal amount");
        require(
            withdrawShareAmount <=
                positions[position_info.chainId][position_info.positionId]
                    .collShareAmount,
            "not enough share amount to withdraw"
        );

        // Harvest reward token and swap to stable token.
        _harvest();
        _swapReward();

        // Record the balance state before remove liquidity.

        uint256[] memory balanceArray = new uint256[](3);
        balanceArray[0] = IERC20(stableToken).balanceOf(address(this));
        balanceArray[1] = IERC20(assetToken).balanceOf(address(this));
        balanceArray[2] = address(this).balance;

        // Calculate collSize to withdraw.
        uint256 totalCollSize = getCollateralSize();
        uint256 collWithdrawSize = (withdrawShareAmount * totalCollSize) /
            totalCollShareAmount;

        // Calculate debt to repay in two tokens.
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = currentDebtAmount();

        // Encode removeLiqiduity call.
        bytes memory data = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
                )
            ),
            stableToken,
            assetToken,
            [
                collWithdrawSize,
                0,
                (stableTokenDebtAmount * collWithdrawSize) /
                    totalCollShareAmount,
                (assetTokenDebtAmount * collWithdrawSize) /
                    totalCollShareAmount,
                0,
                0,
                0
            ]
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        // Calculate token disbursement amount.
        uint256 stableTokenWithdrawAmount = IERC20(stableToken).balanceOf(
            address(this)
        ) -
            balanceArray[0] +
            (balanceArray[0] * withdrawShareAmount) /
            totalCollShareAmount;

        uint256 assetTokenWithdrawAmount = IERC20(assetToken).balanceOf(
            address(this)
        ) -
            balanceArray[1] +
            (balanceArray[1] * withdrawShareAmount) /
            totalCollShareAmount;
        uint256 avaxWithdrawAmount = address(this).balance -
            balanceArray[2] +
            (balanceArray[2] * withdrawShareAmount) /
            totalCollShareAmount;

        // Transfer fund to user (caller).
        IERC20(stableToken).transfer(recipient, stableTokenWithdrawAmount);
        IERC20(assetToken).transfer(recipient, assetTokenWithdrawAmount);
        payable(recipient).transfer(avaxWithdrawAmount);

        // Update position info.
        positions[position_info.chainId][position_info.positionId]
            .collShareAmount -= withdrawShareAmount;
        totalCollShareAmount -= withdrawShareAmount;

        // Emit event.
        emit LogWithdraw(
            recipient,
            withdrawShareAmount,
            stableTokenWithdrawAmount,
            assetTokenWithdrawAmount,
            avaxWithdrawAmount
        );
    }

    function rebalance(
        uint256 expectedRewardsLP
    ) external onlyApertureManager nonReentrant {
        _getPositionInfo();
        // check if the position need rebalance
        // assume token A is the stable token
        // 1. delta-neutrality check
        if (VaultLib.getOffset(pos.amtB, pos.debtAmtB) < _dnThreshold) {
            isDeltaNeutral = true;
        }

        debtRatio = getDebtRatio();
        // 2. debtRatio check
        if ((_minDebtRatio < debtRatio) && (debtRatio < _maxDebtRatio)) {
            isDebtRatioHealthy = true;
        }

        if (isDeltaNeutral && isDebtRatioHealthy) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        reinvest(expectedRewardsLP);

        _getPositionInfo();
        // 1. short: amtB < debtAmtB, R > Rt
        if (pos.debtAmtB > pos.amtB) {
            _reBalanceShort();
        }
        // 2. long: amtB > debtAmtB, R < Rt
        else {
            _rebalanceLong();
        }

        _getPositionInfo();
        if (VaultLib.getOffset(pos.amtB, pos.debtAmtB) >= _dnThreshold) {
            revert HomoraPDNVault_DeltaIsNeutral();
        }

        debtRatio = getDebtRatio();
        if ((debtRatio <= _minDebtRatio) || (debtRatio >= _maxDebtRatio)) {
            revert HomoraPDNVault_DebtRatioIsHealthy();
        }

        emit LogRebalance(pos.collateralSize, getCollateralSize());
    }

    function _reBalanceShort() internal {
        (uint256 reserveA, uint256 reserveB) = _getReserves();

        (
            uint256 collWithdrawAmt,
            uint256 amtARepay,
            uint256 amtBRepay
        ) = VaultLib.rebalanceShort(pos, leverageLevel, reserveA, reserveB);

        bytes memory data1 = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
                )
            ),
            stableToken,
            assetToken,
            [collWithdrawAmt, 0, amtARepay, amtBRepay, 0, 0, 0]
        );

        homoraBank.execute(homoraBankPosId, spell, data1);
    }

    function _rebalanceLong() internal {
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
        IERC20(stableToken).approve(address(homoraBank), MAX_UINT256);
        bytes memory data0 = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
                )
            ),
            stableToken,
            assetToken,
            [amtAReward, 0, 0, amtABorrow, amtBBorrow, 0, 0, 0],
            pid
        );
        homoraBank.execute(homoraBankPosId, spell, data0);
        IERC20(stableToken).approve(address(homoraBank), 0);
    }

    function reinvest(
        uint256 expectedLP
    ) external onlyApertureManager nonReentrant {
        uint256 equityBefore = getCollateralSize();

        // 1. claim rewards
        _harvest();
        _swapReward();

        // 2. reinvest with the current balance
        _reinvestInternal();

        uint256 equityAfter = getCollateralSize();

        require((equityAfter - equityBefore) > expectedLP, "Received less LP than expected");

        emit LogReinvest(equityBefore, equityAfter);
    }

    function _harvest() internal {
        homoraBank.execute(homoraBankPosId, spell, HARVEST_DATA);
    }

    function _reinvestInternal() internal {
        uint256 avaxBalance = address(this).balance;

        if (avaxBalance > 0) {
            _swapAVAX(avaxBalance, stableToken);
        }

        uint256 stableTokenBalance = IERC20(stableToken).balanceOf(address(this));
        uint256 assetTokenBalance = IERC20(assetToken).balanceOf(address(this));

        (uint256 reserve0, ) = _getReserves();
        uint256 liquidity = (((stableTokenBalance * leverageLevel) / 2) *
            IERC20(lpToken).totalSupply()) / reserve0;
        require(liquidity > 0, "Insufficient liquidity minted");

        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance);

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), MAX_UINT256);
        IERC20(assetToken).approve(address(homoraBank), MAX_UINT256);

        // Encode the calling function.
        bytes memory data = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
                )
            ),
            stableToken,
            assetToken,
            [
                _stableTokenAmount,
                _assetTokenAmount,
                0,
                _stableTokenBorrowAmount,
                _assetTokenBorrowAmount,
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
    }

    function _getPositionInfo() internal {
        pos.collateralSize = getCollateralSize();
        (pos.amtA, pos.amtB) = convertCollateralToTokens(pos.collateralSize);
        (pos.debtAmtA, pos.debtAmtB) = currentDebtAmount();
    }

    /// @notice Swap amount of fromToken into toToken
    function _swap(
        uint256 amount,
        address fromToken,
        address toToken
    ) internal returns (uint256) {
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

    /// @notice swap reward tokens into stable tokens
    function _swapReward() internal {
        uint256 rewardAmt = IERC20(rewardToken).balanceOf(address(this));
        if (rewardAmt > 0) {
            _swap(rewardAmt, rewardToken, stableToken);
        }
    }

    /// @notice Get the numbers of 2 tokens in the pool
    function _getReserves()
        internal
        view
        returns (uint256 reserve0, uint256 reserve1)
    {
        (reserve0, reserve1) = VaultLib.getReserves(lpToken, stableToken);
    }

    /// @notice Calculate the debt ratio and return the ratio, multiplied by 1e4
    function getDebtRatio() public view returns (uint256) {
        uint256 collateralValue = homoraBank.getCollateralETHValue;
        uint256 borrowValue = homoraBank.getBorrowETHValue(homoraBankPosId);
        return (borrowValue * 10000) / collateralValue;
    }

    /// @notice Calculate the real time leverage and return the leverage, multiplied by 1e4
    function getLeverage() public view returns (uint256) {
        // 0: stableToken, 1: assetToken
        (uint256 amount0, uint256 amount1) = convertCollateralToTokens(
            getCollateralSize()
        );
        (uint256 debtAmt0, uint256 debtAmt1) = currentDebtAmount();
        (uint256 reserve0, uint256 reserve1) = _getReserves();

        uint256 totalEquity = amount0 +
            (amount1 > 0 ? router.quote(amount1, reserve1, reserve0) : 0);
        uint256 debtEquity = debtAmt0 +
            (debtAmt1 > 0 ? router.quote(debtAmt1, reserve1, reserve0) : 0);

        return (totalEquity * 10000) / (totalEquity - debtEquity);
    }

    function getCollateralSize() public view returns (uint256) {
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        return collateralSize;
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens
    function convertCollateralToTokens(uint256 collAmount)
        public
        view
        returns (uint256 amount0, uint256 amount1)
    {
        (amount0, amount1) = VaultLib.convertCollateralToTokens(lpToken, stableToken, collAmount);
    }

    function quote(address token, uint256 amount) public view returns(uint256) {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        if (token == stableToken) {
            return router.quote(amount, reserve0, reserve1);
        } else {
            return router.quote(amount, reserve1, reserve0);
        }
    }

    /// @notice swap function for external tests, swap stableToken into assetToken
    function swapExternal(address token, uint256 amount0)
        external
        returns (uint256 amt)
    {
        IERC20(token).transferFrom(msg.sender, address(this), amount0);
        if (token == stableToken) {
            amt = _swap(amount0, stableToken, assetToken);
            IERC20(assetToken).transfer(msg.sender, amt);
        } else if (token == assetToken) {
            amt = _swap(amount0, assetToken, stableToken);
            IERC20(stableToken).transfer(msg.sender, amt);
        }
        return amt;
    }
}

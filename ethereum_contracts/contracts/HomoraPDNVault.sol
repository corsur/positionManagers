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
        require(msg.sender == apertureManager, "unauthorized position op");
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

    uint256 private _TR; // target debt ratio * 10000
    uint256 private _MR; // maximum debt ratio * 10000
    uint256 public dnThreshold; // offset percentage * 10000
    uint256 public leverageThreshold; // offset percentage * 10000

    // --- state ---
    // positions[chainId][positionId] stores share information about the position identified by (chainId, positionId).
    mapping(uint16 => mapping(uint128 => Position)) public positions;
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
        _TR = 9500;
        _MR = 9900;
        dnThreshold = 500;
        leverageThreshold = 500;
    }

    fallback() external payable {}

    receive() external payable {}

    function openPosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (stableTokenDepositAmount, assetTokenDepositAmount) = abi.decode(data, (uint256, uint256));
        depositInternal(position_info, _stableTokenDepositAmount, _assetTokenDepositAmount);
    }

    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable onlyApertureManager nonReentrant {
        (stableTokenDepositAmount, assetTokenDepositAmount) = abi.decode(data, (uint256, uint256));
        depositInternal(position_info, _stableTokenDepositAmount, _assetTokenDepositAmount);
    }

    function decreasePosition(
        PositionInfo memory position_info,
        uint256 amount,
        address recipient
    ) external onlyApertureManager nonReentrant {}

    function closePosition(PositionInfo memory position_info, address recipient)
        external
        onlyApertureManager
        nonReentrant
    {}

    /// @notice Set target and maximum debt ratio
    /// @param targetR target ratio * 1e4
    /// @param maxR maximum ratio * 1e4
    function setTargetRatio(uint256 targetR, uint256 maxR) public onlyAdmin {
        _TR = targetR;
        _MR = maxR;
    }

    /// @notice Set delta-neutral offset threshold
    /// @param threshold delta-neutral offset threshold * 1e4
    function setDNThreshold(uint256 threshold) public onlyAdmin {
        dnThreshold = threshold;
    }

    /// @notice Set leverage offset threshold
    /// @param threshold leverage offset threshold * 1e4
    function setLeverageThreshold(uint256 threshold) public onlyAdmin {
        leverageThreshold = threshold;
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

        return (
            _stableTokenAmount,
            0,
            _stableTokenBorrowAmount,
            _assetTokenBorrowAmount
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
        PositionInfo memory position_info,
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) internal {
        // Record the balance state before transfer fund.
        uint256 stableTokenBalanceBefore = IERC20(stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenBalanceBefore = IERC20(assetToken).balanceOf(
            address(this)
        );
        uint256 avaxBalanceBefore = address(this).balance;

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
        (, , , uint256 originalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );

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
        (, , , uint256 finalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
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
            msg.sender,
            IERC20(stableToken).balanceOf(address(this)) -
                stableTokenBalanceBefore
        );
        IERC20(assetToken).transfer(
            msg.sender,
            IERC20(assetToken).balanceOf(address(this)) -
                assetTokenBalanceBefore
        );
        payable(msg.sender).transfer(address(this).balance - avaxBalanceBefore);

        emit LogDeposit(msg.sender, collSize, collShareAmount);
    }

    function withdrawInternal(
        PositionInfo memory position_info,
        uint256 withdrawShareAmount,
        address recipient
    ) internal {
        require(withdrawShareAmount > 0, "inccorect withdraw amount");
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
        uint256 stableTokenBalanceBefore = IERC20(stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenBalanceBefore = IERC20(assetToken).balanceOf(
            address(this)
        );
        uint256 avaxBalanceBefore = address(this).balance;

        // Calculate collSize to withdraw.
        (, , , uint256 totalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
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
            stableTokenBalanceBefore +
            (stableTokenBalanceBefore * withdrawShareAmount) /
            totalCollShareAmount;

        uint256 assetTokenWithdrawAmount = IERC20(assetToken).balanceOf(
            address(this)
        ) -
            assetTokenBalanceBefore +
            (assetTokenBalanceBefore * withdrawShareAmount) /
            totalCollShareAmount;
        uint256 avaxWithdrawAmount = address(this).balance -
            avaxBalanceBefore +
            (avaxBalanceBefore * withdrawShareAmount) /
            totalCollShareAmount;

        // Transfer fund to user (caller).
        IERC20(stableToken).transfer(msg.sender, stableTokenWithdrawAmount);
        IERC20(assetToken).transfer(msg.sender, assetTokenWithdrawAmount);
        payable(msg.sender).transfer(avaxWithdrawAmount);

        // Update position info.
        positions[position_info.chainId][position_info.positionId].collShareAmount -= withdrawShareAmount;
        totalCollShareAmount -= withdrawShareAmount;

        // Emit event.
        emit LogWithdraw(
            msg.sender,
            withdrawShareAmount,
            stableTokenWithdrawAmount,
            assetTokenWithdrawAmount,
            avaxWithdrawAmount
        );
    }

    function rebalance() external {
        // check if the position need rebalance
        bool isDeltaNeutral = false;
        bool isLeverageHealthy = false;
        bool isDebtRatioHealthy = false;

        uint256 collateralSize = getCollateralSize();

        // 1. delta-neutrality check
        (, uint256 assetTokenAmt) = convertCollateralToTokens(collateralSize);
        (, uint256 assetTokenDebtAmt) = currentDebtAmount();
        if (_getOffset(assetTokenAmt, assetTokenDebtAmt) < dnThreshold) {
            isDeltaNeutral = true;
            console.log("Position is delta neutral");
        } else {
            console.log("Position is not delta neutral");
        }

        // 2. leverage check
        uint256 leverage = getLeverage();
        //// offset larger than 5%
        console.log("leverage: %d/10000", leverage);
        if (_getOffset(leverage, leverageLevel * 10000) < leverageThreshold) {
            isLeverageHealthy = true;
        }

        // 3. debtRatio check
        uint256 debtRatio = getDebtRatio();
        console.log("Delta ratio: %d/10000", debtRatio);
        if (debtRatio <= _TR) {
            isDebtRatioHealthy = true;
        }

        if (isDeltaNeutral && isLeverageHealthy && isDebtRatioHealthy) {
            revert HomoraPDNVault_PositionIsHealthy();
        }

        console.log("Execute rebalance");

        // withdraw all lp tokens and repay all the debts
        // here we withdraw 99.99% of the collateral to avoid (collateral credit < borrow credit)
        _removeLiquidityInternal();

        // swap reward tokens into stable tokens
        _swapReward();

        // reinvest
        _reinvestInternal();

        uint256 collateralAfter = getCollateralSize();

        emit LogRebalance(collateralSize, collateralAfter);
    }

    /// @notice withdraw collateral tokens and repay the debt
    function _removeLiquidityInternal() internal {
        uint256 collateralSize = getCollateralSize();
        // TODO: remove this hack after detailed math derivation is implemented.
        uint256 collAmount = (collateralSize * 9999) / 10000;

        (
            uint256 stableTokenAmt,
            uint256 assetTokenAmt
        ) = convertCollateralToTokens(collAmount);
        (
            uint256 stableTokenDebtAmt,
            uint256 assetTokenDebtAmt
        ) = currentDebtAmount();

        uint256 stableTokenRepayAmt = stableTokenAmt > stableTokenDebtAmt
            ? stableTokenDebtAmt
            : stableTokenAmt;
        uint256 assetTokenRepayAmt = assetTokenAmt > assetTokenDebtAmt
            ? assetTokenDebtAmt
            : assetTokenAmt;

        bytes memory data = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
                )
            ),
            stableToken,
            assetToken,
            [collAmount, 0, stableTokenRepayAmt, assetTokenRepayAmt, 0, 0, 0]
        );
        homoraBank.execute(homoraBankPosId, spell, data);
    }

    function reinvest() external {
        uint256 equityBefore = getCollateralSize();

        // 1. claim rewards
        _harvest();
        _swapReward();

        // 2. reinvest with the current balance
        _reinvestInternal();

        uint256 equityAfter = getCollateralSize();
        emit LogReinvest(equityBefore, equityAfter);
    }

    /// @notice harvest rewards
    function _harvest() internal {
        homoraBank.execute(homoraBankPosId, spell, HARVEST_DATA);
    }

    /// @notice reinvest with the current balance
    function _reinvestInternal() internal {
        uint256 stableTokenBalance = IERC20(stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenBalance = IERC20(assetToken).balanceOf(address(this));
        uint256 avaxBalance = address(this).balance;

        if (assetTokenBalance > 0) {
            _swap(assetTokenBalance, assetToken, stableToken);
        }

        if (avaxBalance > 0) {
            _swapAVAX(avaxBalance, stableToken);
        }

        // update token balances
        stableTokenBalance = IERC20(stableToken).balanceOf(address(this));
        assetTokenBalance = IERC20(assetToken).balanceOf(address(this));
        avaxBalance = address(this).balance;

        (uint256 reserve0, ) = _getReserves();
        uint256 liquidity = (((stableTokenBalance * leverageLevel) / 2) *
            IERC20(lpToken).totalSupply()) / reserve0;
        require(liquidity > 0, "Insufficient liquidity minted");

        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance); // (stableTokenBalance, 0, 0, 0); //

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
        if (pair.token0() == stableToken) {
            (reserve0, reserve1, ) = pair.getReserves();
        } else {
            (reserve1, reserve0, ) = pair.getReserves();
        }
    }

    /// @notice Get assetToken's price in terms of stableToken, multiplied by 1e4
    function getTokenPrice() external view returns (uint256) {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return (reserve0 * 10000) / reserve1;
    }

    /// @notice Calculate the debt ratio and return the ratio, multiplied by 1e4
    function getDebtRatio() public view returns (uint256) {
        uint256 collateralValue = homoraBank.getCollateralETHValue(
            homoraBankPosId
        );
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
        returns (uint256, uint256)
    {
        uint256 totalLPSupply = IERC20(address(pair)).totalSupply();

        (uint256 reserve0, uint256 reserve1) = _getReserves();

        uint256 amount0 = (collAmount * reserve0) / totalLPSupply;
        uint256 amount1 = (collAmount * reserve1) / totalLPSupply;
        return (amount0, amount1);
    }

    /// @notice Query the Token factors for token, multiplied by 1e4
    function _getTokenFactor(address token)
        internal
        view
        returns (
            uint16 borrowFactor,
            uint16 collateralFactor,
            uint16 liqIncentive
        )
    {
        IHomoraOracle oracle = IHomoraOracle(address(homoraBank.oracle()));
        return oracle.tokenFactors(token);
    }

    /// @notice Query the Homora's borrow credit factor for token, multiplied by 1e4
    function getBorrowFactor(address token) public view returns (uint16) {
        (uint16 borrowFactor, , ) = _getTokenFactor(token);
        return borrowFactor;
    }

    /// @notice Query the Homora's collateral credit factor for the LP token, multiplied by 1e4
    function getCollateralFactor() public view returns (uint16) {
        (, uint16 stableFactor, ) = _getTokenFactor(stableToken);
        (, uint16 assetFactor, ) = _getTokenFactor(assetToken);
        return stableFactor > assetFactor ? assetFactor : stableFactor;
    }

    /// @notice Calculate offset ratio, multiplied by 1e4
    function _getOffset(uint256 currentVal, uint256 targetVal)
        internal
        pure
        returns (uint256)
    {
        uint256 diff = currentVal > targetVal
            ? currentVal - targetVal
            : targetVal - currentVal;
        if (targetVal == 0) {
            if (diff == 0) return 0;
            else return 2**64;
        } else return (diff * 10000) / targetVal;
    }

    function getBalanceOf(address token) public view returns (uint256) {
        return IERC20(token).balanceOf(address(this));
    }

    function getEquivalentTokenB(uint256 amountA)
        external
        view
        returns (uint256)
    {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return router.quote(amountA, reserve0, reserve1);
    }

    function getEquivalentTokenA(uint256 amountB)
        external
        view
        returns (uint256)
    {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return router.quote(amountB, reserve1, reserve0);
    }

    function getLiquidityMinted(uint256 amount0, uint256 amount1)
        public
        view
        returns (uint256)
    {
        uint256 totalSupply = IERC20(lpToken).totalSupply();
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        uint256 liquidity0 = (amount0 * totalSupply) / reserve0;
        uint256 liquidity1 = (amount1 * totalSupply) / reserve1;
        return liquidity0 > liquidity1 ? liquidity1 : liquidity0;
    }
}

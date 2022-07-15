//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/math/SafeMath.sol";

import "./interfaces/IHomoraBank.sol";
import "./interfaces/IPair.sol";
import "./interfaces/IFactory.sol";
import "./interfaces/IRouter.sol";
import "./interfaces/IOracle.sol";
import "./interfaces/ISpell.sol";

// Is there some other way?
library HomoraSafeMath {
    using SafeMath for uint256;

    /// @dev Computes round-up division.
    function ceilDiv(uint256 a, uint256 b) internal pure returns (uint256) {
        return a.add(b).sub(1).div(b);
    }
}

contract DeltaNeutralVault is ERC20, ReentrancyGuard {
    using SafeMath for uint256;
    using HomoraSafeMath for uint256;

    struct Position {
        uint256 collShareAmount;
    }

    uint256 private constant _NO_ID = 0;

    // --- config ---
    address public stableToken;
    address public assetToken;
    address public spell;
    address public rewardToken;
    address public lpToken;
    uint256 public leverageLevel;
    uint256 public pid; // pool id
    IHomoraBank public homoraBank;
    IPair public pair;
    IRouter public router;

    uint256 private _TR; // target debt ratio * 10000
    uint256 private _MR; // maximum debt ratio * 10000
    uint256 public dnThreshold; // offset percentage * 10000
    uint256 public leverageThreshold; // offset percentage * 10000

    // --- state ---
    mapping(address => Position) public positions;
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
        uint256 stableTokenAmount,
        uint256 assetTokenAmount
    );
    event LogRebalance(uint256 equityBefore, uint256 equityAfter);
    event LogReinvest(uint256 equityBefore, uint256 equityAfter);

    // --- error ---
    error DeltaNeutralVault_PositionsIsHealthy();
    error Insufficient_Liquidity_Mint();

    constructor(
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
        stableToken = _stableToken;
        assetToken = _assetToken;
        homoraBank = IHomoraBank(_homoraBank);
        leverageLevel = _leverageLevel;
        spell = _spell;
        rewardToken = _rewardToken;
        pid = _pid;
        homoraBankPosId = _NO_ID;
        totalCollShareAmount = 0;
        lpToken = ISpell(spell).pairs(stableToken, assetToken);
        require(lpToken != address(0), "Pair does not match the spell.");
        pair = IPair(lpToken);
        router = IRouter(ISpell(spell).router());

        // set config values
        _TR = 9500;
        _MR = 9900;
        dnThreshold = 500;
        leverageThreshold = 500;
    }

    fallback() external payable {}

    receive() external payable {}

    /// @notice Set target and maximum debt ratio
    /// @param targetR target ratio * 1e4
    /// @param maxR maximum ratio * 1e4
    function setTargetRatio(uint256 targetR, uint256 maxR) public {
        _TR = targetR;
        _MR = maxR;
    }

    /// @notice Set delta-neutral offset threshold
    /// @param threshold delta-neutral offset threshold * 1e4
    function setDNThreshold(uint256 threshold) public {
        dnThreshold = threshold;
    }

    /// @notice Set leverage offset threshold
    /// @param threshold leverage offset threshold * 1e4
    function setLeverageThreshold(uint256 threshold) public {
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

    function currentDebtAmount() internal view returns (uint256, uint256) {
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

    function deposit(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) external payable nonReentrant {
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
        ) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount); // (_stableTokenDepositAmount, _assetTokenDepositAmount, 0, 0); //

        // Record original colletral size.
        (, , , uint256 originalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), 2**256 - 1);
        IERC20(assetToken).approve(address(homoraBank), 2**256 - 1);

        bytes memory data1 = abi.encodeWithSelector(
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

        uint256 res = IHomoraBank(homoraBank).execute(
            homoraBankPosId,
            spell,
            data1
        );

        if (homoraBankPosId == _NO_ID) {
            homoraBankPosId = res;
        }

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
            : collSize.mul(totalCollShareAmount).ceilDiv(originalCollSize);

        // Update vault position state.
        totalCollShareAmount += collShareAmount;

        // Update deposit owner's position state.
        positions[msg.sender].collShareAmount += collShareAmount;

        // Return leftover funds to user.
        IERC20(stableToken).transfer(
            msg.sender,
            IERC20(stableToken).balanceOf(address(this))
        );
        IERC20(assetToken).transfer(
            msg.sender,
            IERC20(assetToken).balanceOf(address(this))
        );
        emit LogDeposit(msg.sender, collSize, collShareAmount);
    }

    function withdraw(uint256 withdrawShareAmount)
        external
        payable
        nonReentrant
    {
        require(withdrawShareAmount > 0, "inccorect withdraw amount");
        require(
            withdrawShareAmount <= positions[msg.sender].collShareAmount,
            "not enough share amount to withdraw"
        );

        // Calculate collSize to withdraw.
        (, , , uint256 totalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        uint256 collWithdrawSize = withdrawShareAmount
            .mul(totalCollSize)
            .ceilDiv(totalCollShareAmount);

        (uint256 reserve0, ) = _getReserves();
        uint256 stableWithdrawAmount = reserve0.mul(collWithdrawSize).div(
            IERC20(lpToken).totalSupply()
        );

        // Calculate debt to repay in two tokens.
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = currentDebtAmount();
        uint256 stableTokenRepayAmount = stableTokenDebtAmount
            .mul(collWithdrawSize)
            .ceilDiv(totalCollShareAmount);
        uint256 assetTokenRepayAmount = assetTokenDebtAmount
            .mul(collWithdrawSize)
            .ceilDiv(totalCollShareAmount);

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
                stableTokenRepayAmount,
                assetTokenRepayAmount,
                0,
                0,
                0
            ]
        );

        homoraBank.execute(homoraBankPosId, spell, data);

        uint256 stableTokenWithdrawAmount = IERC20(stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenWithdrawAmount = IERC20(assetToken).balanceOf(
            address(this)
        );
        uint256 ethWithdrawAmount = address(this).balance;

        // Return withdraw funds to user.
        IERC20(stableToken).transfer(msg.sender, stableTokenWithdrawAmount);
        IERC20(assetToken).transfer(msg.sender, assetTokenWithdrawAmount);

        console.log(
            collWithdrawSize,
            stableTokenWithdrawAmount,
            assetTokenWithdrawAmount,
            ethWithdrawAmount
        );

        // Update position info.
        positions[msg.sender].collShareAmount -= withdrawShareAmount;
        totalCollShareAmount -= withdrawShareAmount;

        // Emit event.
        emit LogWithdraw(
            msg.sender,
            withdrawShareAmount,
            stableTokenWithdrawAmount,
            assetTokenWithdrawAmount
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
            revert DeltaNeutralVault_PositionsIsHealthy();
        }

        console.log("Execute rebalance");

        // withdraw all lp tokens and repay all the debts
        // here we withdraw 99.99% of the collateral to avoid (collateral credit < borrow credit)
        _removeLiquidityInternal((collateralSize * 9999) / 10000);

        // swap reward tokens into stable tokens
        _swapReward();

        // reinvest
        _reinvestInternal();

        uint256 collateralAfter = getCollateralSize();

        emit LogRebalance(collateralSize, collateralAfter);
    }

    /// @notice withdraw some amount of collateral tokens and repay part of the debt
    /// @param collAmount amount of collateral token to withdraw
    function _removeLiquidityInternal(uint256 collAmount) internal {
        uint256 collateralSize = getCollateralSize();
        require(collAmount <= collateralSize, "insufficient collateral size");

        (
            uint256 stableTokenAmt,
            uint256 assetTokenAmt
        ) = convertCollateralToTokens(collAmount);
        (
            uint256 stableTokenDebtAmt,
            uint256 assetTokenDebtAmt
        ) = currentDebtAmount();

        // repay debt propotional to the collateral withdraw amount
        // stableTokenDebtAmt =
        //     (stableTokenDebtAmt * collAmount) /
        //     getCollateralSize();
        // assetTokenDebtAmt =
        //     (assetTokenDebtAmt * collAmount) /
        //     getCollateralSize();

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
        bytes memory data = abi.encodeWithSelector(
            bytes4(keccak256("harvestWMasterChef()"))
        );
        homoraBank.execute(homoraBankPosId, spell, data);
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

        if (stableTokenBalance < 1000000) {
            revert Insufficient_Liquidity_Mint();
        }

        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance); // (stableTokenBalance, 0, 0, 0); //

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), 2**256 - 1);
        IERC20(assetToken).approve(address(homoraBank), 2**256 - 1);

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
        IOracle oracle = IOracle(address(homoraBank.oracle()));
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

    function getEquivalentTokenB(uint256 amountA) external view returns (uint256) {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return router.quote(amountA, reserve0, reserve1);
    }

    function getEquivalentTokenA(uint256 amountB) external view returns (uint256) {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return router.quote(amountB, reserve1, reserve0);
    }

    function test() external view {
        console.log("******** test ********");
        uint256 collateralSize = getCollateralSize();
        console.log(collateralSize);
        (uint256 a, uint256 b) = convertCollateralToTokens(collateralSize);
        console.log("collateral:");
        console.log(a, b);
        (uint256 debtAmount0, uint256 debtAmount1) = currentDebtAmount();
        console.log("debt:");
        console.log(debtAmount0, debtAmount1);
        console.log("balance:");
        console.log(getBalanceOf(stableToken), getBalanceOf(assetToken));
    }
}

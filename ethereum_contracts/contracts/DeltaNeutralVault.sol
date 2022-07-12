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

    // struct VaultPosition {
    //     uint256
    // }

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

    uint256 public nextPositionID = 0;
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
        require(
            address(lpToken) != address(0),
            "Pair does not match the spell."
        );
        pair = IPair(lpToken);
        router = IRouter(ISpell(spell).router());

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
        // UniswapV2Router contract address
        // address _router = 0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D;
        // IUniswapV2Router router = IUniswapV2Router(_router);

        // swap all assetTokens into stableTokens
        if (_assetTokenDepositAmount > 0) {
            address[] memory path = new address[](2);
            (path[0], path[1]) = (assetToken, stableToken);
            uint256[] memory amount = router.swapExactTokensForTokens(
                _assetTokenDepositAmount,
                0,
                path,
                address(this),
                block.timestamp
            );
            // update the stableToken amount
            _stableTokenAmount += amount[1];
        }

        // total stableToken leveraged amount
        uint256 totalAmount = _stableTokenAmount * leverageLevel;
        uint256 desiredAmount = totalAmount / 2;
        _stableTokenBorrowAmount = desiredAmount - _stableTokenAmount;
        _assetTokenBorrowAmount = desiredAmount * 10000 / getTokenPrice();

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
        ) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount);

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

    function withdraw(uint256 withdrawShareAmount) external nonReentrant {
        require(withdrawShareAmount > 0, "inccorect withdraw amount.");
        require(
            withdrawShareAmount <= positions[msg.sender].collShareAmount,
            "not enough share amount to withdraw."
        );

        (, , , uint256 totalCollSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        uint256 collWithdrawSize = withdrawShareAmount
            .mul(totalCollSize)
            .ceilDiv(totalCollShareAmount);

        bytes memory data1 = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
                )
            ),
            stableToken,
            assetToken,
            [collWithdrawSize, 0, 0, 0, 0, 0, 0]
        );

        IHomoraBank(homoraBank).execute(
            homoraBankPosId,
            spell,
            data1
        );

        uint256 stableTokenWithdrawAmount = IERC20(stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenWithdrawAmount = IERC20(assetToken).balanceOf(
            address(this)
        );

        // Return withdraw funds to user.
        IERC20(stableToken).transfer(msg.sender, stableTokenWithdrawAmount);
        IERC20(assetToken).transfer(msg.sender, assetTokenWithdrawAmount);

        console.log(
            collWithdrawSize,
            stableTokenWithdrawAmount,
            assetTokenWithdrawAmount
        );

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

        // 1. delta-neutrality check
        (, uint256 assetTokenAmt) = _convertCollateralToTokens();
        (
            uint256 stableTokenDebtAmt,
            uint256 assetTokenDebtAmt
        ) = currentDebtAmount();
        if (_getOffset(assetTokenAmt, assetTokenDebtAmt) < dnThreshold) {
            isDeltaNeutral = true;
        }

        // 2. leverage check
        uint256 leverage = _getLeverage();
        //// offset larger than 5%
        if (_getOffset(leverage, leverageLevel) < leverageThreshold) {
            isLeverageHealthy = true;
        }

        // 3. debtRatio check
        uint256 debtRatio = _getDebtRatio();
        if (debtRatio <= _TR) {
            isDebtRatioHealthy = true;
        }

        if (isDeltaNeutral && isLeverageHealthy && isDebtRatioHealthy) {
            revert DeltaNeutralVault_PositionsIsHealthy();
        }

        // execute rebalance
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );

        // Encode the calling function.
        bytes memory data = abi.encodeWithSelector(
            bytes4(
                keccak256(
                    "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
                )
            ),
            stableToken,
            assetToken,
            [collateralSize, 0, stableTokenDebtAmt, assetTokenDebtAmt, 0, 0, 0]
        );

        // withdraw all lp tokens and repay all the debts
        homoraBank.execute(homoraBankPosId, spell, data);

        // swap reward tokens into stable tokens
        _swapReward();

        // reinvest
        _reinvestInternal();

        (, , , uint256 collateralAfter) = homoraBank.getPositionInfo(
            homoraBankPosId
        );

        emit LogRebalance(collateralSize, collateralAfter);
    }

    function reinvest() external {
        (, , , uint256 equityBefore) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        // 1. claim rewards
        _harvest();
        _swapReward();

        // 2. reinvest with the current balance
        _reinvestInternal();

        (, , , uint256 equityAfter) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
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
        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(stableTokenBalance, assetTokenBalance);

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

    /// @notice swap reward tokens into stable tokens
    function _swapReward() internal {
        uint256 rewardAmt = IERC20(rewardToken).balanceOf(address(this));
        if (rewardAmt > 0) {
            // find the pool for rewardToken/stableToken
            address token = address(0);
            address USDC = 0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E;
            address USDC_e = 0xA7D7079b0FEaD91F3e65f86E8915Cb59c1a4C664;
            address[] memory stableTokenList = new address[](3);
            (stableTokenList[0], stableTokenList[1], stableTokenList[2]) = (
                stableToken,
                USDC,
                USDC_e
            );
            for (uint256 i = 0; i < stableTokenList.length; i++) {
                address rewardPool = ISpell(spell).pairs(
                    stableTokenList[i],
                    rewardToken
                );
                if (rewardPool != address(0)) {
                    token = stableTokenList[i];
                    break;
                }
            }
            require(
                token != address(0),
                "cannot find the pool to swap reward token"
            );

            // swap reward tokens for stable tokens
            if (token != address(0)) {
                address[] memory path = new address[](2);
                (path[0], path[1]) = (rewardToken, token);
                uint256[] memory amount = router.swapExactTokensForTokens(
                    rewardAmt,
                    0,
                    path,
                    address(this),
                    block.timestamp
                );

                // swap the stable tokens (USDC/USDC.e) received into the stableToken of this Vault
                if (token != stableToken) {
                    (path[0], path[1]) = (token, stableToken);
                    router.swapExactTokensForTokens(
                        amount[1],
                        0,
                        path,
                        address(this),
                        block.timestamp
                    );
                }
            }
        }
    }

    function _getReserves()
        internal
        view
        returns (uint256 reserve0, uint256 reserve1)
    {
        // reserve0 = IERC20(stableToken).balanceOf(address(pair));
        // reserve1 = IERC20(assetToken).balanceOf(address(pair));
        if (pair.token0() == stableToken) {
            (reserve0, reserve1, ) = pair.getReserves();
        } else {
            (reserve1, reserve0, ) = pair.getReserves();
        }
    }

    /// @notice Get assetToken's price in terms of stableToken, multiplied by 1e4
    function getTokenPrice() public view returns (uint256) {
        (uint256 reserve0, uint256 reserve1) = _getReserves();
        return reserve0 * 10000 / reserve1;
    }

    /// @notice Calculate the debt ratio and return the ratio * 10000
    function _getDebtRatio() internal view returns (uint256) {
        uint256 collateralValue = homoraBank.getCollateralETHValue(
            homoraBankPosId
        );
        uint256 borrowValue = homoraBank.getBorrowETHValue(homoraBankPosId);
        return (borrowValue * 10000) / collateralValue;
    }

    /// @notice Calculate the real time leverage and return the leverage * 10000
    function _getLeverage() internal view returns (uint256) {
        // 0: stableToken, 1: assetToken
        (uint256 amount0, uint256 amount1) = _convertCollateralToTokens();
        (uint256 debtAmount0, uint256 debtAmount1) = currentDebtAmount();
        // token price of asset token
        uint256 tokenPrice = getTokenPrice();
        uint256 totalEquity = amount0 + amount1 * tokenPrice / 10000;
        uint256 debtEquity = debtAmount0 + debtAmount1 * tokenPrice / 10000;
        return (totalEquity * 10000) / (totalEquity - debtEquity);
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens
    function _convertCollateralToTokens()
        internal
        view
        returns (uint256, uint256)
    {
        uint256 totalLPSupply = IERC20(address(pair)).totalSupply();
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );

        (uint256 reserve0, uint256 reserve1) = _getReserves();

        uint256 amount0 = (collateralSize / totalLPSupply) * reserve0;
        uint256 amount1 = (collateralSize / totalLPSupply) * reserve1;
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
        return (diff * 10000) / targetVal;
    }

    function test() external view {
        // console.log(IERC20(lpToken).totalSupply());
        console.log(getTokenPrice());
    }
}

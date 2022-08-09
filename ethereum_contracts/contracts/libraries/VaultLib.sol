// SPDX-License-Identifier: BUSL-1.1
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";
import "../interfaces/IUniswapPair.sol";
import "./libraries/HomoraMath.sol";

library VaultLib {
    uint256 public constant feeRate = 30;   // feeRate = 0.3%
    uint256 public constant unity = 10000;
    uint256 public constant unityMinusFee = 9970;
    uint256 public constant someLargeNumber = 10**18;

    struct RebalanceHelper {
        uint256 Ka;
        uint256 Kb;
        uint256 Sa;
        uint256 Sb;
        uint256 collWithdrawAmt;
        uint256 amtARepay;
        uint256 amtBRepay;
        uint256 amtAWithdraw;
        uint256 amtBWithdraw;
        uint256 reserveAAfter;
        uint256 reserveBAfter;
        uint256 amtABorrow;
        uint256 amtBBorrow;
        uint256 amtAAfter;
        uint256 amtBAfter;

        uint256 collWithdrawErr;
        uint256 amtABorrowErr;
        uint256 amtBBorrowErr;
        uint256 amtARepayErr;
        uint256 amtBRepayErr;
    }

    struct VaultPosition {
        uint256 collateralSize;
        uint256 amtA;
        uint256 amtB;
        uint256 debtAmtA;
        uint256 debtAmtB;
    }

    /// @notice Calculate offset ratio, multiplied by 1e4
    function getOffset(uint256 currentVal, uint256 targetVal)
        public
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

    /// @notice Return pool reserves. Stable token first
    function getReserves(address lpToken, address stableToken) 
        public 
        view 
        returns (
            uint256 reserve0,
            uint256 reserve1
        ) 
    {
        IUniswapPair pair = IUniswapPair(lpToken);
        if (pair.token0() == stableToken) {
            (reserve0, reserve1, ) = pair.getReserves();
        } else {
            (reserve1, reserve0, ) = pair.getReserves();
        }
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens. Stable token first
    function convertCollateralToTokens(address lpToken, address stableToken, uint256 collAmount)
        public
        view
        returns (uint256 amount0, uint256 amount1)
    {
        uint256 totalLPSupply = IERC20(lpToken).totalSupply();

        (uint256 reserve0, uint256 reserve1) = getReserves(lpToken, stableToken);

        amount0 = (collAmount * reserve0) / totalLPSupply;
        amount1 = (collAmount * reserve1) / totalLPSupply;
    }

    /// @dev Calculate the params passed to Homora to create PDN position
    /// @param Ua The amount of stable token supplied by user
    /// @param Ub The amount of asset token supplied by user
    /// @param Na Stable token pool reserve
    /// @param Nb Asset token pool reserve
    /// @param L Leverage
    function deltaNeutral(
        uint256 Ua,
        uint256 Ub,
        uint256 Na,
        uint256 Nb,
        uint256 L
    )
        internal
        returns (
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        )
    {
        uint256 b = 2 * Nb + (2 * unity - unityMinusFee * L) * Ub / unity
            - L * (unityMinusFee * (Na + Ua) * Ub**2 + (unity * Nb + (unity + unityMinusFee) * Ub) * Ua * Nb)
            / (unity * Na * Nb);
        uint256 c = L * (unityMinusFee * (Na + Ua) * Ub + unity * Nb * Ua) * (Nb + Ub)**2
            / (unity * Na * Nb);
        uint256 squareRoot = HomoraMath.sqrt(b * b + 8 * c);
        require(squareRoot > b, "No positive root");
        assetTokenBorrowAmount = (squareRoot - b) / 4;
        stableTokenBorrowAmount = (L - 2) * (Na + Ua) * assetTokenBorrowAmount
            / (L * (Nb + Ub) + 2 * assetTokenBorrowAmount);
    }

    /// @dev Calculate the amount of collateral to withdraw and the amount of each token to repay by Homora to reach DN
    /// @dev Assume `pos.debtAmtB > pos.amtB`. Check before calling
    /// @param pos HomoraBank's position info
    /// @param leverageLevel Target leverage
    /// @param reserveA Token A's pool reserve
    /// @param reserveB Token B's pool reserve
    function rebalanceShort(
        VaultPosition memory pos,
        uint256 leverageLevel,
        uint256 reserveA,
        uint256 reserveB
    )
        public
        view
        returns (
            uint256,
            uint256,
            uint256
        )
    {
        RebalanceHelper memory vars;
        // Ka << 1, multiply by someLargeNumber 1e18
        vars.Ka = unity * (pos.debtAmtB - pos.amtB) * someLargeNumber
            / (unityMinusFee * (reserveB - pos.debtAmtB));
        vars.Kb = (pos.debtAmtB - pos.amtB) * someLargeNumber / (reserveB - pos.amtB);
        vars.collWithdrawAmt = leverageLevel * (pos.debtAmtA * someLargeNumber + vars.Ka * reserveA)
            / (2 * (someLargeNumber + vars.Ka)) * pos.collateralSize / pos.amtA
            - (leverageLevel - 2) * pos.collateralSize / 2;
        require(vars.collWithdrawAmt > 0, "Invalid collateral withdraw amount");

        vars.amtAWithdraw = pos.amtA * vars.collWithdrawAmt / pos.collateralSize;
        vars.reserveAAfter = reserveA - vars.amtAWithdraw;
        vars.Sa = vars.reserveAAfter * vars.Ka / someLargeNumber;
        if (vars.amtAWithdraw > vars.Sa) {
            vars.amtARepay = vars.amtAWithdraw - vars.Sa;
        } else {
            vars.amtARepay = 0;
            vars.collWithdrawAmt = (reserveA * pos.collateralSize * vars.Ka)
                / ((someLargeNumber + vars.Ka) * pos.amtA);
        }
        vars.amtBWithdraw = (pos.amtB * vars.collWithdrawAmt) / pos.collateralSize;
        vars.reserveBAfter = reserveB - vars.amtBWithdraw;
        vars.Sb = vars.reserveBAfter * vars.Kb / someLargeNumber;
        vars.amtBRepay = vars.amtBWithdraw + vars.Sb;

        vars.collWithdrawErr = (leverageLevel * reserveA * pos.collateralSize) / (2 * someLargeNumber * pos.amtA) + 1;
        vars.amtARepayErr = vars.reserveAAfter / someLargeNumber + 1;
        vars.amtBRepayErr = vars.Kb * ((leverageLevel * reserveB * pos.collateralSize + 2 * someLargeNumber * pos.amtB)
            / (2 * someLargeNumber * pos.collateralSize) + 2) / someLargeNumber + 1;
        require(vars.amtBRepay >= vars.amtBRepayErr, "Invalid token B repay amount");

        return (
            vars.collWithdrawAmt + vars.collWithdrawErr,
            vars.amtARepay > vars.amtARepayErr ? vars.amtARepay - vars.amtARepayErr : 0,
            vars.amtBRepay - vars.amtBRepayErr
        );
    }

    /// @dev Calculate the amount of each token to borrow by Homora to reach DN
    /// @dev Assume `pos.debtAmtB < pos.amtB`. Check before calling
    /// @param pos HomoraBank's position info
    /// @param leverageLevel Target leverage
    /// @param reserveA Token A's pool reserve
    /// @param reserveB Token B's pool reserve
    /// @param amtAReward The amount of rewards in token A
    function rebalanceLong(        
        VaultPosition memory pos,
        uint256 leverageLevel,
        uint256 reserveA,
        uint256 reserveB,
        uint256 amtAReward
    )
        public
        view
        returns (
            uint256,
            uint256
        )
    {
        RebalanceHelper memory vars;
        vars.Sb = (pos.amtB - pos.debtAmtB) * reserveB / (reserveB - pos.amtB);
        vars.Sa = (unityMinusFee * (pos.amtB - pos.debtAmtB) * reserveA)
            / (unity * reserveB - feeRate * pos.amtB - unityMinusFee * pos.debtAmtB);
        vars.amtAAfter = leverageLevel * (pos.amtA * (reserveA - vars.Sa) / reserveA
            - pos.debtAmtA + vars.Sa + amtAReward) / 2; // n_af

        uint256 debtAAfter = (leverageLevel - 2) * vars.amtAAfter / leverageLevel;

        if (debtAAfter > pos.debtAmtA) {
            vars.amtABorrow = debtAAfter - pos.debtAmtA;
            vars.amtBAfter = pos.amtB * reserveA / (reserveA - vars.Sa) * (reserveB + vars.Sb) / reserveB
                * vars.amtAAfter / pos.amtA;
            vars.amtBBorrow = vars.amtBAfter - pos.debtAmtB;
            vars.amtABorrowErr = (leverageLevel - 2) / 2 + 1;
            vars.amtBBorrowErr = (leverageLevel + 2) * reserveB / (2 * reserveA) + 2;
        } else {
            vars.amtABorrow = 0;
            vars.amtBBorrow = vars.Sb * ((unity + unityMinusFee) * reserveB - pos.amtB - unityMinusFee * pos.debtAmtB)
                * (reserveA + amtAReward) / (unity * (reserveB - pos.amtB) * reserveA)
                + amtAReward * reserveB / reserveA;
            vars.amtABorrowErr = 0;
            vars.amtBBorrowErr = 3;
        }

        return (
            vars.amtABorrow + vars.amtABorrowErr,
            vars.amtBBorrow + vars.amtBBorrowErr
        );
    }
}

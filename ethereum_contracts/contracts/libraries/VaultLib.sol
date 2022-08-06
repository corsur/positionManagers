// SPDX-License-Identifier: BUSL-1.1
pragma solidity >=0.8.0 <0.9.0;

import "hardhat/console.sol";
import "../interfaces/IUniswapPair.sol";

library VaultLib {
    uint256 public constant feeRate = 30;
    uint256 public constant unity = 10000;
    uint256 public constant unityMinusFee = 9970;
    uint256 public constant someLargeNumber = 1000000000000000000;

    struct RebalanceHelper {
        uint256 Ka;
        uint256 Kb;
        uint256 Sa;
        uint256 Sb;
        uint256 collWithdrawAmt;
        uint256 amtARepay;
        uint256 amtBRepay;
        uint256 amtBWithdraw;
        uint256 amtAWithdraw;
        uint256 reserveBAfter;
        uint256 reserveAAfter;
        uint256 amtABorrow;
        uint256 amtBBorrow;
        uint256 amtAmin;
        uint256 amtBmin;
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

    /// @notice Evalute the current collateral's amount in terms of 2 tokens
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
        // feeRate = 0.3%
        RebalanceHelper memory vars;
        // Ka << 1, multiply by someLargeNumber 1e18
        vars.Ka = (pos.debtAmtB - pos.amtB) * unity * someLargeNumber /
            ((reserveB - pos.debtAmtB) * unityMinusFee);
        vars.collWithdrawAmt = leverageLevel * (pos.debtAmtA * someLargeNumber + vars.Ka * reserveA) /
            (2 * (someLargeNumber + vars.Ka)) * pos.collateralSize / pos.amtA -
            (leverageLevel - 2) * pos.collateralSize / 2;
        vars.amtAWithdraw = pos.amtA * vars.collWithdrawAmt / pos.collateralSize;
        vars.reserveAAfter = reserveA - vars.amtAWithdraw;
        vars.Sa = vars.reserveAAfter * vars.Ka / someLargeNumber;
        if (vars.amtAWithdraw > vars.Sa) {
            vars.amtARepay = vars.amtAWithdraw - vars.Sa;
        } else {
            vars.amtARepay = 0;
            vars.collWithdrawAmt = (reserveA * pos.collateralSize * vars.Ka) /
                ((someLargeNumber + vars.Ka) * pos.amtA);
        }
        vars.amtBWithdraw = (pos.amtB * vars.collWithdrawAmt) / pos.collateralSize;
        vars.reserveBAfter = reserveB - vars.amtBWithdraw;
        vars.Sb = vars.reserveBAfter * (pos.debtAmtB - pos.amtB) / (reserveB - pos.amtB);
        vars.amtBRepay = vars.amtBWithdraw + vars.Sb;

        vars.collWithdrawErr = (leverageLevel * reserveA * pos.collateralSize) / (2 * someLargeNumber * pos.amtA) + 1;
        vars.amtARepayErr = vars.reserveAAfter / someLargeNumber + 1;
        vars.amtARepay = vars.amtARepay > vars.amtARepayErr ? vars.amtARepay - vars.amtARepayErr : 0;

        return (
            vars.collWithdrawAmt + vars.collWithdrawErr,
            vars.amtARepay,
            vars.amtBRepay
        );
    }

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
            uint256,
            uint256,
            uint256
        )
    {
        // feeRate = 0.3%
        RebalanceHelper memory vars;
        vars.Sb = (pos.amtB - pos.debtAmtB) * reserveB / (reserveB - pos.amtB);
        vars.Sa = (unityMinusFee * (pos.amtB - pos.debtAmtB) * reserveA) /
            (unity * reserveB - feeRate * pos.amtB - unityMinusFee * pos.debtAmtB);
        vars.amtAAfter = leverageLevel * (pos.amtA * (reserveA - vars.Sa) / reserveA - pos.debtAmtA + vars.Sa + amtAReward) / 2; // n_af
        vars.amtBAfter = pos.amtB * reserveA / (reserveA - vars.Sa) * (reserveB + vars.Sb) / reserveB * vars.amtAAfter / pos.amtA;

        uint256 debtAAfter = (leverageLevel - 2) * vars.amtAAfter / leverageLevel;
        if (debtAAfter > pos.debtAmtA) {
            vars.amtABorrow = debtAAfter - pos.debtAmtA;
            vars.amtBBorrow = vars.amtBAfter - pos.debtAmtB;
        } else {
            vars.amtABorrow = 0;
            vars.amtBBorrow = (sqrt **2 - (2 * unity - feeRate) **2 * reserveB **2) * (reserveA + amtAReward) /
                (4 * unity * unityMinusFee * reserveA * reserveB) + amtAReward * reserveB / reserveA;
        }
        vars.amtAmin = vars.amtABorrow + vars.Sa + amtAReward;
        vars.amtBmin = vars.amtBBorrow - vars.Sb;

        return (
            vars.amtABorrow + 10,
            vars.amtBBorrow + 10,
            vars.amtAmin - 20,
            vars.amtBmin - 20
        );
    }
}

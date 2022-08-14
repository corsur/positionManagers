// SPDX-License-Identifier: BUSL-1.1
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/math/Math.sol";
import "../interfaces/IHomoraAvaxRouter.sol";
import "../interfaces/IHomoraBank.sol";
import "../interfaces/IHomoraAdapter.sol";
import "../interfaces/IUniswapPair.sol";
import "../libraries/HomoraAdapterLib.sol";

/// @custom:oz-upgrades-unsafe-allow external-library-linking
library VaultLib {
    using SafeERC20 for IERC20;
    using Math for uint256;
    using HomoraAdapterLib for IHomoraAdapter;

    // --- constants ---
    bytes private constant HARVEST_DATA =
        abi.encodeWithSignature("harvestWMasterChef()");
    bytes4 public constant ADD_LIQUIDITY_SIG =
        bytes4(
            keccak256(
                "addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)"
            )
        );
    bytes4 public constant REMOVE_LIQUIDITY_SIG =
        bytes4(
            keccak256(
                "removeLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256))"
            )
        );

    uint256 public constant _NO_ID = 0;
    uint256 public constant feeRate = 30; // feeRate = 0.3%
    uint256 public constant unity = 10000;
    uint256 public constant unityMinusFee = 9970;
    uint256 public constant someLargeNumber = 10**18;
    uint256 public constant MAX_UINT = 2**256 - 1;

    struct ShortHelper {
        uint256 Ka;
        uint256 Kb;
        uint256 Sa;
        uint256 Sb;
        uint256 collWithdrawAmt;
        uint256 amtARepay;
        uint256 amtBRepay;
        uint256 amtAWithdraw;
        uint256 amtBWithdraw;
        uint256 reserveABefore;
        uint256 reserveBBefore;
        uint256 reserveAAfter;
        uint256 reserveBAfter;
        uint256 collWithdrawErr;
        uint256 amtARepayErr;
        uint256 amtBRepayErr;
    }

    struct LongHelper {
        uint256 Sa;
        uint256 Sb;
        uint256 reserveABefore;
        uint256 reserveBBefore;
        uint256 amtABorrow;
        uint256 amtBBorrow;
        uint256 amtAAfter;
        uint256 amtBAfter;
        uint256 debtAAfter;
        uint256 amtAReward;
        uint256 amtABorrowErr;
        uint256 amtBBorrowErr;
    }

    // Addresses in the pair
    struct PairInfo {
        address stableToken; // token 0
        address assetToken; // token 1
        address lpToken; // ERC-20 LP token address
        address rewardToken;
    }

    // Amounts of tokens in the Homora farming position
    struct VaultPosition {
        uint256 collateralSize; // amount of collateral/LP
        uint256 amtA; // amount of token A in the LP
        uint256 amtB; // amount of token B in the LP
        uint256 debtAmtA; // amount of token A borrowed
        uint256 debtAmtB; // amount of token B borrowed
    }

    /// @dev Calculate the debt ratio as a function of leverage for a delta-neutral position
    function calculateDebtRatio(
        uint256 leverage,
        uint256 collateralFactor,
        uint256 stableBorrowFactor,
        uint256 assetBorrowFactor
    ) public pure returns (uint256) {
        return
            (stableBorrowFactor.mulDiv(leverage - 2, leverage) +
                assetBorrowFactor).mulDiv(10000, 2 * collateralFactor);
    }

    /// @dev Calculate the threshold for delta as a function of leverage and width of debt ratio
    function calculateDeltaThreshold(
        uint256 leverage,
        uint256 debtRatioWidth,
        uint256 collateralFactor,
        uint256 stableBorrowFactor,
        uint256 assetBorrowFactor
    ) public pure returns (uint256) {
        return
            (debtRatioWidth * leverage).mulDiv(
                leverage * collateralFactor,
                leverage *
                    assetBorrowFactor -
                    (leverage - 2) *
                    stableBorrowFactor
            );
    }

    /// @dev Calculate the params passed to Homora to create PDN position
    /// @param pairInfo: Addresses in the pair
    /// @param Ua: The amount of stable token supplied by user
    /// @param Ub: The amount of asset token supplied by user
    /// @param L: Leverage
    function deltaNeutral(
        PairInfo storage pairInfo,
        uint256 Ua,
        uint256 Ub,
        uint256 L
    )
        public
        view
        returns (
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        )
    {
        // Na: Stable token pool reserve
        // Nb: Asset token pool reserve
        (uint256 Na, uint256 Nb) = getReserves(pairInfo);
        uint256 b = 2 *
            Nb +
            2 *
            Ub -
            (L * Ub).mulDiv(unityMinusFee, unity) -
            (L * unityMinusFee * (Na + Ua)).mulDiv(Ub, unity * Na).mulDiv(
                Ub,
                Nb
            ) -
            (L * (unity * Nb + (unity + unityMinusFee) * Ub)).mulDiv(
                Ua,
                unity * Na
            );
        uint256 c = (L * (Nb + Ub)).mulDiv(Nb + Ub, Nb) *
            ((unityMinusFee * (Na + Ua)).mulDiv(Ub, unity * Na) +
                Nb.mulDiv(Ua, Na));
        uint256 squareRoot = Math.sqrt(b * b + 8 * c);
        require(squareRoot > b, "No positive root");
        assetTokenBorrowAmount = (squareRoot - b) / 4;
        stableTokenBorrowAmount =
            ((L - 2) * assetTokenBorrowAmount).mulDiv(
                Na + Ua,
                L * (Nb + Ub) + 2 * assetTokenBorrowAmount
            ) +
            1;
    }

    /// @dev Harvest farming rewards
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param spell: Homora's Spell contract address
    function harvest(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        address spell,
        PairInfo storage pairInfo
    ) external {
        adapter.homoraExecute(
            address(homoraBank),
            homoraBankPosId,
            spell,
            HARVEST_DATA,
            pairInfo,
            0
        );
    }

    function reinvestExec(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        address spell,
        PairInfo storage pairInfo,
        uint256 stableTokenBalance,
        uint256 assetTokenBalance,
        uint256 stableTokenBorrowAmount,
        uint256 assetTokenBorrowAmount,
        uint256 pid
    ) internal {
        // Encode the calling function.
        adapter.homoraExecute(
            address(homoraBank),
            homoraBankPosId,
            spell,
            abi.encodeWithSelector(
                ADD_LIQUIDITY_SIG,
                pairInfo.stableToken,
                pairInfo.assetToken,
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
            ),
            pairInfo,
            0
        );

        // Cancel HomoraBank's allowance.
        adapter.fundAdapterAndApproveHomoraBank(
            address(homoraBank),
            pairInfo.stableToken,
            0
        );
        adapter.fundAdapterAndApproveHomoraBank(
            address(homoraBank),
            pairInfo.assetToken,
            assetTokenBalance
        );
    }

    /// @dev Collect reward tokens and reinvest
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param spell: Homora's Spell contract address
    /// @param pairInfo: Addresses in the pair
    function reinvest(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        address spell,
        PairInfo storage pairInfo,
        uint256 leverageLevel,
        uint256 pid
    ) external {
        uint256 stableTokenBalance = IERC20(pairInfo.stableToken).balanceOf(
            address(this)
        );
        uint256 assetTokenBalance = IERC20(pairInfo.assetToken).balanceOf(
            address(this)
        );

        // Skip reinvest if no balance available.
        if (stableTokenBalance + assetTokenBalance == 0) {
            return;
        }

        (
            uint256 stableTokenBorrowAmount,
            uint256 assetTokenBorrowAmount
        ) = deltaNeutral(
                pairInfo,
                stableTokenBalance,
                assetTokenBalance,
                leverageLevel
            );

        // Approve HomoraBank transferring tokens.
        if (stableTokenBalance > 0) {
            adapter.fundAdapterAndApproveHomoraBank(
                address(homoraBank),
                pairInfo.stableToken,
                stableTokenBalance
            );
        }
        if (assetTokenBalance > 0) {
            adapter.fundAdapterAndApproveHomoraBank(
                address(homoraBank),
                pairInfo.assetToken,
                assetTokenBalance
            );
        }
        reinvestExec(
            homoraBank,
            adapter,
            homoraBankPosId,
            spell,
            pairInfo,
            stableTokenBalance,
            assetTokenBalance,
            stableTokenBorrowAmount,
            assetTokenBorrowAmount,
            pid
        );
    }

    /// @dev Rebalance Homora Bank's farming position assuming delta is short
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param pos: Farming position in Homora Bank
    /// @param spell: Homora's Spell contract address
    /// @param pairInfo: Addresses in the pair
    function rebalanceShort(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        VaultPosition memory pos,
        address spell,
        PairInfo storage pairInfo,
        uint256 leverageLevel
    ) external returns (uint256, uint256) {
        ShortHelper memory vars;
        (vars.reserveABefore, vars.reserveBBefore) = getReserves(pairInfo);
        (
            vars.collWithdrawAmt,
            vars.amtARepay,
            vars.amtBRepay,
            vars.Sa,
            vars.Sb
        ) = _rebalanceShort(pos, leverageLevel, vars);

        adapter.homoraExecute(
            address(homoraBank),
            homoraBankPosId,
            spell,
            abi.encodeWithSelector(
                REMOVE_LIQUIDITY_SIG,
                pairInfo.stableToken,
                pairInfo.assetToken,
                [
                    vars.collWithdrawAmt,
                    0,
                    vars.amtARepay,
                    vars.amtBRepay,
                    0,
                    0,
                    0
                ]
            ),
            pairInfo,
            0
        );

        return (vars.Sa, vars.Sb);
    }

    function populateLongHelper(
        VaultPosition memory pos,
        PairInfo storage pairInfo,
        uint256 leverageLevel
    ) internal view returns (LongHelper memory vars) {
        (vars.reserveABefore, vars.reserveBBefore) = getReserves(pairInfo);
        vars.amtAReward = IERC20(pairInfo.stableToken).balanceOf(address(this));

        (vars.amtABorrow, vars.amtBBorrow, vars.Sa, vars.Sb) = _rebalanceLong(
            pos,
            leverageLevel,
            vars
        );
    }

    function rebalanceLongExec(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        address spell,
        PairInfo storage pairInfo,
        uint256 pid,
        LongHelper memory vars
    ) internal {
        adapter.homoraExecute(
            address(homoraBank),
            homoraBankPosId,
            spell,
            abi.encodeWithSelector(
                ADD_LIQUIDITY_SIG,
                pairInfo.stableToken,
                pairInfo.assetToken,
                [
                    vars.amtAReward,
                    0,
                    0,
                    vars.amtABorrow,
                    vars.amtBBorrow,
                    0,
                    0,
                    0
                ],
                pid
            ),
            pairInfo,
            0
        );

        adapter.fundAdapterAndApproveHomoraBank(
            address(homoraBank),
            pairInfo.stableToken,
            0
        );
    }

    /// @dev Rebalance Homora Bank's farming position assuming delta is long
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param pos: Farming position in Homora Bank
    /// @param spell: Homora's Spell contract address
    /// @param pairInfo: Addresses in the pair
    function rebalanceLong(
        IHomoraBank homoraBank,
        IHomoraAdapter adapter,
        uint256 homoraBankPosId,
        VaultPosition memory pos,
        address spell,
        PairInfo storage pairInfo,
        uint256 leverageLevel,
        uint256 pid
    ) external returns (uint256, uint256) {
        LongHelper memory vars = populateLongHelper(
            pos,
            pairInfo,
            leverageLevel
        );

        if (vars.amtAReward > 0) {
            adapter.fundAdapterAndApproveHomoraBank(
                address(homoraBank),
                pairInfo.stableToken,
                vars.amtAReward
            );
        }

        rebalanceLongExec(
            homoraBank,
            adapter,
            homoraBankPosId,
            spell,
            pairInfo,
            pid,
            vars
        );

        return (vars.Sa, vars.Sb);
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
        if (diff == 0) {
            return 0;
        } else if (targetVal == 0) {
            return MAX_UINT;
        } else {
            return diff.mulDiv(10000, targetVal);
        }
    }

    /// @notice Get the amount of each of the two tokens in the pool. Stable token first
    /// @param pairInfo: Addresses in the pair
    function getReserves(PairInfo storage pairInfo)
        public
        view
        returns (uint256 reserve0, uint256 reserve1)
    {
        IUniswapPair pair = IUniswapPair(pairInfo.lpToken);
        if (pair.token0() == pairInfo.stableToken) {
            (reserve0, reserve1, ) = pair.getReserves();
        } else {
            (reserve1, reserve0, ) = pair.getReserves();
        }
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens. Stable token first
    /// @param pairInfo: Addresses in the pair
    /// @param collAmount: Amount of LP token
    function convertCollateralToTokens(
        PairInfo storage pairInfo,
        uint256 collAmount
    ) public view returns (uint256 amount0, uint256 amount1) {
        if (collAmount == 0) {
            amount0 = 0;
            amount1 = 0;
        } else {
            uint256 totalLPSupply = IERC20(pairInfo.lpToken).totalSupply();
            require(totalLPSupply > 0, "Invalid LP supply");
            (uint256 reserve0, uint256 reserve1) = getReserves(pairInfo);
            amount0 = reserve0.mulDiv(collAmount, totalLPSupply);
            amount1 = reserve1.mulDiv(collAmount, totalLPSupply);
        }
    }

    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    function getCollateralSize(IHomoraBank homoraBank, uint256 homoraBankPosId)
        public
        view
        returns (uint256)
    {
        if (homoraBankPosId == _NO_ID) return 0;
        (, , , uint256 collateralSize) = homoraBank.getPositionInfo(
            homoraBankPosId
        );
        return collateralSize;
    }

    /// @dev Query the current debt amount for both tokens. Stable first
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    function getDebtAmounts(
        IHomoraBank homoraBank,
        uint256 homoraBankPosId,
        PairInfo storage pairInfo
    ) public view returns (uint256, uint256) {
        if (homoraBankPosId == _NO_ID) {
            return (0, 0);
        } else {
            uint256 stableTokenDebtAmount;
            uint256 assetTokenDebtAmount;
            (address[] memory tokens, uint256[] memory debts) = homoraBank
                .getPositionDebts(homoraBankPosId);
            for (uint256 i = 0; i < tokens.length; i++) {
                if (tokens[i] == pairInfo.stableToken) {
                    stableTokenDebtAmount = debts[i];
                } else if (tokens[i] == pairInfo.assetToken) {
                    assetTokenDebtAmount = debts[i];
                }
            }
            return (stableTokenDebtAmount, assetTokenDebtAmount);
        }
    }

    /// @dev Homora position info
    /// @param homoraBank: Instantiated HomoraBank Interface
    /// @param homoraBankPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    function getPositionInfo(
        IHomoraBank homoraBank,
        uint256 homoraBankPosId,
        PairInfo storage pairInfo
    ) external view returns (VaultPosition memory pos) {
        pos.collateralSize = getCollateralSize(homoraBank, homoraBankPosId);
        (pos.amtA, pos.amtB) = convertCollateralToTokens(
            pairInfo,
            pos.collateralSize
        );
        (pos.debtAmtA, pos.debtAmtB) = getDebtAmounts(
            homoraBank,
            homoraBankPosId,
            pairInfo
        );
    }

    /// @dev Calculate the amount of collateral to withdraw and the amount of each token to repay by Homora to reach DN
    /// @dev Assume `pos.debtAmtB > pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param leverageLevel: Target leverage
    /// @param vars: Helper struct
    function _rebalanceShort(
        VaultPosition memory pos,
        uint256 leverageLevel,
        ShortHelper memory vars
    )
        public
        pure
        returns (
            uint256,
            uint256,
            uint256,
            uint256,
            uint256
        )
    {
        // Ka << 1, multiply by someLargeNumber 1e18
        vars.Ka = someLargeNumber.mulDiv(
            unity * (pos.debtAmtB - pos.amtB),
            unityMinusFee * (vars.reserveBBefore - pos.debtAmtB)
        );
        vars.Kb = someLargeNumber.mulDiv(
            pos.debtAmtB - pos.amtB,
            vars.reserveBBefore - pos.amtB
        );
        vars.collWithdrawAmt =
            pos.collateralSize.mulDiv(
                leverageLevel *
                    (pos.debtAmtA *
                        someLargeNumber +
                        vars.Ka *
                        vars.reserveABefore),
                2 * (someLargeNumber + vars.Ka) * pos.amtA
            ) -
            pos.collateralSize.mulDiv(leverageLevel - 2, 2);
        require(vars.collWithdrawAmt > 0, "Invalid collateral withdraw amount");

        vars.amtAWithdraw = pos.amtA.mulDiv(
            vars.collWithdrawAmt,
            pos.collateralSize
        );
        vars.reserveAAfter = vars.reserveABefore - vars.amtAWithdraw;
        vars.Sa = vars.reserveAAfter.mulDiv(vars.Ka, someLargeNumber);
        if (vars.amtAWithdraw > vars.Sa) {
            vars.amtARepay = vars.amtAWithdraw - vars.Sa;
        } else {
            vars.amtARepay = 0;
            vars.collWithdrawAmt = pos.collateralSize.mulDiv(
                vars.Ka * vars.reserveABefore,
                (someLargeNumber + vars.Ka) * pos.amtA
            );
        }
        vars.amtBWithdraw = pos.amtB.mulDiv(
            vars.collWithdrawAmt,
            pos.collateralSize
        );
        vars.reserveBAfter = vars.reserveBBefore - vars.amtBWithdraw;
        vars.Sb = vars.reserveBAfter.mulDiv(vars.Kb, someLargeNumber);
        vars.amtBRepay = vars.amtBWithdraw + vars.Sb;

        vars.collWithdrawErr = (leverageLevel * vars.reserveABefore).mulDiv(
            pos.collateralSize,
            2 * someLargeNumber * pos.amtA,
            Math.Rounding.Up
        );
        vars.amtARepayErr = vars.reserveAAfter.ceilDiv(someLargeNumber);
        vars.amtBRepayErr =
            vars
                .Kb
                .mulDiv(
                    leverageLevel *
                        vars.reserveBBefore *
                        pos.collateralSize +
                        2 *
                        someLargeNumber *
                        pos.amtB,
                    2 * someLargeNumber * pos.collateralSize
                )
                .ceilDiv(someLargeNumber) +
            vars.Kb.ceilDiv(someLargeNumber);
        require(
            vars.amtBRepay >= vars.amtBRepayErr,
            "Invalid token B repay amount"
        );

        return (
            vars.collWithdrawAmt + vars.collWithdrawErr,
            vars.amtARepay > vars.amtARepayErr
                ? vars.amtARepay - vars.amtARepayErr
                : 0,
            vars.amtBRepay - vars.amtBRepayErr,
            vars.Sa,
            vars.Sb
        );
    }

    /// @dev Calculate the amount of each token to borrow by Homora to reach DN
    /// @dev Assume `pos.debtAmtB < pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param leverageLevel: Target leverage
    /// @param vars: Helper struct
    function _rebalanceLong(
        VaultPosition memory pos,
        //        uint256[4] memory pos, // amtA, amtB, debtA, debtB
        uint256 leverageLevel,
        LongHelper memory vars
    )
        public
        pure
        returns (
            uint256,
            uint256,
            uint256,
            uint256
        )
    {
        vars.Sb = vars.reserveBBefore.mulDiv(
            pos.amtB - pos.debtAmtB,
            vars.reserveBBefore - pos.amtB
        );
        vars.Sa = vars.reserveABefore.mulDiv(
            unityMinusFee * (pos.amtB - pos.debtAmtB),
            unity *
                vars.reserveBBefore -
                feeRate *
                pos.amtB -
                unityMinusFee *
                pos.debtAmtB
        );
        vars.amtAAfter = leverageLevel.mulDiv(
            pos.amtA.mulDiv(
                vars.reserveABefore - vars.Sa,
                vars.reserveABefore
            ) -
                pos.debtAmtA +
                vars.Sa +
                vars.amtAReward,
            2
        ); // n_af

        vars.debtAAfter = vars.amtAAfter.mulDiv(
            leverageLevel - 2,
            leverageLevel
        );

        if (vars.debtAAfter > pos.debtAmtA) {
            vars.amtABorrow = vars.debtAAfter - pos.debtAmtA;
            vars.amtBAfter = pos
                .amtB
                .mulDiv(vars.reserveABefore, vars.reserveABefore - vars.Sa)
                .mulDiv(vars.reserveBBefore + vars.Sb, vars.reserveBBefore)
                .mulDiv(vars.amtAAfter, pos.amtA);
            vars.amtBBorrow = vars.amtBAfter - pos.debtAmtB;
            vars.amtABorrowErr = (leverageLevel - 2).ceilDiv(2);
            vars.amtBBorrowErr =
                (leverageLevel + 2).mulDiv(
                    vars.reserveBBefore,
                    2 * vars.reserveABefore,
                    Math.Rounding.Up
                ) +
                1;
        } else {
            vars.amtABorrow = 0;
            vars.amtBBorrow =
                vars
                    .Sb
                    .mulDiv(
                        (unity + unityMinusFee) *
                            vars.reserveBBefore -
                            pos.amtB -
                            unityMinusFee *
                            pos.debtAmtB,
                        unity * (vars.reserveBBefore - pos.amtB)
                    )
                    .mulDiv(
                        vars.reserveABefore + vars.amtAReward,
                        vars.reserveABefore
                    ) +
                vars.amtAReward.mulDiv(
                    vars.reserveBBefore,
                    vars.reserveABefore
                );
            vars.amtABorrowErr = 0;
            vars.amtBBorrowErr = 3;
        }

        return (
            vars.amtABorrow + vars.amtABorrowErr,
            vars.amtBBorrow + vars.amtBBorrowErr,
            vars.Sa,
            vars.Sb
        );
    }

    /// @notice Swap fromToken into toToken
    function swap(
        address router,
        uint256 amount,
        address fromToken,
        address toToken
    )
        public
        returns (
            //        internal
            uint256
        )
    {
        IHomoraAvaxRouter _router = IHomoraAvaxRouter(router);
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        uint256[] memory amounts = _router.getAmountsOut(amount, path);
        IERC20(fromToken).approve(router, amount);
        if (amounts[1] > 0) {
            amounts = _router.swapExactTokensForTokens(
                amount,
                0,
                path,
                address(this),
                block.timestamp
            );
        }
        IERC20(fromToken).approve(router, 0);
        return amounts[1];
    }

    /// @notice Swap native AVAX into toToken
    function swapAVAX(
        address router,
        uint256 amount,
        address toToken
    )
        public
        returns (
            //        internal
            uint256
        )
    {
        IHomoraAvaxRouter _router = IHomoraAvaxRouter(router);
        address fromToken = _router.WAVAX();
        address[] memory path = new address[](2);
        (path[0], path[1]) = (fromToken, toToken);
        uint256[] memory amounts = _router.getAmountsOut(amount, path);
        // Reverted by TraderJoe if amounts[1] == 0
        if (amounts[1] > 0) {
            amounts = _router.swapExactAVAXForTokens{value: amount}(
                0,
                path,
                address(this),
                block.timestamp
            );
        }
        return amounts[1];
    }

    /// @notice Swap reward tokens into stable tokens and collect harvest fee
    function swapRewardCollectFee(
        address router,
        address feeCollector,
        PairInfo storage pairInfo,
        uint256 harvestFee
    ) external {
        uint256 rewardAmt = IERC20(pairInfo.rewardToken).balanceOf(
            address(this)
        );
        if (rewardAmt > 0) {
            uint256 stableRecv = swap(
                router,
                rewardAmt,
                pairInfo.rewardToken,
                pairInfo.stableToken
            );
            uint256 harvestFeeAmt = stableRecv.mulDiv(harvestFee, 10000);
            if (harvestFeeAmt > 0) {
                IERC20(pairInfo.stableToken).safeTransfer(
                    feeCollector,
                    harvestFeeAmt
                );
            }
        }
    }
}

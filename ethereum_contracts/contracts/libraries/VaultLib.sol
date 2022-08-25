// SPDX-License-Identifier: BUSL-1.1
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/math/Math.sol";

import "../interfaces/IApertureCommon.sol";
import "../interfaces/IHomoraBank.sol";
import "../interfaces/IHomoraAdapter.sol";
import "../interfaces/IHomoraOracle.sol";
import "../interfaces/IJoeFactory.sol";
import "../interfaces/IJoePair.sol";
import "../interfaces/IJoeRouter01.sol";

import "../libraries/HomoraAdapterLib.sol";

// Addresses in the pair
struct PairInfo {
    address stableToken; // token 0
    address assetToken; // token 1
    address lpToken; // ERC-20 LP token address
    address rewardToken;
}

// Contract address info
struct ContractInfo {
    address adapter; // Aperture's adapter to interact with Homora
    address bank; // HomoraBank's address
    address oracle; // Homora's Oracle address
    address router; // TraderJoe's router address
    address spell; // Homora's Spell address
}

// User's Aperture position
struct Position {
    uint256 shareAmount;
}

struct VaultState {
    uint256 totalShareAmount;
    uint256 lastCollectionTimestamp; // last timestamp when collecting management fee
}

// Amounts of tokens in the Homora farming position
struct VaultPosition {
    uint256 collateralSize; // amount of collateral/LP
    uint256 amtA; // amount of token A in the LP
    uint256 amtB; // amount of token B in the LP
    uint256 debtA; // amount of token A borrowed
    uint256 debtB; // amount of token B borrowed
}

struct RemoveHelper {
    uint256 L; // leverage * 10000
    uint256 Ka; // Sa = Ka * reserveAAfter
    uint256 Sa; // token A swap amount
    uint256 Sb; // token B receive amount
    uint256 collWithdrawAmt; // amount of LP to remove
    uint256 amtARepay; // token A repay amount, amtARepay = amtAWithdraw - Sa
    uint256 amtBRepay; // token B repay amount, amtBRepay = amtBWithdraw + Sb
    uint256 amtAWithdraw; // token A removed from LP
    uint256 amtBWithdraw; // token B removed from LP
    uint256 reserveA; // A's pool reserve before LP removal
    uint256 reserveB; // B's pool reserve before LP removal
}

struct AddHelper {
    uint256 L; // leverage * 10000
    uint256 Sa; // token A receive amount
    uint256 Sb; // token B swap amount
    uint256 reserveA; // A's pool reserve before swapping
    uint256 reserveB; // B's pool reserve before swapping
    uint256 amtABorrow; // token A borrow amount
    uint256 amtBBorrow; // token B borrow amount
    uint256 amtAAfter; // amount of token A in the LP after rebalance
    uint256 amtBAfter; // amount of token B in the LP after rebalance
    uint256 debtAAfter; // amount of debt in token A after rebalance
    uint256 amtASupply; // amount of rewards swapped to token A
}

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

    Math.Rounding public constant UP = Math.Rounding.Up;
    Math.Rounding public constant DOWN = Math.Rounding.Down;
    uint256 public constant _NO_ID = 0;
    uint256 public constant FEE_RATE = 30; // feeRate = 0.3%
    uint256 public constant UNITY = 10000;
    uint256 public constant UNITY_MINUS_FEE = 9970;
    uint256 public constant TWINS = 20000;
    uint256 public constant SOME_LARGE_NUMBER = 2**112;
    uint256 public constant MAX_UINT = 2**256 - 1;

    error Slippage_Too_Large();

    ///********* Helper functions *********///
    function abs(uint256 x, uint256 y) public pure returns (uint256) {
        return x > y ? x - y : y - x;
    }

    /// @notice Calculate offset ratio, multiplied by 1e4
    function getOffset(uint256 currentVal, uint256 targetVal)
        internal
        pure
        returns (uint256)
    {
        uint256 diff = abs(currentVal, targetVal);
        if (diff == 0) {
            return 0;
        } else if (targetVal == 0) {
            return MAX_UINT;
        } else {
            return diff.mulDiv(UNITY, targetVal);
        }
    }

    /// @notice Get the amount of each of the two tokens in the pool. Stable token first
    /// @param lpToken: LP token address
    /// @param stableToken: Stable token address
    function getReserves(address lpToken, address stableToken)
        internal
        view
        returns (uint256 reserve0, uint256 reserve1)
    {
        IJoePair pair = IJoePair(lpToken);
        if (pair.token0() == stableToken) {
            (reserve0, reserve1, ) = pair.getReserves();
        } else {
            (reserve1, reserve0, ) = pair.getReserves();
        }
    }

    // If fee is on, TraderJoe mints liquidity equivalent to 1/6th of the growth in sqrt(k), which changes `totalSupply`.
    function updateLiquidity(
        address lpToken,
        uint256 reserve0,
        uint256 reserve1,
        uint256 totalSupply
    ) internal view returns (uint256) {
        IJoePair pair = IJoePair(lpToken);
        address feeTo = IJoeFactory(pair.factory()).feeTo();
        uint256 rootKLast = Math.sqrt(pair.kLast());
        if (feeTo != address(0)) {
            if (rootKLast > 0) {
                uint256 rootK = Math.sqrt(reserve0 * reserve1);
                if (rootK > rootKLast) {
                    totalSupply = totalSupply.mulDiv(
                        6 * rootK,
                        rootK * 5 + rootKLast
                    );
                }
            }
        }
        return totalSupply;
    }

    ///********* Homora Bank related functions *********///

    /// @dev Query the amount of collateral/LP in the Homora PDN position
    /// @param homoraBank: HomoraBank's address
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    function getCollateralSize(address homoraBank, uint256 homoraPosId)
        public
        view
        returns (uint256)
    {
        if (homoraPosId == _NO_ID) return 0;
        (, , , uint256 collateralSize) = IHomoraBank(homoraBank)
            .getPositionInfo(homoraPosId);
        return collateralSize;
    }

    /// @notice Evalute the current collateral's amount in terms of 2 tokens. Stable token first
    /// @param collAmount: Amount of LP token
    /// @param lpToken: LP token address
    /// @param stableToken: Stable token address
    function convertCollateralToTokens(
        uint256 collAmount,
        address lpToken,
        address stableToken
    ) public view returns (uint256 amount0, uint256 amount1) {
        if (collAmount == 0) {
            amount0 = 0;
            amount1 = 0;
        } else {
            (uint256 reserve0, uint256 reserve1) = getReserves(
                lpToken,
                stableToken
            );
            // LP may be minted for fees.
            uint256 totalLPSupply = updateLiquidity(
                lpToken,
                reserve0,
                reserve1,
                IERC20(lpToken).totalSupply()
            );
            require(totalLPSupply > 0);
            amount0 = reserve0.mulDiv(collAmount, totalLPSupply);
            amount1 = reserve1.mulDiv(collAmount, totalLPSupply);
        }
    }

    /// @dev Query the current debt amount for both tokens. Stable first
    /// @param homoraBank: HomoraBank's address
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param stableToken: Stable token address
    /// @param assetToken: Asset token address
    function getDebtAmounts(
        address homoraBank,
        uint256 homoraPosId,
        address stableToken,
        address assetToken
    ) public view returns (uint256, uint256) {
        if (homoraPosId == _NO_ID) {
            return (0, 0);
        } else {
            uint256 stableTokenDebtAmount;
            uint256 assetTokenDebtAmount;
            (address[] memory tokens, uint256[] memory debts) = IHomoraBank(
                homoraBank
            ).getPositionDebts(homoraPosId);
            for (uint256 i = 0; i < tokens.length; i++) {
                if (tokens[i] == stableToken) {
                    stableTokenDebtAmount = debts[i];
                } else if (tokens[i] == assetToken) {
                    assetTokenDebtAmount = debts[i];
                }
            }
            return (stableTokenDebtAmount, assetTokenDebtAmount);
        }
    }

    /// @dev Homora position info
    /// @param homoraBank: HomoraBank's address
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    function getPositionInfo(
        address homoraBank,
        uint256 homoraPosId,
        PairInfo storage pairInfo
    ) public view returns (VaultPosition memory pos) {
        pos.collateralSize = getCollateralSize(homoraBank, homoraPosId);
        (pos.amtA, pos.amtB) = convertCollateralToTokens(
            pos.collateralSize,
            pairInfo.lpToken,
            pairInfo.stableToken
        );
        (pos.debtA, pos.debtB) = getDebtAmounts(
            homoraBank,
            homoraPosId,
            pairInfo.stableToken,
            pairInfo.assetToken
        );
    }

    /// @notice Calculate the debt ratio and return the ratio, multiplied by 1e4
    function getDebtRatio(address homoraBank, uint256 homoraPosId)
        public
        view
        returns (uint256)
    {
        return
            homoraPosId == _NO_ID
                ? 0
                : UNITY.mulDiv(
                    IHomoraBank(homoraBank).getBorrowETHValue(homoraPosId),
                    IHomoraBank(homoraBank).getCollateralETHValue(homoraPosId)
                );
    }

    /// @dev Total position value, not weighted by the collateral factor
    function getCollateralETHValue(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo
    ) external view returns (uint256) {
        uint256 collateralSize = getCollateralSize(
            contractInfo.bank,
            homoraPosId
        );
        return
            getTokenETHValue(
                contractInfo.oracle,
                pairInfo.lpToken,
                collateralSize
            );
    }

    /// @dev Total debt value, *not* weighted by the borrow factors
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    function getBorrowETHValue(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo
    ) external view returns (uint256) {
        (
            uint256 stableTokenDebtAmount,
            uint256 assetTokenDebtAmount
        ) = getDebtAmounts(
                contractInfo.bank,
                homoraPosId,
                pairInfo.stableToken,
                pairInfo.assetToken
            );
        return
            (homoraPosId == _NO_ID)
                ? 0
                : getTokenETHValue(
                    contractInfo.oracle,
                    pairInfo.stableToken,
                    stableTokenDebtAmount
                ) +
                    getTokenETHValue(
                        contractInfo.oracle,
                        pairInfo.assetToken,
                        assetTokenDebtAmount
                    );
    }

    ///********* Oracle related functions *********///

    function support(address oracle, address token)
        external
        view
        returns (bool)
    {
        return IHomoraOracle(oracle).support(token);
    }

    function supportLP(address oracle, address lpToken)
        external
        view
        returns (bool)
    {
        (, , uint16 liqIncentive) = IHomoraOracle(oracle).tokenFactors(lpToken);
        return liqIncentive != 0;
    }

    /// @dev Return the value of the given input as ETH per unit, multiplied by 2**112.
    /// @param token The ERC-20 token to check the value.
    function getETHPx(address oracle, address token)
        public
        view
        returns (uint256)
    {
        return IHomoraOracle(oracle).source().getETHPx(token);
    }

    /// @dev Query the collateral factor of the LP token on Homora, 0.84 => 8400
    function getCollateralFactor(address oracle, address lpToken)
        external
        view
        returns (uint256 collateralFactor)
    {
        (, collateralFactor, ) = IHomoraOracle(oracle).tokenFactors(lpToken);
        require(0 < collateralFactor && collateralFactor < UNITY);
    }

    /// @dev Query the borrow factor of the debt token on Homora, 1.04 => 10400
    /// @param token: Address of the ERC-20 debt token
    function getBorrowFactor(address oracle, address token)
        external
        view
        returns (uint256 borrowFactor)
    {
        (borrowFactor, , ) = IHomoraOracle(oracle).tokenFactors(token);
        require(borrowFactor > UNITY);
    }

    /// @dev Return the value of the given token as ETH, assuming `token` is supported by the oracle
    function getTokenETHValue(
        address oracle,
        address token,
        uint256 amount
    ) public view returns (uint256) {
        return getETHPx(oracle, token).mulDiv(amount, SOME_LARGE_NUMBER);
    }

    ///********* Vault related functions *********///

    /// @dev Calculate the debt ratio as a function of leverage for a delta-neutral position
    function calculateDebtRatio(
        uint256 leverage,
        uint256 collateralFactor,
        uint256 stableBorrowFactor,
        uint256 assetBorrowFactor
    ) external pure returns (uint256) {
        return
            (stableBorrowFactor.mulDiv(leverage - TWINS, leverage) +
                assetBorrowFactor).mulDiv(UNITY, 2 * collateralFactor);
    }

    /// @dev Calculate the threshold for delta as a function of leverage and width of debt ratio
    function calculateDeltaThreshold(
        uint256 leverage,
        uint256 debtRatioWidth,
        uint256 collateralFactor,
        uint256 stableBorrowFactor,
        uint256 assetBorrowFactor
    ) external pure returns (uint256) {
        return
            debtRatioWidth.mulDiv(
                leverage**2 * collateralFactor,
                UNITY *
                    (leverage *
                        assetBorrowFactor -
                        (leverage - TWINS) *
                        stableBorrowFactor)
            );
    }

    /// @dev Check if the farming position is delta neutral
    /// @param homoraBank: HomoraBank's address
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    /// @param deltaThreshold: Delta deviation threshold in percentage * 10000
    function isDeltaNeutral(
        address homoraBank,
        uint256 homoraPosId,
        PairInfo storage pairInfo,
        uint256 deltaThreshold
    ) external view returns (bool) {
        // Assume token A is the stable token
        // Position info in Homora Bank
        VaultPosition memory pos = getPositionInfo(
            homoraBank,
            homoraPosId,
            pairInfo
        );
        return getOffset(pos.amtB, pos.debtB) < deltaThreshold;
    }

    function isDebtRatioHealthy(
        address homoraBank,
        uint256 homoraPosId,
        uint256 minDebtRatio,
        uint256 maxDebtRatio
    ) external view returns (bool) {
        if (homoraPosId == _NO_ID) {
            return true;
        } else {
            uint256 debtRatio = getDebtRatio(homoraBank, homoraPosId);
            return (minDebtRatio < debtRatio) && (debtRatio < maxDebtRatio);
        }
    }

    /// @dev Calculate the params passed to Homora to create PDN position
    /// @param pairInfo: Addresses in the pair
    /// @param Ua: The amount of stable token supplied by user
    /// @param Ub: The amount of asset token supplied by user
    /// @param L: Leverage * 10000
    function deltaNeutralMath(
        PairInfo storage pairInfo,
        address router,
        uint256 Ua,
        uint256 Ub,
        uint256 L
    ) internal view returns (uint256 debtAAmt, uint256 debtBAmt) {
        // Na: Stable token pool reserve
        // Nb: Asset token pool reserve
        (uint256 Na, uint256 Nb) = getReserves(
            pairInfo.lpToken,
            pairInfo.stableToken
        );
        uint256 b = 2 *
            Nb +
            2 *
            Ub -
            Ub.mulDiv(L * UNITY_MINUS_FEE, UNITY**2) -
            (Na + Ua).mulDiv(L * UNITY_MINUS_FEE * Ub**2, UNITY**2 * Na * Nb) -
            (UNITY * Nb + (UNITY + UNITY_MINUS_FEE) * Ub).mulDiv(
                L * Ua,
                UNITY**2 * Na
            );
        uint256 c = L.mulDiv((Nb + Ub)**2, UNITY * Nb) *
            (Ub.mulDiv(UNITY_MINUS_FEE * (Na + Ua), UNITY * Na) +
                Ua.mulDiv(Nb, Na));
        uint256 squareRoot = Math.sqrt(b * b + 8 * c);
        debtBAmt = (squareRoot - b) / 4;
        debtAAmt = ((L - TWINS) * debtBAmt).mulDiv(
            Na + Ua,
            L * (Nb + Ub) + TWINS * debtBAmt
        );
        // Internally Homora's Spell swaps Ub token B to A. It will be reverted by TraderJoe if amtAOut == 0
        if (Ub > 0) {
            if (IJoeRouter01(router).getAmountOut(Ub, Nb, Na) == 0) {
                // Let Homora swaps 1 token A to B.
                debtAAmt += 1;
            }
        } else {
            if (Na > Nb) {
                // 1 B swaps more than 1 A.
                debtBAmt += 1;
            } else {
                // 1 A swaps more than 1 B.
                debtAAmt += 1;
            }
        }
    }

    /// @dev Deposit to HomoraBank in a pseudo delta-neutral way
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    /// @param stableDepositAmount: Amount of stable token supplied by user
    /// @param assetDepositAmount: Amount of asset token supplied by user
    /// @param leverageLevel: Target leverage * 10000
    /// @param pid: Pool id
    /// @param value: native token sent
    function deposit(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo,
        uint256 stableDepositAmount,
        uint256 assetDepositAmount,
        uint256 leverageLevel,
        uint256 pid,
        uint256 value
    ) external returns (uint256) {
        // Skip if no balance available.
        if (stableDepositAmount + assetDepositAmount == 0) {
            return homoraPosId;
        }

        (uint256 stableBorrowAmount, uint256 assetBorrowAmount) = deltaNeutralMath(
            pairInfo,
            contractInfo.router,
            stableDepositAmount,
            assetDepositAmount,
            leverageLevel
        );

        return
            addLiquidity(
                contractInfo,
                homoraPosId,
                pairInfo,
                stableDepositAmount,
                assetDepositAmount,
                stableBorrowAmount,
                assetBorrowAmount,
                pid,
                value
            );
    }

    /// @dev Withdraw from HomoraBank
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    /// @param withdrawShareRatio: Ratio of user shares to withdraw multiplied by 1e18
    function withdraw(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo,
        uint256 withdrawShareRatio
    ) external returns (uint256[3] memory) {
        (uint256 stableDebtAmt, uint256 assetDebtAmt) = getDebtAmounts(
            contractInfo.bank,
            homoraPosId,
            pairInfo.stableToken,
            pairInfo.assetToken
        );

        removeLiquidity(
            contractInfo,
            homoraPosId,
            pairInfo,
            // Calculate collSize to withdraw.
            getCollateralSize(contractInfo.bank, homoraPosId).mulDiv(
                withdrawShareRatio,
                SOME_LARGE_NUMBER
            ),
            // Calculate debt to repay in two tokens.
            stableDebtAmt.mulDiv(withdrawShareRatio, SOME_LARGE_NUMBER),
            assetDebtAmt.mulDiv(withdrawShareRatio, SOME_LARGE_NUMBER)
        );

        // Calculate token disbursement amount.
        return [
            // Stable token withdraw amount
            IERC20(pairInfo.stableToken).balanceOf(address(this)),
            // Asset token withdraw amount
            IERC20(pairInfo.assetToken).balanceOf(address(this)),
            // AVAX withdraw amount
            address(this).balance
        ];
    }

    /// @dev Add liquidity through HomoraBank
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    /// @param stableSupply: Amount of stable token supplied to Homora
    /// @param assetSupply: Amount of asset token supplied to Homora
    /// @param stableBorrow: Amount of stable token borrowed from Homora
    /// @param assetBorrow: Amount of asset token borrowed from Homora
    /// @param pid: Pool id
    /// @param value: native token sent
    function addLiquidity(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo,
        uint256 stableSupply,
        uint256 assetSupply,
        uint256 stableBorrow,
        uint256 assetBorrow,
        uint256 pid,
        uint256 value
    ) internal returns (uint256) {
        IHomoraAdapter adapter = IHomoraAdapter(contractInfo.adapter);

        // Approve HomoraBank transferring tokens.
        if (stableSupply > 0) {
            adapter.fundAdapterAndApproveHomoraBank(
                contractInfo.bank,
                pairInfo.stableToken,
                stableSupply
            );
        }
        if (assetSupply > 0) {
            adapter.fundAdapterAndApproveHomoraBank(
                contractInfo.bank,
                pairInfo.assetToken,
                assetSupply
            );
        }

        // Encode the calling function.
        bytes memory addLiquidityBytes = abi.encodeWithSelector(
            ADD_LIQUIDITY_SIG,
            pairInfo.stableToken,
            pairInfo.assetToken,
            [stableSupply, assetSupply, 0, stableBorrow, assetBorrow, 0, 0, 0],
            pid
        );

        // Call Homora's execute() along with any native token received.
        homoraPosId = abi.decode(
            adapter.homoraExecute(
                contractInfo,
                homoraPosId,
                addLiquidityBytes,
                pairInfo,
                value
            ),
            (uint256)
        );

        // Cancel HomoraBank's allowance.
        adapter.fundAdapterAndApproveHomoraBank(
            contractInfo.bank,
            pairInfo.stableToken,
            0
        );
        adapter.fundAdapterAndApproveHomoraBank(
            contractInfo.bank,
            pairInfo.assetToken,
            0
        );
        return homoraPosId;
    }

    /// @dev Remove liquidity through HomoraBank
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    /// @param collWithdrawAmt: Amount of collateral/LP to withdraw by Homora
    /// @param amtARepay: Amount of stable token repaid to Homora
    /// @param amtBRepay: Amount of asset token repaid to Homora
    function removeLiquidity(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo,
        uint256 collWithdrawAmt,
        uint256 amtARepay,
        uint256 amtBRepay
    ) internal {
        IHomoraAdapter(contractInfo.adapter).homoraExecute(
            contractInfo,
            homoraPosId,
            abi.encodeWithSelector(
                REMOVE_LIQUIDITY_SIG,
                pairInfo.stableToken,
                pairInfo.assetToken,
                [collWithdrawAmt, 0, amtARepay, amtBRepay, 0, 0, 0]
            ),
            pairInfo,
            0
        );
    }

    function collectWithdrawFee(
        mapping(uint16 => mapping(uint128 => Position)) storage positions,
        VaultState storage vaultState,
        PositionInfo memory position_info,
        uint256 withdrawShareAmount,
        uint256 withdrawFee
    ) external returns (uint256 withdrawFeeShare) {
        // Shares transferred to fee collector
        withdrawFeeShare = withdrawShareAmount.mulDiv(
            withdrawFee,
            UNITY
        );
        // Update total share amount in the vault.
        vaultState.totalShareAmount -= withdrawShareAmount - withdrawFeeShare;
        // Update user position info
        positions[position_info.chainId][position_info.positionId]
            .shareAmount -= withdrawShareAmount;
        // Update fee collector's position state.
        positions[0][0].shareAmount += withdrawFeeShare;
    }

    function collectManagementFee(
        mapping(uint16 => mapping(uint128 => Position)) storage positions,
        VaultState storage vaultState,
        uint256 managementFee
    ) external {
        uint256 shareAmtMint = managementFee
            .mulDiv(
                vaultState.totalShareAmount - positions[0][0].shareAmount,
                UNITY
            )
            .mulDiv(
                block.timestamp - vaultState.lastCollectionTimestamp,
                31536000
            );
        vaultState.lastCollectionTimestamp = block.timestamp;
        // Update total share amount in the vault.
        vaultState.totalShareAmount += shareAmtMint;
        // Update fee collector's position state.
        positions[0][0].shareAmount += shareAmtMint;
    }

    function collectHarvestFee(
        mapping(uint16 => mapping(uint128 => Position)) storage positions,
        VaultState storage vaultState,
        uint256 harvestFee,
        uint256 equityBefore,
        uint256 equityAfter
    ) external {
        uint256 equityChange = equityAfter - equityBefore;
        // Calculate shares minted for fee collector
        uint256 shareAmtMint = vaultState.totalShareAmount.mulDiv(
            harvestFee * equityChange,
            VaultLib.UNITY * equityAfter - harvestFee * equityChange
        );
        // Update total share amount in the vault.
        vaultState.totalShareAmount += shareAmtMint;
        // Update fee collector's position state.
        positions[0][0].shareAmount += shareAmtMint;
    }

    /// @dev Harvest farming rewards
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pairInfo: Addresses in the pair
    function harvest(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        PairInfo storage pairInfo
    ) external {
        IHomoraAdapter(contractInfo.adapter).homoraExecute(
            contractInfo,
            homoraPosId,
            HARVEST_DATA,
            pairInfo,
            0
        );
    }

    function populateRemoveHelper(
        VaultPosition memory pos,
        address lpToken,
        address stableToken,
        uint256 leverageLevel
    ) internal view returns (RemoveHelper memory vars) {
        vars.L = leverageLevel;
        (vars.reserveA, vars.reserveB) = getReserves(lpToken, stableToken);
        if (pos.amtB < pos.debtB) {
            // Short: amtB < debtAmtB, swap A to B
            vars = rebalanceMathShortRemove(pos, vars);
        } else {
            // Long: amtB > debtAmtB, swap B to A
            vars = rebalanceMathLongRemove(pos, vars);
        }
    }

    /// @dev Rebalance Homora Bank's farming position by removing liquidity and repaying debt
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pos: Farming position in Homora Bank
    /// @param pairInfo: Addresses in the pair
    /// @param leverageLevel: Target leverage * 10000
    /// @param slippage: Slippage in the swap, multiplied by 1e4, 0.1% => 10
    function rebalanceRemove(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        VaultPosition memory pos,
        PairInfo storage pairInfo,
        uint256 leverageLevel,
        uint256 slippage
    ) external {
        RemoveHelper memory vars = populateRemoveHelper(
            pos,
            pairInfo.lpToken,
            pairInfo.stableToken,
            leverageLevel
        );

        // Short: amtB < debtAmtB, swap A to B
        uint256 valueBeforeSwap = getTokenETHValue(
            contractInfo.oracle,
            pairInfo.stableToken,
            vars.Sa
        );
        uint256 valueAfterSwap = getTokenETHValue(
            contractInfo.oracle,
            pairInfo.assetToken,
            vars.Sb
        );
        // Long: amtB > debtAmtB, swap B to A
        if (pos.amtB > pos.debtB) {
            (valueBeforeSwap, valueAfterSwap) = (
                valueAfterSwap,
                valueBeforeSwap
            );
        }

        if (
            valueBeforeSwap > valueAfterSwap &&
            getOffset(valueAfterSwap, valueBeforeSwap) > slippage
        ) {
            revert Slippage_Too_Large();
        }

        removeLiquidity(
            contractInfo,
            homoraPosId,
            pairInfo,
            vars.collWithdrawAmt,
            vars.amtARepay,
            vars.amtBRepay
        );
    }

    function populateAddHelper(
        VaultPosition memory pos,
        address lpToken,
        address stableToken,
        uint256 leverageLevel
    ) internal view returns (AddHelper memory vars) {
        vars.L = leverageLevel;
        (vars.reserveA, vars.reserveB) = getReserves(lpToken, stableToken);
        vars.amtASupply = IERC20(stableToken).balanceOf(address(this));

        if (pos.amtB < pos.debtB) {
            // Short: amtB < debtAmtB, swap A to B
            vars = rebalanceMathShortAdd(pos, vars);
        } else {
            // Long: amtB > debtAmtB, swap B to A
            vars = rebalanceMathLongAdd(pos, vars);
        }
    }

    /// @dev Rebalance Homora Bank's farming position by borrowing tokens and adding liquidity
    /// @param contractInfo: Contract address info including adapter, bank and spell
    /// @param homoraPosId: Position id of the PDN vault in HomoraBank
    /// @param pos: Farming position in Homora Bank
    /// @param pairInfo: Addresses in the pair
    /// @param leverageLevel: Target leverage * 10000
    /// @param slippage: Slippage in the swap, multiplied by 1e4, 0.1% => 10
    /// @param pid: Pool id
    function rebalanceAdd(
        ContractInfo storage contractInfo,
        uint256 homoraPosId,
        VaultPosition memory pos,
        PairInfo storage pairInfo,
        uint256 leverageLevel,
        uint256 slippage,
        uint256 pid
    ) external {
        AddHelper memory vars = populateAddHelper(
            pos,
            pairInfo.lpToken,
            pairInfo.stableToken,
            leverageLevel
        );

        // Long: amtB > debtAmtB, swap B to A
        uint256 valueBeforeSwap = getTokenETHValue(
            contractInfo.oracle,
            pairInfo.assetToken,
            vars.Sb
        );
        uint256 valueAfterSwap = getTokenETHValue(
            contractInfo.oracle,
            pairInfo.stableToken,
            vars.Sa
        );
        // Short: amtB < debtAmtB, swap A to B
        if (pos.amtB < pos.debtB) {
            (valueBeforeSwap, valueAfterSwap) = (
                valueAfterSwap,
                valueBeforeSwap
            );
        }

        if (
            valueBeforeSwap > valueAfterSwap &&
            getOffset(valueAfterSwap, valueBeforeSwap) > slippage
        ) {
            revert Slippage_Too_Large();
        }

        addLiquidity(
            contractInfo,
            homoraPosId,
            pairInfo,
            vars.amtASupply,
            0,
            vars.amtABorrow,
            vars.amtBBorrow,
            pid,
            0
        );
    }

    /// @dev Calculate the amount of collateral to withdraw and debt to repay when delta is short
    /// @dev Assume `pos.debtAmtB > pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param vars: Helper struct
    function rebalanceMathShortRemove(
        VaultPosition memory pos,
        RemoveHelper memory vars
    ) internal pure returns (RemoveHelper memory) {
        // Ka << 1, multiply by someLargeNumber 1e18
        vars.Ka = SOME_LARGE_NUMBER.mulDiv(
            UNITY * (pos.debtB - pos.amtB),
            UNITY_MINUS_FEE * (vars.reserveB - pos.debtB),
            UP
        );
        vars.collWithdrawAmt =
            vars.L *
            pos.collateralSize.mulDiv(
                pos.debtA * SOME_LARGE_NUMBER + vars.Ka * vars.reserveA,
                TWINS * (SOME_LARGE_NUMBER + vars.Ka - 1) * pos.amtA,
                UP // round up to withdraw enough LP to repay A/B
            ) -
            pos.collateralSize.mulDiv(vars.L - TWINS, TWINS, DOWN);
        require(vars.collWithdrawAmt > 0, "Must withdraw >0");

        vars.amtAWithdraw = pos.amtA.mulDiv(
            vars.collWithdrawAmt - 2, // round down repay amounts
            pos.collateralSize,
            DOWN
        );
        vars.Sa = vars.Ka.mulDiv(
            vars.reserveA - vars.amtAWithdraw,
            SOME_LARGE_NUMBER,
            UP
        );
        if (vars.amtAWithdraw > vars.Sa) {
            vars.amtARepay = vars.amtAWithdraw - vars.Sa;
        } else {
            vars.amtARepay = 0;
            vars.collWithdrawAmt = pos.collateralSize.mulDiv(
                vars.Ka * vars.reserveA,
                (SOME_LARGE_NUMBER + vars.Ka) * pos.amtA,
                UP
            );
        }
        vars.amtBWithdraw = pos.amtB.mulDiv(
            vars.collWithdrawAmt - 2,
            pos.collateralSize,
            DOWN
        );
        vars.amtBRepay =
            (vars.amtBWithdraw *
                (vars.reserveB - pos.debtB) +
                vars.reserveB *
                (pos.debtB - pos.amtB)) /
            (vars.reserveB - pos.amtB);
        vars.Sb = vars.amtBRepay - vars.amtBWithdraw;
        return vars;
    }

    /// @dev Calculate the amount of collateral to withdraw and debt to repay when delta is long
    /// @dev Assume `pos.debtAmtB < pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param vars: Helper struct
    function rebalanceMathLongRemove(
        VaultPosition memory pos,
        RemoveHelper memory vars
    ) internal pure returns (RemoveHelper memory) {
        // Ka << 1, multiply by someLargeNumber 1e18
        vars.Ka = SOME_LARGE_NUMBER.mulDiv(
            UNITY_MINUS_FEE * (pos.amtB - pos.debtB),
            UNITY *
                vars.reserveB -
                FEE_RATE *
                pos.amtB -
                UNITY_MINUS_FEE *
                pos.debtB,
            DOWN
        );
        vars.collWithdrawAmt =
            vars.L *
            pos.collateralSize.mulDiv(
                pos.debtA * SOME_LARGE_NUMBER - vars.Ka * vars.reserveA,
                TWINS * (SOME_LARGE_NUMBER - vars.Ka - 1) * pos.amtA,
                UP // round up to withdraw enough LP to repay A/B
            ) -
            pos.collateralSize.mulDiv(vars.L - TWINS, TWINS, DOWN);
        require(vars.collWithdrawAmt > 0, "Must withdraw >0");

        vars.amtBWithdraw = pos.amtB.mulDiv(
            vars.collWithdrawAmt - 2,
            pos.collateralSize,
            DOWN
        );
        vars.Sb = (pos.amtB - pos.debtB).mulDiv(
            vars.reserveB - vars.amtBWithdraw,
            vars.reserveB - pos.amtB,
            UP
        );
        vars.amtBRepay = vars.amtBWithdraw - vars.Sb;
        vars.amtAWithdraw = pos.amtA.mulDiv(
            vars.collWithdrawAmt - 2, // round down repay amounts
            pos.collateralSize,
            DOWN
        );
        vars.amtARepay =
            ((SOME_LARGE_NUMBER - vars.Ka - 1) *
                vars.amtAWithdraw +
                vars.Ka *
                vars.reserveA) /
            SOME_LARGE_NUMBER;
        vars.Sa = vars.amtARepay - vars.amtAWithdraw;
        return vars;
    }

    /// @dev Calculate the amount of each token to borrow from Homora to reach DN when delta is long
    /// @dev Assume `pos.debtAmtB < pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param vars: Helper struct
    function rebalanceMathLongAdd(
        VaultPosition memory pos,
        AddHelper memory vars
    ) internal pure returns (AddHelper memory) {
        vars.Sb = vars.reserveB.mulDiv(
            pos.amtB - pos.debtB,
            vars.reserveB - pos.amtB,
            UP
        );
        vars.Sa = vars.reserveA.mulDiv(
            UNITY_MINUS_FEE * (pos.amtB - pos.debtB),
            UNITY *
                vars.reserveB -
                FEE_RATE *
                pos.amtB -
                UNITY_MINUS_FEE *
                pos.debtB,
            UP
        );
        vars.amtAAfter = vars.L.mulDiv(
            pos.amtA.mulDiv(vars.reserveA - vars.Sa, vars.reserveA, UP) -
                pos.debtA +
                vars.Sa +
                vars.amtASupply,
            TWINS,
            UP
        );
        // n_af

        vars.debtAAfter = vars.amtAAfter.mulDiv(vars.L - TWINS, vars.L, UP);

        if (vars.debtAAfter > pos.debtA) {
            vars.amtABorrow = vars.debtAAfter - pos.debtA;
            // `temp` is necessary to avoid "Stack too deep".
            uint256 temp = pos
                .amtB
                .mulDiv(vars.reserveA, vars.reserveA - vars.Sa, UP)
                .mulDiv(vars.reserveB + vars.Sb, vars.reserveB, UP);
            vars.amtBAfter = temp.mulDiv(vars.amtAAfter, pos.amtA, UP);
            vars.amtBBorrow = vars.amtBAfter - pos.debtB;
        } else {
            vars.amtABorrow = 0;
            vars.amtBBorrow =
                vars
                    .Sb
                    .mulDiv(
                        (UNITY + UNITY_MINUS_FEE) *
                            vars.reserveB -
                            pos.amtB -
                            UNITY_MINUS_FEE *
                            pos.debtB,
                        UNITY * (vars.reserveB - pos.amtB),
                        UP
                    )
                    .mulDiv(
                        vars.reserveA + vars.amtASupply,
                        vars.reserveA,
                        UP
                    ) +
                vars.amtASupply.mulDiv(vars.reserveB, vars.reserveA, UP);
        }
        return vars;
    }

    /// @dev Calculate the amount of each token to borrow from Homora to reach DN when delta is short
    /// @dev Assume `pos.debtAmtB > pos.amtB`. Check before calling
    /// @param pos: Farming position in Homora Bank
    /// @param vars: Helper struct
    function rebalanceMathShortAdd(
        VaultPosition memory pos,
        AddHelper memory vars
    ) internal pure returns (AddHelper memory) {
        vars.Sb = vars.reserveB.mulDiv(
            pos.debtB - pos.amtB,
            vars.reserveB - pos.amtB,
            UP
        );
        vars.Sa = vars.reserveA.mulDiv(
            UNITY * (pos.debtB - pos.amtB),
            UNITY_MINUS_FEE * (vars.reserveB - pos.debtB),
            UP
        );
        vars.amtAAfter = vars.L.mulDiv(
            pos.amtA.mulDiv(vars.reserveA + vars.Sa, vars.reserveA, UP) -
                pos.debtA -
                vars.Sa +
                vars.amtASupply,
            TWINS,
            UP
        );
        // n_af

        vars.debtAAfter = vars.amtAAfter.mulDiv(vars.L - TWINS, vars.L, UP);
        vars.amtABorrow = vars.debtAAfter - pos.debtA;
        // `temp` is necessary to avoid "Stack too deep".
        uint256 temp = pos
            .amtB
            .mulDiv(vars.reserveA, vars.reserveA + vars.Sa, UP)
            .mulDiv(vars.reserveB - vars.Sb, vars.reserveB, UP);
        vars.amtBAfter = temp.mulDiv(vars.amtAAfter, pos.amtA, UP);
        vars.amtBBorrow = vars.amtBAfter - pos.debtB;
        return vars;
    }

    /// @notice Swap fromToken into toToken
    function swap(
        address router,
        uint256 amount,
        address fromToken,
        address toToken
    ) internal returns (uint256) {
        IJoeRouter01 _router = IJoeRouter01(router);
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
    ) external returns (uint256) {
        IJoeRouter01 _router = IJoeRouter01(router);
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

    /// @notice Swap reward tokens into stable tokens
    function swapReward(address router, PairInfo storage pairInfo) external {
        uint256 rewardAmt = IERC20(pairInfo.rewardToken).balanceOf(
            address(this)
        );
        if (rewardAmt > 0) {
            swap(
                router,
                rewardAmt,
                pairInfo.rewardToken,
                pairInfo.stableToken
            );
        }
    }
}

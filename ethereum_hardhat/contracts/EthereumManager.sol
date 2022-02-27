//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.12;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/Wormhole.sol";
import "contracts/interfaces/ICurve.sol";
import "contracts/libraries/BytesLib.sol";

struct OwnershipInfo {
    address ownerAddr; // The owner of the position.
    uint16 chainId; // The chain this position belongs to.
}

struct PositionInfo {
    uint128 positionId; // The EVM position id.
    uint16 chainId; // Chain id, following Wormhole's design.
}

struct Config {
    uint32 crossChainFeeBPS; // Cross-chain fee in bpq.
    address feeSink; // Fee collecting address.
}

struct AssetInfo {
    address assetAddr; // The ERC20 address.
    uint256 amount;
}

// Information on a single swap with a Curve pool.
struct CurveSwapOperation {
    // Curve pool address.
    address pool;
    // Index of the token in the pool to be swapped.
    int128 from_index;
    // Index of the token in the pool to be returned.
    int128 to_index;
    // If true, use exchange_underlying(); otherwise, use exchange().
    bool underlying;
}

contract EthereumManager is Initializable, UUPSUpgradeable, OwnableUpgradeable {
    using SafeERC20 for IERC20;
    using BytesLib for bytes;

    uint16 private constant TERRA_CHAIN_ID = 3;
    uint256 private constant BPS = 10000;
    // The maximum allowed CROSS_CHAIN_FEE_BPS value (100 basis points, i.e. 1%).
    uint32 private constant MAX_CROSS_CHAIN_FEE_BPS = 100;

    // --- Cross-chain instruction format --- //
    // [uint128] position_id
    // [uint16] target_chain_id
    // [uint64] strategy_id
    // [uint32] num_token_transferred
    // [var_len] num_token_transferred * sizeof(sequence_number)
    // [uint32] encoded_action_len
    // [var_len] base64 encoding of params needed by action.

    // These nonce numbers are not used by wormhole yet. They are included
    // here for informational purpose only.
    uint32 private constant INSTRUCTION_NONCE = 1324532;
    uint32 private constant TOKEN_TRANSFER_NONCE = 15971121;

    // Address of Wormhole token bridge wrapped UST.
    address private WORMHOLE_WRAPPED_UST;

    // Cross-chain params.
    uint8 private CONSISTENCY_LEVEL;
    address private WORMHOLE_TOKEN_BRIDGE;
    // Cross-chain fee in basis points (i.e. 0.01% or 0.0001)
    uint32 private CROSS_CHAIN_FEE_BPS;
    // Where fee is sent.
    address private FEE_SINK;

    // Position ids for Ethereum.
    uint128 public nextPositionId;

    // Chain ID to Aperture manager address (32-byte encoded) mapping.
    mapping(uint16 => bytes32) public apertureManagers;

    // The array curveSwapRoutes[from_token][to_token] stores Curve swap operations that achieve the exchange from `from_token` to `to_token`.
    mapping(address => mapping(address => CurveSwapOperation[]))
        private curveSwapRoutes;

    // Stores hashes of completed incoming token transfer.
    mapping(bytes32 => bool) public completedTokenTransfers;

    // Stores wallet address to PositionInfo mapping.
    mapping(uint128 => OwnershipInfo) public positionToOwnership;

    modifier onlyPositionOwner(uint128 positionId) {
        require(positionToOwnership[positionId].ownerAddr == msg.sender);
        _;
    }

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        uint8 _consistencyLevel,
        address _wust,
        address _wormholeTokenBridge,
        bytes32 _terraManagerAddress,
        uint32 _crossChainFeeBPS,
        address _feeSink
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        CONSISTENCY_LEVEL = _consistencyLevel;
        WORMHOLE_WRAPPED_UST = _wust;
        WORMHOLE_TOKEN_BRIDGE = _wormholeTokenBridge;
        apertureManagers[TERRA_CHAIN_ID] = _terraManagerAddress;
        CROSS_CHAIN_FEE_BPS = _crossChainFeeBPS;
        FEE_SINK = _feeSink;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function updateCrossChainFeeBPS(uint32 crossChainFeeBPS)
        external
        onlyOwner
    {
        require(
            crossChainFeeBPS <= MAX_CROSS_CHAIN_FEE_BPS,
            "crossChainFeeBPS exceeds maximum allowed value of 100"
        );
        CROSS_CHAIN_FEE_BPS = crossChainFeeBPS;
    }

    function updateFeeSink(address feeSink) external onlyOwner {
        require(feeSink != address(0), "feeSink address must be non-zero");
        FEE_SINK = feeSink;
    }

    // Owner-only.
    // Updates the Curve swap route for `fromToken` to `toToken` with `route`.
    // The array `tokens` should comprise all tokens on `route` except for `toToken`.
    // Each element of `tokens` needs to be swapped for another token through some Curve pool, so we need to allow the pool to transfer the corresponding token from this contract.
    //
    // Examples:
    // (1) BUSD -> whUST route: [[CURVE_BUSD_3CRV_POOL_ADDR, 0, 1, false], [CURVE_WHUST_3CRV_POOL_ADDR, 1, 0, false]];
    //     tokens: [BUSD_TOKEN_ADDR, 3CRV_TOKEN_ADDR];
    //     The first exchange: BUSD -> 3Crv using the BUSD-3Crv pool;
    //     The second exchange: 3Crv -> whUST using the whUST-3Crv pool.
    // (2) USDC -> whUST route: [[CURVE_WHUST_3CRV_POOL_ADDR, 2, 0, true]];
    //     tokens: [USDC_TOKEN_ADDR];
    //     The only underlying exchange: USDC -> whUST using the whUST-3Crv pool's exchange_underlying() function.
    function updateCurveSwapRoute(
        address fromToken,
        address toToken,
        CurveSwapOperation[] calldata route,
        address[] calldata tokens
    ) external onlyOwner {
        require(route.length > 0 && route.length == tokens.length);
        for (uint256 i = 0; i < route.length; i++) {
            if (
                IERC20(tokens[i]).allowance(address(this), route[i].pool) == 0
            ) {
                IERC20(tokens[i]).safeApprove(route[i].pool, type(uint256).max);
            }
        }
        CurveSwapOperation[] storage storage_route = curveSwapRoutes[fromToken][
            toToken
        ];
        if (storage_route.length != 0) {
            delete curveSwapRoutes[fromToken][toToken];
        }
        for (uint256 i = 0; i < route.length; ++i) {
            storage_route.push(route[i]);
        }
    }

    // Swaps `fromToken` in the amount of `amount` to `toToken`.
    // Revert if the output amount is less `minAmountOut`.
    // Returns the output amount.
    function swapToken(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut
    ) internal returns (uint256) {
        CurveSwapOperation[] memory route = curveSwapRoutes[fromToken][toToken];
        require(route.length > 0, "Swap route does not exist");

        IERC20(fromToken).safeTransferFrom(msg.sender, address(this), amount);
        for (uint256 i = 0; i < route.length; i++) {
            if (route[i].underlying) {
                amount = ICurve(route[i].pool).exchange_underlying(
                    route[i].from_index,
                    route[i].to_index,
                    amount,
                    0
                );
            } else {
                amount = ICurve(route[i].pool).exchange(
                    route[i].from_index,
                    route[i].to_index,
                    amount,
                    0
                );
            }
        }

        require(
            amount >= minAmountOut,
            "Output token amount less than specified minimum"
        );
        return amount;
    }

    function transferAssetFromSender(AssetInfo[] calldata assetInfos) internal {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            IERC20(assetInfos[i].assetAddr).safeTransferFrom(
                msg.sender,
                address(this),
                assetInfos[i].amount
            );
        }
    }

    // TODO(gnarlycow): Look into whether re-entrancy guard is needed for recordNewPositionInfo() which is called by createPosition() and swapTokenAndCreatePosition().
    function recordNewPositionInfo(uint16 targetChainId) internal returns (uint128) {
        uint128 positionId = nextPositionId++;
        positionToOwnership[positionId] = OwnershipInfo(
            msg.sender,
            targetChainId
        );
        return positionId;
    }

    function createPosition(
        uint64 strategyId,
        uint16 targetChainId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedAction
    ) external {
        uint128 positionId = recordNewPositionInfo(targetChainId);
        transferAssetFromSender(assetInfos);
        handleExecuteStrategy(
            strategyId,
            targetChainId,
            assetInfos,
            positionId,
            encodedAction
        );
    }

    function executeStrategy(
        uint128 positionId,
        uint64 strategyId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedAction
    ) external onlyPositionOwner(positionId) {
        transferAssetFromSender(assetInfos);
        handleExecuteStrategy(
            strategyId,
            positionToOwnership[positionId].chainId,
            assetInfos,
            positionId,
            encodedAction
        );
    }

    function swapTokenAndCreatePosition(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        uint64 strategyId,
        uint16 targetChainId,
        bytes calldata encodedAction
    ) external {
        uint128 positionId = recordNewPositionInfo(targetChainId);
        uint256 toTokenAmount = swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        handleExecuteStrategy(
            strategyId,
            targetChainId,
            assetInfos,
            positionId,
            encodedAction
        );
    }

    function swapTokenAndExecuteStrategy(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        uint128 positionId,
        uint64 strategyId,
        bytes calldata encodedAction
    ) external onlyPositionOwner(positionId) {
        uint256 toTokenAmount = swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        handleExecuteStrategy(
            strategyId,
            positionToOwnership[positionId].chainId,
            assetInfos,
            positionId,
            encodedAction
        );
    }

    function handleExecuteStrategy(
        uint64 strategyId,
        uint16 targetChainId,
        AssetInfo[] memory assetInfos,
        uint128 positionId,
        bytes calldata encodedAction
    ) internal {
        bytes memory payload = abi.encodePacked(
            positionId,
            targetChainId,
            strategyId,
            uint32(assetInfos.length)
        );
        // TODO: Relax this once we add cross-chain manager communication.
        require(assetInfos.length == 1);
        for (uint256 i = 0; i < assetInfos.length; i++) {
            // TODO: Check that `token` is allowed by this strategy when we add cross-chain manager communication.
            require(assetInfos[i].assetAddr == WORMHOLE_WRAPPED_UST);

            // Collect fee as needed.
            uint256 amount = assetInfos[i].amount;
            if (CROSS_CHAIN_FEE_BPS != 0) {
                uint256 crossChainFee = (amount * CROSS_CHAIN_FEE_BPS) / BPS;
                IERC20(assetInfos[i].assetAddr).safeTransfer(
                    FEE_SINK,
                    crossChainFee
                );
                amount -= crossChainFee;
            }

            // Allow wormhole to spend USTw from this contract.
            IERC20(assetInfos[i].assetAddr).safeApprove(
                WORMHOLE_TOKEN_BRIDGE,
                amount
            );

            // Initiate token transfer.
            uint64 transferSequence = WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE)
                .transferTokens(
                    assetInfos[i].assetAddr,
                    amount,
                    targetChainId,
                    apertureManagers[TERRA_CHAIN_ID],
                    0,
                    TOKEN_TRANSFER_NONCE
                );
            payload = payload.concat(abi.encodePacked(transferSequence));
        }

        // Send instruction message to Terra manager.
        payload = payload.concat(
            abi.encodePacked(uint32(encodedAction.length), encodedAction)
        );

        WormholeCoreBridge(
            WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE).wormhole()
        ).publishMessage(INSTRUCTION_NONCE, payload, CONSISTENCY_LEVEL);
    }

    function getPositions(address user)
        external
        view
        returns (PositionInfo[] memory)
    {
        uint128 positionCount = 0;
        for (
            uint128 positionId = 0;
            positionId < nextPositionId;
            positionId++
        ) {
            if (positionToOwnership[positionId].ownerAddr == user) {
                positionCount++;
            }
        }

        uint128 userIndex = 0;
        PositionInfo[] memory positionIdVec = new PositionInfo[](positionCount);
        for (
            uint128 positionId = 0;
            positionId < nextPositionId && userIndex < positionCount;
            positionId++
        ) {
            if (positionToOwnership[positionId].ownerAddr == user) {
                positionIdVec[userIndex++] = PositionInfo(
                    positionId,
                    positionToOwnership[positionId].chainId
                );
            }
        }
        return positionIdVec;
    }

    function getConfig() external view returns (Config memory) {
        return Config(CROSS_CHAIN_FEE_BPS, FEE_SINK);
    }
}

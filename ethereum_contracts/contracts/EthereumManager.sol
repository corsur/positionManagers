//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/ICurve.sol";
import "contracts/interfaces/IWormhole.sol";
import "contracts/libraries/BytesLib.sol";

struct StoredPositionInfo {
    address ownerAddr;
    uint16 strategyChainId;
    uint64 strategyId;
}

// Only used by the getPositions() view function.
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

contract EthereumManager is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable,
    ReentrancyGuard
{
    using SafeERC20 for IERC20;
    using BytesLib for bytes;

    uint16 private constant TERRA_CHAIN_ID = 3;
    uint256 private constant BPS = 10000;
    // The maximum allowed CROSS_CHAIN_FEE_BPS value (100 basis points, i.e. 1%).
    uint32 private constant MAX_CROSS_CHAIN_FEE_BPS = 100;

    // Version 0 of the Aperture instructure payload format.
    // See https://github.com/Aperture-Finance/Aperture-Contracts/blob/instruction-dev/packages/aperture_common/src/instruction.rs.
    uint8 private constant INSTRUCTION_VERSION = 0;
    uint8 private constant INSTRUCTION_TYPE_POSITION_OPEN = 0;
    uint8 private constant INSTRUCTION_TYPE_EXECUTE_STRATEGY = 1;
    uint8 private constant INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT = 2;

    // Nonce does not play a meaningful role as sequence numbers distingish different messages emitted by the same address.
    uint32 private constant WORMHOLE_NONCE = 0;

    // isTokenWhitelistedForStrategy[chainId][strategyId][tokenAddress] represents whether the token is allowed for the specified strategy.
    mapping(uint16 => mapping(uint64 => mapping(address => bool)))
        private isTokenWhitelistedForStrategy;

    // Address of the Wormhole token bridge contract.
    address public WORMHOLE_TOKEN_BRIDGE;
    // Address of the Wormhole core bridge contract.
    address public WORMHOLE_CORE_BRIDGE;
    // Consistency level for published Aperture instruction message via Wormhole core bridge.
    // The number of blocks to wait before Wormhole guardians consider a published message final.
    uint8 public CONSISTENCY_LEVEL;
    // Cross-chain fee in basis points (i.e. 0.01% or 0.0001)
    uint32 public CROSS_CHAIN_FEE_BPS;
    // Where fee is sent.
    address public FEE_SINK;

    // Position ids for Ethereum.
    uint128 public nextPositionId;

    mapping(uint16 => bytes32) public chainIdToApertureManager;

    // The array curveSwapRoutes[from_token][to_token] stores Curve swap operations that achieve the exchange from `from_token` to `to_token`.
    mapping(address => mapping(address => CurveSwapOperation[]))
        private curveSwapRoutes;

    // Stored position info by position id.
    mapping(uint128 => StoredPositionInfo) public positionIdToInfo;

    // Hashes of processed incoming Aperture instructions are stored in this mapping.
    mapping(bytes32 => bool) public processedInstructions;

    modifier onlyPositionOwner(uint128 positionId) {
        require(positionIdToInfo[positionId].ownerAddr == msg.sender);
        _;
    }

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        uint8 _consistencyLevel,
        address _wormholeTokenBridge,
        uint32 _crossChainFeeBPS,
        address _feeSink
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        CONSISTENCY_LEVEL = _consistencyLevel;
        WORMHOLE_TOKEN_BRIDGE = _wormholeTokenBridge;
        WORMHOLE_CORE_BRIDGE = WormholeTokenBridge(_wormholeTokenBridge)
            .wormhole();
        require(
            _crossChainFeeBPS <= MAX_CROSS_CHAIN_FEE_BPS,
            "crossChainFeeBPS exceeds maximum allowed value"
        );
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
            "crossChainFeeBPS exceeds maximum allowed value"
        );
        CROSS_CHAIN_FEE_BPS = crossChainFeeBPS;
    }

    function updateFeeSink(address feeSink) external onlyOwner {
        require(feeSink != address(0), "feeSink address must be non-zero");
        FEE_SINK = feeSink;
    }

    // Sets a new Aperture manager address for the specified chain.
    // To remove a manager from the registry, set `managerAddress` to zero.
    function updateApertureManager(uint16 chainId, bytes32 managerAddress)
        external
        onlyOwner
    {
        chainIdToApertureManager[chainId] = managerAddress;
    }

    // Sets whether `tokenAddress` is whitelisted for the specified strategy.
    function updateIsTokenWhitelistedForStrategy(
        uint16 chainId,
        uint64 strategyId,
        address tokenAddress,
        bool isWhitelisted
    ) external onlyOwner {
        isTokenWhitelistedForStrategy[chainId][strategyId][
            tokenAddress
        ] = isWhitelisted;
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
                IERC20(tokens[i]).safeIncreaseAllowance(
                    route[i].pool,
                    type(uint256).max
                );
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
    //
    // Note that `curveSwapRoutes` also acts as a whitelist on `fromToken`.
    // That is to say, if a swap route is not set for `fromToken` -> `toToken`, then this function reverts
    // without calling ` IERC20(fromToken).safeTransferFrom()`.
    // This prevents re-entrancy attacks due to malicious `fromToken` contracts.
    function swapToken(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut
    ) internal returns (uint256) {
        CurveSwapOperation[] memory route = curveSwapRoutes[fromToken][toToken];
        require(route.length > 0, "Swap route does not exist");

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

    // Simulates the swap from `amount` amount of `fromToken` to `toToken` and returns the output amount.
    // Note that this function chains together simulations of Curve pool exchanges; assumes that each Curve pool exchange does not have any side effects on subsequent exchanges.
    function simulateSwapToken(
        address fromToken,
        address toToken,
        uint256 amount
    ) public view returns (uint256) {
        CurveSwapOperation[] memory route = curveSwapRoutes[fromToken][toToken];
        require(route.length > 0, "Swap route does not exist");
        for (uint256 i = 0; i < route.length; i++) {
            if (route[i].underlying) {
                amount = ICurve(route[i].pool).get_dy_underlying(
                    route[i].from_index,
                    route[i].to_index,
                    amount
                );
            } else {
                amount = ICurve(route[i].pool).get_dy(
                    route[i].from_index,
                    route[i].to_index,
                    amount
                );
            }
        }
        return amount;
    }

    function validateAndTransferAssetFromSender(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] calldata assetInfos
    ) internal {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            require(
                isTokenWhitelistedForStrategy[strategyChainId][strategyId][
                    assetInfos[i].assetAddr
                ],
                "token not allowed"
            );
            IERC20(assetInfos[i].assetAddr).safeTransferFrom(
                msg.sender,
                address(this),
                assetInfos[i].amount
            );
        }
    }

    function recordNewPositionInfo(uint16 strategyChainId, uint64 strategyId)
        internal
        nonReentrant
        returns (uint128)
    {
        uint128 positionId = nextPositionId++;
        positionIdToInfo[positionId] = StoredPositionInfo(
            msg.sender,
            strategyChainId,
            strategyId
        );
        return positionId;
    }

    function sendTokensCrossChainAndConstructCommonPayload(
        uint8 instructionType,
        uint16 strategyChainId,
        AssetInfo[] memory assetInfos,
        uint128 positionId,
        bytes calldata encodedData
    ) internal returns (bytes memory) {
        bytes32 strategyChainApertureManager = chainIdToApertureManager[
            strategyChainId
        ];
        require(
            strategyChainApertureManager != 0,
            "unexpected strategyChainId"
        );
        bytes memory payload = abi.encodePacked(
            INSTRUCTION_VERSION,
            instructionType,
            positionId,
            strategyChainId,
            uint32(assetInfos.length)
        );
        for (uint256 i = 0; i < assetInfos.length; i++) {
            // Collect cross-chain fees if applicable.
            uint256 amount = assetInfos[i].amount;
            uint256 crossChainFee = (amount * CROSS_CHAIN_FEE_BPS) / BPS;
            if (crossChainFee > 0) {
                IERC20(assetInfos[i].assetAddr).safeTransfer(
                    FEE_SINK,
                    crossChainFee
                );
                amount -= crossChainFee;
            }

            // Allow wormhole token bridge contract to transfer this token out of here.
            IERC20(assetInfos[i].assetAddr).safeIncreaseAllowance(
                WORMHOLE_TOKEN_BRIDGE,
                amount
            );

            // Initiate token transfer.
            uint64 transferSequence = WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE)
                .transferTokens(
                    assetInfos[i].assetAddr,
                    amount,
                    strategyChainId,
                    strategyChainApertureManager,
                    /*arbiterFee=*/
                    0,
                    WORMHOLE_NONCE
                );

            // Append sequence to payload.
            payload = payload.concat(abi.encodePacked(transferSequence));
        }

        // Append encoded data: the length as a uint32, followed by the encoded bytes themselves.
        return
            payload.concat(
                abi.encodePacked(uint32(encodedData.length), encodedData)
            );
    }

    function publishPositionOpenInstruction(
        uint16 strategyChainId,
        uint64 strategyId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) internal {
        // Initiate token transfers and construct partial instruction payload.
        bytes
            memory partial_payload = sendTokensCrossChainAndConstructCommonPayload(
                INSTRUCTION_TYPE_POSITION_OPEN,
                strategyChainId,
                assetInfos,
                positionId,
                encodedPositionOpenData
            );

        // Append `strategyId` to the instruction to complete the payload and publish it via Wormhole.
        WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).publishMessage(
            WORMHOLE_NONCE,
            partial_payload.concat(abi.encodePacked(strategyId)),
            CONSISTENCY_LEVEL
        );
    }

    function createPosition(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedPositionOpenData
    ) external {
        uint128 positionId = recordNewPositionInfo(strategyChainId, strategyId);
        validateAndTransferAssetFromSender(
            strategyChainId,
            strategyId,
            assetInfos
        );
        publishPositionOpenInstruction(
            strategyChainId,
            strategyId,
            positionId,
            assetInfos,
            encodedPositionOpenData
        );
    }

    function swapTokenAndCreatePosition(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        uint64 strategyId,
        uint16 strategyChainId,
        bytes calldata encodedPositionOpenData
    ) external {
        require(
            isTokenWhitelistedForStrategy[strategyChainId][strategyId][toToken],
            "toToken not allowed"
        );
        uint128 positionId = recordNewPositionInfo(strategyChainId, strategyId);
        IERC20(fromToken).safeTransferFrom(msg.sender, address(this), amount);
        uint256 toTokenAmount = swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        publishPositionOpenInstruction(
            strategyChainId,
            strategyId,
            positionId,
            assetInfos,
            encodedPositionOpenData
        );
    }

    function publishExecuteStrategyInstruction(
        uint16 strategyChainId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedActionData
    ) internal {
        WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).publishMessage(
            WORMHOLE_NONCE,
            sendTokensCrossChainAndConstructCommonPayload(
                INSTRUCTION_TYPE_EXECUTE_STRATEGY,
                strategyChainId,
                assetInfos,
                positionId,
                encodedActionData
            ),
            CONSISTENCY_LEVEL
        );
    }

    function executeStrategy(
        uint128 positionId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedActionData
    ) external onlyPositionOwner(positionId) {
        uint16 strategyChainId = positionIdToInfo[positionId].strategyChainId;
        validateAndTransferAssetFromSender(
            strategyChainId,
            positionIdToInfo[positionId].strategyId,
            assetInfos
        );
        publishExecuteStrategyInstruction(
            strategyChainId,
            positionId,
            assetInfos,
            encodedActionData
        );
    }

    function swapTokenAndExecuteStrategy(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        uint128 positionId,
        bytes calldata encodedActionData
    ) external onlyPositionOwner(positionId) {
        IERC20(fromToken).safeTransferFrom(msg.sender, address(this), amount);
        uint256 toTokenAmount = swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        publishExecuteStrategyInstruction(
            positionIdToInfo[positionId].strategyChainId,
            positionId,
            assetInfos,
            encodedActionData
        );
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
            if (positionIdToInfo[positionId].ownerAddr == user) {
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
            if (positionIdToInfo[positionId].ownerAddr == user) {
                positionIdVec[userIndex++] = PositionInfo(
                    positionId,
                    positionIdToInfo[positionId].strategyChainId
                );
            }
        }
        return positionIdVec;
    }

    function decodeWormholeVM(bytes calldata encodedVM)
        internal
        view
        returns (WormholeCoreBridge.VM memory)
    {
        (
            WormholeCoreBridge.VM memory decodedVM,
            bool valid,
            string memory reason
        ) = WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).parseAndVerifyVM(
                encodedVM
            );
        require(valid, reason);
        return decodedVM;
    }

    // Validates and completes the incoming token transfer encoded as `encodedTokenTransfer`.
    // Returns (the incoming ERC-20 token address, token amount).
    function validateAndCompleteIncomingTokenTransfer(
        WormholeTokenBridge wormholeTokenBridge,
        bytes calldata encodedTokenTransfer,
        uint16 expectedEmitterChainId,
        uint64 expectedSequence
    ) internal returns (address, uint256) {
        WormholeCoreBridge.VM memory tokenTransferVM = decodeWormholeVM(
            encodedTokenTransfer
        );
        require(
            tokenTransferVM.emitterChainId == expectedEmitterChainId,
            "emitterChainId mismatch"
        );
        require(
            tokenTransferVM.sequence == expectedSequence,
            "sequence mismatch"
        );
        // Note that we delegate the validation of tokenTransferVM.emitterAddress to Wormhole Token Bridge.

        WormholeTokenBridge.Transfer memory transfer = wormholeTokenBridge
            .parseTransfer(tokenTransferVM.payload);
        require(
            transfer.to == bytes32(uint256(uint160(address(this)))),
            "token recipient is not this Aperture manager"
        );
        // Note that we delegate the validation of `transfer.toChain` to Wormhole Token Bridge.

        if (!wormholeTokenBridge.isTransferCompleted(tokenTransferVM.hash)) {
            wormholeTokenBridge.completeTransfer(encodedTokenTransfer);
        }

        if (transfer.tokenChain == wormholeTokenBridge.chainId()) {
            address tokenAddress = address(
                uint160(uint256(transfer.tokenAddress))
            );
            // Query and normalize decimals.
            (, bytes memory queriedDecimals) = address(tokenAddress).staticcall(
                abi.encodeWithSignature("decimals()")
            );
            uint8 decimals = abi.decode(queriedDecimals, (uint8));
            uint256 tokenAmount = transfer.amount;
            if (decimals > 8) {
                tokenAmount *= 10**(decimals - 8);
            }
            return (tokenAddress, tokenAmount);
        } else {
            return (
                wormholeTokenBridge.wrappedAsset(
                    transfer.tokenChain,
                    transfer.tokenAddress
                ),
                transfer.amount
            );
        }
    }

    function processSingleTokenDisbursementInstruction(
        WormholeCoreBridge.VM memory instructionVM,
        bytes[] calldata encodedTokenTransferVMs
    ) internal {
        WormholeTokenBridge wormholeTokenBridge = WormholeTokenBridge(
            WORMHOLE_TOKEN_BRIDGE
        );
        uint256 index = 2;

        // Parse sequence.
        uint64 sequence = instructionVM.payload.toUint64(index);
        index += 8;

        // Parse and validate recipient chain.
        uint16 recipientChain = instructionVM.payload.toUint16(index);
        require(
            recipientChain == wormholeTokenBridge.chainId(),
            "instruction not intended for this chain"
        );
        index += 2;

        // Parse recipient address.
        address recipient = address(
            uint160(instructionVM.payload.toUint256(index))
        );
        index += 32;

        // Parse and validate token transfer VMs.
        require(
            encodedTokenTransferVMs.length == 1,
            "unexpected encodedTokenTransferVMs length"
        );
        (
            address tokenAddress,
            uint256 tokenAmount
        ) = validateAndCompleteIncomingTokenTransfer(
                wormholeTokenBridge,
                encodedTokenTransferVMs[0],
                instructionVM.emitterChainId,
                sequence
            );

        // Process swap request if present.
        if (instructionVM.payload.length > index) {
            // Swap is requested, so we parse desired token to swap to and the minimum output amount.
            address desiredTokenAddress = address(
                uint160(instructionVM.payload.toUint256(index))
            );
            index += 32;
            uint256 minOutputAmount = instructionVM.payload.toUint256(index);
            index += 32;
            require(
                index == instructionVM.payload.length,
                "invalid instruction payload"
            );

            // Perform swap if the output amount meets the required threshold.
            uint256 simulatedOutputAmount = simulateSwapToken(
                tokenAddress,
                desiredTokenAddress,
                tokenAmount
            );
            if (simulatedOutputAmount >= minOutputAmount) {
                uint256 actualOutputAmount = swapToken(
                    tokenAddress,
                    desiredTokenAddress,
                    tokenAmount,
                    minOutputAmount
                );
                tokenAddress = desiredTokenAddress;
                tokenAmount = actualOutputAmount;
            }
        }

        // Disburse to the recipient.
        SafeERC20.safeTransfer(IERC20(tokenAddress), recipient, tokenAmount);
    }

    function processApertureInstruction(
        bytes calldata encodedInstructionVM,
        bytes[] calldata encodedTokenTransferVMs
    ) external {
        // Parse and validate instruction VM.
        WormholeCoreBridge.VM memory instructionVM = decodeWormholeVM(
            encodedInstructionVM
        );
        require(
            chainIdToApertureManager[instructionVM.emitterChainId] ==
                instructionVM.emitterAddress,
            "unexpected emitterAddress"
        );
        require(
            !processedInstructions[instructionVM.hash],
            "instruction already processed"
        );

        // Mark this instruction as processed so it cannot be replayed.
        processedInstructions[instructionVM.hash] = true;

        // Parse version / instruction type.
        // Note that Solidity checks array index for possible out of bounds, so there is no need for us to do so again.
        require(
            instructionVM.payload[0] == 0,
            "unexpected instruction version"
        );
        if (
            uint8(instructionVM.payload[1]) ==
            INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT
        ) {
            processSingleTokenDisbursementInstruction(
                instructionVM,
                encodedTokenTransferVMs
            );
        } else {
            revert("unsupported instruction type");
        }
    }
}

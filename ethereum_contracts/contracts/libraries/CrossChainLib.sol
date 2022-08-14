//SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/IWormhole.sol";
import "contracts/interfaces/IApertureCommon.sol";
import "contracts/libraries/BytesLib.sol";
import "contracts/libraries/CurveRouterLib.sol";

// Version 0 of the Aperture instructure payload format.
// See https://github.com/Aperture-Finance/Aperture-Contracts/blob/instruction-dev/packages/aperture_common/src/instruction.rs.
uint8 constant INSTRUCTION_VERSION = 0;
uint8 constant INSTRUCTION_TYPE_POSITION_OPEN = 0;
uint8 constant INSTRUCTION_TYPE_EXECUTE_STRATEGY = 1;
uint8 constant INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT = 2;

struct WormholeCrossChainContext {
    // Address of the Wormhole token bridge contract.
    address tokenBridge;
    // Address of the Wormhole core bridge contract.
    address coreBridge;
    // Consistency level for published Aperture instruction message via Wormhole core bridge.
    // The number of blocks to wait before Wormhole guardians consider a published message final.
    uint8 consistencyLevel;
}

struct CrossChainFeeContext {
    // Cross-chain fee in basis points (i.e. 0.01% or 0.0001)
    uint32 feeBps;
    // Where collected cross-chain fees go.
    address feeSink;
}

struct CrossChainContext {
    WormholeCrossChainContext wormholeContext;
    CrossChainFeeContext feeContext;
    // Registered Aperture Manager contract addresses on various chains.
    mapping(uint16 => bytes32) chainIdToApertureManager;
    // Hashes of processed incoming Aperture instructions are stored in this mapping.
    mapping(bytes32 => bool) processedInstructions;
}

/// @custom:oz-upgrades-unsafe-allow external-library-linking
library CrossChainLib {
    using SafeERC20 for IERC20;
    using BytesLib for bytes;
    using CurveRouterLib for CurveRouterContext;

    // 1 basis point equals 0.0001 in decimal form, so 10000 basis points = 1.
    uint256 private constant BPS = 10000;

    // The maximum allowed CrossChainFeeContext.feeBps value (100 basis points, i.e. 1%).
    uint32 private constant MAX_FEE_BPS = 100;

    // Nonce does not play a meaningful role as sequence numbers distingish different messages emitted by the same address.
    uint32 private constant WORMHOLE_NONCE = 0;

    // Initiates outgoing transfer of `assetInfos` to the Aperture manager on `recipientChainId` via Wormhole Token Bridge.
    // Returns encoded transfer sequences.
    function getOutgoingTokenTransferSequencePayload(
        AssetInfo[] memory assetInfos,
        uint16 recipientChainId,
        CrossChainContext storage context
    ) internal returns (bytes memory payloadTransferSequences) {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            if (assetInfos[i].assetType == AssetType.NativeToken) {
                revert("unsupported cross-chain native token");
            }

            // Collect cross-chain fees if applicable.
            uint256 amount = assetInfos[i].amount;
            uint256 fee = (assetInfos[i].amount * context.feeContext.feeBps) /
                BPS;
            if (fee > 0) {
                IERC20(assetInfos[i].assetAddr).safeTransfer(
                    context.feeContext.feeSink,
                    fee
                );
                amount -= fee;
            }

            // Allow wormhole token bridge contract to transfer this token out of here.
            IERC20(assetInfos[i].assetAddr).safeIncreaseAllowance(
                context.wormholeContext.tokenBridge,
                amount
            );

            // Initiate token transfer.
            uint64 transferSequence = WormholeTokenBridge(
                context.wormholeContext.tokenBridge
            ).transferTokens(
                    assetInfos[i].assetAddr,
                    amount,
                    recipientChainId,
                    /*recipient=*/
                    context.chainIdToApertureManager[recipientChainId],
                    /*arbiterFee=*/
                    0,
                    WORMHOLE_NONCE
                );

            // Append sequence to payload.
            payloadTransferSequences = payloadTransferSequences.concat(
                abi.encodePacked(transferSequence)
            );
        }
    }

    function sendTokensCrossChainAndConstructCommonPayload(
        uint16 strategyChainId,
        uint8 instructionType,
        AssetInfo[] memory assetInfos,
        uint128 positionId,
        bytes calldata encodedData,
        CrossChainContext storage context
    ) internal returns (bytes memory) {
        require(
            context.chainIdToApertureManager[strategyChainId] != 0,
            "unexpected strategyChainId"
        );
        return
            abi.encodePacked(
                INSTRUCTION_VERSION,
                instructionType,
                positionId,
                strategyChainId,
                uint32(assetInfos.length),
                getOutgoingTokenTransferSequencePayload(
                    assetInfos,
                    strategyChainId,
                    context
                ),
                uint32(encodedData.length),
                encodedData
            );
    }

    function validateAndUpdateFeeContext(
        CrossChainContext storage self,
        CrossChainFeeContext calldata newFeeContext
    ) external {
        require(newFeeContext.feeBps <= MAX_FEE_BPS, "feeBps too large");
        require(newFeeContext.feeSink != address(0), "feeSink can't be null");
        self.feeContext = newFeeContext;
    }

    function publishPositionOpenInstruction(
        CrossChainContext storage self,
        uint16 strategyChainId,
        uint64 strategyId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) external {
        // Initiate token transfers and construct partial instruction payload.
        bytes
            memory partial_payload = sendTokensCrossChainAndConstructCommonPayload(
                strategyChainId,
                INSTRUCTION_TYPE_POSITION_OPEN,
                assetInfos,
                positionId,
                encodedPositionOpenData,
                self
            );
        // Append `strategyId` to the instruction to complete the payload and publish it via Wormhole.
        WormholeCoreBridge(self.wormholeContext.coreBridge).publishMessage(
            WORMHOLE_NONCE,
            partial_payload.concat(abi.encodePacked(strategyId)),
            self.wormholeContext.consistencyLevel
        );
    }

    function publishExecuteStrategyInstruction(
        CrossChainContext storage self,
        uint16 strategyChainId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedActionData
    ) external {
        WormholeCoreBridge(self.wormholeContext.coreBridge).publishMessage(
            WORMHOLE_NONCE,
            sendTokensCrossChainAndConstructCommonPayload(
                strategyChainId,
                INSTRUCTION_TYPE_EXECUTE_STRATEGY,
                assetInfos,
                positionId,
                encodedActionData,
                self
            ),
            self.wormholeContext.consistencyLevel
        );
    }

    function decodeWormholeVM(
        bytes calldata encodedVM,
        address wormholeCoreBridge
    ) internal view returns (WormholeCoreBridge.VM memory) {
        (
            WormholeCoreBridge.VM memory decodedVM,
            bool valid,
            string memory reason
        ) = WormholeCoreBridge(wormholeCoreBridge).parseAndVerifyVM(encodedVM);
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
            encodedTokenTransfer,
            wormholeTokenBridge.wormhole()
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
            "unexpected token recipient"
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
        CrossChainContext storage self,
        WormholeCoreBridge.VM memory instructionVM,
        bytes[] calldata encodedTokenTransferVMs,
        CurveRouterContext storage curveRouterContext
    ) external {
        WormholeTokenBridge wormholeTokenBridge = WormholeTokenBridge(
            self.wormholeContext.tokenBridge
        );
        uint256 index = 2;

        // Parse sequence.
        uint64 sequence = instructionVM.payload.toUint64(index);
        index += 8;

        // Parse and validate recipient chain.
        uint16 recipientChain = instructionVM.payload.toUint16(index);
        require(
            recipientChain == wormholeTokenBridge.chainId(),
            "unexpected recipientChain"
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
            "invalid encodedTokenTransferVMs length"
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
                "invalid ix payload"
            );

            // Swap and disburse if the output amount meets the required threshold.
            uint256 simulatedOutputAmount = curveRouterContext
                .simulateSwapToken(
                    tokenAddress,
                    desiredTokenAddress,
                    tokenAmount
                );
            if (simulatedOutputAmount >= minOutputAmount) {
                uint256 outputAmount = curveRouterContext.swapToken(
                    tokenAddress,
                    desiredTokenAddress,
                    tokenAmount,
                    minOutputAmount
                );
                SafeERC20.safeTransfer(
                    IERC20(desiredTokenAddress),
                    recipient,
                    outputAmount
                );
                return;
            }
        }

        // No swap has been performed; disburse the original token directly to the recipient.
        SafeERC20.safeTransfer(IERC20(tokenAddress), recipient, tokenAmount);
    }

    function parsePositionOpenExecuteStrategyInstructionCommonFields(
        CrossChainContext storage self,
        WormholeCoreBridge.VM memory instructionVM,
        bytes[] calldata encodedTokenTransferVMs
    )
        internal
        returns (
            uint256 index,
            uint128 positionId,
            uint16 strategyChainId,
            AssetInfo[] memory assetInfos,
            bytes memory encodedData
        )
    {
        index = 2;

        positionId = instructionVM.payload.toUint128(index);
        index += 16;

        strategyChainId = instructionVM.payload.toUint16(index);
        index += 2;
        require(
            strategyChainId ==
                WormholeTokenBridge(self.wormholeContext.tokenBridge).chainId(),
            "strategyChainId mismatch"
        );

        uint256 assetInfoLen = uint256(instructionVM.payload.toUint32(index));
        index += 4;

        require(
            assetInfoLen == encodedTokenTransferVMs.length,
            "num tokens mismatch"
        );
        assetInfos = new AssetInfo[](assetInfoLen);
        for (uint256 i = 0; i < assetInfoLen; ++i) {
            uint64 sequence = instructionVM.payload.toUint64(index);
            index += 8;

            // TODO: Look into whether an Wormhole incoming transfer could unwrap to native ether.
            assetInfos[i].assetType = AssetType.Token;
            (
                assetInfos[i].assetAddr,
                assetInfos[i].amount
            ) = validateAndCompleteIncomingTokenTransfer(
                WormholeTokenBridge(self.wormholeContext.tokenBridge),
                encodedTokenTransferVMs[i],
                instructionVM.emitterChainId,
                sequence
            );
        }

        uint256 encodedDataLen = instructionVM.payload.toUint32(index);
        index += 4;

        encodedData = instructionVM.payload.slice(index, encodedDataLen);
        index += encodedDataLen;
    }

    function parsePositionOpenInstruction(
        CrossChainContext storage self,
        WormholeCoreBridge.VM memory instructionVM,
        bytes[] calldata encodedTokenTransferVMs
    )
        external
        returns (
            uint128 positionId,
            uint16 strategyChainId,
            uint64 strategyId,
            AssetInfo[] memory assetInfos,
            bytes memory encodedPositionOpenData
        )
    {
        uint256 index;
        (
            index,
            positionId,
            strategyChainId,
            assetInfos,
            encodedPositionOpenData
        ) = parsePositionOpenExecuteStrategyInstructionCommonFields(
            self,
            instructionVM,
            encodedTokenTransferVMs
        );

        strategyId = instructionVM.payload.toUint64(index);
        index += 8;
        require(index == instructionVM.payload.length, "invalid payload");
    }

    function parseExecuteStrategyInstruction(
        CrossChainContext storage self,
        WormholeCoreBridge.VM memory instructionVM,
        bytes[] calldata encodedTokenTransferVMs
    )
        external
        returns (
            uint128 positionId,
            uint16 strategyChainId,
            AssetInfo[] memory assetInfos,
            bytes memory encodedActionData
        )
    {
        uint256 index;
        (
            index,
            positionId,
            strategyChainId,
            assetInfos,
            encodedActionData
        ) = parsePositionOpenExecuteStrategyInstructionCommonFields(
            self,
            instructionVM,
            encodedTokenTransferVMs
        );
        require(index == instructionVM.payload.length, "invalid payload");
    }

    function decodeInstructionVM(
        CrossChainContext storage self,
        bytes calldata encodedInstructionVM
    )
        external
        returns (
            WormholeCoreBridge.VM memory instructionVM,
            uint8 instructionType
        )
    {
        // Parse and validate instruction VM.
        instructionVM = CrossChainLib.decodeWormholeVM(
            encodedInstructionVM,
            self.wormholeContext.coreBridge
        );
        require(
            self.chainIdToApertureManager[instructionVM.emitterChainId] ==
                instructionVM.emitterAddress,
            "unexpected emitterAddress"
        );
        require(
            !self.processedInstructions[instructionVM.hash],
            "ix already processed"
        );

        // Mark this instruction as processed so it cannot be replayed.
        self.processedInstructions[instructionVM.hash] = true;

        // Parse version / instruction type.
        // Note that Solidity checks array index for possible out of bounds, so there is no need for us to do so again.
        require(instructionVM.payload[0] == 0, "invalid instruction version");
        instructionType = uint8(instructionVM.payload[1]);
    }
}

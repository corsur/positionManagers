//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/ICrossChain.sol";
import "./interfaces/IWormhole.sol";

import "./libraries/BytesLib.sol";
import "./libraries/CurveRouterLib.sol";

/// @custom:oz-upgrades-unsafe-allow external-library-linking
contract ApertureManager is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable,
    ReentrancyGuardUpgradeable
{
    using SafeERC20 for IERC20;
    using BytesLib for bytes;
    using CurveRouterLib for CurveRouterContext;

    CurveRouterContext curveRouterContext;

    // isTokenWhitelistedForStrategy[chainId][strategyId][tokenAddress] represents whether the token is allowed for the specified strategy.
    mapping(uint16 => mapping(uint64 => mapping(address => bool)))
        private isTokenWhitelistedForStrategy;

    // Address of the Wormhole token bridge contract.
    address public WORMHOLE_TOKEN_BRIDGE;
    // Address of the Wormhole core bridge contract.
    address public WORMHOLE_CORE_BRIDGE;
    // Address of the cross chain contract.
    address public CROSS_CHAIN;

    // Information about positions held by users of this chain.
    uint128 public nextPositionId;
    mapping(uint128 => StoredPositionInfo) public positionIdToInfo;

    mapping(uint16 => bytes32) public chainIdToApertureManager;

    // Hashes of processed incoming Aperture instructions are stored in this mapping.
    mapping(bytes32 => bool) public processedInstructions;

    // Information about strategies managed by this Aperture manager.
    uint64 public nextStrategyId;
    mapping(uint64 => StrategyMetadata) public strategyIdToMetadata;

    modifier onlyPositionOwner(uint128 positionId) {
        require(positionIdToInfo[positionId].ownerAddr == msg.sender);
        _;
    }

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(address _crossChain) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        WORMHOLE_TOKEN_BRIDGE = ICrossChain(_crossChain).WORMHOLE_TOKEN_BRIDGE();
        WORMHOLE_CORE_BRIDGE = ICrossChain(_crossChain).WORMHOLE_CORE_BRIDGE();
        CROSS_CHAIN = _crossChain;
    }

    // Owner only.
    // Updates the Curve swap route for `fromToken` to `toToken` with `route`.
    // See CurveRouterLib.sol for more information.
    function updateCurveRoute(
        address fromToken,
        address toToken,
        CurveSwapOperation[] calldata route,
        address[] calldata tokens
    ) external onlyOwner {
        curveRouterContext.updateRoute(fromToken, toToken, route, tokens);
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function addStrategy(
        string calldata _name,
        string calldata _version,
        address _strategyManager
    ) external onlyOwner {
        uint64 strategyId = nextStrategyId++;
        strategyIdToMetadata[strategyId] = StrategyMetadata(
            _name,
            _version,
            _strategyManager
        );
    }

    function removeStrategy(uint64 _strategyId) external onlyOwner {
        delete strategyIdToMetadata[_strategyId];
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

    function validateAndTransferAssetFromSender(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] calldata assetInfos
    ) internal {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            if (assetInfos[i].assetType == AssetType.NativeToken) {
                require(
                    msg.value == assetInfos[i].amount,
                    "insufficient msg.value"
                );
            } else {
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

    // Approve target contract (strategy manager/cross chain) to move assets from `this`.
    function approveAssetTransferToTarget(
        AssetInfo[] memory assetInfos,
        address target
    ) internal {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            if (assetInfos[i].assetType != AssetType.NativeToken) {
                IERC20(assetInfos[i].assetAddr).approve(
                    target,
                    assetInfos[i].amount
                );
            }
        }
    }

    function createPositionInternal(
        uint128 positionId,
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).chainId()
        ) {
            require(
                chainIdToApertureManager[strategyChainId] != 0,
                "unexpected strategyChainId"
            );
            approveAssetTransferToTarget(assetInfos, CROSS_CHAIN);
            ICrossChain(CROSS_CHAIN).publishPositionOpenInstruction(
                ICrossChain.StrategyChainInfo(
                    chainIdToApertureManager[strategyChainId],
                    strategyChainId
                ),
                strategyId,
                positionId,
                assetInfos,
                encodedPositionOpenData
            );
        } else {
            StrategyMetadata memory strategy = strategyIdToMetadata[strategyId];
            require(
                strategy.strategyManager != address(0),
                "invalid strategyId"
            );

            // Approve strategy manager to move assets.
            for (uint256 i = 0; i < assetInfos.length; i++) {
                address assetAddr = assetInfos[i].assetAddr;
                uint256 amount = assetInfos[i].amount;
                IERC20(assetAddr).approve(strategy.strategyManager, amount);
            }
            approveAssetTransferToTarget(assetInfos, strategy.strategyManager);

            IStrategyManager(strategy.strategyManager).openPosition{
                value: msg.value
            }(
                PositionInfo(positionId, strategyChainId),
                encodedPositionOpenData
            );
        }
    }

    function createPosition(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedPositionOpenData
    ) external payable {
        uint128 positionId = recordNewPositionInfo(strategyChainId, strategyId);
        validateAndTransferAssetFromSender(
            strategyChainId,
            strategyId,
            assetInfos
        );
        createPositionInternal(
            positionId,
            strategyChainId,
            strategyId,
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
        uint256 toTokenAmount = curveRouterContext.swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(AssetType.Token, toToken, toTokenAmount);
        createPositionInternal(
            positionId,
            strategyChainId,
            strategyId,
            assetInfos,
            encodedPositionOpenData
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
        executeStrategyInternal(
            positionId,
            strategyChainId,
            assetInfos,
            encodedActionData
        );
    }

    function executeStrategyInternal(
        uint128 positionId,
        uint16 strategyChainId,
        AssetInfo[] calldata assetInfos,
        bytes calldata encodedActionData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).chainId()
        ) {
            require(
                chainIdToApertureManager[strategyChainId] != 0,
                "unexpected strategyChainId"
            );
            approveAssetTransferToTarget(assetInfos, CROSS_CHAIN);
            ICrossChain(CROSS_CHAIN).publishExecuteStrategyInstruction(
                ICrossChain.StrategyChainInfo(
                    chainIdToApertureManager[strategyChainId],
                    strategyChainId
                ),
                positionId,
                assetInfos,
                encodedActionData
            );
        } else {
            StrategyMetadata memory strategy = strategyIdToMetadata[
                positionIdToInfo[positionId].strategyId
            ];
            require(
                strategy.strategyManager != address(0),
                "invalid strategyId"
            );
            // Parse action based on encodedActionData.
            require(encodedActionData.length > 0, "invalid encodedActionData");
            Action action = Action(uint8(encodedActionData[0]));
            require(action != Action.Open, "invalid action");
            if (action == Action.Increase) {
                IStrategyManager(strategy.strategyManager).increasePosition{
                    value: msg.value
                }(
                    PositionInfo(positionId, strategyChainId),
                    encodedActionData[1:]
                );
            } else if (action == Action.Decrease) {
                IStrategyManager(strategy.strategyManager).decreasePosition(
                    PositionInfo(positionId, strategyChainId),
                    encodedActionData[1:]
                );
            } else {
                revert("invalid action");
            }
        }
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
        uint256 toTokenAmount = curveRouterContext.swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(AssetType.Token, toToken, toTokenAmount);
        ICrossChain(CROSS_CHAIN).publishExecuteStrategyInstruction(
            ICrossChain.StrategyChainInfo(
                chainIdToApertureManager[
                    positionIdToInfo[positionId].strategyChainId
                ],
                positionIdToInfo[positionId].strategyChainId
            ),
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
            "ix already processed"
        );

        // Mark this instruction as processed so it cannot be replayed.
        processedInstructions[instructionVM.hash] = true;

        // Parse version / instruction type.
        // Note that Solidity checks array index for possible out of bounds, so there is no need for us to do so again.
        require(instructionVM.payload[0] == 0, "invalid instruction version");
        uint8 instructionType = uint8(instructionVM.payload[1]);
        if (instructionType == INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT) {
            processSingleTokenDisbursementInstruction(
                instructionVM,
                encodedTokenTransferVMs
            );
        }
        /*else if (instructionType == INSTRUCTION_TYPE_POSITION_OPEN) {
            revert("INSTRUCTION_TYPE_POSITION_OPEN about to be supported");
        } else if (instructionType == INSTRUCTION_TYPE_EXECUTE_STRATEGY) {
            revert("INSTRUCTION_TYPE_EXECUTE_STRATEGY about to be supported");
        } */
        else {
            revert("invalid ix type");
        }
    }
}

//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "./interfaces/IApertureCommon.sol";
import "./interfaces/IWormhole.sol";

import "./libraries/BytesLib.sol";
import "./libraries/CrossChainLib.sol";
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
    using CrossChainLib for CrossChainContext;
    using CurveRouterLib for CurveRouterContext;

    CrossChainContext public crossChainContext;
    CurveRouterContext curveRouterContext;

    // isTokenWhitelistedForStrategy[chainId][strategyId][tokenAddress] represents whether the token is allowed for the specified strategy.
    mapping(uint16 => mapping(uint64 => mapping(address => bool)))
        private isTokenWhitelistedForStrategy;

    // Information about positions held by users of this chain.
    uint128 public nextPositionId;
    mapping(uint128 => StoredPositionInfo) public positionIdToInfo;

    // Information about strategies managed by this Aperture manager.
    uint64 public nextStrategyId;
    mapping(uint64 => StrategyMetadata) public strategyIdToMetadata;

    // Whether an address is allowed to call `disburseAssets()`.
    mapping(address => bool) public allowedToDisburseAssets;

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        WormholeContext calldata wormholeContext,
        CrossChainFeeContext calldata crossChainFeeContext
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        crossChainContext.updateWormholeContext(wormholeContext);
        crossChainContext.feeContext = crossChainFeeContext;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function updateWormholeCrossChainContext(
        WormholeContext calldata newWormholeContext
    ) external onlyOwner {
        crossChainContext.updateWormholeContext(newWormholeContext);
    }

    function updateCrossChainFeeContext(
        CrossChainFeeContext calldata crossChainFeeContext
    ) external onlyOwner {
        crossChainContext.validateAndUpdateFeeContext(crossChainFeeContext);
    }

    // Sets a new Aperture manager address for the specified chain.
    // To remove a manager from the registry, set `managerAddress` to zero.
    function updateApertureManager(uint16 chainId, bytes32 managerAddress)
        external
        onlyOwner
    {
        // Chain ID must be positive and not the ID of this chain.
        require(
            chainId > 0 &&
                chainId !=
                WormholeCoreBridge(
                    crossChainContext.inferredWormholeContext.coreBridge
                ).chainId(),
            "invalid chainId"
        );
        crossChainContext.chainIdToApertureManager[chainId] = managerAddress;
    }

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
        allowedToDisburseAssets[_strategyManager] = true;
    }

    function removeStrategy(uint64 _strategyId) external onlyOwner {
        delete allowedToDisburseAssets[
            strategyIdToMetadata[_strategyId].strategyManager
        ];
        delete strategyIdToMetadata[_strategyId];
    }

    // Sets whether `tokenAddress` is whitelisted for the specified strategy.
    function updateIsTokenWhitelistedForStrategy(
        uint16 chainId,
        uint64 strategyId,
        address tokenAddress,
        bool isWhitelisted
    ) external onlyOwner {
        require(tokenAddress != address(0), "zero tokenAddress");
        isTokenWhitelistedForStrategy[chainId][strategyId][
            tokenAddress
        ] = isWhitelisted;
    }

    modifier onlyPositionOwner(uint128 positionId) {
        require(positionIdToInfo[positionId].ownerAddr == msg.sender);
        _;
    }

    function validateAndTransferAssetFromSender(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] memory assetInfos
    ) internal returns (AssetInfo[] memory) {
        for (uint256 i = 0; i < assetInfos.length; i++) {
            require(
                isTokenWhitelistedForStrategy[strategyChainId][strategyId][
                    assetInfos[i].assetAddr
                ],
                "token not allowed"
            );
            require(assetInfos[i].amount > 0, "zero amount");
            IERC20(assetInfos[i].assetAddr).safeTransferFrom(
                msg.sender,
                address(this),
                assetInfos[i].amount
            );
        }

        // This message carries some ether, so we wrap it to WETH and add it to `assetInfos`.
        if (msg.value > 0) {
            require(
                isTokenWhitelistedForStrategy[strategyChainId][strategyId][
                    crossChainContext.inferredWormholeContext.weth
                ],
                "weth not allowed"
            );
            return crossChainContext.wrapEtherAndExpandAssetInfos(assetInfos);
        }
        return assetInfos;
    }

    function recordNewPositionInfo(uint16 strategyChainId, uint64 strategyId)
        internal
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

    function createPositionInternal(
        uint128 positionId,
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] memory assetInfos,
        bytes memory encodedPositionOpenData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(
                crossChainContext.inferredWormholeContext.coreBridge
            ).chainId()
        ) {
            crossChainContext.publishPositionOpenInstruction(
                strategyChainId,
                strategyId,
                positionId,
                assetInfos,
                encodedPositionOpenData
            );
        } else {
            StrategyMetadata storage strategy = strategyIdToMetadata[
                strategyId
            ];
            require(
                strategy.strategyManager != address(0),
                "invalid strategyId"
            );

            // Approve strategy manager to transfer these assets out.
            for (uint256 i = 0; i < assetInfos.length; i++) {
                if (assetInfos[i].assetAddr != address(0)) {
                    IERC20(assetInfos[i].assetAddr).approve(
                        strategy.strategyManager,
                        assetInfos[i].amount
                    );
                }
            }

            IStrategyManager(strategy.strategyManager).openPosition(
                PositionInfo(positionId, strategyChainId),
                assetInfos,
                encodedPositionOpenData
            );
        }
    }

    function createPosition(
        uint16 strategyChainId,
        uint64 strategyId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) external payable nonReentrant {
        uint128 positionId = recordNewPositionInfo(strategyChainId, strategyId);
        assetInfos = validateAndTransferAssetFromSender(
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
    ) external nonReentrant {
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
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        createPositionInternal(
            positionId,
            strategyChainId,
            strategyId,
            assetInfos,
            encodedPositionOpenData
        );
    }

    function executeStrategyInternal(
        uint128 positionId,
        uint16 strategyChainId,
        AssetInfo[] memory assetInfos,
        bytes memory encodedActionData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(
                crossChainContext.inferredWormholeContext.coreBridge
            ).chainId()
        ) {
            crossChainContext.publishExecuteStrategyInstruction(
                strategyChainId,
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
            // Note that Solidity checks array index for possible out of bounds, so there is no need to validate encodedActionData.length.
            Action action = Action(uint8(encodedActionData[0]));
            if (action == Action.Increase) {
                IStrategyManager(strategy.strategyManager).increasePosition(
                    PositionInfo(positionId, strategyChainId),
                    assetInfos,
                    encodedActionData.slice(1, encodedActionData.length - 1)
                );
            } else if (action == Action.Decrease || action == Action.Close) {
                // encodedActionData:
                // Index 0: action enum.
                // Index 1~2: recipient.chainId (uint16).
                // Index 3~34: recipient.recipientAddr (bytes32).
                // Index 35 and later: strategy-specific data.

                // Parse recipient from `encodedActionData`.
                Recipient memory recipient;
                recipient.chainId = encodedActionData.toUint16(1);
                recipient.recipientAddr = encodedActionData.toBytes32(3);

                // Strategy-specific encoded params.
                bytes memory data = encodedActionData.slice(
                    35,
                    encodedActionData.length - 35
                );

                if (action == Action.Decrease) {
                    IStrategyManager(strategy.strategyManager).decreasePosition(
                            PositionInfo(positionId, strategyChainId),
                            recipient,
                            data
                        );
                } else {
                    IStrategyManager(strategy.strategyManager).closePosition(
                        PositionInfo(positionId, strategyChainId),
                        recipient,
                        data
                    );
                }
            } else {
                revert("invalid action");
            }
        }
    }

    function executeStrategy(
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedActionData
    ) external payable nonReentrant onlyPositionOwner(positionId) {
        uint16 strategyChainId = positionIdToInfo[positionId].strategyChainId;
        assetInfos = validateAndTransferAssetFromSender(
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

    function swapTokenAndExecuteStrategy(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        uint128 positionId,
        bytes calldata encodedActionData
    ) external nonReentrant onlyPositionOwner(positionId) {
        IERC20(fromToken).safeTransferFrom(msg.sender, address(this), amount);
        uint256 toTokenAmount = curveRouterContext.swapToken(
            fromToken,
            toToken,
            amount,
            minAmountOut
        );
        AssetInfo[] memory assetInfos = new AssetInfo[](1);
        assetInfos[0] = AssetInfo(toToken, toTokenAmount);
        executeStrategyInternal(
            positionId,
            positionIdToInfo[positionId].strategyChainId,
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

    function processApertureInstruction(
        bytes calldata encodedInstructionVM,
        bytes[] calldata encodedTokenTransferVMs
    ) external nonReentrant {
        (
            WormholeCoreBridge.VM memory instructionVM,
            uint8 instructionType
        ) = crossChainContext.decodeInstructionVM(encodedInstructionVM);
        if (instructionType == INSTRUCTION_TYPE_SINGLE_TOKEN_DISBURSEMENT) {
            crossChainContext.processSingleTokenDisbursementInstruction(
                instructionVM,
                encodedTokenTransferVMs,
                curveRouterContext
            );
        } else if (
            instructionType == INSTRUCTION_TYPE_MULTI_TOKEN_DISBURSEMENT
        ) {
            crossChainContext.processMultiTokenDisbursementInstruction(
                instructionVM,
                encodedTokenTransferVMs
            );
        } else if (instructionType == INSTRUCTION_TYPE_POSITION_OPEN) {
            (
                uint128 positionId,
                uint16 strategyChainId,
                uint64 strategyId,
                AssetInfo[] memory assetInfos,
                bytes memory encodedPositionOpenData
            ) = crossChainContext.parsePositionOpenInstruction(
                    instructionVM,
                    encodedTokenTransferVMs
                );
            createPositionInternal(
                positionId,
                strategyChainId,
                strategyId,
                assetInfos,
                encodedPositionOpenData
            );
        } else if (instructionType == INSTRUCTION_TYPE_EXECUTE_STRATEGY) {
            (
                uint128 positionId,
                uint16 strategyChainId,
                AssetInfo[] memory assetInfos,
                bytes memory encodedActionData
            ) = crossChainContext.parseExecuteStrategyInstruction(
                    instructionVM,
                    encodedTokenTransferVMs
                );
            executeStrategyInternal(
                positionId,
                strategyChainId,
                assetInfos,
                encodedActionData
            );
        } else {
            revert("invalid ix type");
        }
    }

    function disburseAssets(
        AssetInfo[] memory assetInfos,
        Recipient calldata recipient
    ) external payable {
        require(
            allowedToDisburseAssets[msg.sender],
            "sender not allowed to disburse"
        );
        if (
            recipient.chainId ==
            crossChainContext.inferredWormholeContext.thisChainId
        ) {
            address to = address(uint160(uint256(recipient.recipientAddr)));
            for (uint256 i = 0; i < assetInfos.length; ++i) {
                IERC20(assetInfos[i].assetAddr).safeTransfer(
                    to,
                    assetInfos[i].amount
                );
            }
            if (msg.value > 0) {
                payable(to).transfer(msg.value);
            }
        } else {
            crossChainContext.publishMultiTokenDisbursementInstruction(
                assetInfos,
                recipient
            );
        }
    }
}

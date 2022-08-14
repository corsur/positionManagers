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

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        WormholeCrossChainContext calldata wormholeCrossChainContext,
        CrossChainFeeContext calldata crossChainFeeContext
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        crossChainContext.wormholeContext = wormholeCrossChainContext;
        crossChainContext.feeContext = crossChainFeeContext;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function updateWormholeCrossChainContext(
        WormholeCrossChainContext calldata wormholeCrossChainContext
    ) external onlyOwner {
        crossChainContext.wormholeContext = wormholeCrossChainContext;
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
                WormholeCoreBridge(crossChainContext.wormholeContext.coreBridge)
                    .chainId(),
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
    }

    function removeStrategy(uint64 _strategyId) external onlyOwner {
        delete strategyIdToMetadata[_strategyId];
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

    modifier onlyPositionOwner(uint128 positionId) {
        require(positionIdToInfo[positionId].ownerAddr == msg.sender);
        _;
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
        bytes memory encodedPositionOpenData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(crossChainContext.wormholeContext.coreBridge)
                .chainId()
        ) {
            crossChainContext.publishPositionOpenInstruction(
                strategyChainId,
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

    function executeStrategyInternal(
        uint128 positionId,
        uint16 strategyChainId,
        AssetInfo[] memory assetInfos,
        bytes memory encodedActionData
    ) internal {
        if (
            strategyChainId !=
            WormholeCoreBridge(crossChainContext.wormholeContext.coreBridge)
                .chainId()
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
            require(action != Action.Open, "invalid action");
            if (action == Action.Increase) {
                IStrategyManager(strategy.strategyManager).increasePosition{
                    value: msg.value
                }(
                    PositionInfo(positionId, strategyChainId),
                    encodedActionData.slice(1, encodedActionData.length - 1)
                );
            } else if (action == Action.Decrease) {
                IStrategyManager(strategy.strategyManager).decreasePosition(
                    PositionInfo(positionId, strategyChainId),
                    encodedActionData.slice(1, encodedActionData.length - 1)
                );
            } else {
                revert("invalid action");
            }
        }
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
    ) external {
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
}

//SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;
import "hardhat/console.sol";

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "./interfaces/IWormhole.sol";
import "./interfaces/IApertureCommon.sol";
import "./interfaces/ICrossChain.sol";
import "./libraries/BytesLib.sol";

contract CrossChain is Ownable, ICrossChain {
    using SafeERC20 for IERC20;
    using BytesLib for bytes;

    address public MANAGER;

    uint256 private constant BPS = 10000;
    // The maximum allowed CROSS_CHAIN_FEE_BPS value (100 basis points, i.e. 1%).
    uint32 private constant MAX_CROSS_CHAIN_FEE_BPS = 100;

    // Nonce does not play a meaningful role as sequence numbers distingish different messages emitted by the same address.
    uint32 private constant WORMHOLE_NONCE = 0;

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

    constructor(
        uint8 _consistencyLevel,
        address _wormholeTokenBridge,
        uint32 _crossChainFeeBPS,
        address _feeSink
    ) {
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

    modifier onlyManager {
        require(msg.sender == MANAGER, "only manager contract allowed");
        _;
    }

    function updateManager(address manager) 
        external
        onlyOwner
    {
        require(manager != address(0), "manager address must be non-zero");
        MANAGER = manager;
    }

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

    function sendTokensCrossChainAndConstructCommonPayload(
        StrategyChainInfo memory strategyChainInfo,
        uint8 instructionType,
        AssetInfo[] memory assetInfos,
        uint128 positionId,
        bytes calldata encodedData
    ) internal returns (bytes memory) {
        bytes memory payload = abi.encodePacked(
            INSTRUCTION_VERSION,
            instructionType,
            positionId,
            strategyChainInfo.chainId,
            uint32(assetInfos.length)
        );
        for (uint256 i = 0; i < assetInfos.length; i++) {
            if (assetInfos[i].assetType == AssetType.NativeToken) {
                revert("unsupported cross-chain native token");
            }

            // Transfer token from sender. 
            IERC20(assetInfos[i].assetAddr).safeTransferFrom(
                    msg.sender,
                    address(this),
                    assetInfos[i].amount
                );
            
            // Collect cross-chain fees if applicable.
            AmountAndFee memory amountAndFee = AmountAndFee(
                assetInfos[i].amount,
                (assetInfos[i].amount * CROSS_CHAIN_FEE_BPS) / BPS
            );
            if (amountAndFee.crossChainFee > 0) {
                IERC20(assetInfos[i].assetAddr).safeTransfer(
                    FEE_SINK,
                    amountAndFee.crossChainFee
                );
                amountAndFee.amount -= amountAndFee.crossChainFee;
            }

            // Allow wormhole token bridge contract to transfer this token out of here.
            IERC20(assetInfos[i].assetAddr).safeIncreaseAllowance(
                WORMHOLE_TOKEN_BRIDGE,
                amountAndFee.amount
            );

            // Initiate token transfer.
            uint64 transferSequence = WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE)
                .transferTokens(
                    assetInfos[i].assetAddr,
                    amountAndFee.amount,
                    strategyChainInfo.chainId,
                    strategyChainInfo.managerAddr,
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
        StrategyChainInfo memory strategyChainInfo,
        uint64 strategyId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) external onlyManager {
        // Initiate token transfers and construct partial instruction payload.
        bytes
            memory partial_payload = sendTokensCrossChainAndConstructCommonPayload(
                strategyChainInfo,
                INSTRUCTION_TYPE_POSITION_OPEN,
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

    function publishExecuteStrategyInstruction(
        StrategyChainInfo memory strategyChainInfo,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedActionData
    ) external onlyManager {
        WormholeCoreBridge(WORMHOLE_CORE_BRIDGE).publishMessage(
            WORMHOLE_NONCE,
            sendTokensCrossChainAndConstructCommonPayload(
                strategyChainInfo,
                INSTRUCTION_TYPE_EXECUTE_STRATEGY,
                assetInfos,
                positionId,
                encodedActionData
            ),
            CONSISTENCY_LEVEL
        );
    }
}

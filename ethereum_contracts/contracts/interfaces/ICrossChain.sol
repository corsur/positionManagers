// SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "./IApertureCommon.sol";

interface ICrossChain {
    struct StrategyChainInfo {
        bytes32 managerAddr;
        uint16 chainId;
    }
    struct AmountAndFee {
        uint256 amount;
        uint256 crossChainFee;
    }
    function updateCrossChainFeeBPS(uint32 crossChainFeeBPS) external;
    function updateFeeSink(address feeSink) external;
    function updateManager(address manager) external;
    function publishPositionOpenInstruction(
        StrategyChainInfo memory strategyChainInfo,
        uint64 strategyId,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedPositionOpenData
    ) external;
    function publishExecuteStrategyInstruction(
        StrategyChainInfo memory strategyChainInfo,
        uint128 positionId,
        AssetInfo[] memory assetInfos,
        bytes calldata encodedActionData
    ) external;
    function WORMHOLE_TOKEN_BRIDGE() external returns (address);
    function WORMHOLE_CORE_BRIDGE() external returns (address);
}

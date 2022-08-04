//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

enum Action {
    Open,
    Increase,
    Decrease,
    Close
}

struct StoredPositionInfo {
    address ownerAddr;
    uint16 strategyChainId;
    uint64 strategyId;
}

struct PositionInfo {
    uint128 positionId; // The EVM position id.
    uint16 chainId; // Chain id, following Wormhole's design.
}

struct StrategyMetadata {
    string name;
    string version;
    address strategyManager;
}

struct Config {
    uint32 crossChainFeeBPS; // Cross-chain fee in bps.
    address feeSink; // Fee collecting address.
}

struct AssetInfo {
    address assetAddr; // The ERC20 address.
    uint256 amount;
}

interface IStrategyManager {
    function openPosition(
        address recipient,
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable;

    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable;

    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external;
}

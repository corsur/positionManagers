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
    /// @dev Open a new Aperture position for `recipient`
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
    function openPosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable;

    /// @dev Increase an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Amount of assets supplied by user and minimum equity received after adding liquidity, etc
    function increasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external payable;

    /// @dev Decrease an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: The recipient, the amount of shares to withdraw and the minimum amount of assets returned, etc
    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external;

    /// @dev Close an existing Aperture position
    /// @param position_info: Aperture position info
    /// @param data: Owner of the position on Aperture and the minimum amount of assets returned, etc
    function closePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external;
}

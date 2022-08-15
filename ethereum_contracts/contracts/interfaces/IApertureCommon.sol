//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

// Actions that can be taken on an existing Aperture position.
enum Action {
    Invalid, // The zero value is reserved for representing an invalid action.
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

// The address and amount of an ERC-20 token.
struct AssetInfo {
    address assetAddr;
    uint256 amount;
}

struct Recipient {
    uint16 chainId;
    bytes32 recipientAddr;
}

interface IStrategyManager {
    /// @dev Open a new Aperture position.
    /// @param position_info: Aperture position info.
    /// @param assets: Information about assets to open this position with.
    /// @param data: Generic bytes encoding strategy-specific params.
    function openPosition(
        PositionInfo memory position_info,
        AssetInfo[] calldata assets,
        bytes calldata data
    ) external;

    /// @dev Increase an existing Aperture position.
    /// @param position_info: Aperture position info.
    /// @param assets: Information about assets to increase this position with.
    /// @param data: Generic bytes encoding strategy-specific params.
    function increasePosition(
        PositionInfo memory position_info,
        AssetInfo[] calldata assets,
        bytes calldata data
    ) external;

    /// @dev Decrease an existing Aperture position.
    /// @param position_info: Aperture position info.
    /// @param recipient: Chain id and address of the recipient.
    /// @param data: Generic bytes encoding strategy-specific params.
    function decreasePosition(
        PositionInfo memory position_info,
        Recipient calldata recipient,
        bytes calldata data
    ) external;

    /// @dev Close an existing Aperture position.
    /// @param position_info: Aperture position info.
    /// @param recipient: Chain id and address of the recipient.
    /// @param data: Generic bytes encoding strategy-specific params.
    function closePosition(
        PositionInfo memory position_info,
        Recipient calldata recipient,
        bytes calldata data
    ) external;
}

interface IApertureManager {
    function disburseAssets(
        AssetInfo[] memory assetInfos,
        Recipient calldata recipient
    ) external payable;
}

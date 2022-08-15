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

struct Config {
    uint32 crossChainFeeBPS; // Cross-chain fee in bps.
    address feeSink; // Fee collecting address.
}

// The address and amount of an ERC-20 token.
struct AssetInfo {
    address assetAddr;
    uint256 amount;
}

struct ApertureFeeConfig {
    uint256 withdrawFee; // multiplied by 1e4
    uint256 harvestFee; // multiplied by 1e4
    uint256 managementFee; // multiplied by 1e4
}

struct ApertureVaultLimits {
    uint256 maxCapacity; // Maximum amount allowed in stable across the vault
    uint256 maxOpenPerTx; // Maximum amount allowed in stable to add in one transaction
    uint256 maxWithdrawPerTx; // Maximum amount allowed in stable to withdraw in one transaction
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
    /// @param position_info: Aperture position info
    /// @param data: Generic bytes encoding strategy-specific params.
    function decreasePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external;

    /// @dev Close an existing Aperture position.
    /// @param position_info: Aperture position info
    /// @param data: Generic bytes encoding strategy-specific params.
    function closePosition(
        PositionInfo memory position_info,
        bytes calldata data
    ) external;
}

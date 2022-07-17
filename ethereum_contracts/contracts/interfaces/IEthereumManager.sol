//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

struct StoredPositionInfo {
    address ownerAddr;
    uint16 strategyChainId;
    uint64 strategyId;
}

// Only used by the getPositions() view function.
struct PositionInfo {
    uint128 positionId; // The EVM position id.
    uint16 chainId; // Chain id, following Wormhole's design.
}

struct StrategyMetadata {
    string name;
    string version;
    address manager;
}

struct Config {
    uint32 crossChainFeeBPS; // Cross-chain fee in bpq.
    address feeSink; // Fee collecting address.
}

struct AssetInfo {
    address assetAddr; // The ERC20 address.
    uint256 amount;
}

/*
interface IDeltaNeutralInvest {
    function openPosition(bytes calldata positionData)
        external
        returns (uint128 positionId);

    function updatePosition(uint128 _positionId, bytes memory position_)
        external;

    function removePosition(uint128 positionId) external;

    function getPosition(uint128 positionId)
        external
        view
        returns (bytes memory);
}
*/

// SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.12;

interface ICurve {
    function N_COINS() external view returns (int128);

    function BASE_N_COINS() external view returns (int128);

    function coins(uint256 i) external view returns (address);

    function base_coins(uint256 i) external view returns (address);

    function get_dy(
        int128 i,
        int128 j,
        uint256 dx
    ) external view returns (uint256);

    function get_dy_underlying(
        int128 i,
        int128 j,
        uint256 dx
    ) external view returns (uint256);

    function exchange(
        int128 i,
        int128 j,
        uint256 dx,
        uint256 min_dy
    ) external returns (uint256);

    function exchange_underlying(
        int128 i,
        int128 j,
        uint256 dx,
        uint256 min_dy
    ) external returns (uint256);
}

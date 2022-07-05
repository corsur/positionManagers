// SPDX-License-Identifier: agpl-3.0
pragma solidity ^0.8.13;

import "../libraries/AaveV3DataTypes.sol";

interface IAaveV3 {
    function supply(
        address asset,
        uint256 amount,
        address onBehalfOf,
        uint16 referralCode
    ) external;

    function withdraw(
        address asset,
        uint256 amount,
        address to
    ) external;

    function depositETH(
        address pool,
        address onBehalfOf,
        uint16 referralCode
    ) external payable;

    function withdrawETH(
        address pool,
        uint256 amount,
        address to
    ) external payable;

    function getReserveData(address)
        external
        view
        returns (AaveV3DataTypes.ReserveData memory);
}

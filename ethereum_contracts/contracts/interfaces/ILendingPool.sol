pragma solidity ^0.8.13;

import "../libraries/AaveV2DataTypes.sol";

interface ILendingPool {
    function deposit(
        address asset,
        uint256 amount,
        address onBehalfOf,
        uint16 referralCode
    ) external;

    function getReserveData(address asset)
        external
        view
        returns (AaveV2DataTypes.ReserveData memory);
}

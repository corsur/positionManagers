// SPDX-License-Identifier: BSD-3-Clause
pragma solidity ^0.8.13;

interface CErc20 {
    function mint(uint256) external returns (uint256);

    function exchangeRateCurrent() external returns (uint256);

    function exchangeRateStored() external view returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemUnderlying(uint256) external returns (uint256);

    function balanceOf(address) external view returns (uint256);

    function balanceOfUnderlying(address) external returns (uint256);
}

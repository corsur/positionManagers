// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

interface JWrappedNative {
    function mint(uint256) external returns (uint256);

    function mintNative() external payable returns (uint256);

    function exchangeRateCurrent() external returns (uint256);

    function exchangeRateStored() external view returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemNative(uint256) external returns (uint256);

    function redeemUnderlying(uint256) external returns (uint256);

    function redeemUnderlyingNative(uint256) external returns (uint256);

    function balanceOf(address) external view returns (uint256);

    function balanceOfUnderlying(address) external returns (uint256);
}

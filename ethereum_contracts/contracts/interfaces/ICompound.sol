// SPDX-License-Identifier: BSD-3-Clause
pragma solidity >=0.8.0 <0.9.0;

interface ICompound {
    function mint(uint256) external returns (uint256);

    function mint() external payable;

    function mintNative() external payable returns (uint256);

    function exchangeRateCurrent() external returns (uint256);

    function exchangeRateStored() external view returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function supplyRatePerTimestamp() external returns (uint256);

    function supplyRatePerSecond() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemNative(uint256) external returns (uint256);

    function redeemUnderlying(uint256) external returns (uint256);

    function redeemUnderlyingNative(uint256) external returns (uint256);

    function balanceOf(address) external view returns (uint256);

    function balanceOfUnderlying(address) external returns (uint256);
}

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

interface SafeBoxAvax {
    function deposit() external payable;

    function withdraw(uint256 amount) external;

    function balanceOf(address) external returns (uint256);
}

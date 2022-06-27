// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

interface SafeBox {
    function deposit(uint256 amount) external;

    function withdraw(uint256 amount) external;

    function balanceOf(address) external returns (uint256);
}

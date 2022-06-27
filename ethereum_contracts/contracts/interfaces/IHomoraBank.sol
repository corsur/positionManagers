//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

interface IHomoraBank {
    function execute(
        uint positionId,
        address spell,
        bytes memory data
    ) external payable returns (uint);
    function support(
        address token
    ) external view returns (bool);
}

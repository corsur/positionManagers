//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

interface IHomoraBank {

    function execute(
        uint positionId,
        address spell,
        bytes memory data
    ) external payable returns (uint);

    function getPositionInfo(
        uint positionId
    ) external view returns (
        address,
        address,
        uint,
        uint
    );

    function getPositionDebts(
        uint positionId
    ) external view returns (address[] memory, uint[] memory);
}

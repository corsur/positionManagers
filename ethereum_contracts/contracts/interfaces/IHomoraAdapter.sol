// SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

interface IHomoraAdapter {
    function doWork(
        address target,
        uint256 value,
        bytes calldata data
    ) external payable returns (bytes memory);

    function homoraExecute(
        uint256 positionId,
        address spell,
        uint256 value,
        bytes memory data
    ) external payable returns (uint256);
}

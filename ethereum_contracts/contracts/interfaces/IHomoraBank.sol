//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

interface IHomoraBank {
    function execute(
        uint256 positionId,
        address spell,
        bytes memory data
    ) external payable returns (uint256);

    function getPositionInfo(uint256 positionId)
        external
        view
        returns (
            address,
            address,
            uint256,
            uint256
        );

    function getPositionDebts(uint256 positionId)
        external
        view
        returns (address[] memory, uint256[] memory);

    function oracle() external view returns (address);

    function getCollateralETHValue(uint256 positionId)
        external
        view
        returns (uint256);

    function getBorrowETHValue(uint256 positionId)
        external
        view
        returns (uint256);

    function accrue(address token) external;
}

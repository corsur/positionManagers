//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

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
            address owner,
            address collToken,
            uint256 collId,
            uint256 collateralSize
        );
    
    function oracle() external view returns (address);
    function getCollateralETHValue(uint positionId) external view returns (uint);
    function getBorrowETHValue(uint positionId) external view returns (uint);
    function support(address token) external view returns (bool);
}

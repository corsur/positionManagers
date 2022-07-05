// SPDX-License-Identifier: MIT

pragma solidity >=0.6.12;

interface IOracle {
    function tokenFactors(address token)
        external
        view
        returns (
            uint16 borrowFactor,
            uint16 collateralFactor,
            uint16 liqIncentive
        );
}

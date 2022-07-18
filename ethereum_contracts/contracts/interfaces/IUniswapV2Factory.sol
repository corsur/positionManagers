// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.8.0 <0.9.0;

interface IFactory {
    function getPair(address tokenA, address tokenB)
        external
        view
        returns (address pair);
}

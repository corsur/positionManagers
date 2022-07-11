pragma solidity ^0.8.13;

interface ISpell {
    function pairs(address tokenA, address tokenB) 
      external
      view
      returns(address);
}
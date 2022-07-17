// SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

interface ICurveSwap {
    function simulateSwapToken(
        address fromToken,
        address toToken,
        uint256 amount
    ) external view returns (uint256);

    function swapToken(
        address fromToken,
        address toToken,
        uint256 amount,
        uint256 minAmountOut,
        address recipient
    ) external returns (uint256);
}

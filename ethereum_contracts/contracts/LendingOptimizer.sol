pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

interface CErc20 {
    function mint(uint256) external returns (uint256);

    function exchangeRateCurrent() external returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemUnderlying(uint256) external returns (uint256);
}

contract LendingOptimizer {
    using SafeERC20 for IERC20;

    function supplyTokenToCompound(uint256 amount) external returns (uint256) {
        address addr = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48; // USDC address
        address cAddr = 0x39AA39c021dfbaE8faC545936693aC917d5E7563; // cUSDC address

        IERC20 token = IERC20(addr);
        CErc20 cToken = CErc20(cAddr);

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve compound contract to mint from this contract
        token.safeApprove(cAddr, amount);

        return cToken.mint(amount);
    }

    function supplyTokenToAave(address token, uint256 amount) external {
        address addr = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48; // USDC address
    }

    function supply(address token, uint256 amount) external {}
}

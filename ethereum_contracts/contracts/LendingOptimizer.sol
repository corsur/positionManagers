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

interface ILendingPool {
    function deposit(
        address asset,
        uint256 amount,
        address onBehalfOf,
        uint16 referralCode
    ) external;
}

contract LendingOptimizer {
    using SafeERC20 for IERC20;

    function supplyTokenToCompound(uint256 amount) external {
        address addr = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48; // USDC address
        address cAddr = 0x39AA39c021dfbaE8faC545936693aC917d5E7563; // cUSDC address

        IERC20 token = IERC20(addr);
        CErc20 cToken = CErc20(cAddr);

        require(amount <= token.allowance(msg.sender, address(this)));

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve compound contract to transfer from this contract
        token.safeApprove(cAddr, amount);

        cToken.mint(amount);
    }

    function supplyTokenToAave(uint256 amount) external {
        address addr = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48; // USDC address
        address aaveLendingPoolAddr = 0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9;

        IERC20 token = IERC20(addr);
        ILendingPool pool = ILendingPool(aaveLendingPoolAddr);

        require(amount <= token.allowance(msg.sender, address(this)));

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve AAVE LendingPool contract to make a deposit
        token.safeApprove(aaveLendingPoolAddr, amount);

        pool.deposit(addr, amount, address(this), 0);
    }

    function supply(address token, uint256 amount) external {}
}

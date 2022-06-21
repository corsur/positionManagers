//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/CErc20.sol";
import "contracts/interfaces/ILendingPool.sol";
import "contracts/interfaces/CEth.sol";
import "contracts/interfaces/WETHGateway.sol";

import "./libraries/AaveV2DataTypes.sol";

contract LendingOptimizer is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable
{
    using SafeERC20 for IERC20;

    mapping(address => address) toC;

    address public CETH_ADDR;
    address public ILENDINGPOOL_ADDR;
    address public WETH_ADDR;
    address public WETHGATEWAY_ADDR;

    function initialize(
        address _cETHAddr,
        address _lendingPoolAddr,
        address _wethAddr,
        address _wethGatewayAddr
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        CETH_ADDR = _cETHAddr;
        ILENDINGPOOL_ADDR = _lendingPoolAddr;
        WETH_ADDR = _wethAddr;
        WETHGATEWAY_ADDR = _wethGatewayAddr;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function addCompoundTokenMapping(address tokenAddr, address cTokenAddr)
        external
        onlyOwner
    {
        toC[tokenAddr] = cTokenAddr;
    }

    function balanceErc20(address tokenAddr) external returns (uint256) {
        uint256 compoundBalance = CErc20(toC[tokenAddr]).balanceOfUnderlying(
            address(this)
        );
        IERC20 aToken = IERC20(
            ILendingPool(ILENDINGPOOL_ADDR)
                .getReserveData(tokenAddr)
                .aTokenAddress
        );
        uint256 aaveBalance = aToken.balanceOf(address(this));

        return compoundBalance + aaveBalance;
    }

    function balanceEth() external returns (uint256) {
        CEth cToken = CEth(CETH_ADDR);
        IERC20 aToken = IERC20(
            ILendingPool(ILENDINGPOOL_ADDR)
                .getReserveData(WETH_ADDR)
                .aTokenAddress
        );

        return
            cToken.balanceOfUnderlying(address(this)) +
            aToken.balanceOf(address(this));
    }

    function supplyTokenToCompound(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve compound contract to transfer from this contract
        token.safeApprove(toC[tokenAddr], amount);

        CErc20(toC[tokenAddr]).mint(amount);
    }

    function supplyTokenToAave(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve AAVE LendingPool contract to make a deposit
        token.safeApprove(ILENDINGPOOL_ADDR, amount);

        ILendingPool(ILENDINGPOOL_ADDR).deposit(
            tokenAddr,
            amount,
            address(this),
            /* referralCode= */
            0
        );
    }

    function supplyEth() external payable {
        CEth cToken = CEth(CETH_ADDR); // cETH
        uint256 cInterestAdj = cToken.supplyRatePerBlock() *
            6570 *
            365 *
            (10**9);

        uint256 aInterestAdj = ILendingPool(ILENDINGPOOL_ADDR)
            .getReserveData(WETH_ADDR)
            .currentLiquidityRate;

        if (cInterestAdj >= aInterestAdj) {
            cToken.mint{value: msg.value}();
        } else {
            WETHGateway(WETHGATEWAY_ADDR).depositETH{value: msg.value}(
                ILENDINGPOOL_ADDR,
                address(this),
                /* referralCode = */
                0
            );
        }
    }

    function supply(address tokenAddr, uint256 amount) external {
        require(
            toC[tokenAddr] != 0x0000000000000000000000000000000000000000 &&
                toC[tokenAddr] != address(0)
        );

        /*
          Compound interest rate APY:
          (((Rate / ETH Mantissa * Blocks Per Day + 1) ^ Days Per Year)) - 1.

          AAVE:
          ((1 + ((liquidityRate / RAY) / SECONDS_PER_YEAR)) ^ SECONDS_PER_YEAR) - 1.

          We simplify the inequality between the two formula by altering 
          the compounding term, making days per year to seconds per year 
          or vice versa. This affects the final APY trivially. This allows
          the terms in both sides to cancel, eventually becoming
          compoundSupplyRate * 6570 * 365 * (10 ** 9) ? aaveLiquidityRate.
        */
        uint256 cInterestAdj = CErc20(toC[tokenAddr]).supplyRatePerBlock() *
            6570 *
            365 *
            (10**9);
        uint256 aInterestAdj = ILendingPool(ILENDINGPOOL_ADDR)
            .getReserveData(tokenAddr)
            .currentLiquidityRate;

        if (cInterestAdj >= aInterestAdj) {
            supplyTokenToCompound(tokenAddr, amount);
        } else {
            supplyTokenToAave(tokenAddr, amount);
        }
    }

    function withdrawEth(uint8 percent) external payable {
        require(percent >= 0 && percent <= 100);
        CEth cToken = CEth(CETH_ADDR);

        if (cToken.balanceOf(address(this)) > 0) {
            uint256 redeemAmount = (cToken.balanceOf(address(this)) * percent) /
                100;
            uint256 redeemAmountUnderlying = (cToken.balanceOfUnderlying(
                address(this)
            ) * percent) / 100;

            cToken.redeem(redeemAmount);
            payable(msg.sender).transfer(redeemAmountUnderlying);
        } else {
            IERC20 aToken = IERC20(
                ILendingPool(ILENDINGPOOL_ADDR)
                    .getReserveData(WETH_ADDR)
                    .aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * percent) / 100;

            aToken.safeApprove(WETHGATEWAY_ADDR, amount);
            WETHGateway(WETHGATEWAY_ADDR).withdrawETH(
                ILENDINGPOOL_ADDR,
                amount,
                msg.sender
            );
        }
    }

    // Needed to receive ETH
    receive() external payable {}

    function withdraw(address tokenAddr, uint8 percent) external {
        require(
            toC[tokenAddr] != 0x0000000000000000000000000000000000000000 &&
                toC[tokenAddr] != address(0) &&
                percent >= 0 &&
                percent <= 100
        );

        CErc20 cToken = CErc20(toC[tokenAddr]);

        if (cToken.balanceOf(address(this)) > 0) {
            uint256 redeemAmount = (cToken.balanceOf(address(this)) * percent) /
                100;
            uint256 redeemAmountUnderlying = (cToken.balanceOfUnderlying(
                address(this)
            ) * percent) / 100;

            cToken.redeem(redeemAmount);
            IERC20(tokenAddr).safeTransfer(msg.sender, redeemAmountUnderlying);
        } else {
            ILendingPool pool = ILendingPool(ILENDINGPOOL_ADDR);
            IERC20 aToken = IERC20(
                pool.getReserveData(tokenAddr).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * percent) / 100;
            pool.withdraw(tokenAddr, amount, msg.sender);
        }
    }
}

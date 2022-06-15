pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/CErc20.sol";
import "contracts/interfaces/IERC20Metadata.sol";
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

    address public CETH_ADDR; // 0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5 on ethereum
    address public ILENDINGPOOL_ADDR; // 0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9 on ethereum
    address public WETH_ADDR; // 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 on ethereum
    address public WETHGATEWAY_ADDR; // 0xcc9a0B7c43DC2a5F023Bb9b738E45B0Ef6B06E04 on ethereum

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

    function supplyTokenToCompound(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);
        CErc20 cToken = CErc20(toC[tokenAddr]);

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve compound contract to transfer from this contract
        token.safeApprove(toC[tokenAddr], amount);

        cToken.mint(amount);
    }

    function supplyTokenToAave(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);
        ILendingPool pool = ILendingPool(ILENDINGPOOL_ADDR); // address is AAVE LendingPool

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(msg.sender, address(this), amount);

        // approve AAVE LendingPool contract to make a deposit
        token.safeApprove(ILENDINGPOOL_ADDR, amount);

        pool.deposit(
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

        ILendingPool pool = ILendingPool(ILENDINGPOOL_ADDR);
        uint256 aInterestAdj = pool
            .getReserveData(WETH_ADDR)
            .currentLiquidityRate;

        WETHGateway wETH = WETHGateway(WETHGATEWAY_ADDR);

        // console.log(cInterestAdj);
        // console.log(aInterestAdj);

        if (cInterestAdj >= aInterestAdj) {
            cToken.mint{value: msg.value}();
        } else {
            wETH.depositETH{value: msg.value}(
                ILENDINGPOOL_ADDR,
                address(this),
                /* referralCode = */
                0
            );
        }
    }

    // handle error when compound or aave does not support token
    function supply(address tokenAddr, uint256 amount)
        external
        returns (uint256)
    {
        require(
            toC[tokenAddr] != 0x0000000000000000000000000000000000000000 &&
                toC[tokenAddr] != address(0) &&
                toC[tokenAddr] != address(0x0)
        );

        IERC20 token = IERC20(tokenAddr);
        IERC20Metadata tokenMetadata = IERC20Metadata(tokenAddr);
        CErc20 cToken = CErc20(toC[tokenAddr]);
        ILendingPool pool = ILendingPool(ILENDINGPOOL_ADDR); // AAVE LendingPool address

        // Interest rate formula adjusted to directly compare compound vs aave
        /*
          In Compound, interest rate APY is calculated with the formula
          ((((Rate / ETH Mantissa * Blocks Per Day + 1) ^ Days Per Year)) - 1).
          In AAVE, the formula is
          ((1 + ((liquidityRate / RAY) / SECONDS_PER_YEAR)) ^ SECONDS_PER_YEAR) - 1.
          We simplify the inequality between the two formula by altering 
          the compounding term, making days per year to seconds per year 
          or vice versa. This affects the final APY trivially. This allows
          the terms in both sides to cancel, eventually becoming
          compoundSupplyRate * 6570 * 365 * (10 ** 9) ? aaveLiquidityRate.
        */
        uint256 cInterestAdj = cToken.supplyRatePerBlock() *
            6570 *
            365 *
            (10**9);
        uint256 aInterestAdj = pool
            .getReserveData(tokenAddr)
            .currentLiquidityRate;

        // console.log(cInterestAdj);
        // console.log(aInterestAdj);

        if (cInterestAdj >= aInterestAdj) {
            supplyTokenToCompound(tokenAddr, amount);
        } else {
            supplyTokenToAave(tokenAddr, amount);
        }
    }
}

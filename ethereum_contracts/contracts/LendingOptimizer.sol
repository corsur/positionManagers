//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

// import "contracts/interfaces/IERC20Metadata.sol";

import "contracts/interfaces/Pool.sol";
import "contracts/interfaces/WETHGateway.sol";
import "contracts/interfaces/CErc20.sol";
import "contracts/interfaces/QiAvax.sol";
import "contracts/interfaces/JAvax.sol";
import "contracts/interfaces/JWrappedNative.sol";

import "./libraries/AaveV3DataTypes.sol";

contract LendingOptimizer is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable
{
    using SafeERC20 for IERC20;

    mapping(address => address) mapBenqi;
    mapping(address => address) mapIron;
    mapping(address => address) mapJoe;

    address public AAVE_POOL_ADDR;
    address public WETH_GATEWAY_ADDR;
    address public WAVAX_ADDR;
    address public QIAVAX_ADDR;
    address public JAVAX_ADDR;

    function initialize(
        address _aavePoolAddr,
        address _wethGateAddr,
        address _wavaxAddr,
        address _qiAvaxAddr,
        address _jAvaxAddr
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        AAVE_POOL_ADDR = _aavePoolAddr;
        WETH_GATEWAY_ADDR = _wethGateAddr;
        WAVAX_ADDR = _wavaxAddr;
        QIAVAX_ADDR = _qiAvaxAddr;
        JAVAX_ADDR = _jAvaxAddr;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function addBenqiTokenMapping(address tokenAddr, address qiTokenAddr)
        external
        onlyOwner
    {
        mapBenqi[tokenAddr] = qiTokenAddr;
    }

    function addJoeTokenMapping(address tokenAddr, address jTokenAddr)
        external
        onlyOwner
    {
        mapJoe[tokenAddr] = jTokenAddr;
    }

    function addIronTokenMapping(address tokenAddr, address iTokenAddr)
        external
        onlyOwner
    {
        mapIron[tokenAddr] = iTokenAddr;
    }

    // ERC-20 functions
    function supplyTokenAave(address tokenAddr, uint256 amount) external {
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);
        token.safeApprove(AAVE_POOL_ADDR, amount);

        Pool(AAVE_POOL_ADDR).supply(
            tokenAddr,
            amount,
            address(this),
            0 // referralCode
        );
    }

    function withdrawTokenAave(address tokenAddr, uint16 basisPoint) external {
        require(basisPoint >= 0 && basisPoint <= 10000);

        Pool pool = Pool(AAVE_POOL_ADDR);
        IERC20 aToken = IERC20(pool.getReserveData(tokenAddr).aTokenAddress);
        uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) / 10000;
        pool.withdraw(tokenAddr, amount, msg.sender);
    }

    function supplyTokenBenqi(address tokenAddr, uint256 amount) external {
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);
        token.safeApprove(mapBenqi[tokenAddr], amount);
        CErc20(mapBenqi[tokenAddr]).mint(amount);
    }

    function withdrawTokenBenqi(address tokenAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000);
        CErc20 cToken = CErc20(mapBenqi[tokenAddr]);
        uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
            basisPoint) / 10000;
        cToken.redeemUnderlying(amount);
        IERC20(tokenAddr).safeTransfer(msg.sender, amount);
    }

    function supplyTokenIron(address tokenAddr, uint256 amount) external {
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);
        token.safeApprove(mapIron[tokenAddr], amount);
        CErc20(mapIron[tokenAddr]).mint(amount);
    }

    function withdrawTokenIron(address tokenAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000);
        CErc20 cToken = CErc20(mapIron[tokenAddr]);
        uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
            basisPoint) / 10000;
        cToken.redeemUnderlying(amount);
        IERC20(tokenAddr).safeTransfer(msg.sender, amount);
    }

    function supplyTokenJoe(address tokenAddr, uint256 amount) external {
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);
        token.safeApprove(mapJoe[tokenAddr], amount);
        CErc20(mapJoe[tokenAddr]).mint(amount);
    }

    function withdrawTokenJoe(address tokenAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000);
        CErc20 cToken = CErc20(mapJoe[tokenAddr]);
        uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
            basisPoint) / 10000;
        cToken.redeemUnderlying(amount);
        IERC20(tokenAddr).safeTransfer(msg.sender, amount);
    }

    // AVAX functions
    function supplyAvaxAave() external payable {
        WETHGateway(WETH_GATEWAY_ADDR).depositETH{value: msg.value}(
            AAVE_POOL_ADDR,
            address(this),
            0 // referralCode
        );
    }

    function withdrawAvaxAave(uint16 basisPoint) external payable {
        require(basisPoint >= 0 && basisPoint <= 10000);

        IERC20 aToken = IERC20(
            Pool(AAVE_POOL_ADDR).getReserveData(WAVAX_ADDR).aTokenAddress
        );
        uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) / 10000;

        aToken.safeApprove(WETH_GATEWAY_ADDR, amount);
        WETHGateway(WETH_GATEWAY_ADDR).withdrawETH(
            AAVE_POOL_ADDR,
            amount,
            msg.sender
        );
    }

    function supplyAvaxBenqi() external payable {
        QiAvax(QIAVAX_ADDR).mint{value: msg.value}();
    }

    function withdrawAvaxBenqi(uint16 basisPoint) external payable {
        require(basisPoint <= 10000);
        QiAvax qiToken = QiAvax(QIAVAX_ADDR);
        uint256 amount = (qiToken.balanceOfUnderlying(address(this)) *
            basisPoint) / 10000;
        qiToken.redeemUnderlying(amount);
        payable(msg.sender).transfer(amount);
    }

    function supplyAvaxJoe() external payable {
        JWrappedNative(JAVAX_ADDR).mintNative{value: msg.value}();
    }

    function withdrawAvaxJoe(uint16 basisPoint) external payable {
        require(basisPoint <= 10000);
        JWrappedNative jToken = JWrappedNative(JAVAX_ADDR);
        uint256 amount = (jToken.balanceOfUnderlying(address(this)) *
            basisPoint) / 10000;
        jToken.redeemUnderlyingNative(amount);
        payable(msg.sender).transfer(amount);
    }

    receive() external payable {}

    // function balanceErc20(address tokenAddr) external view returns (uint256) {
    //     CErc20 cToken = CErc20(tokenMap[tokenAddr]);
    //     // there might be a little discrepancy between real and calculated value
    //     // due to exchange rate multiplication
    //     uint256 compoundBalance = (cToken.balanceOf(address(this)) *
    //         cToken.exchangeRateStored()) / (10**18);
    //     IERC20 aToken = IERC20(
    //         ILendingPool(ILENDINGPOOL_ADDR)
    //             .getReserveData(tokenAddr)
    //             .aTokenAddress
    //     );
    //     uint256 aaveBalance = aToken.balanceOf(address(this));

    //     // console.log(compoundBalance + aaveBalance);

    //     return compoundBalance + aaveBalance;
    // }

    // function balanceEth() external view returns (uint256) {
    //     CEth cToken = CEth(CETH_ADDR);
    //     IERC20 aToken = IERC20(
    //         ILendingPool(ILENDINGPOOL_ADDR)
    //             .getReserveData(WETH_ADDR)
    //             .aTokenAddress
    //     );

    //     uint256 compoundBalance = (cToken.balanceOf(address(this)) *
    //         cToken.exchangeRateStored()) / (10**18);

    //     // console.log(compoundBalance + aToken.balanceOf(address(this)));

    //     return compoundBalance + aToken.balanceOf(address(this));
    // }

    // function supply(address tokenAddr, uint256 amount) external {
    //     require(tokenMap[tokenAddr] != address(0));

    //     /*
    //       Compound interest rate APY:
    //       (((Rate / ETH Mantissa * Blocks Per Day + 1) ^ Days Per Year)) - 1.

    //       AAVE:
    //       ((1 + ((liquidityRate / RAY) / SECONDS_PER_YEAR)) ^ SECONDS_PER_YEAR) - 1.

    //       We simplify the inequality between the two formula by altering
    //       the compounding term, making days per year to seconds per year
    //       or vice versa. This affects the final APY trivially. This allows
    //       the terms in both sides to cancel, eventually becoming
    //       compoundSupplyRate * 6570 * 365 * (10 ** 9) ? aaveLiquidityRate.
    //     */
    //     uint256 cInterestAdj = CErc20(tokenMap[tokenAddr])
    //         .supplyRatePerBlock() *
    //         6570 *
    //         365 *
    //         (10**9);
    //     uint256 aInterestAdj = ILendingPool(ILENDINGPOOL_ADDR)
    //         .getReserveData(tokenAddr)
    //         .currentLiquidityRate;

    //     if (cInterestAdj >= aInterestAdj) {
    //         supplyTokentoCompound(tokenAddr, amount);
    //     } else {
    //         supplyTokenToAave(tokenAddr, amount);
    //     }
    // }

    // function withdraw(address tokenAddr, uint16 basisPoint) external {
    //     require(
    //         tokenMap[tokenAddr] != address(0) &&
    //             basisPoint >= 0 &&
    //             basisPoint <= 10000
    //     );

    //     CErc20 cToken = CErc20(tokenMap[tokenAddr]);

    //     if (cToken.balanceOf(address(this)) > 0) {
    //         uint256 redeemAmount = (cToken.balanceOf(address(this)) *
    //             basisPoint) / 10000;
    //         uint256 redeemAmountUnderlying = (cToken.balanceOfUnderlying(
    //             address(this)
    //         ) * basisPoint) / 10000;

    //         cToken.redeem(redeemAmount);
    //         IERC20(tokenAddr).safeTransfer(msg.sender, redeemAmountUnderlying);
    //     } else {
    //         ILendingPool pool = ILendingPool(ILENDINGPOOL_ADDR);
    //         IERC20 aToken = IERC20(
    //             pool.getReserveData(tokenAddr).aTokenAddress
    //         );
    //         uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
    //             10000;
    //         pool.withdraw(tokenAddr, amount, msg.sender);
    //     }
    // }
}

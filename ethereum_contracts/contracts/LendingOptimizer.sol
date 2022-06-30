// SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/IAave.sol";
import "contracts/interfaces/ICompound.sol";

import "./libraries/AaveV3DataTypes.sol";

contract LendingOptimizer is
    Initializable,
    UUPSUpgradeable,
    OwnableUpgradeable
{
    using SafeERC20 for IERC20;

    enum Market {
        AAVE,
        BENQI,
        IRONBANK,
        TRADERJOE
    }

    mapping(Market => mapping(address => address)) mapCompound;

    address public AAVE_POOL; // Aave Lending Pool
    address public WETH_GATE; // WETH Gateway
    address public WAVAX; // Wrapped Avax
    address public QIAVAX; // Benqi Avax
    address public IBAVAX; // Iron Bank Wrapped Avax
    address public TJAVAX; // Trader Joe Avax

    function initialize(
        address _aavePoolAddr,
        address _wethGateAddr,
        address _wavaxAddr,
        address _qiAvaxAddr,
        address _ibAvaxAddr,
        address _tjAvaxAddr
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        AAVE_POOL = _aavePoolAddr;
        WETH_GATE = _wethGateAddr;
        WAVAX = _wavaxAddr;
        QIAVAX = _qiAvaxAddr;
        IBAVAX = _ibAvaxAddr;
        TJAVAX = _tjAvaxAddr;
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    // Map tokens to compound tokens
    function addCompoundMapping(
        Market market,
        address tokenAddr,
        address cTokenAddr
    ) external onlyOwner {
        require(market != Market.AAVE);
        mapCompound[market][tokenAddr] = cTokenAddr;
    }

    function supplyToken(address tokenAddr, uint256 amount) external {
        // Interest rate APY formulas:
        // Aave: (currentLiquidityRate / (10 ^ 27) / 31536000 + 1) ^ 31536000 - 1
        // Benqi: (supplyRatePerTimestamp / (10 ^ 18) + 1) ^ 31536000 - 1
        //        distribution apy is not accounted for here
        // Iron Bank: (supplyRatePerBlock / (10 ^ 18) * 86400 + 1) ^ 365 - 1
        //          = (supplyRatePerBlock / (10 ^ 18) + 1) ^ 31536000 - 1
        // Trader Joe: (supplyRatePerSecond / (10 ^ 18) + 1) ^ 31536000 - 1

        uint256 avIR = IAave(AAVE_POOL)
            .getReserveData(tokenAddr)
            .currentLiquidityRate; // Aave

        ICompound cToken;
        uint256 qiIR;
        uint256 ibIR;
        uint256 tjIR;

        if (mapCompound[Market.BENQI][tokenAddr] != address(0)) {
            cToken = ICompound(mapCompound[Market.BENQI][tokenAddr]);
            qiIR = cToken.supplyRatePerTimestamp() * 31536000 * (10**9);
        }

        if (mapCompound[Market.IRONBANK][tokenAddr] != address(0)) {
            cToken = ICompound(mapCompound[Market.IRONBANK][tokenAddr]);
            ibIR = cToken.supplyRatePerBlock() * 31536000 * (10**9);
        }

        if (mapCompound[Market.TRADERJOE][tokenAddr] != address(0)) {
            cToken = ICompound(mapCompound[Market.TRADERJOE][tokenAddr]);
            tjIR = cToken.supplyRatePerSecond() * 31536000 * (10**9);
        }

        Market market;
        if (avIR > qiIR && avIR > ibIR && avIR > tjIR) market = Market.AAVE;
        else if (qiIR > avIR && qiIR > ibIR && qiIR > tjIR)
            market = Market.BENQI;
        else if (ibIR > avIR && ibIR > qiIR && ibIR > tjIR)
            market = Market.IRONBANK;
        else market = Market.TRADERJOE;

        // transfer tokens from supplier to this contract
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);

        // mint aTokens, cTokens, etc.
        if (market == Market.AAVE) {
            token.safeApprove(AAVE_POOL, amount);
            IAave(AAVE_POOL).supply(
                tokenAddr,
                amount,
                address(this),
                0 // referralCode
            );
        } else {
            address cTokenAddr = mapCompound[market][tokenAddr];
            token.safeApprove(cTokenAddr, amount);
            ICompound(cTokenAddr).mint(amount);
            uint256 mintedBalance = ICompound(cTokenAddr).balanceOfUnderlying(
                address(this)
            );
            require(mintedBalance > 0);
        }
    }

    function withdrawToken(address tokenAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000);

        ICompound qiToken = ICompound(mapCompound[Market.BENQI][tokenAddr]);
        ICompound ibToken = ICompound(mapCompound[Market.IRONBANK][tokenAddr]);
        ICompound tjToken = ICompound(mapCompound[Market.TRADERJOE][tokenAddr]);

        Market market;
        if (
            mapCompound[Market.BENQI][tokenAddr] != address(0) &&
            qiToken.balanceOf(address(this)) > 0
        ) market = Market.BENQI;
        else if (
            mapCompound[Market.IRONBANK][tokenAddr] != address(0) &&
            ibToken.balanceOf(address(this)) > 0
        ) market = Market.IRONBANK;
        else if (
            mapCompound[Market.TRADERJOE][tokenAddr] != address(0) &&
            tjToken.balanceOf(address(this)) > 0
        ) market = Market.TRADERJOE;
        else market = Market.AAVE;

        if (market == Market.AAVE) {
            IAave pool = IAave(AAVE_POOL);
            address aTokenAddress = pool
                .getReserveData(tokenAddr)
                .aTokenAddress;
            require(aTokenAddress != address(0));
            IERC20 aToken = IERC20(aTokenAddress);
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            pool.withdraw(tokenAddr, amount, msg.sender);
        } else {
            ICompound cToken = ICompound(mapCompound[market][tokenAddr]);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlying(amount);
            IERC20(tokenAddr).safeTransfer(msg.sender, amount);
        }
    }

    function supplyAvax() external payable {
        uint256 avIR = IAave(AAVE_POOL)
            .getReserveData(WAVAX)
            .currentLiquidityRate;
        uint256 qiIR = ICompound(QIAVAX).supplyRatePerTimestamp() *
            31536000 *
            (10**9);
        uint256 ibIR = ICompound(IBAVAX).supplyRatePerBlock() *
            31536000 *
            (10**9);
        uint256 tjIR = ICompound(TJAVAX).supplyRatePerSecond() *
            31536000 *
            (10**9);

        // Find highest interest rate
        Market market;
        if (avIR > qiIR && avIR > ibIR && avIR > tjIR) market = Market.AAVE;
        else if (qiIR > avIR && qiIR > ibIR && qiIR > tjIR)
            market = Market.BENQI;
        else if (ibIR > avIR && ibIR > qiIR && ibIR > tjIR)
            market = Market.IRONBANK;
        else market = Market.TRADERJOE;

        // Mint
        if (market == Market.AAVE) {
            IAave(WETH_GATE).depositETH{value: msg.value}(
                AAVE_POOL,
                address(this),
                0 // referralCode
            );
        } else if (market == Market.BENQI)
            ICompound(QIAVAX).mint{value: msg.value}();
        else if (market == Market.IRONBANK)
            ICompound(IBAVAX).mintNative{value: msg.value}();
        else ICompound(TJAVAX).mintNative{value: msg.value}();
    }

    function withdrawAvax(uint16 basisPoint) external payable {
        require(basisPoint <= 10000);

        Market market;
        if (ICompound(QIAVAX).balanceOf(address(this)) > 0)
            market = Market.BENQI;
        else if (ICompound(IBAVAX).balanceOf(address(this)) > 0)
            market = Market.IRONBANK;
        else if (ICompound(TJAVAX).balanceOf(address(this)) > 0)
            market = Market.TRADERJOE;
        else market = Market.AAVE;

        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(
                IAave(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            aToken.safeApprove(WETH_GATE, amount);
            IAave(WETH_GATE).withdrawETH(AAVE_POOL, amount, msg.sender);
        } else if (market == Market.BENQI) {
            ICompound cToken = ICompound(QIAVAX);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlying(amount);
            payable(msg.sender).transfer(amount);
        } else {
            ICompound cToken = market == Market.IRONBANK
                ? ICompound(IBAVAX)
                : ICompound(TJAVAX);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlyingNative(amount);
            payable(msg.sender).transfer(amount);
        }
    }

    receive() external payable {} // necessary, doesn't compile otherwise

    function compoundBalance(address cTokenAddr)
        private
        view
        returns (uint256)
    {
        if (cTokenAddr == address(0)) return 0;
        ICompound cToken = ICompound(cTokenAddr);
        // there might be a little discrepancy between real and calculated value
        // due to exchange rate multiplication
        return
            (cToken.balanceOf(address(this)) * cToken.exchangeRateStored()) /
            (10**18);
    }

    function tokenBalance(address tokenAddr) external view returns (uint256) {
        uint256 aaveBalance;
        address aTokenAddress = IAave(AAVE_POOL)
            .getReserveData(tokenAddr)
            .aTokenAddress;
        if (aTokenAddress != address(0))
            aaveBalance = IERC20(aTokenAddress).balanceOf(address(this));

        // console.log(aaveBalance);
        // console.log(compoundBalance(mapCompound[Market.BENQI][tokenAddr]));
        // console.log(compoundBalance(mapCompound[Market.IRONBANK][tokenAddr]));
        // console.log(compoundBalance(mapCompound[Market.TRADERJOE][tokenAddr]));

        return
            aaveBalance +
            compoundBalance(mapCompound[Market.BENQI][tokenAddr]) +
            compoundBalance(mapCompound[Market.IRONBANK][tokenAddr]) +
            compoundBalance(mapCompound[Market.TRADERJOE][tokenAddr]);
    }

    function avaxBalance() external view returns (uint256) {
        IERC20 aToken = IERC20(
            IAave(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
        );

        return
            aToken.balanceOf(address(this)) +
            compoundBalance(QIAVAX) +
            compoundBalance(IBAVAX) +
            compoundBalance(TJAVAX);
    }
}

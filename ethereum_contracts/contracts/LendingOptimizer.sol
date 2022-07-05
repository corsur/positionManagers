// SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/interfaces/IAaveV3.sol";
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
        TRADERJOE,
        NONE
    }

    uint256 constant NUM_MARKETS = 4;

    mapping(Market => mapping(address => address)) compoundTokenAddr; // Lending Market => (ERC-20 tokenAddr => Compound ERC-20 tokenAddr)
    mapping(address => Market) currentMarket; // Which lending market is the token with tokenAddr currently in

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
        require(
            market != Market.AAVE &&
                market != Market.NONE &&
                tokenAddr != address(0),
            "Error: invalid compound token mapping"
        );
        compoundTokenAddr[market][tokenAddr] = cTokenAddr;
    }

    // Returns the lending market with the highest interest rate for tokenAddr
    function compareInterestRates(address tokenAddr) internal returns (Market) {
        // Interest rate APY formulas: (31536000 = seconds per year)
        // Aave: (currentLiquidityRate / (10 ^ 27) / 31536000 + 1) ^ 31536000 - 1
        // Benqi, Iron Bank, Trader Joe: (supplyRatePerTimestamp / (10 ^ 18) + 1) ^ 31536000 - 1
        // Benqi distribution apy is not accounted for yet
        uint256[NUM_MARKETS] memory interestRates; // interest rates
        uint256 factor = 31536000 * (10**9);

        Market[NUM_MARKETS] memory markets;
        for (uint256 i = 0; i < NUM_MARKETS; i++) markets[i] = Market(i);

        // Assign interest rates
        if (tokenAddr == WAVAX) {
            interestRates[uint256(Market.AAVE)] = IAaveV3(AAVE_POOL)
                .getReserveData(WAVAX)
                .currentLiquidityRate;
            interestRates[uint256(Market.BENQI)] =
                ICompound(QIAVAX).supplyRatePerTimestamp() *
                factor;
            interestRates[uint256(Market.IRONBANK)] =
                ICompound(IBAVAX).supplyRatePerBlock() *
                factor;
            interestRates[uint256(Market.TRADERJOE)] =
                ICompound(TJAVAX).supplyRatePerSecond() *
                factor;
        } else {
            interestRates[uint256(Market.AAVE)] = IAaveV3(AAVE_POOL)
                .getReserveData(tokenAddr)
                .currentLiquidityRate; // 0 if token not supported

            address qiAddr = compoundTokenAddr[Market.BENQI][tokenAddr]; // Benqi token address
            address ibAddr = compoundTokenAddr[Market.IRONBANK][tokenAddr]; // Iron Bank token address
            address tjAddr = compoundTokenAddr[Market.TRADERJOE][tokenAddr]; // Trader Joe token address

            if (qiAddr != address(0))
                // Benqi assign interest rate
                interestRates[uint256(Market.BENQI)] =
                    ICompound(qiAddr).supplyRatePerTimestamp() *
                    factor;
            if (ibAddr != address(0))
                // Iron Bank assign interest rate
                interestRates[uint256(Market.IRONBANK)] =
                    ICompound(ibAddr).supplyRatePerBlock() *
                    factor;
            if (tjAddr != address(0))
                // Trader Joe assign interest rate
                interestRates[uint256(Market.TRADERJOE)] =
                    ICompound(tjAddr).supplyRatePerSecond() *
                    factor;
        }

        // Find market with max interest rate, linear search
        Market bestMarket;
        uint256 bestInterestRate = 0;
        for (uint256 i = 0; i < NUM_MARKETS; i++) {
            if (interestRates[i] > bestInterestRate) {
                bestInterestRate = interestRates[i];
                bestMarket = markets[i];
            }
        }

        require(bestInterestRate > 0); // Revert if token not supported by any market
        currentMarket[tokenAddr] = bestMarket;

        return bestMarket;
    }

    // Compound token balance converted to underlying amount
    // there might be a little discrepancy between real and calculated value
    // due to exchange rate multiplication
    function underlyingBalance(address cTokenAddr, uint256 amount)
        internal
        view
        returns (uint256)
    {
        if (cTokenAddr == address(0)) return 0;
        return (amount * ICompound(cTokenAddr).exchangeRateStored()) / (10**18);
    }

    function supplyTokenInternal(
        address tokenAddr,
        uint256 amount,
        bool optimizeCall // is it optimize() that is calling this function
    ) internal {
        IERC20 token = IERC20(tokenAddr);

        // transfer tokens from supplier to this contract
        if (!optimizeCall)
            token.safeTransferFrom(msg.sender, address(this), amount);

        Market market = compareInterestRates(tokenAddr);
        if (market == Market.AAVE) {
            // Supply to Aave
            token.safeApprove(AAVE_POOL, amount);
            IAaveV3(AAVE_POOL).supply(
                tokenAddr,
                amount,
                address(this),
                0 // referralCode
            );
        } else {
            // Supply to markets using Compound interface
            address cTokenAddr = compoundTokenAddr[market][tokenAddr];
            token.safeApprove(cTokenAddr, amount);
            ICompound(cTokenAddr).mint(amount);
            require(
                ICompound(cTokenAddr).balanceOf(address(this)) > 0, // Minted balance
                "Error: please supply more tokens, internal exchange rate too high relative to token supplied."
            );
        }
    }

    function withdrawTokenInternal(
        address tokenAddr,
        uint16 basisPoint,
        bool optimizeCall
    ) internal returns (uint256) {
        address receiver = optimizeCall ? address(this) : msg.sender;
        Market market = currentMarket[tokenAddr];
        if (market == Market.AAVE) {
            IAaveV3 pool = IAaveV3(AAVE_POOL);
            IERC20 aToken = IERC20(
                pool.getReserveData(tokenAddr).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            pool.withdraw(tokenAddr, amount, receiver);
            return amount;
        } else {
            ICompound cToken = ICompound(compoundTokenAddr[market][tokenAddr]);
            uint256 amount = (cToken.balanceOf(address(this)) * basisPoint) /
                10000;
            cToken.redeem(amount);
            amount = IERC20(tokenAddr).balanceOf(address(this));
            if (receiver != address(this))
                IERC20(tokenAddr).safeTransfer(receiver, amount);
            return amount;
        }
    }

    function supplyAvaxInternal(bool optimizeCall, uint256 amount) internal {
        amount = optimizeCall ? amount : msg.value;
        Market market = compareInterestRates(WAVAX);
        if (market == Market.AAVE) {
            IAaveV3(WETH_GATE).depositETH{value: amount}(
                AAVE_POOL,
                address(this),
                0 // referralCode
            );
        } else if (market == Market.BENQI)
            ICompound(QIAVAX).mint{value: amount}();
        else if (market == Market.IRONBANK)
            ICompound(IBAVAX).mintNative{value: amount}();
        else if (market == Market.TRADERJOE)
            ICompound(TJAVAX).mintNative{value: amount}();
    }

    function withdrawAvaxInternal(uint16 basisPoint, bool optimizeCall)
        internal
        returns (uint256)
    {
        address receiver = optimizeCall ? address(this) : msg.sender;
        Market market = currentMarket[WAVAX];
        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(
                IAaveV3(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            aToken.safeApprove(WETH_GATE, amount);
            IAaveV3(WETH_GATE).withdrawETH(AAVE_POOL, amount, receiver);
            return amount;
        } else {
            address cTokenAddr;
            if (market == Market.BENQI) cTokenAddr = QIAVAX;
            else if (market == Market.IRONBANK) cTokenAddr = IBAVAX;
            else if (market == Market.TRADERJOE) cTokenAddr = TJAVAX;

            ICompound cToken = ICompound(cTokenAddr);
            uint256 amount = (cToken.balanceOf(address(this)) * basisPoint) /
                10000;
            market == Market.BENQI
                ? cToken.redeem(amount)
                : cToken.redeemNative(amount);
            amount = underlyingBalance(cTokenAddr, amount);
            payable(receiver).transfer(amount);
            return amount;
        }
    }

    function tokenBalance(address tokenAddr) external view returns (uint256) {
        uint256 avBalance;
        address aTokenAddress = IAaveV3(AAVE_POOL)
            .getReserveData(tokenAddr)
            .aTokenAddress;
        if (aTokenAddress != address(0))
            avBalance = IERC20(aTokenAddress).balanceOf(address(this));

        address qiAddr = compoundTokenAddr[Market.BENQI][tokenAddr];
        address ibAddr = compoundTokenAddr[Market.IRONBANK][tokenAddr];
        address tjAddr = compoundTokenAddr[Market.TRADERJOE][tokenAddr];

        uint256 qiBalance = qiAddr == address(0)
            ? 0
            : underlyingBalance(
                qiAddr,
                ICompound(qiAddr).balanceOf(address(this))
            );
        uint256 ibBalance = ibAddr == address(0)
            ? 0
            : underlyingBalance(
                ibAddr,
                ICompound(ibAddr).balanceOf(address(this))
            );
        uint256 tjBalance = tjAddr == address(0)
            ? 0
            : underlyingBalance(
                tjAddr,
                ICompound(tjAddr).balanceOf(address(this))
            );

        return avBalance + qiBalance + ibBalance + tjBalance;
    }

    function avaxBalance() external view returns (uint256) {
        IERC20 aToken = IERC20(
            IAaveV3(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
        );

        return
            aToken.balanceOf(address(this)) +
            underlyingBalance(
                QIAVAX,
                ICompound(QIAVAX).balanceOf(address(this))
            ) +
            underlyingBalance(
                IBAVAX,
                ICompound(IBAVAX).balanceOf(address(this))
            ) +
            underlyingBalance(
                TJAVAX,
                ICompound(TJAVAX).balanceOf(address(this))
            );
    }

    function supplyToken(address tokenAddr, uint256 amount) external {
        supplyTokenInternal(tokenAddr, amount, false);
    }

    function withdrawToken(address tokenAddr, uint16 basisPoint)
        external
        returns (uint256)
    {
        require(basisPoint <= 10000, "Error: must be lower than 10000.");
        return withdrawTokenInternal(tokenAddr, basisPoint, false);
    }

    function supplyAvax() external payable {
        supplyAvaxInternal(false, 0); // 0 for amount, is an unused argument
    }

    function withdrawAvax(uint16 basisPoint)
        external
        payable
        returns (uint256)
    {
        require(basisPoint <= 10000, "Error: must be lower than 10000.");
        return withdrawAvaxInternal(basisPoint, false);
    }

    receive() external payable {}

    function optimizeToken(address tokenAddr) external {
        uint256 amount = withdrawTokenInternal(tokenAddr, 10000, true);
        supplyTokenInternal(tokenAddr, amount, true);
    }

    function optimizeAvax() external payable {
        uint256 amount = withdrawAvaxInternal(10000, true);
        supplyAvaxInternal(true, amount);
    }
}

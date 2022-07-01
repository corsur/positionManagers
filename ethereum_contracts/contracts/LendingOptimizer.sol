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
        TRADERJOE
    }

    uint256 constant NUMBER_OF_MARKETS = 4;

    // Lending Market => (ERC-20 tokenAddr => Compound ERC-20 tokenAddr)
    mapping(Market => mapping(address => address)) compoundTokenAddr;

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
        require(market != Market.AAVE && tokenAddr != address(0));
        compoundTokenAddr[market][tokenAddr] = cTokenAddr;
    }

    // Find market with max interest rate, linear search
    function maxInterestRate(
        Market[NUMBER_OF_MARKETS] memory markets,
        uint256[NUMBER_OF_MARKETS] memory interestRates
    ) internal pure returns (Market) {
        Market bestMarket;
        uint256 bestInterestRate = 0;
        for (uint256 i = 0; i < NUMBER_OF_MARKETS; i++) {
            if (interestRates[i] > bestInterestRate) {
                bestInterestRate = interestRates[i];
                bestMarket = markets[i];
            }
        }
        return bestMarket;
    }

    // Returns the lending market with the highest interest rate for tokenAddr
    function compareInterestRates(address tokenAddr) internal returns (Market) {
        // Interest rate APY formulas: (31536000 = seconds per year)
        // Aave: (currentLiquidityRate / (10 ^ 27) / 31536000 + 1) ^ 31536000 - 1
        // Benqi, Iron Bank, Trader Joe: (supplyRatePerTimestamp / (10 ^ 18) + 1) ^ 31536000 - 1
        // Benqi distribution apy is not accounted for yet
        uint256[NUMBER_OF_MARKETS] memory interestRates; // interest rates
        uint256 factor = 31536000 * (10**9);

        Market[NUMBER_OF_MARKETS] memory markets;
        for (uint256 i = 0; i < NUMBER_OF_MARKETS; i++) markets[i] = Market(i);

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

        // revert if token not supported by any market
        uint256 interestRateSum;
        for (uint256 i = 0; i < NUMBER_OF_MARKETS; i++)
            interestRateSum += interestRates[i];
        require(interestRateSum != 0);

        return maxInterestRate(markets, interestRates);
    }

    // Which lending market is the token with tokenAddr currently in
    function currentMarket(address tokenAddr) internal view returns (Market) {
        if (tokenAddr == WAVAX) {
            if (ICompound(QIAVAX).balanceOf(address(this)) > 0)
                return Market.BENQI;
            else if (ICompound(IBAVAX).balanceOf(address(this)) > 0)
                return Market.IRONBANK;
            else if (ICompound(TJAVAX).balanceOf(address(this)) > 0)
                return Market.TRADERJOE;
            else return Market.AAVE;
        } else {
            address[NUMBER_OF_MARKETS] cTokenAddrs = [
                IAaveV3(AAVE_POOL).getReserveData(tokenAddr).aTokenAddress,
                compoundTokenAddr[Market.BENQI][tokenAddr],
                compoundTokenAddr[Market.IRONBANK][tokenAddr],
                compoundTokenAddr[Market.TRADERJOE][tokenAddr]
            ];

            bool marketExists = false;
            for (uint256 i = 0; i < NUMBER_OF_MARKETS; i++)
                marketExists || (cTokenAddrs[i] != address(0));
            require(!marketExists);

            if (
                cTokenAddrs[uint256(Market.BENQI)] != address(0) &&
                ICompound(cTokenAddrs[uint256(Market.BENQI)]).balanceOf(
                    address(this)
                ) >
                0
            ) return Market.BENQI;
            else if (
                ibAddr != address(0) &&
                ICompound(ibAddr).balanceOf(address(this)) > 0
            ) return Market.IRONBANK;
            else if (
                tjAddr != address(0) &&
                ICompound(tjAddr).balanceOf(address(this)) > 0
            ) return Market.TRADERJOE;
            else return Market.AAVE;
        }
    }

    function supplyToken(address tokenAddr, uint256 amount) public {
        // transfer tokens from supplier to this contract
        IERC20 token = IERC20(tokenAddr);
        token.safeTransferFrom(msg.sender, address(this), amount);

        // supply
        Market market = compareInterestRates(tokenAddr);
        if (market == Market.AAVE) {
            token.safeApprove(AAVE_POOL, amount);
            IAaveV3(AAVE_POOL).supply(
                tokenAddr,
                amount,
                address(this),
                0 // referralCode
            );
        } else {
            address cTokenAddr = compoundTokenAddr[market][tokenAddr];
            token.safeApprove(cTokenAddr, amount);
            ICompound(cTokenAddr).mint(amount);
            uint256 mintedBalance = ICompound(cTokenAddr).balanceOfUnderlying(
                address(this)
            );
            require(mintedBalance > 0);
        }
    }

    function withdrawToken(address tokenAddr, uint16 basisPoint)
        public
        returns (uint256)
    {
        require(basisPoint <= 10000);

        Market market = currentMarket(tokenAddr);
        if (market == Market.AAVE) {
            IAaveV3 pool = IAaveV3(AAVE_POOL);
            IERC20 aToken = IERC20(
                pool.getReserveData(tokenAddr).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            pool.withdraw(tokenAddr, amount, msg.sender);
            return amount;
        } else {
            ICompound cToken = ICompound(compoundTokenAddr[market][tokenAddr]);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlying(amount);
            IERC20(tokenAddr).safeTransfer(msg.sender, amount);
            return amount;
        }
    }

    function supplyAvax() public payable {
        Market market = compareInterestRates(WAVAX);

        // Mint
        if (market == Market.AAVE) {
            IAaveV3(WETH_GATE).depositETH{value: msg.value}(
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

    function withdrawAvax(uint16 basisPoint) public payable returns (uint256) {
        require(basisPoint <= 10000);

        Market market = currentMarket(WAVAX);
        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(
                IAaveV3(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
            );
            uint256 amount = (aToken.balanceOf(address(this)) * basisPoint) /
                10000;
            aToken.safeApprove(WETH_GATE, amount);
            IAaveV3(WETH_GATE).withdrawETH(AAVE_POOL, amount, msg.sender);
            return amount;
        } else if (market == Market.BENQI) {
            ICompound cToken = ICompound(QIAVAX);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlying(amount);
            payable(msg.sender).transfer(amount);
            return amount;
        } else {
            ICompound cToken = market == Market.IRONBANK
                ? ICompound(IBAVAX)
                : ICompound(TJAVAX);
            uint256 amount = (cToken.balanceOfUnderlying(address(this)) *
                basisPoint) / 10000;
            cToken.redeemUnderlyingNative(amount);
            payable(msg.sender).transfer(amount);
            return amount;
        }
    }

    receive() external payable {}

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
        address aTokenAddress = IAaveV3(AAVE_POOL)
            .getReserveData(tokenAddr)
            .aTokenAddress;
        if (aTokenAddress != address(0))
            aaveBalance = IERC20(aTokenAddress).balanceOf(address(this));

        console.log(aaveBalance);
        console.log(
            compoundBalance(compoundTokenAddr[Market.BENQI][tokenAddr])
        );
        console.log(
            compoundBalance(compoundTokenAddr[Market.IRONBANK][tokenAddr])
        );
        console.log(
            compoundBalance(compoundTokenAddr[Market.TRADERJOE][tokenAddr])
        );

        return
            aaveBalance +
            compoundBalance(compoundTokenAddr[Market.BENQI][tokenAddr]) +
            compoundBalance(compoundTokenAddr[Market.IRONBANK][tokenAddr]) +
            compoundBalance(compoundTokenAddr[Market.TRADERJOE][tokenAddr]);
    }

    function avaxBalance() external view returns (uint256) {
        IERC20 aToken = IERC20(
            IAaveV3(AAVE_POOL).getReserveData(WAVAX).aTokenAddress
        );

        console.log(aToken.balanceOf(address(this)));
        console.log(compoundBalance(QIAVAX));
        console.log(compoundBalance(IBAVAX));
        console.log(compoundBalance(TJAVAX));

        return
            aToken.balanceOf(address(this)) +
            compoundBalance(QIAVAX) +
            compoundBalance(IBAVAX) +
            compoundBalance(TJAVAX);
    }

    // function optimizeToken(address tokenAddr) external {
    //     uint256 amount = withdrawToken(tokenAddr, 10000);
    //     IERC20 token = IERC20(tokenAddr);
    //     Market market = compareInterestRates(tokenAddr);
    //     if (market == Market.AAVE) {
    //         token.safeApprove(AAVE_POOL, amount);
    //         IAaveV3(AAVE_POOL).supply(
    //             tokenAddr,
    //             amount,
    //             address(this),
    //             0 // referralCode
    //         );
    //     } else {
    //         address cTokenAddr = compoundTokenAddr[market][tokenAddr];
    //         token.safeApprove(cTokenAddr, amount);
    //         ICompound(cTokenAddr).mint(amount);
    //         uint256 mintedBalance = ICompound(cTokenAddr).balanceOfUnderlying(
    //             address(this)
    //         );
    //         require(mintedBalance > 0);
    //     }
    // }

    /*
    function optimizeToken(address tokenAddr) external {
        // Withdraw
        Market market = currentMarket(tokenAddr);
        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(
                IAaveV3(AAVE_POOL).getReserveData(tokenAddr).aTokenAddress
            );
            IAaveV3(AAVE_POOL).withdraw(
                tokenAddr,
                aToken.balanceOf(address(this)), // amount
                address(this)
            );
        } else {
            ICompound cToken = ICompound(compoundTokenAddr[market][tokenAddr]);
            uint256 amount = cToken.balanceOfUnderlying(address(this));
            cToken.redeemUnderlying(amount);
            IERC20(tokenAddr).safeTransfer(address(this), amount);
        }

        // Re-supply
        IERC20 token = IERC20(tokenAddr);
        market = compareInterestRates(tokenAddr);
        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(
                IAaveV3(AAVE_POOL).getReserveData(tokenAddr).aTokenAddress
            );
            uint256 amount = aToken.balanceOf(address(this));
            token.safeApprove(AAVE_POOL, amount);
            IAaveV3(AAVE_POOL).supply(
                tokenAddr,
                amount,
                address(this),
                0 // referralCode
            );
        } else {
            address cTokenAddr = compoundTokenAddr[market][tokenAddr];
            uint256 amount = token.balanceOf(address(this));
            token.safeApprove(cTokenAddr, amount);
            ICompound(cTokenAddr).mint(amount);
            uint256 mintedBalance = ICompound(cTokenAddr).balanceOfUnderlying(
                address(this)
            );
            require(mintedBalance > 0);
        }
    }*/

    //     function optimizeAvax() external payable {
    //         withdrawAvax(10000);
    //         supplyAvax();
    //     }
}

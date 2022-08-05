// SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

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
        NONE,
        AAVE,
        BENQI,
        IRONBANK,
        TRADERJOE
    }

    uint256 constant NUM_MARKETS = 4;

    mapping(Market => mapping(address => address)) cAddrMap; // market => uToken (underlying token) => cToken
    mapping(address => Market) currentMarket; // uToken => Market
    mapping(address => mapping(address => uint256)) userShare; // uToken => userAddr => shares
    mapping(address => uint256) totalShare; // uToken => total

    address public AAVE_V3_POOL; // Aave Lending Pool
    address[NUM_MARKETS + 1] public AVAX; // Addresses of AVAX tokens

    /* SETUP */

    function initialize(
        address _pool,
        address _wAvax, // Wrapped AVAX
        address _wEth, // WETH Gateway (Aave)
        address _qiAvax, // Benqi AVAX
        address _iAvax, // Iron Bank AVAX
        address _jAvax // Trader Joe AVAX
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        AAVE_V3_POOL = _pool;
        AVAX = [_wAvax, _wEth, _qiAvax, _iAvax, _jAvax];
    }

    receive() external payable {}

    function _authorizeUpgrade(address) internal override onlyOwner {}

    // Map underlying token addr to compound token addr
    function addCompoundMapping(
        Market market,
        address uAddr,
        address cAddr
    ) external onlyOwner {
        require(market != Market.NONE && market != Market.AAVE);
        cAddrMap[market][uAddr] = cAddr;
    }

    /* INTERNAL HELPER FUNCTIONS */

    function compoundToUnderlying(address cAddr, uint256 amount)
        internal
        view
        returns (uint256)
    {
        require(cAddr != address(0));
        return (amount * ICompound(cAddr).exchangeRateStored()) / (10**18);
    }

    function apyAdjusted(address uAddr, Market market)
        internal
        returns (uint256)
    {
        // Interest rate APY formulas: (31536000 = seconds per year)
        // Aave: (currentLiquidityRate / (10 ^ 27) / 31536000 + 1) ^ 31536000 - 1
        // Rest: (supplyRatePerTimestamp / (10 ^ 18) + 1) ^ 31536000 - 1
        // Benqi distribution APY not accounted yet

        uint256 factor = 31536000 * (10**9);

        if (market == Market.AAVE) {
            return
                IAaveV3(AAVE_V3_POOL)
                    .getReserveData(uAddr)
                    .currentLiquidityRate; // 0 if token not supported
        } else {
            address addr = uAddr == AVAX[uint256(market)]
                ? AVAX[uint256(market)]
                : cAddrMap[market][uAddr];
            if (addr == address(0)) return 0;
            if (market == Market.BENQI)
                return ICompound(addr).supplyRatePerTimestamp() * factor;
            else if (market == Market.IRONBANK)
                return ICompound(addr).supplyRatePerBlock() * factor;
            else if (market == Market.TRADERJOE)
                return ICompound(addr).supplyRatePerSecond() * factor;
        }

        return 0;
    }

    // Returns the lending market with the highest interest rate for uAddr
    function bestMarket(address uAddr) internal returns (Market) {
        uint256[NUM_MARKETS + 1] memory interestRates;
        for (uint256 i = 1; i <= NUM_MARKETS; i++)
            interestRates[i] = apyAdjusted(uAddr, Market(i));

        Market market;
        uint256 bestInterestRate = 0;
        for (uint256 i = 1; i <= NUM_MARKETS; i++) {
            if (interestRates[i] > bestInterestRate) {
                bestInterestRate = interestRates[i];
                market = Market(i);
            }
        }

        require(market != Market.NONE); // Revert if token not supported by any market

        return market;
    }

    /* ERC-20 TOKEN FUNCTIONS */

    function supplyToken(address uAddr, uint256 amount) external {
        IERC20 uToken = IERC20(uAddr);
        uToken.safeTransferFrom(msg.sender, address(this), amount); // user to contract

        // first supply
        if (currentMarket[uAddr] == Market.NONE)
            currentMarket[uAddr] = bestMarket(uAddr);

        uint256 prevBalance;
        uint256 supplied;

        // token exchange
        if (currentMarket[uAddr] == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(uAddr)
                .aTokenAddress;
            uToken.safeApprove(AAVE_V3_POOL, amount);
            IERC20 aToken = IERC20(aAddr);
            prevBalance = aToken.balanceOf(address(this));
            IAaveV3(AAVE_V3_POOL).supply(uAddr, amount, address(this), 0); // 0 = referralCode
            supplied = amount;
        } else {
            address cAddr = cAddrMap[currentMarket[uAddr]][uAddr];
            uToken.safeApprove(cAddr, amount);
            ICompound cToken = ICompound(cAddr);
            prevBalance = cToken.balanceOf(address(this));
            cToken.mint(amount);
            supplied = cToken.balanceOf(address(this)) - prevBalance;
            require(supplied > 0); // due to exchange rate
        }

        // update shares
        if (prevBalance == 0) {
            userShare[uAddr][msg.sender] += supplied;
            totalShare[uAddr] += supplied;
        } else {
            uint256 shares = (supplied * totalShare[uAddr]) / prevBalance;
            userShare[uAddr][msg.sender] += shares;
            totalShare[uAddr] += shares;
        }
    }

    function withdrawToken(address uAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000 && userShare[uAddr][msg.sender] > 0);

        uint256 shares = (userShare[uAddr][msg.sender] * basisPoint) / 10000;

        if (currentMarket[uAddr] == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(uAddr)
                .aTokenAddress;
            uint256 amount = (IERC20(aAddr).balanceOf(address(this)) * shares) /
                totalShare[uAddr];
            IAaveV3(AAVE_V3_POOL).withdraw(uAddr, amount, msg.sender);
        } else {
            address cAddr = cAddrMap[currentMarket[uAddr]][uAddr];
            uint256 amount = (ICompound(cAddr).balanceOf(address(this)) *
                shares) / totalShare[uAddr];
            ICompound(cAddr).redeem(amount);
            IERC20(uAddr).safeTransfer(
                msg.sender,
                IERC20(uAddr).balanceOf(address(this))
            );
        }

        // update shares
        userShare[uAddr][msg.sender] -= shares;
        totalShare[uAddr] -= shares;
    }

    function optimizeToken(address uAddr) external returns (bool) {
        Market market = currentMarket[uAddr];
        Market newMarket = bestMarket(uAddr);
        if (market == newMarket) return false;

        uint256 balance;
        IAaveV3 pool = IAaveV3(AAVE_V3_POOL);

        // withdraw
        if (market == Market.AAVE) {
            IERC20 aToken = IERC20(pool.getReserveData(uAddr).aTokenAddress);
            balance = aToken.balanceOf(address(this));
            pool.withdraw(uAddr, balance, address(this));
        } else {
            ICompound cToken = ICompound(cAddrMap[market][uAddr]);
            balance = cToken.balanceOf(address(this));
            cToken.redeem(balance);
        }

        // supply
        IERC20 uToken = IERC20(uAddr);
        balance = uToken.balanceOf(address(this));
        currentMarket[uAddr] = newMarket;

        if (newMarket == Market.AAVE) {
            uToken.safeApprove(AAVE_V3_POOL, balance);
            pool.supply(uAddr, balance, address(this), 0); // 0 = referralCode
        } else {
            uToken.safeApprove(cAddrMap[newMarket][uAddr], balance);
            ICompound(cAddrMap[newMarket][uAddr]).mint(balance);
        }

        return true;
    }

    /* AVAX FUNCTIONS */

    function supplyAvax() external payable {
        // first supply
        if (currentMarket[AVAX[0]] == Market.NONE)
            currentMarket[AVAX[0]] = bestMarket(AVAX[0]);

        Market market = currentMarket[AVAX[0]];
        uint256 prevBalance;
        uint256 amount = address(this).balance;
        uint256 supplied;

        // token exchange
        if (market == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(AVAX[0])
                .aTokenAddress;
            IERC20 aToken = IERC20(aAddr);
            prevBalance = aToken.balanceOf(address(this));
            IAaveV3(AVAX[uint256(market)]).depositETH{value: amount}(
                AAVE_V3_POOL,
                address(this),
                0 // referralCode
            );
            supplied = aToken.balanceOf(address(this)) - prevBalance;
        } else {
            address cAddr = AVAX[uint256(market)];
            ICompound cToken = ICompound(cAddr);
            prevBalance = cToken.balanceOf(address(this));
            if (market == Market.BENQI) cToken.mint{value: amount}();
            else cToken.mintNative{value: amount}();
            supplied = cToken.balanceOf(address(this)) - prevBalance;
            require(supplied > 0); // due to exchange rate
        }

        // update shares
        if (prevBalance == 0) {
            userShare[AVAX[0]][msg.sender] += supplied;
            totalShare[AVAX[0]] += supplied;
        } else {
            uint256 shares = (supplied * totalShare[AVAX[0]]) / prevBalance;
            userShare[AVAX[0]][msg.sender] += shares;
            totalShare[AVAX[0]] += shares;
        }
    }

    function withdrawAvax(uint16 basisPoint) external payable {
        require(basisPoint <= 10000 && userShare[AVAX[0]][msg.sender] > 0);

        uint256 shares = (userShare[AVAX[0]][msg.sender] * basisPoint) / 10000;
        Market market = currentMarket[AVAX[0]];

        if (market == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(AVAX[0])
                .aTokenAddress;
            uint256 amount = (IERC20(aAddr).balanceOf(address(this)) * shares) /
                totalShare[AVAX[0]];
            IERC20(aAddr).safeApprove(AVAX[uint256(market)], amount);
            IAaveV3(AVAX[uint256(Market.AAVE)]).withdrawETH(
                AAVE_V3_POOL,
                amount,
                msg.sender
            );
        } else {
            address cAddr = AVAX[uint256(market)];
            ICompound cToken = ICompound(cAddr);
            uint256 amount = (ICompound(cAddr).balanceOf(address(this)) *
                shares) / totalShare[AVAX[0]];
            market == Market.BENQI
                ? cToken.redeem(amount)
                : cToken.redeemNative(amount);
            payable(msg.sender).transfer(address(this).balance);
        }

        // update shares
        userShare[AVAX[0]][msg.sender] -= shares;
        totalShare[AVAX[0]] -= shares;
    }

    function optimizeAvax() external payable returns (bool) {
        Market market = currentMarket[AVAX[0]];
        Market newMarket = bestMarket(AVAX[0]);
        if (market == newMarket) return false;

        uint256 amount;

        // withdraw
        if (market == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(AVAX[0])
                .aTokenAddress;
            amount = IERC20(aAddr).balanceOf(address(this));
            IERC20(aAddr).safeApprove(AVAX[uint256(market)], amount);
            IAaveV3(AVAX[uint256(market)]).withdrawETH(
                AAVE_V3_POOL,
                amount,
                address(this)
            );
        } else {
            address cAddr = AVAX[uint256(market)];
            ICompound cToken = ICompound(cAddr);
            amount = cToken.balanceOf(address(this));
            market == Market.BENQI
                ? cToken.redeem(amount)
                : cToken.redeemNative(amount);
        }

        // supply
        amount = address(this).balance;
        currentMarket[AVAX[0]] = newMarket;

        if (newMarket == Market.AAVE) {
            IAaveV3(AVAX[uint256(newMarket)]).depositETH{value: amount}(
                AAVE_V3_POOL,
                address(this),
                0 // referralCode
            );
        } else {
            ICompound cToken = ICompound(AVAX[uint256(newMarket)]);
            if (newMarket == Market.BENQI) cToken.mint{value: amount}();
            else cToken.mintNative{value: amount}();
        }

        return true;
    }

    /* GET BALANCE */

    function getBalance(address uAddr, address user)
        external
        view
        returns (uint256)
    {
        Market market = currentMarket[uAddr];
        if (market == Market.NONE || totalShare[uAddr] == 0) return 0;

        if (market == Market.AAVE) {
            address aAddr = IAaveV3(AAVE_V3_POOL)
                .getReserveData(uAddr)
                .aTokenAddress;
            IERC20 aToken = IERC20(aAddr);
            uint256 userBalance = (aToken.balanceOf(address(this)) *
                userShare[uAddr][user]) / totalShare[uAddr];
            return userBalance;
        } else {
            address cAddr = uAddr == AVAX[0]
                ? AVAX[uint256(market)]
                : cAddrMap[market][uAddr];
            ICompound cToken = ICompound(cAddr);
            uint256 userBalance = (cToken.balanceOf(address(this)) *
                userShare[uAddr][user]) / totalShare[uAddr];
            return compoundToUnderlying(cAddr, userBalance);
        }
    }
}

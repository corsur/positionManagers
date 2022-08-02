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

    address public POOL; // Aave Lending Pool
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
        POOL = _pool;
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
        // Benqi distribution APY not accounted yet, formula in code is simplified from Aave = Rest equation

        uint256 factor = 31536000 * (10**9);

        if (market == Market.AAVE) {
            return IAaveV3(POOL).getReserveData(uAddr).currentLiquidityRate; // 0 if token not supported
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

        return Market.AAVE;
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
            address aAddr = IAaveV3(POOL).getReserveData(uAddr).aTokenAddress;
            uToken.safeApprove(POOL, amount);
            IERC20 aToken = IERC20(aAddr);
            prevBalance = aToken.balanceOf(address(this));
            IAaveV3(POOL).supply(uAddr, amount, address(this), 0); // 0 = referralCode
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
        if (userShare[uAddr][msg.sender] == 0) {
            userShare[uAddr][msg.sender] += supplied;
            totalShare[uAddr] += supplied;
        } else {
            uint256 shares = (supplied * totalShare[uAddr]) / prevBalance;
            userShare[uAddr][msg.sender] += shares;
            totalShare[uAddr] += shares;
        }
    }

    function withdrawToken(address uAddr, uint16 basisPoint) external {
        require(basisPoint <= 10000 && totalShare[uAddr] > 0);

        uint256 shares = (userShare[uAddr][msg.sender] * basisPoint) / 10000;

        if (currentMarket[uAddr] == Market.AAVE) {
            address aAddr = IAaveV3(POOL).getReserveData(uAddr).aTokenAddress;
            uint256 amount = (IERC20(aAddr).balanceOf(address(this)) * shares) /
                totalShare[uAddr];
            IAaveV3(POOL).withdraw(uAddr, amount, msg.sender);
        } else {
            address cAddr = cAddrMap[currentMarket[uAddr]][uAddr];
            uint256 amount = (ICompound(cAddr).balanceOf(address(this)) *
                shares) / totalShare[uAddr];
            ICompound(cAddr).redeem(amount);
        }

        // update shares
        userShare[uAddr][msg.sender] -= shares;
        totalShare[uAddr] -= shares;
    }

    function optimizeToken(address uAddr) external {
        Market market;
        uint256 balance;
        IAaveV3 pool = IAaveV3(POOL);

        // withdraw
        market = currentMarket[uAddr];
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

        market = bestMarket(uAddr);
        currentMarket[uAddr] = market;
        if (market == Market.AAVE) {
            uToken.safeApprove(POOL, balance);
            pool.supply(uAddr, balance, address(this), 0); // 0 = referralCode
        } else {
            uToken.safeApprove(cAddrMap[market][uAddr], balance);
            ICompound(cAddrMap[market][uAddr]).mint(balance);
        }
    }

    function tokenBalance(address uAddr) external view returns (uint256) {
        Market market = currentMarket[uAddr];
        if (market == Market.NONE || totalShare[uAddr] == 0) return 0;

        if (market == Market.AAVE) {
            address aAddr = IAaveV3(POOL).getReserveData(uAddr).aTokenAddress;
            IERC20 aToken = IERC20(aAddr);
            uint256 userBalance = (aToken.balanceOf(address(this)) *
                userShare[uAddr][msg.sender]) / totalShare[uAddr];
            return userBalance;
        } else {
            address cAddr = cAddrMap[market][uAddr];
            ICompound cToken = ICompound(cAddr);
            uint256 userBalance = (cToken.balanceOf(address(this)) *
                userShare[uAddr][msg.sender]) / totalShare[uAddr];
            return compoundToUnderlying(cAddr, userBalance);
        }
    }

    /* AVAX FUNCTIONS */

    /*
    function supplyAvaxInternal(
        bool isOptimize,
        uint256 amount,
        address user
    ) internal {
        if (!isOptimize) {
            if (balanceMap[AVAX[0]][user].market == Market(0)) users.push(user);
            amount = msg.value;
        }

        Market market = bestMarket(AVAX[0]);
        if (market == Market.AAVE) {
            IAaveV3(AVAX[uint256(market)]).depositETH{value: amount}(
                POOL,
                address(this),
                0 // referralCode
            );
            balanceMap[AVAX[0]][user].amount += amount;
        } else {
            ICompound cToken = ICompound(AVAX[uint256(market)]);
            uint256 cMinted = cToken.balanceOf(address(this));
            if (market == Market.BENQI) cToken.mint{value: amount}();
            else cToken.mintNative{value: amount}();
            cMinted = cToken.balanceOf(address(this)) - cMinted;
            require(cMinted > 0);
            balanceMap[AVAX[0]][user].amount += cMinted;
        }
    }

    function withdrawAvaxInternal(
        uint16 basisPoint,
        bool isOptimize,
        address user
    ) internal returns (uint256) {
        address receiver = isOptimize ? address(this) : user;
        Market market = balanceMap[AVAX[0]][user].market;
        if (market == Market.AAVE) {
            uint256 amount = (balanceMap[AVAX[0]][user].amount * basisPoint) /
                10000;
            IERC20(IAaveV3(POOL).getReserveData(AVAX[0]).aTokenAddress)
                .safeApprove(AVAX[uint256(Market.AAVE)], amount);
            IAaveV3(AVAX[uint256(Market.AAVE)]).withdrawETH(
                POOL,
                amount,
                receiver
            );
            balanceMap[AVAX[0]][user].amount -= amount;
            return amount;
        } else {
            ICompound cToken = ICompound(AVAX[uint256(market)]);
            uint256 amount = (balanceMap[AVAX[0]][user].amount * basisPoint) /
                10000;
            uint256 uAmount = address(this).balance;
            uint256 cAmount = cToken.balanceOf(address(this));
            market == Market.BENQI
                ? cToken.redeem(amount)
                : cToken.redeemNative(amount);
            cAmount -= cToken.balanceOf(address(this));
            amount = address(this).balance - uAmount;
            payable(receiver).transfer(amount);
            balanceMap[AVAX[0]][user].amount -= cAmount;
            return amount;
        }
    }
    */

    /*
    function avaxBalance() external view returns (uint256) {
        Market market = balanceMap[AVAX[0]][msg.sender].market;
        if (market == Market.AAVE) {
            return balanceMap[AVAX[0]][msg.sender].amount;
        } else {
            return
                uBalance(
                    AVAX[uint256(market)],
                    balanceMap[AVAX[0]][msg.sender].amount
                );
        }
    }
    */

    /*
    function supplyAvax() external payable {
        supplyAvaxInternal(false, 0, msg.sender); // 0 for amount, is an unused argument
    }

    function withdrawAvax(uint16 basisPoint)
        external
        payable
        returns (uint256)
    {
        require(basisPoint <= 10000);
        return withdrawAvaxInternal(basisPoint, false, msg.sender);
    }
    */

    /*
    function optimizeAvax() external payable {
        uint256 amount = withdrawAvaxInternal(10000, true, msg.sender);
        supplyAvaxInternal(true, amount, msg.sender);
    }
    */
}

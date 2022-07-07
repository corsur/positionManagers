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

    struct Balance {
        Market market; // which market tokens are from
        uint256 amount; // amount of cToken or aToken
    }

    mapping(Market => mapping(address => address)) cAddrMap; // market => uToken (underlying token) => cToken
    mapping(address => mapping(address => Balance)) balanceMap; // uToken => user => Balance

    address public POOL; // Aave Lending Pool
    address[NUM_MARKETS + 1] public AVAX; // Addresses of AVAX tokens for each market at its enum position
    address[] public users; // List of users

    function initialize(
        address _pool,
        address _wAvax, // Wrapped AVAX
        address _wEthGateway, // WETH Gateway (Aave)
        address _qiAvax, // Benqi AVAX
        address _iAvax, // Iron Bank AVAX
        address _jAvax // Trader Joe AVAX
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        POOL = _pool;
        AVAX = [_wAvax, _wEthGateway, _qiAvax, _iAvax, _jAvax];
        users = new address[](0);
    }

    receive() external payable {}

    function _authorizeUpgrade(address) internal override onlyOwner {}

    // Map tokens to compound tokens
    function addCompoundMapping(
        Market market,
        address uAddr, // Address of underlying token, u will stand for underlying throughout
        address cAddr // Address of compound exchange token, c will stand for compound throughout, a for aave
    ) external onlyOwner {
        require(market != Market.NONE && market != Market.AAVE);
        cAddrMap[market][uAddr] = cAddr;
    }

    // Returns the lending market with the highest interest rate for uAddr
    // Interest rate APY formulas: (31536000 = seconds per year)
    // - Aave: (currentLiquidityRate / (10 ^ 27) / 31536000 + 1) ^ 31536000 - 1
    // - Rest: (supplyRatePerTimestamp / (10 ^ 18) + 1) ^ 31536000 - 1
    // - Benqi distribution APY is not accounted for yet
    function compareInterestRates(address uAddr, address user)
        internal
        returns (Market)
    {
        uint256[NUM_MARKETS + 1] memory rates; // interest rates
        uint256 factor = 31536000 * (10**9);

        if (uAddr == AVAX[0]) {
            rates[uint256(Market.AAVE)] = IAaveV3(POOL)
                .getReserveData(AVAX[0])
                .currentLiquidityRate;
            rates[uint256(Market.BENQI)] =
                ICompound(AVAX[uint256(Market.BENQI)])
                    .supplyRatePerTimestamp() *
                factor;
            rates[uint256(Market.IRONBANK)] =
                ICompound(AVAX[uint256(Market.IRONBANK)]).supplyRatePerBlock() *
                factor;
            rates[uint256(Market.TRADERJOE)] =
                ICompound(AVAX[uint256(Market.TRADERJOE)])
                    .supplyRatePerSecond() *
                factor;
        } else {
            rates[uint256(Market.AAVE)] = IAaveV3(POOL)
                .getReserveData(uAddr)
                .currentLiquidityRate; // 0 if token not supported
            rates[uint256(Market.BENQI)] = cAddrMap[Market.BENQI][uAddr] !=
                address(0)
                ? ICompound(cAddrMap[Market.BENQI][uAddr])
                    .supplyRatePerTimestamp() * factor
                : 0;
            rates[uint256(Market.IRONBANK)] = cAddrMap[Market.IRONBANK][
                uAddr
            ] != address(0)
                ? ICompound(cAddrMap[Market.IRONBANK][uAddr])
                    .supplyRatePerBlock() * factor
                : 0;
            rates[uint256(Market.TRADERJOE)] = cAddrMap[Market.TRADERJOE][
                uAddr
            ] != address(0)
                ? ICompound(cAddrMap[Market.TRADERJOE][uAddr])
                    .supplyRatePerSecond() * factor
                : 0;
        }

        Market bestMarket;
        uint256 bestInterestRate = 0;
        for (uint256 i = 1; i <= NUM_MARKETS; i++) {
            if (rates[i] > bestInterestRate) {
                bestInterestRate = rates[i];
                bestMarket = Market(i);
            }
        }

        require(bestInterestRate > 0 && bestMarket != Market.NONE); // Revert if token not supported by any market
        balanceMap[uAddr][user].market = bestMarket;

        return bestMarket;
    }

    // cToken to uToken amount, potential small discrepancy due to exchange rate
    function uBalance(address cAddr, uint256 amount)
        internal
        view
        returns (uint256)
    {
        if (cAddr == address(0)) return 0;
        return (amount * ICompound(cAddr).exchangeRateStored()) / (10**18);
    }

    function supplyTokenInternal(
        address uAddr,
        uint256 amount,
        bool isOptimize, // is it optimize() that is calling this function
        address user
    ) internal {
        IERC20 uToken = IERC20(uAddr);

        if (!isOptimize) {
            if (balanceMap[uAddr][user].market == Market(0)) users.push(user);
            uToken.safeTransferFrom(user, address(this), amount); // transfer tokens from supplier to this contract
        }

        Market market = compareInterestRates(uAddr, user);
        balanceMap[uAddr][user].market = market;
        if (market == Market.AAVE) {
            uToken.safeApprove(POOL, amount);
            IAaveV3(POOL).supply(uAddr, amount, address(this), 0); // 0 = referralCode
            balanceMap[uAddr][user].amount += amount;
        } else {
            uToken.safeApprove(cAddrMap[market][uAddr], amount);
            ICompound cToken = ICompound(cAddrMap[market][uAddr]);
            uint256 cMinted = cToken.balanceOf(address(this));
            cToken.mint(amount);
            cMinted = cToken.balanceOf(address(this)) - cMinted;
            require(cMinted > 0); // cMinted must be above 0, otherwise supply more
            balanceMap[uAddr][user].amount += cMinted;
        }
    }

    function withdrawTokenInternal(
        address uAddr,
        uint16 basisPoint,
        bool isOptimize,
        address user
    ) internal returns (uint256) {
        address receiver = isOptimize ? address(this) : user;
        Market market = balanceMap[uAddr][user].market;
        if (market == Market.AAVE) {
            IAaveV3 pool = IAaveV3(POOL);
            uint256 amount = (balanceMap[uAddr][user].amount * basisPoint) /
                10000;
            pool.withdraw(uAddr, amount, receiver);
            balanceMap[uAddr][user].amount -= amount;
            return amount;
        } else {
            ICompound cToken = ICompound(cAddrMap[market][uAddr]);
            uint256 amount = (balanceMap[uAddr][user].amount * basisPoint) /
                10000;
            uint256 uMinted = IERC20(uAddr).balanceOf(address(this));
            uint256 cMinted = cToken.balanceOf(address(this));
            cToken.redeem(amount);
            amount = IERC20(uAddr).balanceOf(address(this)) - uMinted;
            cMinted -= cToken.balanceOf(address(this));
            if (receiver != address(this))
                IERC20(uAddr).safeTransfer(receiver, amount);
            balanceMap[uAddr][user].amount -= cMinted;
            return amount;
        }
    }

    function supplyAvaxInternal(
        bool isOptimize,
        uint256 amount,
        address user
    ) internal {
        if (!isOptimize) {
            if (balanceMap[AVAX[0]][user].market == Market(0)) users.push(user);
            amount = msg.value;
        }

        Market market = compareInterestRates(AVAX[0], user);
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

    function tokenBalance(address uAddr) external view returns (uint256) {
        Market market = balanceMap[uAddr][msg.sender].market;
        if (market == Market.AAVE) {
            return balanceMap[uAddr][msg.sender].amount;
        } else {
            address cAddr = cAddrMap[market][uAddr];
            return uBalance(cAddr, balanceMap[uAddr][msg.sender].amount);
        }
    }

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

    function supplyToken(address uAddr, uint256 amount) external {
        supplyTokenInternal(uAddr, amount, false, msg.sender);
    }

    function withdrawToken(address uAddr, uint16 basisPoint)
        external
        returns (uint256)
    {
        require(basisPoint <= 10000);
        return withdrawTokenInternal(uAddr, basisPoint, false, msg.sender);
    }

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

    function optimizeToken(address uAddr) external {
        for (uint256 i = 0; i < users.length; i++) {
            uint256 amount = withdrawTokenInternal(
                uAddr,
                10000,
                true,
                users[i]
            );
            supplyTokenInternal(uAddr, amount, true, users[i]);
        }
    }

    function optimizeAvax() external payable {
        for (uint256 i = 0; i < users.length; i++) {
            uint256 amount = withdrawAvaxInternal(10000, true, users[i]);
            supplyAvaxInternal(true, amount, users[i]);
        }
    }
}

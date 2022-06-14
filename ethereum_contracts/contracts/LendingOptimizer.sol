pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "./libraries/DataTypes.sol";

interface IERC20Metadata {
    function decimals() external view returns (uint8);
}

interface CErc20 {
    function mint(uint256) external returns (uint256);

    function exchangeRateCurrent() external returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemUnderlying(uint256)
        external
        returns (
            uint256,
            uint128,
            uint128,
            uint128,
            uint128,
            uint128,
            uint40,
            address,
            address,
            address,
            address,
            uint8
        );
}

interface ILendingPool {
    function deposit(
        address asset,
        uint256 amount,
        address onBehalfOf,
        uint16 referralCode
    ) external;

    function getReserveData(address asset)
        external
        view
        returns (DataTypes.ReserveData memory);
}

interface CEth {
    function mint() external payable;

    function exchangeRateCurrent() external returns (uint256);

    function supplyRatePerBlock() external returns (uint256);

    function redeem(uint256) external returns (uint256);

    function redeemUnderlying(uint256) external returns (uint256);
}

interface WETHGateway {
    function depositETH(
        address lendingPool,
        address onBehalfOf,
        uint16 referralCode
    ) external payable;

contract LendingOptimizer {
    using SafeERC20 for IERC20;

    mapping(address => address) toC;

    function toCAddr() private {
        // SUSHI, COMP not included because not on aave
        toC[
            0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9
        ] = 0xe65cdB6479BaC1e22340E4E755fAE7E509EcD06c; // AAVE
        toC[
            0x0D8775F648430679A709E98d2b0Cb6250d2887EF
        ] = 0x6C8c6b02E7b2BE14d4fA6022Dfd6d75921D90E4E; // BAT
        toC[
            0x6B175474E89094C44Da98b954EedeAC495271d0F
        ] = 0x5d3a536E4D6DbD6114cc1Ead35777bAB948E3643; // DAI
        toC[
            0x956F47F50A910163D8BF957Cf5846D573E7f87CA
        ] = 0x7713DD9Ca933848F6819F38B8352D9A15EA73F67; // FEI
        toC[
            0x514910771AF9Ca656af840dff83E8264EcF986CA
        ] = 0xFAce851a4921ce59e912d19329929CE6da6EB0c7; // LINK
        toC[
            0x9f8F72aA9304c8B593d555F12eF6589cC3A579A2
        ] = 0x95b4eF2869eBD94BEb4eEE400a99824BF5DC325b; // MKR
        toC[
            0x0000000000085d4780B73119b644AE5ecd22b376
        ] = 0x12392F67bdf24faE0AF363c24aC620a2f67DAd86; // TUSD
        toC[
            0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984
        ] = 0x35A18000230DA775CAc24873d00Ff85BccdeD550; // UNI
        toC[
            0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
        ] = 0x39AA39c021dfbaE8faC545936693aC917d5E7563; // USDC
        toC[
            0x8E870D67F660D95d5be530380D0eC0bd388289E1
        ] = 0x041171993284df560249B57358F931D9eB7b925D; // USDP
        toC[
            0xdAC17F958D2ee523a2206206994597C13D831ec7
        ] = 0xf650C3d88D12dB855b8bf7D11Be6C55A4e07dCC9; // USDT
        toC[
            0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599
        ] = 0xC11b1268C1A384e55C48c2391d8d480264A3A7F4; // WBTC
        toC[
            0x0bc529c00C6401aEF6D220BE8C6Ea1667F6Ad93e
        ] = 0x80a2AE356fc9ef4305676f7a3E2Ed04e12C33946; // YFI
        toC[
            0xE41d2489571d322189246DaFA5ebDe1F4699F498
        ] = 0xB3319f5D18Bc0D84dD1b4825Dcde5d5f7266d407; // ZRX
    }

    function supplyTokenToCompound(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);
        CErc20 cToken = CErc20(toC[tokenAddr]);

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(tx.origin, address(this), amount);

        // approve compound contract to transfer from this contract
        token.safeApprove(toC[tokenAddr], amount);

        cToken.mint(amount);
    }

    function supplyTokenToAave(address tokenAddr, uint256 amount) private {
        IERC20 token = IERC20(tokenAddr);
        ILendingPool pool = ILendingPool(
            0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9
        ); // address is AAVE LendingPool

        // approve and transfer tokens from investor wallet to this contract
        token.safeTransferFrom(tx.origin, address(this), amount);

        // approve AAVE LendingPool contract to make a deposit
        token.safeApprove(0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9, amount);

        pool.deposit(
            tokenAddr,
            amount,
            address(this),
            /* referralCode= */
            0
        );
    }

    function supplyEth() external payable {
        CEth cToken = CEth(0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5); // cETH
        uint256 cInterestAdj = cToken.supplyRatePerBlock() *
            6570 *
            365 *
            (10**9);

        ILendingPool pool = ILendingPool(
            0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9
        );
        uint256 aInterestAdj = pool
            .getReserveData(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2)
            .currentLiquidityRate;

        WETHGateway wETH = WETHGateway(
            0xcc9a0B7c43DC2a5F023Bb9b738E45B0Ef6B06E04
        );

        // console.log(cInterestAdj);
        // console.log(aInterestAdj);

        if (cInterestAdj >= aInterestAdj) {
            cToken.mint{value: msg.value}();
        } else {
            wETH.depositETH{value: msg.value}(
                0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9,
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
        toCAddr();

        require(toC[tokenAddr] != 0x0000000000000000000000000000000000000000);

        IERC20 token = IERC20(tokenAddr);
        IERC20Metadata tokenMetadata = IERC20Metadata(tokenAddr);
        CErc20 cToken = CErc20(toC[tokenAddr]);
        ILendingPool pool = ILendingPool(
            0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9
        ); // AAVE LendingPool address

        // Interest rate formula adjusted to directly compare compound vs aave
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

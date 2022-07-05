//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
// import "@openzeppelin/contracts/math/SafeMath.sol";

import "./interfaces/IHomoraBank.sol";
import "./interfaces/IUniswapV2Pair.sol";
import "./interfaces/IUniswapV2Factory.sol";
import "./interfaces/IUniswapV2Router.sol";
import "./interfaces/IOracle.sol";

contract DeltaNeutralVault is ERC20 {
    struct UserPosition {
        address owner;
        uint256 positionID;
        uint256 amtAUser; // assume to be stable token
        uint256 amtBUser; // assume to be asset token
        uint256 amtAAfterSwap; // token A
        uint256 amtABorrow;
        uint256 amtBBorrow;
        uint256 amtLPShare;
    }

    struct VaultPosition {
        address owner;
        address collToken;
        uint256 collId;
        uint256 collateralSize;
        uint256 positionID;
        uint256 stablePositionEquity;
        uint256 stablePositionDebtValue;
        uint256 assetPositionEquity;
        uint256 assetPositionDebtValue;
    }

    struct Amounts {
        uint256 amtAUser; // Supplied tokenA amount
        uint256 amtBUser; // Supplied tokenB amount
        uint256 amtLPUser; // Supplied LP token amount
        uint256 amtABorrow; // Borrow tokenA amount
        uint256 amtBBorrow; // Borrow tokenB amount
        uint256 amtLPBorrow; // Borrow LP token amount
        uint256 amtAMin; // Desired tokenA amount (slippage control)
        uint256 amtBMin; // Desired tokenB amount (slippage control)
    }

    uint256 private constant _NOT_ENTERED = 1;
    uint256 private constant _ENTERED = 2;
    uint256 private _status = _NOT_ENTERED;
    uint256 private _HomoraPositionID = 0;
    uint256 private _TR; // target debt ratio * 10000
    uint256 private _MR; // maximum debt ratio * 10000

    // --- config ---
    address public stableToken;
    address public assetToken;
    address public homoraBank;
    address public spell;
    address public lpToken;
    uint256 public leverageLevel;
    uint256 public nextPositionID = 0;
    VaultPosition public position;

    // --- state ---
    mapping(uint256 => UserPosition) public UserPositions;

    constructor(
        string memory _name,
        string memory _symbol,
        address _stableToken,
        address _assetToken,
        uint256 _leverageLevel,
        address _homoraBank,
        address _spell,
        address _lpToken
    ) ERC20(_name, _symbol) {
        stableToken = _stableToken;
        assetToken = _assetToken;
        leverageLevel = _leverageLevel;
        homoraBank = _homoraBank;
        spell = _spell;
        lpToken = _lpToken;
    }

    modifier nonReentrant() {
        require(_status == _NOT_ENTERED, "Reentrant call");
        _status = _ENTERED;
        _;
        _status = _NOT_ENTERED;
    }

    function deltaNeutral(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    )
        internal
        returns (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        )
    {
        _stableTokenAmount = _stableTokenDepositAmount;
        // UniswapV2Router contract address
        address _router = 0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D;
        IUniswapV2Router router = IUniswapV2Router(_router);

        // swap all assetTokens into stableTokens
        if (_assetTokenDepositAmount > 0) {
            address[] memory path = new address[](2);
            (path[0], path[1]) = (assetToken, stableToken);
            uint256[] memory amount = router.swapExactTokensForTokens(
                _assetTokenDepositAmount,
                0,
                path,
                address(this),
                block.timestamp
            );
            // update the stableToken amount
            _stableTokenAmount += amount[1];
        }

        // total stableToken leveraged amount
        uint256 totalAmount = _stableTokenAmount * leverageLevel;
        uint256 desiredAmount = totalAmount / 2;
        _stableTokenBorrowAmount = desiredAmount - _stableTokenAmount;
        _assetTokenBorrowAmount = desiredAmount / getTokenPrice();

        return (
            _stableTokenAmount,
            0,
            _stableTokenBorrowAmount,
            _assetTokenBorrowAmount
        );
    }

    function deposit(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) public payable nonReentrant returns (uint256) {
        (
            uint256 _stableTokenAmount,
            uint256 _assetTokenAmount,
            uint256 _stableTokenBorrowAmount,
            uint256 _assetTokenBorrowAmount
        ) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount);
        // Encode the calling function.
        bytes memory data = abi.encodePacked(
            bytes4(
                keccak256(
                    "addLiquidityWERC20(address tokenA, address tokenB, Amounts amt)"
                )
            ),
            abi.encode(
                stableToken,
                assetToken,
                [
                    _stableTokenAmount,
                    _assetTokenAmount,
                    0,
                    _stableTokenBorrowAmount,
                    _assetTokenBorrowAmount,
                    0,
                    0,
                    0
                ]
            )
        );
        _HomoraPositionID = IHomoraBank(homoraBank).execute(
            _HomoraPositionID,
            spell,
            data
        );
        console.log(_HomoraPositionID);
        position.positionID = _HomoraPositionID;
        uint256 userPositionID = nextPositionID++;
        return userPositionID;
    }

    /// @notice Get assetToken's price in terms of stableToken
    function getTokenPrice() public view returns (uint256) {
        address _uniswapV2Factory = 0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac;
        IUniswapV2Factory factory = IUniswapV2Factory(_uniswapV2Factory);
        IUniswapV2Pair pair = IUniswapV2Pair(
            factory.getPair(stableToken, assetToken)
        );
        require(address(pair) != address(0), "Pair does not exist");

        (uint256 reserve0, uint256 reserve1, ) = pair.getReserves();
        if (pair.token0() == stableToken) {
            return reserve1 / reserve0;
        } else {
            return reserve0 / reserve1;
        }
    }

    function test() public view returns (uint) {
        address _uniswapV2Factory = 0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac;
        IUniswapV2Factory factory = IUniswapV2Factory(_uniswapV2Factory);
        IUniswapV2Pair pair = IUniswapV2Pair(
            factory.getPair(stableToken, assetToken)
        );
        require(address(pair) != address(0), "Pair does not exist");

        (uint256 reserve0, uint256 reserve1, ) = pair.getReserves();
        console.log(reserve0);
        console.log(reserve1);
        return reserve1;
    }

    /// @notice Calculate the debt ratio and return the ratio * 10000
    function _getDebtRatio() internal view returns (uint256) {
        uint256 collateralValue = IHomoraBank(homoraBank).getCollateralETHValue(
            _HomoraPositionID
        );
        uint256 borrowValue = IHomoraBank(homoraBank).getBorrowETHValue(
            _HomoraPositionID
        );
        return (borrowValue * 10000) / collateralValue;
    }

    /// @notice Query the Token factors for token, multiplied by 1e4
    function _getTokenFactor(address token)
        internal
        view
        returns (
            uint16 borrowFactor,
            uint16 collateralFactor,
            uint16 liqIncentive
        )
    {
        IOracle oracle = IOracle(0xeED9cfb1e69792AaeE0BF55F6af617853E9f29B8);
        return oracle.tokenFactors(token);
    }

    /// @notice Query the Homora's borrow credit factor for token, multiplied by 1e4
    function getBorrowFactor(address token) public view returns (uint16) {
        (uint16 borrowFactor, , ) = _getTokenFactor(token);
        return borrowFactor;
    }

    /// @notice Query the Homora's collateral credit factor for the LP token, multiplied by 1e4
    function getCollateralFactor() public view returns (uint16) {
        (, uint16 stableFactor, ) = _getTokenFactor(stableToken);
        (, uint16 assetFactor, ) = _getTokenFactor(assetToken);
        return stableFactor > assetFactor ? assetFactor : stableFactor;
    }

    function getVaultPositionInfo() public returns (VaultPosition memory) {
        (
            address owner,
            address collToken,
            uint256 collId,
            uint256 collateralSize
        ) = IHomoraBank(homoraBank).getPositionInfo(_HomoraPositionID);
        position.owner = owner;
        position.collToken = collToken;
        position.collId = collId;
        position.collateralSize = collateralSize;
        return position;
    }
}

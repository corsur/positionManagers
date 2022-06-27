//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

import "./interfaces/IHomoraBank.sol";

contract DeltaNeutralVault is ERC20  {
    struct Position {
        uint positionId;
        address owner;
        uint256 stableAmount;
        uint256 assetAmount;
        uint256 stableDebtAmount;
        uint256 assetDebtAmount;
    }

    uint private constant _NOT_ENTERED = 1;
    uint private constant _ENTERED = 2;
    uint private _status = _NOT_ENTERED;

    // --- config ---
    address public stableToken;
    address public assetToken;
    address public homoraBank;
    address public spell;
    address public lpToken;
    uint256 public leverageLevel;

    // --- state ---
    mapping(uint => Position) public positions;


    constructor (
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
    ) internal returns (
        uint256 _stableTokenAmount,
        uint256 _assetTokenAmount,
        uint256 _stableTokenBorrowAmount,
        uint256 _assetTokenBorrowAmount
    )
    {
        return (_stableTokenDepositAmount, _assetTokenDepositAmount, 0, 0);
    }

    function deposit(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) public payable nonReentrant returns (uint256) {
        (uint256 _stableTokenAmount, uint256 _assetTokenAmount, uint256 _stableTokenBorrowAmount, uint256 _assetTokenBorrowAmount) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount);
        // Encode the calling function.
        bytes memory data = abi.encodePacked(
            bytes4(keccak256('addLiquidityWERC20(address tokenA, address tokenB, Amounts amt)')),
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
        uint res = IHomoraBank(homoraBank).execute(
            0,
            spell,
            data
            );
        console.log(res);
        return 1;
    }
    
}
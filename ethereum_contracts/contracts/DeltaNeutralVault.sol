//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.13;

import "hardhat/console.sol";

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/math/SafeMath.sol";

import "./interfaces/IHomoraBank.sol";
import "./interfaces/ISpell.sol";


// Is there some other way?
library HomoraSafeMath {
  using SafeMath for uint;

  /// @dev Computes round-up division.
  function ceilDiv(uint a, uint b) internal pure returns (uint) {
    return a.add(b).sub(1).div(b);
  }
}

contract DeltaNeutralVault is ERC20, ReentrancyGuard {
    using SafeMath for uint256;
    using HomoraSafeMath for uint256;

    struct Position {
        uint256 stableDebtShareAmount;
        uint256 assetDebtShareAmount;
        uint256 collShareAmount;
    }

    uint private constant _NO_ID = 0;

    // --- config ---
    address public stableToken;
    address public assetToken;
    address public spell;
    address public lpToken;
    address public wstaking;
    uint256 public leverageLevel;
    IHomoraBank public homoraBank;

    // --- state ---
    mapping(address => Position) public positions;
    uint256 public homoraBankPosId;
    uint256 public totalStableDebtShareAmount;
    uint256 public totalAssetDebtShareAmount;
    uint256 public totalCollShareAmount;

    // --- event ---
    event LogDeposit(address indexed _from, uint256 stableDebtShareAmount, uint256 assetDebtShareAmount, uint collShareAmount);


    constructor (
        string memory _name,
        string memory _symbol,
        address _stableToken,
        address _assetToken,
        uint256 _leverageLevel,
        address _homoraBank,
        address _spell,
        address _lpToken,
        address _wstaking
    ) ERC20(_name, _symbol) {
        stableToken = _stableToken;
        assetToken = _assetToken;
        homoraBank = IHomoraBank(_homoraBank);
        leverageLevel = _leverageLevel;
        spell = _spell;
        lpToken = _lpToken;
        wstaking = _wstaking;
        homoraBankPosId = _NO_ID;
        totalStableDebtShareAmount = 0;
        totalAssetDebtShareAmount = 0;
        totalCollShareAmount = 0;
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
        return (_stableTokenDepositAmount, _assetTokenDepositAmount, _stableTokenDepositAmount * leverageLevel, 0);
    }

    function currentDebtAmount() internal view returns (uint, uint) {
        (address[] memory tokens, uint[] memory debts) = homoraBank.getPositionDebts(homoraBankPosId);
        uint256 stableTokenDebtAmount = 0;
        uint256 assetTokenDebtAmount = 0;

        for (uint i = 0; i < tokens.length; i++) {
            if (tokens[i] == stableToken) {
                stableTokenDebtAmount = debts[i];
            }
            if (tokens[i] == assetToken) {
                assetTokenDebtAmount = debts[i];
            }
        }
        return (stableTokenDebtAmount, assetTokenDebtAmount);
    }

    function withdraw(
        uint256 ratio
    ) external nonReentrant {



    }

    function deposit(
        uint256 _stableTokenDepositAmount,
        uint256 _assetTokenDepositAmount
    ) external payable nonReentrant {
        // Transfer user's deposit.
        if (_stableTokenDepositAmount > 0) IERC20(stableToken).transferFrom(msg.sender, address(this), _stableTokenDepositAmount);
        if (_assetTokenDepositAmount > 0) IERC20(assetToken).transferFrom(msg.sender, address(this), _assetTokenDepositAmount);

        (uint256 _stableTokenAmount, uint256 _assetTokenAmount, uint256 _stableTokenBorrowAmount, uint256 _assetTokenBorrowAmount) = deltaNeutral(_stableTokenDepositAmount, _assetTokenDepositAmount);

        // Record original debts and colletral size.
        (uint256 originalStableDebtAmount, uint256 originalAssetDebtAmount) = currentDebtAmount();
        (, , , uint256 originalCollSize) = homoraBank.getPositionInfo(homoraBankPosId);

        // Approve HomoraBank transferring tokens.
        IERC20(stableToken).approve(address(homoraBank), 2**256-1);
        IERC20(assetToken).approve(address(homoraBank), 2**256-1);

        // Encode the calling function.
        bytes memory data0 = abi.encodeWithSelector(
            bytes4(keccak256("addLiquidityWERC20(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256))")),
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
        );

        // bytes memory data1 = abi.encodeWithSelector(
        //     bytes4(keccak256("addLiquidityWStakingRewards(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),address)")),
        //     stableToken,
        //     assetToken,
        //     [
        //         0,
        //         0,
        //         0,
        //         0,
        //         0,
        //         0,
        //         0,
        //         0
        //     ],
        //     wstaking
        // );

        uint res = IHomoraBank(homoraBank).execute(
            homoraBankPosId,
            spell,
            data0
            );

        if (homoraBankPosId == _NO_ID) {
            homoraBankPosId = res;
        }

        // Cancel HomoraBank's allowance.
        IERC20(stableToken).approve(address(homoraBank), 0);
        IERC20(assetToken).approve(address(homoraBank), 0);

        // Calculate share amount.
        (uint256 finalStableDebtAmount, uint256 finalAssetDebtAmount) = currentDebtAmount();
        (, , ,uint256 finalCollSize) = homoraBank.getPositionInfo(homoraBankPosId);

        uint256 stableDebtAmount = finalStableDebtAmount.sub(originalStableDebtAmount);
        uint256 assetDebtAmount = finalAssetDebtAmount.sub(originalAssetDebtAmount);
        uint256 collSize = finalCollSize - originalCollSize;

        uint256 stableDebtShareAmount = originalStableDebtAmount == 0 ? stableDebtAmount : stableDebtAmount.mul(totalStableDebtShareAmount).ceilDiv(originalStableDebtAmount);
        uint256 assetDebtShareAmount = originalAssetDebtAmount == 0 ? assetDebtAmount : assetDebtAmount.mul(totalAssetDebtShareAmount).ceilDiv(originalAssetDebtAmount);
        uint256 collShareAmount = originalCollSize == 0 ? collSize : collSize.mul(totalCollShareAmount).ceilDiv(originalCollSize);

        // Update total position state.
        totalStableDebtShareAmount += stableDebtShareAmount;
        totalAssetDebtShareAmount += assetDebtShareAmount;
        totalCollShareAmount += collShareAmount;

        // Update deposit owner's position state.
        positions[msg.sender].stableDebtShareAmount += stableDebtShareAmount;
        positions[msg.sender].assetDebtShareAmount += assetDebtShareAmount;
        positions[msg.sender].collShareAmount += collShareAmount;

        // Return leftover funds to user.
        IERC20(stableToken).transfer(msg.sender, IERC20(stableToken).balanceOf(address(this)));
        IERC20(assetToken).transfer(msg.sender, IERC20(assetToken).balanceOf(address(this)));
        emit LogDeposit(msg.sender, stableDebtShareAmount, assetDebtShareAmount, collShareAmount);
    }
    
}
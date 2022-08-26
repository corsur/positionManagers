//SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "../interfaces/IHomoraAdapter.sol";
import "../libraries/VaultLib.sol";

library HomoraAdapterLib {
    using SafeERC20 for IERC20;

    bytes4 private constant ERC20_APPROVE_SIG =
        bytes4(keccak256("approve(address,uint256)"));
    bytes4 private constant ERC20_TRANSFER_SIG =
        bytes4(keccak256("transfer(address,uint256)"));

    /// @dev fund adapter contract and approve HomoraBank to use the fund.
    /// @param tokenAddr the token to transfer and approve.
    /// @param amount the amount to transfer and approve.
    function fundAdapterAndApproveHomoraBank(
        IHomoraAdapter self,
        address homoraBank,
        address tokenAddr,
        uint256 amount
    ) external {
        IERC20(tokenAddr).safeTransfer(address(self), amount);
        self.doWork(
            tokenAddr,
            0,
            abi.encodeWithSelector(ERC20_APPROVE_SIG, homoraBank, amount)
        );
    }

    function pullTokenFromAdapter(
        IHomoraAdapter self,
        address tokenAddr,
        uint256 amount
    ) internal {
        if (amount > 0) {
            self.doWork(
                tokenAddr,
                0,
                abi.encodeWithSelector(
                    ERC20_TRANSFER_SIG,
                    address(this),
                    amount
                )
            );
        }
    }

    function pullETHFromAdapter(IHomoraAdapter self, uint256 amount) internal {
        self.doWork(address(this), amount, "");
    }

    function pullAllAssets(IHomoraAdapter self, address[] memory tokens)
        public
    {
        for (uint256 i = 0; i < tokens.length; i++) {
            pullTokenFromAdapter(
                self,
                tokens[i],
                IERC20(tokens[i]).balanceOf(address(self))
            );
        }
        pullETHFromAdapter(self, address(self).balance);
    }

    function homoraExecute(
        IHomoraAdapter self,
        ContractInfo storage contractInfo,
        uint256 posId,
        bytes memory spellBytes,
        PairInfo storage pairInfo,
        uint256 value
    ) external returns (uint256) {
        uint256 returnPosId = self.homoraExecute{value: value}(
            posId,
            contractInfo.spell,
            value,
            spellBytes
        );
        address[] memory tokens = new address[](4);
        tokens[0] = pairInfo.stableToken;
        tokens[1] = pairInfo.assetToken;
        tokens[2] = pairInfo.lpToken;
        tokens[3] = pairInfo.rewardToken;
        pullAllAssets(self, tokens);
        return returnPosId;
    }
}

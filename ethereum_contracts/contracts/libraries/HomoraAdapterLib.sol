//SPDX-License-Identifier: MIT
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "../interfaces/IHomoraAdapter.sol";
import "../libraries/VaultLib.sol";

library HomoraAdapterLib {
    using SafeERC20 for IERC20;

    bytes4 private constant HOMORA_EXECUTE_SIG =
        bytes4(keccak256("execute(uint256,address,bytes)"));
    bytes4 private constant ERC20_APPROVE_SIG =
        bytes4(keccak256("approve(address,uint256)"));
    bytes4 private constant ERC20_TRANSFER_SIG =
        bytes4(keccak256("transfer(address,uint256)"));

    function adapterApproveHomoraBank(
        IHomoraAdapter self,
        address homoraBank,
        address tokenAddr,
        uint256 amount
    ) public {
        self.doWork(
            tokenAddr,
            0,
            abi.encodeWithSelector(ERC20_APPROVE_SIG, homoraBank, amount)
        );
    }

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
        adapterApproveHomoraBank(self, homoraBank, tokenAddr, amount);
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

    function pullAllAssets(
        IHomoraAdapter self,
        address tokenA,
        address tokenB,
        address rewardToken
    ) public {
        pullTokenFromAdapter(
            self,
            tokenA,
            IERC20(tokenA).balanceOf(address(self))
        );
        pullTokenFromAdapter(
            self,
            tokenB,
            IERC20(tokenB).balanceOf(address(self))
        );
        pullTokenFromAdapter(
            self,
            rewardToken,
            IERC20(rewardToken).balanceOf(address(self))
        );
        pullETHFromAdapter(self, address(self).balance);
    }

    function homoraExecute(
        IHomoraAdapter self,
        ContractInfo storage contractInfo,
        uint256 posId,
        bytes memory spellBytes,
        PairInfo storage pairInfo,
        uint256 value
    ) internal returns (bytes memory) {
        bytes memory homoraExecuteBytes = abi.encodeWithSelector(
            HOMORA_EXECUTE_SIG,
            posId,
            contractInfo.spell,
            spellBytes
        );

        bytes memory returndata = self.doWork{value: value}(
            contractInfo.bank,
            value,
            homoraExecuteBytes
        );
        pullAllAssets(
            self,
            pairInfo.stableToken,
            pairInfo.assetToken,
            pairInfo.rewardToken
        );
        return returndata;
    }
}

//SPDX-License-Identifier: Unlicense
pragma solidity >=0.8.0 <0.9.0;

import "@openzeppelin/contracts/access/Ownable.sol";
import "../interfaces/IHomoraBank.sol";

// Immutable adapter contract to interact with HomoraBank.
contract HomoraAdapter is Ownable {
    /// @dev Homora bank.
    IHomoraBank public homoraBank;

    /// @dev List of whitelisted contracts that can interact with HomoraAdapter.
    mapping(address => bool) public whitelistedCallers;

    /// @dev List of whitelisted contracts that can be called via HomoraAdapter.
    mapping(address => bool) public whitelistedTargets;

    /// @dev Only whitelisted contract addresses can interact with the annotated functions.
    modifier onlyWhitelistedCaller() {
        require(whitelistedCallers[msg.sender], "unauthorized caller");
        _;
    }

    constructor(address homoraBankAddr) {
        homoraBank = IHomoraBank(homoraBankAddr);
    }

    /// @dev Call to the target using the given data. It can not be used to call
    ///     Homora bank.
    /// @param target The address target to call.
    /// @param value The amount of native token to send along to callee.
    /// @param data The data used in the call.
    function doWork(
        address target,
        uint256 value,
        bytes calldata data
    ) external payable onlyWhitelistedCaller returns (bytes memory) {
        require(whitelistedTargets[target], "unauthorized target.");
        (bool ok, bytes memory returndata) = target.call{value: value}(data);
        if (!ok) {
            if (returndata.length > 0) {
                // The easiest way to bubble the revert reason is using memory via assembly
                // solhint-disable-next-line no-inline-assembly
                assembly {
                    let returndata_size := mload(returndata)
                    revert(add(32, returndata), returndata_size)
                }
            } else {
                revert("bad doWork call");
            }
        }

        // Call status is okay.
        return returndata;
    }

    /// @dev Dedicated call function for Homora Bank.
    /// @param positionId The position id associated with this call.
    /// @param spell Homora spell contract address.
    /// @param data Bytes data for Homora to execute.
    function homoraExecute(
        uint256 positionId,
        address spell,
        bytes memory data
    ) external payable onlyWhitelistedCaller returns (uint256) {
        return homoraBank.execute(positionId, spell, data);
    }

    receive() external payable {}

    /// @dev Grant or revoke access for caller contracts.
    /// @param caller The address target to call.
    /// @param val The data used in the call.
    function setCaller(address caller, bool val) external onlyOwner {
        require(caller != address(0), "Invalid caller");
        whitelistedCallers[caller] = val;
    }

    function setTarget(address target, bool val) external onlyOwner {
        // Explicitly disallow HomoraBank as a generic calling target.
        require(
            target != address(homoraBank),
            "Disallow generic call to Homora bank"
        );
        require(target != address(0), "Invalid target");
        whitelistedTargets[target] = val;
    }
}

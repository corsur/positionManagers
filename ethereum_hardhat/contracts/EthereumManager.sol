//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.4;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/Wormhole.sol";

struct OwnershipInfo {
    address ownerAddr; // The owner of the position.
    uint16 chainId; // The chain this position belongs to.
}

struct PositionInfo {
    uint128 positionId;
    uint16 chainId;
}

contract EthereumManager is Initializable, UUPSUpgradeable, OwnableUpgradeable {
    uint16 private constant TERRA_CHAIN_ID = 3;

    // --- Cross-chain instruction format --- //
    // [uint128] position_id
    // [uint16] target_chain_id
    // [uint64] strategy_id
    // [uint32] num_token_transferred
    // [var_len] num_token_transferred * sizeof(sequence_number)
    // [uint32] encoded_action_len
    // [var_len] base64 encoding of params needed by action.

    // These nonce numbers are not used by wormhole yet. They are included
    // here for informational purpose only.
    uint32 private constant INSTRUCTION_NONCE = 1324532;
    uint32 private constant TOKEN_TRANSFER_NONCE = 15971121;

    uint8 private CONSISTENCY_LEVEL;
    address private WORMHOLE_TOKEN_BRIDGE;
    bytes32 private TERRA_MANAGER_ADDRESS;

    // Position ids for Ethereum.
    uint128 private nextPositionId;

    // Wormhole-wrapped Terra stablecoin tokens that are whitelisted in Terra Anchor Market. Example: UST.
    mapping(address => bool) public whitelistedStableTokens;

    // Stores hashes of completed incoming token transfer.
    mapping(bytes32 => bool) public completedTokenTransfers;

    // Stores wallet address to PositionInfo mapping.
    mapping(uint128 => OwnershipInfo) public positionToOwnership;

    // `initializer` is a modifier from OpenZeppelin to ensure contract is
    // only initialized once (thanks to Initializable).
    function initialize(
        uint8 _consistencyLevel,
        address _wust,
        address _wormholeTokenBridge,
        bytes32 _terraManagerAddress
    ) public initializer {
        __Ownable_init();
        __UUPSUpgradeable_init();
        CONSISTENCY_LEVEL = _consistencyLevel;
        // TODO: support more stablecoins.
        whitelistedStableTokens[_wust] = true;
        WORMHOLE_TOKEN_BRIDGE = _wormholeTokenBridge;
        TERRA_MANAGER_ADDRESS = _terraManagerAddress;
        console.log("Successfully deployed contract.");
    }

    // Only owner of this logic contract can upgrade.
    function _authorizeUpgrade(address) internal override onlyOwner {}

    function createPosition(
        uint64 strategyId,
        uint16 targetChainId,
        address token,
        uint256 amount,
        uint32 encodedActionLen,
        bytes calldata encodedAction
    ) external {
        // Check that `token` is a whitelisted stablecoin token.
        require(whitelistedStableTokens[token]);

        // Craft ownership info for bookkeeping.
        uint128 positionId = nextPositionId++;
        OwnershipInfo memory ownershipInfo = OwnershipInfo(
            msg.sender,
            targetChainId
        );
        positionToOwnership[positionId] = ownershipInfo;

        handleExecuteStrategy(
            strategyId,
            targetChainId,
            token,
            amount,
            positionId,
            encodedActionLen,
            encodedAction
        );
    }

    function executeStrategy(
        uint128 positionId,
        uint64 strategyId,
        address token,
        uint256 amount,
        uint32 encodedActionLen,
        bytes calldata encodedAction
    ) external {
        // Check that `token` is a whitelisted stablecoin token.
        require(whitelistedStableTokens[token]);

        // Check that msg.sender owns this position.
        require(positionToOwnership[positionId].ownerAddr == msg.sender);

        handleExecuteStrategy(
            strategyId,
            positionToOwnership[positionId].chainId,
            token,
            amount,
            positionId,
            encodedActionLen,
            encodedAction
        );
    }

    function handleExecuteStrategy(
        uint64 strategyId,
        uint16 targetChainId,
        address token,
        uint256 amount,
        uint128 positionId,
        uint32 encodedActionLen,
        bytes calldata encodedAction
    ) internal {
        // Transfer ERC-20 token from message sender to this contract.
        SafeERC20.safeTransferFrom(
            IERC20(token),
            msg.sender,
            address(this),
            amount
        );
        // Allow wormhole to spend USTw from this contract.
        SafeERC20.safeApprove(IERC20(token), WORMHOLE_TOKEN_BRIDGE, amount);
        // Initiate token transfer.
        uint64 tokenTransferSequence = WormholeTokenBridge(
            WORMHOLE_TOKEN_BRIDGE
        ).transferTokens(
                token,
                amount,
                targetChainId,
                TERRA_MANAGER_ADDRESS,
                0,
                TOKEN_TRANSFER_NONCE
            );
        // Send instruction message to Terra manager.
        WormholeCoreBridge(
            WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE).wormhole()
        ).publishMessage(
                INSTRUCTION_NONCE,
                abi.encodePacked(
                    positionId,
                    targetChainId,
                    strategyId,
                    uint32(1),
                    tokenTransferSequence,
                    encodedActionLen,
                    encodedAction
                ),
                CONSISTENCY_LEVEL
            );
    }

    function getPositions(address user)
        external
        view
        returns (PositionInfo[] memory)
    {
        uint128 positionCount = 0;
        for (uint32 i = 0; i < nextPositionId; i++) {
            if (positionToOwnership[i].ownerAddr == user) {
                positionCount++;
            }
        }

        uint128 userIndex = 0;
        PositionInfo[] memory positionIdVec = new PositionInfo[](positionCount);
        for (
            uint32 i = 0;
            i < nextPositionId && userIndex < positionCount;
            i++
        ) {
            if (positionToOwnership[i].ownerAddr == user) {
                positionIdVec[userIndex++] = PositionInfo(
                    i,
                    positionToOwnership[i].chainId
                );
            }
        }
        return positionIdVec;
    }
}

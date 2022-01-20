//SPDX-License-Identifier: Unlicense
pragma solidity ^0.8.4;

import "hardhat/console.sol";

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import "contracts/BytesLib.sol";
import "contracts/Wormhole.sol";

contract EthereumManager is Initializable, UUPSUpgradeable, OwnableUpgradeable {
    uint16 private constant TERRA_CHAIN_ID = 3;

    uint8 private constant OP_CODE_DEPOSIT_STABLE = 0;
    uint8 private constant OP_CODE_REPAY_STABLE = 1;
    uint8 private constant OP_CODE_UNLOCK_COLLATERAL = 2;
    uint8 private constant OP_CODE_BORROW_STABLE = 3;
    uint8 private constant OP_CODE_CLAIM_REWARDS = 4;
    uint8 private constant OP_CODE_REDEEM_STABLE = 5;
    uint8 private constant OP_CODE_LOCK_COLLATERAL = 6;

    uint32 private constant INSTRUCTION_NONCE = 1324532;
    uint32 private constant TOKEN_TRANSFER_NONCE = 15971121;

    uint8 private CONSISTENCY_LEVEL;
    address private WORMHOLE_TOKEN_BRIDGE;
    bytes32 private TERRA_ANCHOR_BRIDGE_ADDRESS;

    // Wormhole-wrapped Terra stablecoin tokens that are whitelisted in Terra Anchor Market. Example: UST.
    mapping(address => bool) public whitelistedStableTokens;
    // Wormhole-wrapped Terra Anchor yield-generating tokens that can be redeemed for Terra stablecoins. Example: aUST.
    mapping(address => bool) public whitelistedAnchorStableTokens;
    // Wormhole-wrapped Terra cw20 tokens that can be used as collateral in Anchor. Examples: bLUNA, bETH.
    mapping(address => bool) public whitelistedCollateralTokens;

    // Stores hashes of completed incoming token transfer.
    mapping(bytes32 => bool) public completedTokenTransfers;

    function initialize(
        uint8 _consistencyLevel,
        address _wust,
        address _aust,
        address[] memory _collateralTokens,
        address _wormholeTokenBridge,
        bytes32 _terraAnchorBridgeAddress
    ) initializer public {
        __Ownable_init();
        __UUPSUpgradeable_init();
        CONSISTENCY_LEVEL = _consistencyLevel;
        whitelistedStableTokens[_wust] = true;
        whitelistedAnchorStableTokens[_aust] = true;
        for (uint8 i = 0; i < _collateralTokens.length; i++) {
            whitelistedCollateralTokens[_collateralTokens[i]] = true;
        }
        WORMHOLE_TOKEN_BRIDGE = _wormholeTokenBridge;
        TERRA_ANCHOR_BRIDGE_ADDRESS = _terraAnchorBridgeAddress;
        console.log("Deployed Contract");
    }

    function _authorizeUpgrade(address) internal override onlyOwner {}

    function encodeAddress(address addr)
        internal
        pure
        returns (bytes32 encodedAddress)
    {
        return bytes32(uint256(uint160(addr)));
    }

    function handleStableToken(
        address token,
        uint256 amount,
        uint8 opCode
    ) internal {
        // Check that `token` is a whitelisted stablecoin token.
        // require(whitelistedStableTokens[token]);
        handleToken(token, amount, opCode);
    }

    function handleToken(
        address token,
        uint256 amount,
        uint8 opCode
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
                TERRA_CHAIN_ID,
                TERRA_ANCHOR_BRIDGE_ADDRESS,
                0,
                TOKEN_TRANSFER_NONCE
            );
        // Send instruction message to Terra manager.
        WormholeCoreBridge(
            WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE).wormhole()
        ).publishMessage(
                INSTRUCTION_NONCE,
                abi.encodePacked(
                    opCode,
                    encodeAddress(msg.sender),
                    tokenTransferSequence
                ),
                CONSISTENCY_LEVEL
            );
    }

    function depositStable(address token, uint256 amount) external {
        handleStableToken(token, amount, OP_CODE_DEPOSIT_STABLE);
    }

    function repayStable(address token, uint256 amount) external {
        handleStableToken(token, amount, OP_CODE_REPAY_STABLE);
    }

    function unlockCollateral(
        bytes32 collateralTokenTerraAddress,
        uint128 amount
    ) external {
        WormholeCoreBridge(
            WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE).wormhole()
        ).publishMessage(
                INSTRUCTION_NONCE,
                abi.encodePacked(
                    OP_CODE_UNLOCK_COLLATERAL,
                    encodeAddress(msg.sender),
                    collateralTokenTerraAddress,
                    amount
                ),
                CONSISTENCY_LEVEL
            );
    }

    function borrowStable(uint256 amount) external {
        WormholeCoreBridge(
            WormholeTokenBridge(WORMHOLE_TOKEN_BRIDGE).wormhole()
        ).publishMessage(
                INSTRUCTION_NONCE,
                abi.encodePacked(
                    OP_CODE_BORROW_STABLE,
                    encodeAddress(msg.sender),
                    amount
                ),
                CONSISTENCY_LEVEL
            );
    }

    function redeemStable(address token, uint256 amount) external {
        // require(whitelistedAnchorStableTokens[token]);
        handleToken(token, amount, OP_CODE_REDEEM_STABLE);
    }

    function lockCollateral(address token, uint256 amount) external {
        // require(whitelistedCollateralTokens[token]);
        handleToken(token, amount, OP_CODE_LOCK_COLLATERAL);
    }

    struct IncomingTokenTransferInfo {
        uint16 chainId;
        bytes32 tokenRecipientAddress;
        uint64 tokenTransferSequence;
        uint64 instructionSequence;
    }

    using BytesLib for bytes;

    function parseIncomingTokenTransferInfo(bytes memory encoded)
        public
        pure
        returns (IncomingTokenTransferInfo memory incomingTokenTransferInfo)
    {
        uint256 index = 0;

        incomingTokenTransferInfo.chainId = encoded.toUint16(index);
        index += 2;

        incomingTokenTransferInfo.tokenRecipientAddress = encoded
            .toBytes32(index);
        index += 32;

        incomingTokenTransferInfo.tokenTransferSequence = encoded.toUint64(
            index
        );
        index += 8;

        incomingTokenTransferInfo.instructionSequence = encoded.toUint64(
            index
        );
        index += 8;

        require(
            encoded.length == index,
            "invalid IncomingTokenTransferInfo encoded length"
        );
    }

    // operations are bundled into two messages:
    // - a token transfer messsage from the token bridge
    // - a generic message providing context to the token transfer
    function processTokenTransferInstruction(
        bytes memory encodedIncomingTokenTransferInfo,
        bytes memory encodedTokenTransfer
    ) external {
        WormholeTokenBridge wormholeTokenBridge = WormholeTokenBridge(
            WORMHOLE_TOKEN_BRIDGE
        );
        WormholeCoreBridge wormholeCoreBridge = WormholeCoreBridge(
            wormholeTokenBridge.wormhole()
        );

        (
            WormholeCoreBridge.VM memory incomingTokenTransferInfoVM,
            bool validIncomingTokenTransferInfo,
            string memory reasonIncomingTokenTransferInfo
        ) = wormholeCoreBridge.parseAndVerifyVM(
                encodedIncomingTokenTransferInfo
            );
        require(
            validIncomingTokenTransferInfo,
                reasonIncomingTokenTransferInfo
        );
        require(
            incomingTokenTransferInfoVM.emitterChainId == TERRA_CHAIN_ID
        );
        require(
            incomingTokenTransferInfoVM.emitterAddress ==
                TERRA_ANCHOR_BRIDGE_ADDRESS
        );
        require(!completedTokenTransfers[incomingTokenTransferInfoVM.hash]);

        // block replay attacks
        completedTokenTransfers[incomingTokenTransferInfoVM.hash] = true;
        IncomingTokenTransferInfo
            memory incomingTokenTransferInfo = parseIncomingTokenTransferInfo(
                incomingTokenTransferInfoVM.payload
            );

        (
            WormholeCoreBridge.VM memory tokenTransferVM,
            bool valid,
            string memory reason
        ) = wormholeCoreBridge.parseAndVerifyVM(encodedTokenTransfer);
        require(valid, reason);
        require(tokenTransferVM.emitterChainId == TERRA_CHAIN_ID);
        // No need to check emitter address; this is checked by completeTransfer.
        // ensure that the provided transfer vaa is the one referenced by the transfer info
        require(
            tokenTransferVM.sequence ==
                incomingTokenTransferInfo.tokenTransferSequence
        );

        WormholeTokenBridge.Transfer memory transfer = WormholeTokenBridge(
            WORMHOLE_TOKEN_BRIDGE
        ).parseTransfer(encodedTokenTransfer);
        // No need to check that recipient chain matches this chain; this is checked by completeTransfer.
        require(transfer.to == encodeAddress(address(this)));
        require(transfer.toChain == incomingTokenTransferInfo.chainId);

        if (
            !wormholeTokenBridge.isTransferCompleted(tokenTransferVM.hash)
        ) {
            wormholeTokenBridge.completeTransfer(encodedTokenTransfer);
        }

        // forward the tokens to the appropriate recipient
        SafeERC20.safeTransfer(
            IERC20(address(uint160(uint256(transfer.tokenAddress)))),
            address(
                uint160(
                    uint256(
                        incomingTokenTransferInfo.tokenRecipientAddress
                    )
                )
            ),
            transfer.amount
        );
    }
}
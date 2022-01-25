const { ethers, upgrades } = require("hardhat");
const {
  CHAIN_ID_TERRA,
  CHAIN_ID_ETHEREUM_ROPSTEN,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  hexToUint8Array,
  parseSequencesFromLogEth,
  parseSequenceFromLogTerra,
  redeemOnEth,
} = require("@certusone/wormhole-sdk");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_MANAGER_ADDR,
  ETH_WORMHOLE_ADDR,
  TERRA_TOKEN_BRIDGE_ADDR,
} = require("../constants");
const { getSignedVAAWithRetry } = require("./wormhole.js");
const { ethWallet } = require("./eth.js");

const erc20ABI = [
  // Read-Only Functions
  "function balanceOf(address owner) view returns (uint256)",
  // Authenticated Functions
  "function approve(address spender, uint256 value) returns (bool)",
];

let utf8Encode = new TextEncoder();

async function deployEthereumManagerHardhat() {
  const consistencyLevel = 1;
  const EthereumManager = await ethers.getContractFactory("EthereumManager");
  const ethereumManager = await upgrades.deployProxy(
    EthereumManager,
    [
      consistencyLevel,
      ETH_UST_CONTRACT_ADDR,
      ETH_TOKEN_BRIDGE_ADDR,
      hexToUint8Array(await getEmitterAddressTerra(TERRA_MANAGER_ADDR)),
      0,
      "0x689961608D2d7047F5411F9d9004D440449CbD27",
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await ethereumManager.deployed();
  return ethereumManager;
}

async function deployEthereumManager() {
  console.log("Using eth wallet address", ethWallet.address);
  // Deploying EthereumManager contract.
  const consistencyLevel = 1;
  const EthereumManager = await ethers.getContractFactory(
    "EthereumManager",
    ethWallet
  );
  const ethereumManager = await upgrades.deployProxy(
    EthereumManager,
    [
      consistencyLevel,
      ETH_UST_CONTRACT_ADDR,
      ETH_TOKEN_BRIDGE_ADDR,
      hexToUint8Array(await getEmitterAddressTerra(TERRA_MANAGER_ADDR)),
      0,
      "0x689961608D2d7047F5411F9d9004D440449CbD27",
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await ethereumManager.deployed();
  return ethereumManager;
}

async function approveERC20(tokenAddr, spender, amount) {
  var ethUST = new ethers.Contract(tokenAddr, erc20ABI, ethWallet);
  const ethUSTbalance = await ethUST.balanceOf(ethWallet.address);
  console.log("UST balance: ", ethUSTbalance);
  await ethUST.approve(spender, amount);
}

function getStableYieldOpenRequest() {
  const actionData = {
    open_position: {},
  };
  const encodedActionData = utf8Encode.encode(
    Buffer.from(JSON.stringify(actionData)).toString("base64")
  );
  return encodedActionData;
}

function getDeltaNeutralOpenRequest() {
  const deltaNeutralParams = {
    target_min_collateral_ratio: "2.3",
    target_max_collateral_ratio: "2.7",
    mirror_asset_cw20_addr: "terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx",
  };
  const actionData = {
    open_position: {
      data: Buffer.from(JSON.stringify(deltaNeutralParams)).toString("base64"),
    },
  };
  const encodedActionData = utf8Encode.encode(
    Buffer.from(JSON.stringify(actionData)).toString("base64")
  );
  return encodedActionData;
}

function getStableYieldIncreaseRequest() {
  const encodedIncreasePostionActionData = utf8Encode.encode(
    Buffer.from(
      JSON.stringify({
        increase_position: {},
      })
    ).toString("base64")
  );
  return encodedIncreasePostionActionData;
}

function getCloseRequest(redeemAddr) {
  const closeActionData = {
    close_position: {
      recipient: {
        external_chain: {
          recipient_chain: CHAIN_ID_ETHEREUM_ROPSTEN,
          recipient: Buffer.from(
            getEmitterAddressEth(redeemAddr),
            "hex"
          ).toString("base64"),
        },
      },
    },
  };

  const encodedCloseActionData = utf8Encode.encode(
    Buffer.from(JSON.stringify(closeActionData)).toString("base64")
  );
  return encodedCloseActionData;
}

async function getVAAs(txReceipt, ethereumManagerAddr) {
  let [tokenTransferSeq, genericMessagingSeq] = parseSequencesFromLogEth(
    txReceipt,
    ETH_WORMHOLE_ADDR
  );
  console.log(
    "token seq: ",
    tokenTransferSeq,
    "generic seq: ",
    genericMessagingSeq
  );
  let ethTokenBridgeEmitterAddress = getEmitterAddressEth(
    ETH_TOKEN_BRIDGE_ADDR
  );

  console.log("chain id: ", CHAIN_ID_ETHEREUM_ROPSTEN);
  console.log("ethTokenBridgeEmitterAddress: ", ethTokenBridgeEmitterAddress);
  let ethManagerEmitterAddress = getEmitterAddressEth(ethereumManagerAddr);
  console.log("ethManagerEmitterAddress: ", ethManagerEmitterAddress);

  console.log("getting signed VAA for token transfer");
  let tokenTransferVAA = await getSignedVAAWithRetry(
    CHAIN_ID_ETHEREUM_ROPSTEN,
    ethTokenBridgeEmitterAddress,
    tokenTransferSeq
  );

  // Fetch the VAAs for generic message and token transfer.
  console.log("getting signed VAA for generic messages.");
  let genericMessagingVAA = await getSignedVAAWithRetry(
    CHAIN_ID_ETHEREUM_ROPSTEN,
    ethManagerEmitterAddress,
    genericMessagingSeq
  );
  return [genericMessagingVAA, tokenTransferVAA];
}

async function getVAA(txReceipt, ethereumManagerAddr) {
  let [genericMessagingSeq] = parseSequencesFromLogEth(
    txReceipt,
    ETH_WORMHOLE_ADDR
  );
  console.log("generic seq: ", genericMessagingSeq);

  console.log("chain id: ", CHAIN_ID_ETHEREUM_ROPSTEN);
  let ethManagerEmitterAddress = getEmitterAddressEth(ethereumManagerAddr);
  console.log("ethManagerEmitterAddress: ", ethManagerEmitterAddress);

  // Fetch the VAAs for generic message and token transfer.
  console.log("getting signed VAA for generic messages.");
  let genericMessagingVAA = await getSignedVAAWithRetry(
    CHAIN_ID_ETHEREUM_ROPSTEN,
    ethManagerEmitterAddress,
    genericMessagingSeq
  );
  return genericMessagingVAA;
}

module.exports = {
  deployEthereumManagerHardhat: deployEthereumManagerHardhat,
  deployEthereumManager: deployEthereumManager,
  approveERC20: approveERC20,
  getStableYieldOpenRequest: getStableYieldOpenRequest,
  getVAAs: getVAAs,
  getStableYieldIncreaseRequest: getStableYieldIncreaseRequest,
  getCloseRequest: getCloseRequest,
  getVAA: getVAA,
  getDeltaNeutralOpenRequest: getDeltaNeutralOpenRequest,
};

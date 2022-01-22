const { ethers, upgrades } = require("hardhat");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_MANAGER_ADDR,
  ETH_WORMHOLE_ADDR,
} = require("../constants");
const { getSignedVAAWithRetry } = require("../utils/wormhole.js");
const {
  CHAIN_ID_ETHEREUM_ROPSTEN,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  hexToUint8Array,
  parseSequencesFromLogEth,
} = require("@certusone/wormhole-sdk");
const { ethWallet } = require("../utils/eth.js");
const { processVAAs, registerWithTerraManager } = require("../utils/terra.js");

describe("EthereumManager integration test", function () {
  it("Should initiate Ethereum cross-chain tx and trigger Terra tx", async function () {
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
      ],
      { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await ethereumManager.deployed();
    console.log(
      `Successfully deployed Ethereum Proxy Address: ${ethereumManager.address}`
    );

    // Send 600 UST tokens from ETH -> Terra.
    const amount = 600 * 1e6;
    const erc20ABI = [
      // Read-Only Functions
      "function balanceOf(address owner) view returns (uint256)",
      // Authenticated Functions
      "function approve(address spender, uint256 value) returns (bool)",
    ];
    var ethUST = new ethers.Contract(
      ETH_UST_CONTRACT_ADDR,
      erc20ABI,
      ethWallet
    );
    const ethUSTbalance = await ethUST.balanceOf(ethWallet.address);
    console.log("UST balance: ", ethUSTbalance);
    // Approve Ethereum Manager to use UST balance.
    await ethUST.approve(ethereumManager.address, amount);

    // Base64 encoding of the Action enum on Terra side.
    const actionDataBase64 =
      "ewoJIm9wZW5fcG9zaXRpb24iOiB7CgkJImRhdGEiOiAiZXdvZ0lDQWdJblJoY21kbGRGOXRhVzVmWTI5c2JHRjBaWEpoYkY5eVlYUnBieUk2SUNJeUxqTWlMQW9nSUNBZ0luUmhjbWRsZEY5dFlYaGZZMjlzYkdGMFpYSmhiRjl5WVhScGJ5STZJQ0l5TGpjaUxBb2dJQ0FnSW0xcGNuSnZjbDloYzNObGRGOWpkekl3WDJGa1pISWlPaUFpZEdWeWNtRXhlWE0wWkhkM2VtRmxibXBuTW1kNU1ESnRjMnh0WXprMlpqSTJOM2gyY0hOcVlYUTNaM2dpQ24wPSIKCX0KfQ==";
    let utf8Encode = new TextEncoder();
    const encodedActionData = utf8Encode.encode(actionDataBase64);
    let createPositionTX = await ethereumManager.createPosition(
      0,
      3,
      ETH_UST_CONTRACT_ADDR,
      amount,
      encodedActionData.length,
      encodedActionData,
      {gasLimit: 600000}
    );

    let receipt = await createPositionTX.wait();
    let [tokenTransferSeq, genericMessagingSeq] = parseSequencesFromLogEth(
      receipt,
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
    let ethManagerEmitterAddress = getEmitterAddressEth(ethereumManager.address);
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

    console.log("Registering with Terra Manager");
    await registerWithTerraManager(
      CHAIN_ID_ETHEREUM_ROPSTEN,
      Array.from(hexToUint8Array(getEmitterAddressEth(ethereumManager.address)))
    );

    // Redeem the VAA for the wormhole transfer on the Terra side.
    console.log("Processing VAAs");
    await processVAAs(
      Buffer.from(genericMessagingVAA).toString("base64"),
      Buffer.from(tokenTransferVAA).toString("base64")
    );
  });
});

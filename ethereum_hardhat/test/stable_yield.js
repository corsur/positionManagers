const { ethers, upgrades } = require("hardhat");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_MANAGER_ADDR,
  ETH_WORMHOLE_ADDR,
  TERRA_TOKEN_BRIDGE_ADDR,
} = require("../constants");
const { getSignedVAAWithRetry } = require("../utils/wormhole.js");
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
const { ethWallet } = require("../utils/eth.js");
const {
  processVAAs,
  processVAA,
  registerWithTerraManager,
} = require("../utils/terra.js");

describe.only("Stable yield integration", function () {
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
    const amount = 22 * 1e6;
    const strategyId = 1;
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
    const actionData = {
      open_position: {},
    };
    let utf8Encode = new TextEncoder();
    const encodedActionData = utf8Encode.encode(
      Buffer.from(JSON.stringify(actionData)).toString("base64")
    );
    let createPositionTX = await ethereumManager.createPosition(
      strategyId,
      CHAIN_ID_TERRA,
      ETH_UST_CONTRACT_ADDR,
      amount,
      encodedActionData.length,
      encodedActionData,
      { gasLimit: 600000 }
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
    let ethManagerEmitterAddress = getEmitterAddressEth(
      ethereumManager.address
    );
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

    console.log("Successfully opened position.");

    // Add more UST.
    const positionId = 0;
    const additionalAmount = 10 * 1e6;
    await ethUST.approve(ethereumManager.address, additionalAmount);
    const encodedIncreasePostionActionData = utf8Encode.encode(
      Buffer.from(
        JSON.stringify({
          increase_position: {},
        })
      ).toString("base64")
    );

    let increasePositionTX = await ethereumManager.executeStrategy(
      positionId,
      strategyId,
      ETH_UST_CONTRACT_ADDR,
      additionalAmount,
      encodedIncreasePostionActionData.length,
      encodedIncreasePostionActionData,
      { gasLimit: 600000 }
    );
    let increaseTXReceipt = await increasePositionTX.wait();

    let [tokenTransferIncreaseSeq, genericMessagingIncreaseSeq] = parseSequencesFromLogEth(
      increaseTXReceipt,
      ETH_WORMHOLE_ADDR
    );
    console.log("Getting sequences for increase position TX.");
    console.log(
      "token seq: ",
      tokenTransferIncreaseSeq,
      "generic seq: ",
      genericMessagingIncreaseSeq
    );

    console.log("getting signed VAA for token transfer");
    let tokenTransferIncreaseVAA = await getSignedVAAWithRetry(
      CHAIN_ID_ETHEREUM_ROPSTEN,
      ethTokenBridgeEmitterAddress,
      tokenTransferIncreaseSeq
    );

    // Fetch the VAAs for generic message and token transfer.
    console.log("getting signed VAA for generic messages.");
    let genericMessagingIncreaseVAA = await getSignedVAAWithRetry(
      CHAIN_ID_ETHEREUM_ROPSTEN,
      ethManagerEmitterAddress,
      genericMessagingIncreaseSeq
    );

    // Redeem the VAA for the wormhole transfer on the Terra side.
    console.log("Processing VAAs");
    await processVAAs(
      Buffer.from(genericMessagingIncreaseVAA).toString("base64"),
      Buffer.from(tokenTransferIncreaseVAA).toString("base64")
    );
    console.log("Succesfully increased position");

    // Close position.
    const closeActionData = {
      close_position: {
        recipient: {
          external_chain: {
            recipient_chain: CHAIN_ID_ETHEREUM_ROPSTEN,
            recipient: Buffer.from(
              getEmitterAddressEth(ethWallet.address),
              "hex"
            ).toString("base64"),
          },
        },
      },
    };

    console.log("close action data: ", closeActionData);
    const encodedCloseActionData = utf8Encode.encode(
      Buffer.from(JSON.stringify(closeActionData)).toString("base64")
    );
    console.log("encoded close action data: ", encodedCloseActionData);
    let closePositionTX = await ethereumManager.executeStrategy(
      positionId,
      0,
      ETH_UST_CONTRACT_ADDR,
      0,
      encodedCloseActionData.length,
      encodedCloseActionData
    );
    let closeTXReceipt = await closePositionTX.wait();
    console.log("close position tx receipt: ", closeTXReceipt);

    let [genericMessagingCloseSeq] = parseSequencesFromLogEth(
      closeTXReceipt,
      ETH_WORMHOLE_ADDR
    );
    console.log("generic seq: ", genericMessagingCloseSeq);

    console.log("getting signed VAA for generic messages.");
    let genericMessagingCloseVAA = await getSignedVAAWithRetry(
      CHAIN_ID_ETHEREUM_ROPSTEN,
      ethManagerEmitterAddress,
      genericMessagingCloseSeq
    );

    const terraRes = await processVAA(
      Buffer.from(genericMessagingCloseVAA).toString("base64")
    );
    let terraWithdrawSeq = parseSequenceFromLogTerra(terraRes);
    const terraTokenTransferVAABytes = await getSignedVAAWithRetry(
      CHAIN_ID_TERRA,
      await getEmitterAddressTerra(TERRA_TOKEN_BRIDGE_ADDR),
      terraWithdrawSeq
    );
    console.log("Redeeming on ETH");

    console.log(
      await redeemOnEth(
        ETH_TOKEN_BRIDGE_ADDR,
        ethWallet,
        terraTokenTransferVAABytes
      )
    );
  });
});

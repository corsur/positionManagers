const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_TOKEN_BRIDGE_ADDR,
  STABLE_YIELD,
} = require("../constants");
const { getSignedVAAWithRetry } = require("../utils/wormhole.js");
const {
  CHAIN_ID_TERRA,
  CHAIN_ID_ETHEREUM_ROPSTEN,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  hexToUint8Array,
  parseSequenceFromLogTerra,
  redeemOnEth,
} = require("@certusone/wormhole-sdk");
const { ethWallet } = require("../utils/eth.js");
const {
  processVAAs,
  processVAA,
  registerWithTerraManager,
} = require("../utils/terra.js");
const {
  deployEthereumManager,
  approveERC20,
  getStableYieldOpenRequest,
  getStableYieldIncreaseRequest,
  getCloseRequest,
  getVAAs,
  getVAA,
} = require("../utils/helpers.js");

describe("Stable yield integration test", function () {
  it("Should open, increase and close.", async function () {
    const ethereumManager = await deployEthereumManager();
    console.log(
      `Successfully deployed Ethereum Proxy Address: ${ethereumManager.address}`
    );

    // Send 600 UST tokens from ETH -> Terra.
    const amount = 22 * 1e6;
    await approveERC20(ETH_UST_CONTRACT_ADDR, ethereumManager.address, amount);
    console.log(
      `Approved ${ethereumManager.address} to spend ${amount / 1e6} UST`
    );

    // Base64 encoding of the Action enum on Terra side.
    const openActionRequest = getStableYieldOpenRequest();
    let createPositionTX = await ethereumManager.createPosition(
      STABLE_YIELD,
      CHAIN_ID_TERRA,
      [{ assetAddr: ETH_UST_CONTRACT_ADDR, amount: amount }],
      openActionRequest.length,
      openActionRequest,
      { gasLimit: 600000 }
    );

    const [genericMessagingVAA, tokenTransferVAA] = await getVAAs(
      await createPositionTX.wait(),
      ethereumManager.address
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
    await approveERC20(
      ETH_UST_CONTRACT_ADDR,
      ethereumManager.address,
      additionalAmount
    );

    const increaseActionRequest = getStableYieldIncreaseRequest();
    let increasePositionTX = await ethereumManager.executeStrategy(
      positionId,
      STABLE_YIELD,
      [{ assetAddr: ETH_UST_CONTRACT_ADDR, amount: additionalAmount }],
      increaseActionRequest.length,
      increaseActionRequest,
      { gasLimit: 600000 }
    );

    const [genericMessagingIncreaseVAA, tokenTransferIncreaseVAA] =
      await getVAAs(await increasePositionTX.wait(), ethereumManager.address);

    // Redeem the VAA for the wormhole transfer on the Terra side.
    console.log("Processing VAAs");
    await processVAAs(
      Buffer.from(genericMessagingIncreaseVAA).toString("base64"),
      Buffer.from(tokenTransferIncreaseVAA).toString("base64")
    );
    console.log("Succesfully increased position");

    // Close position.
    const closeActionRequest = getCloseRequest(ethereumManager.address);
    let closePositionTX = await ethereumManager.executeStrategy(
      positionId,
      STABLE_YIELD,
      [],
      closeActionRequest.length,
      closeActionRequest
    );

    const genericMessagingCloseVAA = await getVAA(
      await closePositionTX.wait(),
      ethereumManager.address
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

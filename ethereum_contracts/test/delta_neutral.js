const {
  ETH_UST_CONTRACT_ADDR,
  TERRA_TOKEN_BRIDGE_ADDR,
  DELTA_NEUTRAL,
  TERRA_MANAGER_ADDR,
} = require("../constants");
const { getSignedVAAWithRetry } = require("../utils/wormhole.js");
const {
  CHAIN_ID_TERRA,
  CHAIN_ID_ETHEREUM_ROPSTEN,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  redeemOnTerra,
  parseSequencesFromLogTerra,
} = require("@certusone/wormhole-sdk");
const { ethWallet } = require("../utils/eth.js");
const {
  terraWallet,
  signAndBroadcast,
  processVAAs,
  processVAA,
  registerWithTerraManager,
} = require("../utils/terra.js");
const {
  deployEthereumManager,
  approveERC20,
  getDeltaNeutralOpenRequest,
  getCloseRequest,
  getVAAs,
  getVAA,
} = require("../utils/helpers");

async function deployOpenAndClose(shouldSelfClaimTokenTransfer = false) {
  const ethereumManager = await deployEthereumManager();
  console.log(
    `Successfully deployed Ethereum Proxy Address: ${ethereumManager.address}`
  );

  // Send 600 UST tokens from ETH -> Terra.
  const amount = 600 * 1e6;
  await approveERC20(ETH_UST_CONTRACT_ADDR, ethereumManager.address, amount);

  // Update cross-chain fee.
  await ethereumManager.updateCrossChainFeeBPS(10, {
    gasLimit: 600000,
  });

  // Base64 encoding of the Action enum on Terra side.
  const encodedActionData = getDeltaNeutralOpenRequest();
  let createPositionTX = await ethereumManager.createPosition(
    CHAIN_ID_TERRA,
    DELTA_NEUTRAL,
    [{ assetAddr: ETH_UST_CONTRACT_ADDR, amount: amount }],
    encodedActionData,
    { gasLimit: 900000 }
  );

  const [genericMessagingVAA, tokenTransferVAA] = await getVAAs(
    await createPositionTX.wait(),
    ethereumManager.address
  );

  console.log("Registering with Terra Manager");
  await registerWithTerraManager(
    CHAIN_ID_ETHEREUM_ROPSTEN,
    Buffer.from(getEmitterAddressEth(ethereumManager.address), "hex").toString(
      "base64"
    )
  );

  // Self-claim token transfer on Terra side. This is to stress test Terra
  // Constract will still work if the token is already claimed.
  if (shouldSelfClaimTokenTransfer) {
    console.log("Redeem tokens on Terra.");
    const redeemOnTerraMsg = await redeemOnTerra(
      TERRA_TOKEN_BRIDGE_ADDR,
      terraWallet.key.accAddress,
      tokenTransferVAA
    );
    await signAndBroadcast([redeemOnTerraMsg]);
    console.log("Redeemed tokens on Terra.");
  }

  // Redeem the VAA for the wormhole transfer on the Terra side.
  console.log("Processing VAAs");
  await processVAAs(
    Buffer.from(genericMessagingVAA).toString("base64"),
    Buffer.from(tokenTransferVAA).toString("base64")
  );

  console.log("Successfully opened position.");

  // Close position.
  const positionId = 0;
  const encodedCloseActionData = getCloseRequest(ethWallet.address);
  let closePositionTX = await ethereumManager.executeStrategy(
    positionId,
    [],
    encodedCloseActionData
  );
  console.log("Sent close request on ETH.");

  const genericMessagingCloseVAA = await getVAA(
    await closePositionTX.wait(),
    ethereumManager.address
  );

  console.log("Processing close VAA on Terra.");
  const terraRes = await processVAA(
    Buffer.from(genericMessagingCloseVAA).toString("base64")
  );

  let [terraGenericMessagingSeq, terraTokenSeq] =
    parseSequencesFromLogTerra(terraRes);

  console.log(
    `Terra token seq: ${terraTokenSeq}, generic seq: ${terraGenericMessagingSeq}`
  );

  const terraTokenTransferVAABytes = await getSignedVAAWithRetry(
    CHAIN_ID_TERRA,
    await getEmitterAddressTerra(TERRA_TOKEN_BRIDGE_ADDR),
    terraTokenSeq
  );

  const terraGenericMessagingVAABytes = await getSignedVAAWithRetry(
    CHAIN_ID_TERRA,
    await getEmitterAddressTerra(TERRA_MANAGER_ADDR),
    terraGenericMessagingSeq
  );

  // Process Terra's VAA on ETH.
  await ethereumManager.processApertureInstruction(
    terraGenericMessagingVAABytes,
    [terraTokenTransferVAABytes]
  );
  console.log("Finished processing on ETH");
}

describe("Delta-neutral integration test", function () {
  it("Should initiate Ethereum cross-chain tx and trigger Terra tx", async function () {
    await deployOpenAndClose();
  });

  it("Should still work after self-claiming token transfer", async function () {
    await deployOpenAndClose(/*shouldSelfClaimTokenTransfer=*/ true);
  });
});

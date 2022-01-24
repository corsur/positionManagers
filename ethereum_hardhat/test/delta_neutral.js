const { ethers, upgrades } = require("hardhat");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_TOKEN_BRIDGE_ADDR,
  DELTA_NEUTRAL,
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
  redeemOnTerra,
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

  // Base64 encoding of the Action enum on Terra side.
  const encodedActionData = getDeltaNeutralOpenRequest();
  let createPositionTX = await ethereumManager.createPosition(
    DELTA_NEUTRAL,
    CHAIN_ID_TERRA,
    ETH_UST_CONTRACT_ADDR,
    amount,
    encodedActionData.length,
    encodedActionData,
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

  // Slef-claim token transfer on Terra side. This is to stress test Terra
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
  const encodedCloseActionData = getCloseRequest(ethereumManager.address);
  let closePositionTX = await ethereumManager.executeStrategy(
    positionId,
    DELTA_NEUTRAL,
    ETH_UST_CONTRACT_ADDR,
    0,
    encodedCloseActionData.length,
    encodedCloseActionData
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
}

describe.only("EthereumManager integration test", function () {
  it("Should initiate Ethereum cross-chain tx and trigger Terra tx", async function () {
    await deployOpenAndClose();
  });

  it("Should still work after self-claiming token transfer", async function () {
    await deployOpenAndClose(/*shouldSelfClaimTokenTransfer=*/ true);
  });
});

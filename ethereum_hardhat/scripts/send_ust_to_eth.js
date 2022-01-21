const { ethers } = require("hardhat");
const {
  MsgExecuteContract,
  LCDClient,
  isTxError,
  MnemonicKey,
} = require("@terra-money/terra.js");
const {
  TERRA_TOKEN_BRIDGE_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  WORMHOLE_RPC_HOST,
  ETH_NODE_URL,
  ETH_PRV_KEY_1,
} = require("../constants");
const {
  CHAIN_ID_ETHEREUM_ROPSTEN,
  CHAIN_ID_TERRA,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  getSignedVAA,
  parseSequenceFromLogTerra,
  redeemOnEth,
} = require("@certusone/wormhole-sdk");
const {
  NodeHttpTransport,
} = require("@improbable-eng/grpc-web-node-http-transport");

const ethProvider = new ethers.providers.JsonRpcProvider(ETH_NODE_URL);
const ethWallet = new ethers.Wallet(ETH_PRV_KEY_1, ethProvider);

const terra = new LCDClient({
  URL: "https://bombay-lcd.terra.dev",
  chainID: "bombay-12",
});

const terraWallet = terra.wallet(
  new MnemonicKey({
    mnemonic:
      "plastic evidence song forest fence daughter nuclear road angry knife wing punch sustain suit resist vapor thrive diesel collect easily minimum thing cost phone",
  })
);

async function signAndBroadcast(msgs) {
  await new Promise((resolve) => setTimeout(resolve, 1000));
  const tx = await terraWallet.createAndSignTx({
    msgs: msgs,
  });
  const txResult = await terra.tx.broadcast(tx);
  if (isTxError(txResult)) {
    throw new Error(txResult.raw_log);
  }
  let txInfo = txResult;
  txInfo.tx = tx;
  return txInfo;
}

async function getSignedVAAWithRetry(emitterChain, emitterAddress, sequence) {
  process.stdout.write(`Fetching VAA...`);
  while (true) {
    try {
      const { vaaBytes } = await getSignedVAA(
        WORMHOLE_RPC_HOST,
        emitterChain,
        emitterAddress,
        sequence,
        {
          transport: NodeHttpTransport(),
        }
      );
      if (vaaBytes !== undefined) {
        process.stdout.write(`✅\n`);
        return vaaBytes;
      }
    } catch (e) {}
    process.stdout.write(".");
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

async function main() {
  console.log("Bridging UST from Terra address: ", terraWallet.key.accAddress);
  let msgs = [
    new MsgExecuteContract(
      terraWallet.key.accAddress,
      TERRA_TOKEN_BRIDGE_ADDR,
      { deposit_tokens: {} },
      { uusd: "100000000" }
    ),
    new MsgExecuteContract(
      terraWallet.key.accAddress,
      TERRA_TOKEN_BRIDGE_ADDR,
      {
        initiate_transfer: {
          asset: {
            info: { native_token: { denom: "uusd" } },
            amount: "100000000",
          },
          recipient_chain: CHAIN_ID_ETHEREUM_ROPSTEN,
          recipient: Buffer.from(
            getEmitterAddressEth(ethWallet.address),
            "hex"
          ).toString("base64"),
          fee: "0",
          nonce: 0,
        },
      },
      { uusd: "1000000" }
    ),
  ];
  console.log("Broadcasting tx to terra");
  let res = await signAndBroadcast(msgs);
  let seq = parseSequenceFromLogTerra(res);

  console.log("Getting VAA");
  let vaaBytes = await getSignedVAAWithRetry(
    CHAIN_ID_TERRA,
    await getEmitterAddressTerra(TERRA_TOKEN_BRIDGE_ADDR),
    seq
  );
  console.log("Redeeming on ETH");

  console.log(await redeemOnEth(ETH_TOKEN_BRIDGE_ADDR, ethWallet, vaaBytes));
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });

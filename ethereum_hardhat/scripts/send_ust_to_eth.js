const {
  MsgExecuteContract,
  LCDClient,
  isTxError,
  MnemonicKey,
} = require("@terra-money/terra.js");
const {
  TERRA_TOKEN_BRIDGE_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
} = require("../constants");
const {
  CHAIN_ID_ETHEREUM_ROPSTEN,
  CHAIN_ID_TERRA,
  getEmitterAddressEth,
  getEmitterAddressTerra,
  parseSequenceFromLogTerra,
  redeemOnEth,
} = require("@certusone/wormhole-sdk");

const { getSignedVAAWithRetry } = require("../utils/wormhole.js");
const { ethWallet } = require("../utils/eth.js");
const { terraWallet, signAndBroadcast } = require("../utils/terra.js");

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

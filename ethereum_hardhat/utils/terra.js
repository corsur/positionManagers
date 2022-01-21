const { LCDClient, isTxError, MnemonicKey } = require("@terra-money/terra.js");

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

module.exports = {
    terra: terra,
    terraWallet: terraWallet,
    signAndBroadcast: signAndBroadcast,
}
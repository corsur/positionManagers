const {
  LCDClient,
  isTxError,
  MnemonicKey,
  MsgExecuteContract,
} = require("@terra-money/terra.js");
const { TERRA_MANAGER_ADDR, TERRA_WALLET_MNEMONIC } = require("../constants");

const terra = new LCDClient({
  URL: "https://bombay-lcd.terra.dev",
  chainID: "bombay-12",
});

const terraWallet = terra.wallet(
  new MnemonicKey({
    mnemonic: TERRA_WALLET_MNEMONIC,
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

async function registerWithTerraManager(chainId, ethManagerAddrByteArray) {
  let msg = new MsgExecuteContract(
    terraWallet.key.accAddress,
    TERRA_MANAGER_ADDR,
    {
      register_external_chain_manager: {
        chain_id: chainId,
        aperture_manager_addr: ethManagerAddrByteArray,
      },
    }
  );
  return await signAndBroadcast([msg]);
}

async function processVAAs(genericMessagingVAA, tokenTransferVAA) {
  const vaas = {
    process_cross_chain_instruction: {
      instruction_vaa: genericMessagingVAA,
      token_transfer_vaas: [tokenTransferVAA],
    },
  };
  let msg = new MsgExecuteContract(
    terraWallet.key.accAddress,
    TERRA_MANAGER_ADDR,
    vaas
  );
  return await signAndBroadcast([msg]);
}

module.exports = {
  terra: terra,
  terraWallet: terraWallet,
  signAndBroadcast: signAndBroadcast,
  processVAAs: processVAAs,
  registerWithTerraManager: registerWithTerraManager,
};

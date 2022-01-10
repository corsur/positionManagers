import {
  LCDClient,
  MnemonicKey,
  isTxError,
  MsgMigrateContract,
} from "@terra-money/terra.js";
import * as fs from "fs";

const gasPrices = {
  uusd: 0.15,
};

const gasAdjustment = 1.5;
var sequence = -1;

async function initializeSequence(wallet) {
  const account_and_sequence = await wallet.accountNumberAndSequence();
  sequence = account_and_sequence.sequence;
}

function getAndIncrementSequence() {
  return sequence++;
}

const testnet = new LCDClient({
  URL: "https://bombay-lcd.terra.dev",
  chainID: "bombay-12",
  gasPrices: gasPrices,
  gasAdjustment: gasAdjustment,
});

const mainnet = new LCDClient({
  URL: "https://lcd.terra.dev",
  chainID: "columbus-5",
  gasPrices: gasPrices,
  gasAdjustment: gasAdjustment,
});

const test_wallet = testnet.wallet(
  new MnemonicKey({
    mnemonic:
      "plastic evidence song forest fence daughter nuclear road angry knife wing punch sustain suit resist vapor thrive diesel collect easily minimum thing cost phone",
  })
);

async function migrate_contract(contract_addr, new_code_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgMigrateContract(
        test_wallet.key.accAddress,
        contract_addr,
        new_code_id,
        {}),
    ],
    sequence: getAndIncrementSequence(),
  });
  const txResult = await testnet.tx.broadcast(tx);
  if (isTxError(txResult)) {
    throw new Error(
      `Migrate code failed. code: ${txResult.code}, codespace: ${txResult.codespace}, raw_log: ${txResult.raw_log}`
    );
  }
}

await initializeSequence(test_wallet);
await migrate_contract("terra1pvq5zdh4frjh773nfhk2shqvv5jlm450v8a9yh", 32427);
// "../artifacts/terra_manager-aarch64.wasm"
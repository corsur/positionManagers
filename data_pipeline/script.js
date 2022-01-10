import {
  LCDClient,
  MnemonicKey,
  MsgExecuteContract,
  MsgInstantiateContract,
  MsgStoreCode,
  isTxError,
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

async function deploy() {
  // Initialize sequence number.
  await initializeSequence(test_wallet);
  console.log("Using address: ", test_wallet.key.accAddress);

  // Get next position id to establish limit.
  const next_pos_res = await testnet.wasm.contractQuery("terra1pvq5zdh4frjh773nfhk2shqvv5jlm450v8a9yh", {
    get_next_position_id: {}
  });
  console.log('next position id: ', next_pos_res);

  // Get delta-neutral position manager address.
  const delta_neutral_pos_mgr_res = await testnet.wasm.contractQuery("terra1pvq5zdh4frjh773nfhk2shqvv5jlm450v8a9yh", {
    "get_strategy_metadata": {
      "strategy_id": "0"
    }
  });
  const manager_addr = delta_neutral_pos_mgr_res.manager_addr;
  console.log("∆-neutral position manager addr: ", delta_neutral_pos_mgr_res);

  // Loop over all positions to craft <wallet, position_id + metadata> map.
  for (var i = 0; i < parseInt(next_pos_res.next_position_id); i++) {
    const position_addr = await testnet.wasm.contractQuery(manager_addr, {
      "get_position_contract_addr": {
        "position": {
            "chain_id": 3,
            "position_id": i.toString()
        }
      }
    });
    console.log('position info: ', position_addr);

    // Get position info.
    const position_info = await testnet.wasm.contractQuery(position_addr, {
      get_position_info: {}
    });
    console.log('position info: ', position_info);
  }
}

await deploy();

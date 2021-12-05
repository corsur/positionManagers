import { LCDClient, MnemonicKey, MsgStoreCode, MsgInstantiateContract, isTxError } from '@terra-money/terra.js';
import * as fs from 'fs';

const testnet = new LCDClient({
  URL: 'https://bombay-lcd.terra.dev',
  chainID: 'bombay-12',
});
const mainnet = new LCDClient({
    URL: 'https://lcd.terra.dev',
    chainID: 'columbus-5',
  });

const test_wallet = terra.wallet(new MnemonicKey({
  mnemonic:
    'plastic evidence song forest fence daughter nuclear road angry knife wing punch sustain suit resist vapor thrive diesel collect easily minimum thing cost phone',
}));

function store_code(wasm_file) {
  test_wallet.createAndSignTx({
    msgs: [new MsgStoreCode(test_wallet.key.acc_address, fs.readFileSync(wasm_file).toString('base64'))],
    memo: 'Store Aperture code',
  })
  .then(tx => terra.tx.broadcast(tx))
  .then(result => {
    console.log(result);
    if (isTxError(result)) {
      throw new Error(
        `Failed to store code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
      );
    }
  });
  const {
    store_code: { code_id },
  } = storeCodeTxResult.logs[0].eventsByType;
  return code_id;
}

function instantiate_delta_neutral_position_manager(code_id) {
  test_wallet.createAndSignTx({
    msgs: [new MsgInstantiateContract(/*sender=*/test_wallet.key.acc_address, /*admin=*/test_wallet.key.acc_address, code_id, {
      // TODO: fill out instantiate msg.
    },
    /*init_coins=*/{},)],
    memo: 'Store Aperture code',
  })
  .then(tx => terra.tx.broadcast(tx))
  .then(result => {
    console.log(result);
    if (isTxError(result)) {
      throw new Error(
        `Failed to store code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
      );
    }
  });
}

function instantiate_delta_neutral_position(code_id) {
  test_wallet.createAndSignTx({
    msgs: [new MsgInstantiateContract(/*sender=*/test_wallet.key.acc_address, /*admin=*/test_wallet.key.acc_address, code_id, {
      // TODO: fill out instantiate msg.
    },
    /*init_coins=*/{},)],
    memo: 'Store Aperture code',
  })
  .then(tx => terra.tx.broadcast(tx))
  .then(result => {
    console.log(result);
    if (isTxError(result)) {
      throw new Error(
        `Failed to store code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
      );
    }
  });
}

function deploy() {
  var delta_neutral_position_manager_code_id = store_code('../artifacts/delta_neutral_position_manager.wasm')
  var delta_neutral_position_code_id = store_code('../artifacts/delta_neutral_position.wasm');
}

deploy();

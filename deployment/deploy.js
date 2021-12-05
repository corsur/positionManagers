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

const test_wallet = testnet.wallet(new MnemonicKey({
  mnemonic:
    'plastic evidence song forest fence daughter nuclear road angry knife wing punch sustain suit resist vapor thrive diesel collect easily minimum thing cost phone',
}));

async function store_code(wasm_file) {
  const storeCodeTx = await test_wallet.createAndSignTx({
    msgs: [new MsgStoreCode(test_wallet.key.acc_address, fs.readFileSync(wasm_file).toString('base64'))],
  });
  const storeCodeTxResult = await terra.tx.broadcast(storeCodeTx);
  console.log(storeCodeTxResult);
  if (isTxError(storeCodeTxResult)) {
    throw new Error(
      `Store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
    );
  }
  const {
    store_code: { code_id },
  } = storeCodeTxResult.logs[0].eventsByType;
  return code_id;
}

// Instantiate message refers to various contract addresses on the 'bombay-12' testnet.
function instantiate_delta_neutral_position_manager(manager_code_id, position_code_id) {
  test_wallet.createAndSignTx({
    msgs: [new MsgInstantiateContract(/*sender=*/test_wallet.key.acc_address, /*admin=*/test_wallet.key.acc_address, manager_code_id, {
      delta_neutral_position_code_id: position_code_id,
      controller: test_wallet.key.acc_address,
      min_uusd_amount: 100 * 1e6,
      anchor_ust_cw20_addr: "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
      mirror_cw20_addr: "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u",
      spectrum_cw20_addr: "terra1kvsxd94ue6f4rtchv2l6me5k07uh26s7637cza",
      anchor_market_addr: "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
      mirror_collateral_oracle_addr: "terra1q3ls6u2glsazdeu7dxggk8d04elnvmsg0ung6n",
      mirror_lock_addr: "terra1pcxghd4dyf950mcs0kmlp7lvnrjsnl6qlfldwj",
      mirror_mint_addr: "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w",
      mirror_oracle_addr: "terra1uvxhec74deupp47enh7z5pk55f3cvcz8nj4ww9",
      mirror_staking_addr: "terra1a06dgl27rhujjphsn4drl242ufws267qxypptx",
      spectrum_gov_addr: "terra1x3l2tkkwzzr0qsnrpy3lf2cm005zxv7pun26x4",
      spectrum_mirror_farms_addr: "terra1hasdl7l6xtegnch8mjyw2g7mfh9nt3gtdtmpfu",
      spectrum_staker_addr: "terra15nwqmmmza9y643apneg0ddwt0ekk38qdevnnjt",
      terraswap_factory_addr: "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf",
      collateral_ratio_safety_margin: 0.3,
    },
    /*init_coins=*/{},)],
    memo: 'Instantiate delta-neutral position manager',
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
  instantiate_delta_neutral_position_manager(delta_neutral_position_manager_code_id, delta_neutral_position_code_id);
}

deploy();

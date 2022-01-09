import {
  Coin,
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

async function store_code(wasm_file) {
  const storeCodeTx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgStoreCode(
        test_wallet.key.accAddress,
        fs.readFileSync(wasm_file).toString("base64")
      ),
    ],
    sequence: getAndIncrementSequence(),
  });
  const storeCodeTxResult = await testnet.tx.broadcast(storeCodeTx);
  if (isTxError(storeCodeTxResult)) {
    throw new Error(
      `Store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
    );
  }
  const {
    store_code: { code_id },
  } = storeCodeTxResult.logs[0].eventsByType;
  return parseInt(code_id[0]);
}

function getContractAddress(response) {
  const {
    instantiate_contract: { contract_address },
  } = response.logs[0].eventsByType;
  return contract_address[0];
}

async function instantiate_terra_manager(terra_manager_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgInstantiateContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*admin=*/ test_wallet.key.accAddress,
        terra_manager_id,
        {},
        /*init_coins=*/ {}
      ),
    ],
    memo: "Instantiate Terra Manager",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `Instantiate Terra Manager contract failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
  return getContractAddress(response);
}

async function instantiate_delta_neutral_position_manager(
  terra_manager_addr,
  manager_code_id,
  position_code_id
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgInstantiateContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*admin=*/ test_wallet.key.accAddress,
        manager_code_id,
        {
          admin_addr: test_wallet.key.accAddress,
          manager_addr: terra_manager_addr,
          delta_neutral_position_code_id: position_code_id,
          allow_position_increase: false,
          allow_position_decrease: false,
          controller: test_wallet.key.accAddress,
          min_delta_neutral_uusd_amount: (1000 * 1e6).toString(),
          anchor_ust_cw20_addr: "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
          mirror_cw20_addr: "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u",
          spectrum_cw20_addr: "terra1kvsxd94ue6f4rtchv2l6me5k07uh26s7637cza",
          anchor_market_addr: "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
          mirror_collateral_oracle_addr:
            "terra1q3ls6u2glsazdeu7dxggk8d04elnvmsg0ung6n",
          mirror_lock_addr: "terra1pcxghd4dyf950mcs0kmlp7lvnrjsnl6qlfldwj",
          mirror_mint_addr: "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w",
          mirror_oracle_addr: "terra1uvxhec74deupp47enh7z5pk55f3cvcz8nj4ww9",
          mirror_staking_addr: "terra1a06dgl27rhujjphsn4drl242ufws267qxypptx",
          spectrum_gov_addr: "terra1x3l2tkkwzzr0qsnrpy3lf2cm005zxv7pun26x4",
          spectrum_mirror_farms_addr:
            "terra1hasdl7l6xtegnch8mjyw2g7mfh9nt3gtdtmpfu",
          spectrum_staker_addr: "terra15nwqmmmza9y643apneg0ddwt0ekk38qdevnnjt",
          terraswap_factory_addr:
            "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf",
          collateral_ratio_safety_margin: "0.3",
          fee_collection_config: {
            performance_rate: "0.1",
            treasury_addr: test_wallet.key.accAddress
          }
        },
        /*init_coins=*/ {}
      ),
    ],
    memo: "Instantiate delta-neutral position manager",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `Instantiate contract failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
  return getContractAddress(response);
}
async function add_delta_neutral_strategy_to_terra_manager(
  terra_manager_addr,
  delta_neutral_position_manager_addr,
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          add_strategy: {
            name: "DN",
            version: "v0",
            manager_addr: delta_neutral_position_manager_addr
          }
        }
      ),
    ],
    memo: "Add delta-neutral strategy",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `add_delta_neutral_strategy_to_terra_manager failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
}

async function deploy() {
  // Initialize sequence number.
  await initializeSequence(test_wallet);
  console.log("Deploying using address: ", test_wallet.key.accAddress);

  /******************************************/
  /***** Store bytecode onto blockchain *****/
  /******************************************/
  const terra_manager_id = await store_code("../artifacts/terra_manager-aarch64.wasm");
  console.log("terra_manager_id: ", terra_manager_id);

  const delta_neutral_position_manager_id = await store_code(
    "../artifacts/delta_neutral_position_manager-aarch64.wasm"
  );
  console.log(
    "delta_neutral_position_manager_id: ",
    delta_neutral_position_manager_id
  );

  const delta_neutral_position_id = await store_code(
    "../artifacts/delta_neutral_position-aarch64.wasm"
  );
  console.log("delta_neutral_position_id: ", delta_neutral_position_id);
  /***************************************************/
  /***** End of storing bytecode onto blockchain *****/
  /***************************************************/

  /*********************************/
  /***** Instantiate contracts *****/
  /*********************************/

  const terra_manager_addr = await instantiate_terra_manager(terra_manager_id);
  console.log("Terra manager contract address: ", terra_manager_addr);

  const delta_neutral_position_manager_addr =
    await instantiate_delta_neutral_position_manager(
      terra_manager_addr,
      delta_neutral_position_manager_id,
      delta_neutral_position_id
    );
  console.log(
    "delta neutral position manager address: ",
    delta_neutral_position_manager_addr
  );
  /*****************************************/
  /***** End of contract instantiation *****/
  /*****************************************/

  // Add delta-neutral strategy to Terra manager.
  await add_delta_neutral_strategy_to_terra_manager(terra_manager_addr, delta_neutral_position_manager_addr);
  console.log(
    "Registered delta-neutral strategy with Terra manager."
  );
  return terra_manager_addr;
}

async function open_delta_neutral_position(terra_manager_addr, ust_amount) {
  await initializeSequence(test_wallet);
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          "create_position": {
            "data": "ewogICAgInRhcmdldF9taW5fY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjMiLAogICAgInRhcmdldF9tYXhfY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjciLAogICAgIm1pcnJvcl9hc3NldF9jdzIwX2FkZHIiOiAidGVycmExeXM0ZHd3emFlbmpnMmd5MDJtc2xtYzk2ZjI2N3h2cHNqYXQ3Z3giCn0=",
            "assets": [
              {
                "info": {
                  "native_token": {
                    "denom": "uusd"
                  }
                },
                "amount": (ust_amount * 1e6).toString()
              }
            ],
            "strategy": {
              "chain_id": 3,
              "strategy_id": "0"
            }
          }
        },
        [new Coin("uusd", (ust_amount * 1e6).toString())]
      ),
    ],
    memo: "Open delta neutral position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `open_delta_neutral_position failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
  console.log("Opened delta-neutral position with ust amount: ", ust_amount.toString());
}

const terra_manager_addr = await deploy();
await open_delta_neutral_position(terra_manager_addr, 1000);
await open_delta_neutral_position(terra_manager_addr, 2000);
// await open_delta_neutral_position(terra_manager_addr, 9000000);

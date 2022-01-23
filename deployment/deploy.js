import {
  Coin,
  LCDClient,
  MnemonicKey,
  MsgExecuteContract,
  MsgInstantiateContract,
  MsgMigrateContract,
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

async function instantiate_terra_manager(terra_manager_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgInstantiateContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*admin=*/ test_wallet.key.accAddress,
        terra_manager_id,
        {
          admin_addr: test_wallet.key.accAddress,
          wormhole_token_bridge_addr: "terra1pseddrv0yfsn76u4zxrjmtf45kdlmalswdv39a",
          wormhole_core_bridge_addr: "terra1pd65m0q9tl3v8znnz5f5ltsfegyzah7g42cx5v",
          cross_chain_outgoing_fee_rate: "0.001",
          cross_chain_outgoing_fee_collector_addr: "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g",
        },
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
          terra_manager_addr: terra_manager_addr,
          delta_neutral_position_code_id: position_code_id,
          allow_position_increase: false,
          allow_position_decrease: false,
          controller: test_wallet.key.accAddress,
          min_delta_neutral_uusd_amount: (100 * 1e6).toString(),
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
          astroport_factory_addr:
            "terra1x6f9mf9p7p255y3rwrk0kfynzp0kr8m4ervxn4",
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

async function instantiate_stable_yield_manager(
  terra_manager_addr,
  stable_yield_manager_code_id,
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgInstantiateContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*admin=*/ test_wallet.key.accAddress,
        stable_yield_manager_code_id,
        {
          admin_addr: test_wallet.key.accAddress,
          terra_manager_addr: terra_manager_addr,
          accrual_rate_per_period: "1.00000002987",
          seconds_per_period: 6,
          anchor_market_addr: "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
          anchor_ust_cw20_addr: "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl"
        },
        /*init_coins=*/ {}
      ),
    ],
    memo: "Instantiate stable-yield manager",
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

async function add_stable_yield_strategy_to_terra_manager(
  terra_manager_addr,
  stable_yield_manager_addr,
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          add_strategy: {
            name: "StableYield",
            version: "v0",
            manager_addr: stable_yield_manager_addr
          }
        }
      ),
    ],
    memo: "Add stable-yield strategy",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `add_stable_yield_strategy_to_terra_manager failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
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

  const stable_yield_manager_id = await store_code(
    "../artifacts/stable_yield_manager-aarch64.wasm"
  );
  console.log("stable_yield_manager_id: ", stable_yield_manager_id);
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

  const stable_yield_manager_addr =
    await instantiate_stable_yield_manager(terra_manager_addr, stable_yield_manager_id);
  console.log(
    "stable yield manager address: ",
    stable_yield_manager_addr
  );
  /*****************************************/
  /***** End of contract instantiation *****/
  /*****************************************/

  // Add delta-neutral strategy to Terra manager.
  await add_delta_neutral_strategy_to_terra_manager(terra_manager_addr, delta_neutral_position_manager_addr);
  console.log(
    "Registered delta-neutral strategy with Terra manager."
  );
  await add_stable_yield_strategy_to_terra_manager(terra_manager_addr, stable_yield_manager_addr);
  console.log(
    "Registered stable-yield strategy with Terra manager."
  );
  return terra_manager_addr;
}

async function open_delta_neutral_position(terra_manager_addr, ust_amount) {
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

async function open_stable_yield_position(terra_manager_addr, ust_amount) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          "create_position": {
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
              "strategy_id": "1"
            }
          }
        },
        [new Coin("uusd", (ust_amount * 1e6).toString())]
      ),
    ],
    memo: "Open stable yield position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `open_stable_yield_position failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
  console.log("Opened delta-neutral position with ust amount: ", ust_amount.toString());
}

async function upload_and_migrate_contract(contract_addr) {
  await initializeSequence(test_wallet);
  console.log("Deploying using address: ", test_wallet.key.accAddress);

  const new_code_id = await store_code("../artifacts/terra_manager-aarch64.wasm");
  console.log("new code id: ", new_code_id);

  await migrate_contract(contract_addr, new_code_id);
  console.log("contract migrated.");
}

const terra_manager_addr = await deploy();
await open_delta_neutral_position(terra_manager_addr, 500);
await open_stable_yield_position(terra_manager_addr, 600);
// await upload_and_migrate_contract('terra1uqryzpauak8tljlj9cl2gl99spgxqjvd008wvp');

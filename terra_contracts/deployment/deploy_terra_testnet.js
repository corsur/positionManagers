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
        {}
      ),
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
          wormhole_token_bridge_addr:
            "terra1pseddrv0yfsn76u4zxrjmtf45kdlmalswdv39a",
          wormhole_core_bridge_addr:
            "terra1pd65m0q9tl3v8znnz5f5ltsfegyzah7g42cx5v",
          cross_chain_outgoing_fee_rate: "0.001",
          cross_chain_outgoing_fee_collector_addr:
            "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g",
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
          controller: test_wallet.key.accAddress,
          min_open_uusd_amount: (100 * 1e6).toString(),
          min_reinvest_uusd_amount: (10 * 1e6).toString(),
          anchor_ust_cw20_addr: "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
          mirror_cw20_addr: "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u",
          spectrum_cw20_addr: "terra1kvsxd94ue6f4rtchv2l6me5k07uh26s7637cza",
          anchor_market_addr: "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
          mirror_collateral_oracle_addr:
            "terra1q3ls6u2glsazdeu7dxggk8d04elnvmsg0ung6n",
          mirror_lock_addr: "terra1pcxghd4dyf950mcs0kmlp7lvnrjsnl6qlfldwj",
          mirror_mint_addr: "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w",
          mirror_oracle_addr: "terra1sdr3rya4h039f4htfm42q44x3dlaxra7hc7p8e",
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
            collector_addr: test_wallet.key.accAddress,
          },
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

async function instantiate_anchor_earn_proxy(
  terra_manager_addr,
  anchor_earn_proxy_code_id
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgInstantiateContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*admin=*/ test_wallet.key.accAddress,
        anchor_earn_proxy_code_id,
        {
          admin_addr: test_wallet.key.accAddress,
          terra_manager_addr: terra_manager_addr,
          anchor_market_addr: "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
          anchor_ust_cw20_addr: "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
        },
        /*init_coins=*/ {}
      ),
    ],
    memo: "Instantiate Anchor Earn proxy",
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
  delta_neutral_position_manager_addr
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
            manager_addr: delta_neutral_position_manager_addr,
          },
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

async function add_anchor_earn_proxy_strategy_to_terra_manager(
  terra_manager_addr,
  anchor_earn_proxy_addr
) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          add_strategy: {
            name: "AnchorEarnProxy",
            version: "v0",
            manager_addr: anchor_earn_proxy_addr,
          },
        }
      ),
    ],
    memo: "Add Anchor Earn proxy strategy",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `add_anchor_earn_proxy_strategy_to_terra_manager failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
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
  const terra_manager_id = await store_code(
    "../artifacts/terra_manager-aarch64.wasm"
  );
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

  const anchor_earn_proxy_id = await store_code(
    "../artifacts/anchor_earn_proxy-aarch64.wasm"
  );
  console.log("anchor_earn_proxy_id: ", anchor_earn_proxy_id);
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

  const anchor_earn_proxy_addr = await instantiate_anchor_earn_proxy(
    terra_manager_addr,
    anchor_earn_proxy_id
  );
  console.log("anchor earn proxy address: ", anchor_earn_proxy_addr);
  /*****************************************/
  /***** End of contract instantiation *****/
  /*****************************************/

  // Add delta-neutral strategy to Terra manager.
  await add_delta_neutral_strategy_to_terra_manager(
    terra_manager_addr,
    delta_neutral_position_manager_addr
  );
  console.log("Registered delta-neutral strategy with Terra manager.");
  await add_anchor_earn_proxy_strategy_to_terra_manager(
    terra_manager_addr,
    anchor_earn_proxy_addr
  );
  console.log("Registered Anchor Earn proxy strategy with Terra manager.");
  return terra_manager_addr;
}

async function open_delta_neutral_position(terra_manager_addr, ust_amount) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          create_position: {
            data: "ewogICAgInRhcmdldF9taW5fY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjMiLAogICAgInRhcmdldF9tYXhfY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjciLAogICAgIm1pcnJvcl9hc3NldF9jdzIwX2FkZHIiOiAidGVycmExeXM0ZHd3emFlbmpnMmd5MDJtc2xtYzk2ZjI2N3h2cHNqYXQ3Z3giCn0=",
            assets: [
              {
                info: {
                  native_token: {
                    denom: "uusd",
                  },
                },
                amount: (ust_amount * 1e6).toString(),
              },
            ],
            strategy: {
              chain_id: 3,
              strategy_id: "0",
            },
          },
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
  console.log(
    "Opened delta-neutral position with ust amount: ",
    ust_amount.toString()
  );
}

async function open_anchor_earn_proxy_position(terra_manager_addr, ust_amount) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          create_position: {
            assets: [
              {
                info: {
                  native_token: {
                    denom: "uusd",
                  },
                },
                amount: (ust_amount * 1e6).toString(),
              },
            ],
            strategy: {
              chain_id: 3,
              strategy_id: "1",
            },
          },
        },
        [new Coin("uusd", (ust_amount * 1e6).toString())]
      ),
    ],
    memo: "Open Anchor Earn proxy position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `open_anchor_earn_proxy_position failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
    );
  }
  console.log(
    "Opened delta-neutral position with ust amount: ",
    ust_amount.toString()
  );
}

async function upload_and_migrate_contract(contract_addr) {
  await initializeSequence(test_wallet);
  console.log("Deploying using address: ", test_wallet.key.accAddress);

  const new_code_id = await store_code(
    "../artifacts/terra_manager-aarch64.wasm"
  );
  console.log("new code id: ", new_code_id);

  await migrate_contract(contract_addr, new_code_id);
  console.log("contract migrated.");
}

/*
const terra_manager_addr = await deploy();
console.log(
  `Successfully deployed TerraManager at address: ${terra_manager_addr}`
);
*/

async function may_3_2022_testnet_migration() {
  await initializeSequence(test_wallet);
  const terra_manager_addr = "terra1pzmq3sacc2z3pk8el3rk0q584qtuuhnv4fwp8n";
  const dn_manager_addr = "terra1qycrwtsmxnnklc42yzexveyhjls657qhuwhmlw";
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgMigrateContract(
        test_wallet.key.accAddress,
        terra_manager_addr,
        69423,
        {}
      ),
      new MsgMigrateContract(
        test_wallet.key.accAddress,
        dn_manager_addr,
        69424,
        {
          fee_collection_config: {
            performance_rate: "0.1",
            off_market_position_open_service_fee_uusd: "2000000",
            collector_addr: test_wallet.key.accAddress
          },
          position_open_allowed_mirror_assets: ["terra16vfxm98rxlc8erj4g0sj5932dvylgmdufnugk0",
          "terra1qg9ugndl25567u03jrr79xur2yk9d632fke3h2",
          "terra1nslem9lgwx53rvgqwd8hgq7pepsry6yr3wsen4",
          "terra1djnlav60utj06kk9dl7defsv8xql5qpryzvm3h",
          "terra18yx7ff8knc98p07pdkhm3u36wufaeacv47fuha",
          "terra1ax7mhqahj6vcqnnl675nqq2g9wghzuecy923vy",
          "terra12s2h8vlztjwu440khpc0063p34vm7nhu25w4p9",
          "terra12saaecsqwxj04fn0jsv4jmdyp6gylptf5tksge",
          "terra15dr4ah3kha68kam7a907pje9w6z2lpjpnrkd06",
          "terra1fdkfhgk433tar72t4edh6p6y9rmjulzc83ljuw",
          "terra1fucmfp8x4mpzsydjaxyv26hrkdg4vpdzdvf647",
          "terra18gphn8r437p2xmjpw7a79hgsglf5y4t0x7s5ee",
          "terra14gq9wj0tt6vu0m4ec2tkkv4ln3qrtl58lgdl2c",
          "terra1qre9crlfnulcg0m68qqywqqstplgvrzywsg3am",
          "terra179na3xcvjastpptnh9g6lnf75hqqjnsv9mqm3j",
          "terra1avryzxnsn2denq7p2d7ukm6nkck9s0rz2llgnc",
          "terra1fs6c6y65c65kkjanjwvmnrfvnm2s58ph88t9ky",
          "terra13myzfjdmvqkama2tt3v5f7quh75rv78w8kq6u6",
          "terra1csr22xvxs6r3gkjsl7pmjkmpt39mwjsrm0e2r8",
          "terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"]
        }
      ),
      new MsgExecuteContract(
        test_wallet.key.accAddress,
        dn_manager_addr,
        {
          update_admin_config: {
            delta_neutral_position_code_id: 69425
          }
        }
      )
    ],
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  console.log(response);
}

await may_3_2022_testnet_migration();

// Sample function call to open a delta-neutral position:
// await open_delta_neutral_position(terra_manager_addr, 1000);

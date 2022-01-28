import { LCDClient, MnemonicKey } from "@terra-money/terra.js";
import { DynamoDBClient, PutItemCommand } from "@aws-sdk/client-dynamodb";
import big from "big.js";

const client = new DynamoDBClient({ region: "us-west-2" });

const gasPrices = {
  uusd: 0.15,
};

const gasAdjustment = 1.5;

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

const terra_manager_mainnet = "terra1ajkmy2c0g84seh66apv9x6xt6kd3ag80jmcvtz";
const terra_manager_testnet = "terra1pzmq3sacc2z3pk8el3rk0q584qtuuhnv4fwp8n";

const position_ticks_dev = "position_ticks_dev";
const position_ticks_prod = "position_ticks";

const strategy_tvl_dev = "strategy_tvl_dev";
const strategy_tvl_prod = "strategy_tvl";

async function run_pipeline() {
  if (process.argv.length <= 2) {
    console.log("ERROR: please specify prod or dev argument.");
    return;
  }

  var terra_manager = "";
  var position_ticks_table = "";
  var strategy_tvl_table = "";

  if (process.argv[2] == "testnet") {
    terra_manager = terra_manager_testnet;
    position_ticks_table = position_ticks_dev;
    strategy_tvl_table = strategy_tvl_dev;
  } else if (process.argv[2] == "mainnet") {
    terra_manager = terra_manager_mainnet;
    position_ticks_table = position_ticks_prod;
    strategy_tvl_table = strategy_tvl_prod;
  } else {
    console.log("Malformed environment variable: ", process.argv[2]);
    return;
  }

  console.log(
    `Generating data for ${process.argv[2]} with terra manager address: ${terra_manager}`
  );
  console.log(
    `Position ticks table: ${position_ticks_table}. Strategy tvl table: ${strategy_tvl_table}`
  );

  const delta_neutral_strategy_id = "0";
  const terra_chain_id = 3;
  var mAssetToTVL = {};

  // Get next position id to establish limit.
  const next_position_res = await testnet.wasm.contractQuery(terra_manager, {
    get_next_position_id: {},
  });
  console.log("next position id: ", next_position_res.next_position_id);

  // Get delta neutral position manager.
  const delta_neutral_pos_mgr_res = await testnet.wasm.contractQuery(
    terra_manager,
    {
      get_strategy_metadata: {
        strategy_id: delta_neutral_strategy_id,
      },
    }
  );
  const position_manager_addr = delta_neutral_pos_mgr_res.manager_addr;
  console.log(
    "Delta-neutral position manager addr: ",
    delta_neutral_pos_mgr_res
  );

  // Loop over all positions to craft <wallet, position_id + metadata> map.
  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Query position metadata.
    const position_metadata_res = await testnet.wasm.contractQuery(
      terra_manager,
      {
        get_terra_position_info: {
          position_id: i.toString(),
        },
      }
    );
    console.log("position metadata response: ", position_metadata_res);

    const holder_addr = position_metadata_res.holder;
    console.log("Holder: ", holder_addr);

    const position_addr = await testnet.wasm.contractQuery(
      position_manager_addr,
      {
        get_position_contract_addr: {
          position: {
            chain_id: terra_chain_id,
            position_id: i.toString(),
          },
        },
      }
    );
    console.log("position addr: ", position_addr);

    // Get position info.
    const position_info = await testnet.wasm.contractQuery(position_addr, {
      get_position_info: {},
    });
    console.log("position info: ", position_info);
    if (position_info.detailed_info == null) {
      console.log("Position id ", i, " is closed.");
      continue;
    }
    // Process position ticks.
    const uusd_value = big(position_info.detailed_info.uusd_value);
    await write_position_ticks(
      position_ticks_table,
      i,
      parseInt(new Date().getTime() / 1e3),
      terra_chain_id,
      uusd_value
    );

    // Process per-strategy level aggregate metrics.
    const mirror_asset_addr = position_info.mirror_asset_cw20_addr;
    console.log("Processing asset addr: ", mirror_asset_addr);
    if (mirror_asset_addr in mAssetToTVL) {
      mAssetToTVL[mirror_asset_addr] =
        mAssetToTVL[mirror_asset_addr].add(uusd_value);
    } else {
      mAssetToTVL[mirror_asset_addr] = uusd_value;
    }
  }

  // Persist per-strategy level aggregate metrics.
  for (var strategy_id in mAssetToTVL) {
    const tvl_uusd = mAssetToTVL[strategy_id];
    await write_strategy_metrics(strategy_tvl_table, strategy_id, tvl_uusd);
  }
}

await run_pipeline();

async function write_strategy_metrics(table_name, strategy_id, tvl_uusd) {
  const input = {
    TableName: table_name,
    Item: {
      strategy_id: { S: strategy_id.toString() },
      tvl_uusd: { S: tvl_uusd.toString() },
      timestamp_sec: { N: parseInt(new Date().getTime() / 1e3).toString() },
    },
  };
  const command = new PutItemCommand(input);
  try {
    const results = await client.send(command);
    console.log(results);
  } catch (err) {
    console.error(err);
  }
}

async function write_position_ticks(
  table_name,
  position_id,
  timestamp_sec,
  chain_id,
  uusd_value
) {
  const input = {
    TableName: table_name,
    Item: {
      position_id: { N: position_id.toString() },
      timestamp_sec: { N: timestamp_sec.toString() },
      chain_id: { N: chain_id.toString() },
      uusd_value: { N: uusd_value.toString() },
    },
  };
  const command = new PutItemCommand(input);
  try {
    const results = await client.send(command);
    console.log(results);
  } catch (err) {
    console.error(err);
  }
}

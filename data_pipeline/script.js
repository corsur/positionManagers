import {
  LCDClient,
  MnemonicKey,
} from "@terra-money/terra.js";
import { DynamoDBClient, PutItemCommand } from "@aws-sdk/client-dynamodb";
import big from 'big.js';



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

async function run_pipeline() {
  const terra_manager = "terra1ettwsfevaz65sqf269m9txs8mv923zas44aaj0";
  const delta_neutral_strategy_id = "0";
  const terra_chain_id = 3;
  var mAssetToTVL = {};

  // Get next position id to establish limit.
  const next_position_res = await testnet.wasm.contractQuery(terra_manager, {
    get_next_position_id: {}
  });
  console.log('next position id: ', next_position_res.next_position_id);

  // Get delta neutral position manager.
  const delta_neutral_pos_mgr_res = await testnet.wasm.contractQuery(terra_manager, {
    "get_strategy_metadata": {
      "strategy_id": delta_neutral_strategy_id
    }
  });
  const position_manager_addr = delta_neutral_pos_mgr_res.manager_addr;
  console.log("Delta-neutral position manager addr: ", delta_neutral_pos_mgr_res);

  // Loop over all positions to craft <wallet, position_id + metadata> map.
  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Query position metadata.
    const position_metadata_res = await testnet.wasm.contractQuery(terra_manager, {
      get_terra_position_info: {
        position_id: i.toString()
      }
    });
    console.log('position metadata response: ', position_metadata_res);

    const holder_addr = position_metadata_res.holder;
    console.log("Holder: ", holder_addr);

    const position_addr = await testnet.wasm.contractQuery(position_manager_addr, {
      "get_position_contract_addr": {
        "position": {
          "chain_id": terra_chain_id,
          "position_id": i.toString()
        }
      }
    });
    console.log('position addr: ', position_addr);

    // Get position info.
    const position_info = await testnet.wasm.contractQuery(position_addr, {
      get_position_info: {}
    });
    console.log('position info: ', position_info);
    if (position_info.detailed_info == null) {
      console.log("Position id ", i, " is closed.");
      continue;
    }
    // Process position ticks.
    const uusd_value = big(position_info.detailed_info.uusd_value);
    await write_position_ticks(i, parseInt(new Date().getTime() / 10e3), terra_chain_id, uusd_value);

    // Process per-strategy level aggregate metrics.
    const mirror_asset_addr = position_info.detailed_info.state.mirror_asset_cw20_addr;
    console.log('Processing asset addr: ', mirror_asset_addr);
    if (mirror_asset_addr in mAssetToTVL) {
      mAssetToTVL[mirror_asset_addr] = mAssetToTVL[mirror_asset_addr].add(uusd_value);
    } else {
      mAssetToTVL[mirror_asset_addr] = uusd_value;
    }
  }

  // Persist per-strategy level aggregate metrics.
  for (var strategy_id in mAssetToTVL) {
    const tvl_uusd = mAssetToTVL[strategy_id];
    await write_strategy_metrics(strategy_id, tvl_uusd);
  }
}

await run_pipeline();

async function write_strategy_metrics(strategy_id, tvl_uusd) {
  const input = {
    TableName: "strategy_tvl",
    Item: {
      strategy_id: { S: strategy_id.toString() },
      tvl_uusd: { S: tvl_uusd.toString() },
      timestamp_sec: { N: parseInt((new Date().getTime() / 10e3)).toString() }
    }
  };
  const command = new PutItemCommand(input);
  try {
    const results = await client.send(command);
    console.log(results);
  } catch (err) {
    console.error(err);
  }
}

async function write_position_ticks(position_id, timestamp_sec, chain_id, uusd_value) {
  const input = {
    TableName: "position_ticks",
    Item: {
      position_id: { N: position_id.toString() },
      timestamp_sec: { N: timestamp_sec.toString() },
      chain_id: { N: chain_id.toString() },
      uusd_value: { N: uusd_value.toString() },
    }
  };
  const command = new PutItemCommand(input);
  try {
    const results = await client.send(command);
    console.log(results);
  } catch (err) {
    console.error(err);
  }
}

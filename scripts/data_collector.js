import {
  BatchWriteItemCommand,
  DynamoDBClient,
  PutItemCommand,
} from "@aws-sdk/client-dynamodb";
import {
  CloudWatchClient,
  PutMetricDataCommand,
} from "@aws-sdk/client-cloudwatch";
import big from "big.js";
import { ArgumentParser } from "argparse";
import {
  delay,
  DELTA_NEUTRAL_STRATEGY_ID,
  DYNAMODB_BATCH_WRITE_ITEM_LIMIT,
  mainnetTerraData,
  TERRA_CHAIN_ID,
  TERRA_MANAGER_MAINNET,
  TERRA_MANAGER_TESTNET,
  testnetTerra,
} from "./utils/terra.js";

// Global variables setup.
var blockchain_network = undefined;
const client = new DynamoDBClient({ region: "us-west-2" });
const position_ticks_dev = "position_ticks_dev";
const position_ticks_prod = "position_ticks";
const strategy_tvl_dev = "strategy_tvl_dev";
const strategy_tvl_prod = "strategy_tvl";
const terraswap_data_table_dev = "terraswap_data_dev";
const terraswap_data_table_prod = "terraswap_data";
const terraswap_api_address_dev = "https://api-bombay.terraswap.io/pairs";
const terraswap_api_address_prod = "https://api.terraswap.io/dashboard/pairs";

const cw_client = new CloudWatchClient({ region: "us-west-2" });
// Metrics definitions.
var metrics = {};
const CONTRACT_QUERY_ERROR = "CONTRACT_QUERY_ERROR";
const DATA_COLLECTOR_START = "DATA_COLLECTOR_START";
const TOTAL_POSITION_COVERED = "TOTAL_POSITION_COVERED";
const GET_NEXT_POSITION_ID_FAILURE = "GET_NEXT_POSITION_ID_FAILURE";
const GET_POSITION_MANAGER_FAILURE = "GET_POSITION_MANAGER_FAILURE";
const GET_POSITION_CONTRACT_FAILURE = "GET_POSITION_CONTRACT_FAILURE";
const GET_POSITION_INFO_FAILURE = "GET_POSITION_INFO_FAILURE";
const DB_POSITION_TICKS_WRITE_FAILURE = "DB_POSITION_TICKS_WRITE_FAILURE";
const DB_POSITION_TICKS_WRITE_SUCCESS = "DB_POSITION_TICKS_WRITE_SUCCESS";
const DB_STRATEGY_TVL_WRITE_FAILURE = "DB_STRATEGY_TVL_WRITE_FAILURE";
const DB_STRATEGY_TVL_WRITE_SUCCESS = "DB_STRATEGY_TVL_WRITE_SUCCESS";

async function run_pipeline() {
  // Setup argument parsing and input specs.
  const parser = new ArgumentParser({
    description: "Data collector for Aperture.",
  });
  parser.add_argument("-n", "--network", {
    help: "The blockchain network to operate on. Either mainnet or testnet.",
    required: true,
    type: "str",
    choices: ["mainnet", "testnet"],
  });
  parser.add_argument("-q", "--qps", {
    help: "Number of rebalance per second.",
    required: false,
    default: 10,
    type: "int",
  });

  const { network, qps } = parser.parse_args();
  blockchain_network = network;

  // Initialize metric counters.
  metrics[CONTRACT_QUERY_ERROR] = 0;
  metrics[DATA_COLLECTOR_START] = 0;
  metrics[TOTAL_POSITION_COVERED] = 0;
  metrics[GET_NEXT_POSITION_ID_FAILURE] = 0;
  metrics[GET_POSITION_MANAGER_FAILURE] = 0;
  metrics[GET_POSITION_CONTRACT_FAILURE] = 0;
  metrics[GET_POSITION_INFO_FAILURE] = 0;
  metrics[DB_POSITION_TICKS_WRITE_FAILURE] = 0;
  metrics[DB_POSITION_TICKS_WRITE_SUCCESS] = 0;
  metrics[DB_STRATEGY_TVL_WRITE_FAILURE] = 0;
  metrics[DB_STRATEGY_TVL_WRITE_SUCCESS] = 0;

  // Signal run status.
  metrics[DATA_COLLECTOR_START] = 1;

  // Setup network specific variables.
  var terra_manager = "";
  var position_ticks_table = "";
  var strategy_tvl_table = "";
  var terraswap_data_table = "";
  var terraswap_api_address = "";
  var connection = undefined;

  if (blockchain_network == "testnet") {
    terra_manager = TERRA_MANAGER_TESTNET;
    position_ticks_table = position_ticks_dev;
    strategy_tvl_table = strategy_tvl_dev;
    terraswap_data_table = terraswap_data_table_dev;
    terraswap_api_address = terraswap_api_address_dev;
    connection = testnetTerra;
  } else if (blockchain_network == "mainnet") {
    terra_manager = TERRA_MANAGER_MAINNET;
    position_ticks_table = position_ticks_prod;
    strategy_tvl_table = strategy_tvl_prod;
    terraswap_data_table = terraswap_data_table_prod;
    terraswap_api_address = terraswap_api_address_prod;
    connection = mainnetTerraData;
  } else {
    console.log(`Invalid network argument ${blockchain_network}`);
    return;
  }

  console.log(
    `Generating data for ${blockchain_network} with terra manager address: ${terra_manager}`
  );
  console.log(
    `Position ticks table: ${position_ticks_table}. Strategy tvl table: ${strategy_tvl_table}`
  );

  var mAssetToTVL = {};

  // Get next position id to establish limit.
  var next_position_res = undefined;
  try {
    next_position_res = await connection.wasm.contractQuery(terra_manager, {
      get_next_position_id: {},
    });
  } catch (error) {
    console.log("Failed to get next position id.");
    metrics[GET_NEXT_POSITION_ID_FAILURE]++;
    return;
  }
  console.log("next position id: ", next_position_res.next_position_id);

  // Get delta neutral position manager.
  var delta_neutral_pos_mgr_res = undefined;
  try {
    delta_neutral_pos_mgr_res = await connection.wasm.contractQuery(
      terra_manager,
      {
        get_strategy_metadata: {
          strategy_id: DELTA_NEUTRAL_STRATEGY_ID,
        },
      }
    );
  } catch (error) {
    console.log(
      `Failed to get delta-neutral strategy manager with error ${error}`
    );
    metrics[GET_POSITION_MANAGER_FAILURE]++;
    return;
  }

  const position_manager_addr = delta_neutral_pos_mgr_res.manager_addr;
  console.log(`Delta-neutral position manager addr: ${position_manager_addr}`);

  // Loop over all positions to craft <wallet, position_id + metadata> map.
  var promises = [];
  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Trottle request.
    if (i % qps == 0) {
      await delay(1000);
    }

    promises.push(doWork(connection, position_manager_addr, mAssetToTVL, i));
  }

  const resolved_promises = (await Promise.allSettled(promises)).filter(
    (promise) => promise.status == "fulfilled" && promise.value != undefined
  );
  console.log(`Total position ticks to send: ${resolved_promises.length}`);

  // Construct position ticks batch write request.
  var position_ticks_items = [];
  // Iterating over resolved promises.
  for (const [index, resolved_promise] of resolved_promises.entries()) {
    const raw_item = resolved_promise.value;
    position_ticks_items.push({
      PutRequest: {
        Item: {
          position_id: { N: raw_item.position_id.toString() },
          timestamp_sec: {
            N: parseInt(new Date().getTime() / 1e3).toString(),
          },
          chain_id: { N: TERRA_CHAIN_ID.toString() },
          uusd_value: { N: raw_item.uusd_value.toString() },
        },
      },
    });
    // Only send batch write if either of the followings is true:
    //   1. We've reached DynamoDB batch size limit.
    //   2. Or, this is the last batch.
    if (
      position_ticks_items.length == DYNAMODB_BATCH_WRITE_ITEM_LIMIT ||
      index == resolved_promises.length - 1
    ) {
      // Construct batch request.
      const position_ticks_batch = new BatchWriteItemCommand({
        RequestItems: {
          [position_ticks_table]: position_ticks_items,
        },
      });

      // Send request and send metrics as needed.
      try {
        await client.send(position_ticks_batch);
        metrics[TOTAL_POSITION_COVERED] += position_ticks_items.length;
        console.log("Position ticks batch write is successful.");
        metrics[DB_POSITION_TICKS_WRITE_SUCCESS]++;
      } catch (error) {
        console.log(
          `Failed to batch write for position ticks with error: ${error}`
        );
        metrics[DB_POSITION_TICKS_WRITE_FAILURE]++;
      } finally {
        position_ticks_items = [];
      }
    }
  }

  // Persist per-strategy level aggregate metrics.
  for (var strategy_id in mAssetToTVL) {
    const tvl_uusd = mAssetToTVL[strategy_id];
    await write_strategy_metrics(strategy_tvl_table, strategy_id, tvl_uusd);
  }

  // Query and persist Terraswap data.
  // axios.get(terraswap_api_address)
  //   .then((response) => {
  //     response.data.forEach(pair_data => {
  //       const timestamp_sec = Date.parse(pair_data.timestamp).getTime() / 1e3;
  //       await write_terraswap_data(terraswap_data_table, timestamp_sec, pair_data.pairAddress, pair_data.apr);
  //     });
  //   }, (error) => {
  //     console.log(error);
  //   });
}

async function doWork(
  connection,
  position_manager_addr,
  mAssetToTVL,
  position_id
) {
  // Get address for position contract.
  var position_addr = undefined;
  try {
    position_addr = await connection.wasm.contractQuery(position_manager_addr, {
      get_position_contract_addr: {
        position: {
          chain_id: TERRA_CHAIN_ID,
          position_id: position_id.toString(),
        },
      },
    });
  } catch (error) {
    console.log(`Failed to get position contract address with error ${error}`);
    metrics[GET_POSITION_CONTRACT_FAILURE]++;
    return;
  }

  // Get detailed position info.
  var position_info = undefined;
  try {
    position_info = await connection.wasm.contractQuery(position_addr, {
      get_position_info: {},
    });
  } catch (error) {
    console.log(`Failed to get position info with error ${error}`);
    metrics[GET_POSITION_INFO_FAILURE]++;
    return;
  }

  if (position_info.detailed_info == null) {
    return;
  }

  // Process per-strategy level aggregate metrics.
  const uusd_value = big(position_info.detailed_info.uusd_value);
  const mirror_asset_addr = position_info.mirror_asset_cw20_addr;
  if (mirror_asset_addr in mAssetToTVL) {
    mAssetToTVL[mirror_asset_addr] =
      mAssetToTVL[mirror_asset_addr].add(uusd_value);
  } else {
    mAssetToTVL[mirror_asset_addr] = uusd_value;
  }

  return {
    position_id: position_id,
    uusd_value: uusd_value,
  };
}

async function write_terraswap_data(
  table_name,
  timestamp_sec,
  pairAddress,
  apr
) {
  const input = {
    TableName: table_name,
    Item: {
      timestamp_sec: { N: timestamp_sec },
      pair_address: { S: pairAddress },
      apr: { S: apr },
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
    await client.send(command);
    metrics[DB_STRATEGY_TVL_WRITE_SUCCESS]++;
  } catch (err) {
    console.error(`Strategy TVL write failed with error:  ${err}`);
    metrics[DB_STRATEGY_TVL_WRITE_FAILURE]++;
  }
}

async function publishMetrics(metrics_and_count) {
  var metrics_data = [];
  for (var key in metrics_and_count) {
    const metric_data = {
      MetricName: key,
      Timestamp: new Date(),
      Dimensions: [
        {
          Name: "Network",
          Value: blockchain_network,
        },
      ],
      Unit: "Count",
      Value: metrics_and_count[key],
    };
    metrics_data.push(metric_data);
  }
  const metrics_to_publish = {
    MetricData: metrics_data,
    Namespace: "ApertureDataCollector",
  };
  try {
    await cw_client.send(new PutMetricDataCommand(metrics_to_publish));
  } catch (error) {
    console.log("FATAL: Failed to send metrics to CloudWatch.");
  }
}

// Start.
try {
  await run_pipeline();
} catch (error) {
  console.log(`Uncaught error at data pipeline: ${error}`);
} finally {
  await publishMetrics(metrics);
  console.log("Data collector script execution completed.");
}

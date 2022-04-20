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
import {
  ArgumentParser
} from "argparse";
import {
  DELTA_NEUTRAL_STRATEGY_ID,
  DYNAMODB_BATCH_WRITE_ITEM_LIMIT,
  mainnetTerraData,
  TERRA_CHAIN_ID,
  TERRA_MANAGER_MAINNET,
  TERRA_MANAGER_TESTNET,
  testnetTerra,
} from "./utils/terra.js";
import {
  generateRangeArray
} from "./utils/hive.js";
import pool from "@ricokahler/pool";
import {
  getPositionInfoQueries,
} from "./utils/graphql_queries.js";
import axios from "axios";
import axiosRetry from "axios-retry";

// Configure retry mechanism global Axios instance.
axiosRetry(axios, {
  retries: 3,
  retryDelay: axiosRetry.exponentialDelay
});

// Global variables setup.

var blockchain_network = undefined;
const region = "us-west-2";
const client = new DynamoDBClient({
  region
});
const position_ticks_dev = "position_ticks_dev";
const position_ticks_prod = "position_ticks";
const strategy_tvl_dev = "strategy_tvl_dev";
const strategy_tvl_prod = "strategy_tvl";
const latest_strategy_tvl_dev = "latest_strategy_tvl_dev";
const latest_strategy_tvl_prod = "latest_strategy_tvl";
const terra_hive_address_dev = "https://testnet-hive.terra.dev/graphql";
const terra_hive_address_prod = "https://hive.terra.dev/graphql";

const cw_client = new CloudWatchClient({
  region
});
// Metrics definitions.
var metrics = {};
const CONTRACT_QUERY_ERROR = "CONTRACT_QUERY_ERROR";
const HIVE_QUERY_ERROR = "HIVE_QUERY_ERROR";
const DATA_COLLECTOR_START = "DATA_COLLECTOR_START";
const TOTAL_POSITION_COVERED = "TOTAL_POSITION_COVERED";
const GET_NEXT_POSITION_ID_FAILURE = "GET_NEXT_POSITION_ID_FAILURE";
const GET_POSITION_MANAGER_FAILURE = "GET_POSITION_MANAGER_FAILURE";
const GET_POSITION_CONTRACT_FAILURE = "GET_POSITION_CONTRACT_FAILURE";
const GET_POSITION_INFO_FAILURE = "GET_POSITION_INFO_FAILURE";
const BATCH_GET_POSITION_INFO_FAILURE = "BATCH_GET_POSITION_INFO_FAILURE";
const DB_POSITION_TICKS_WRITE_FAILURE = "DB_POSITION_TICKS_WRITE_FAILURE";
const DB_POSITION_TICKS_WRITE_SUCCESS = "DB_POSITION_TICKS_WRITE_SUCCESS";
const DB_STRATEGY_TVL_WRITE_FAILURE = "DB_STRATEGY_TVL_WRITE_FAILURE";
const DB_STRATEGY_TVL_WRITE_SUCCESS = "DB_STRATEGY_TVL_WRITE_SUCCESS";
const DB_LATEST_STRATEGY_TVL_WRITE_FAILURE = "DB_LATEST_STRATEGY_TVL_WRITE_FAILURE";
const DB_LATEST_STRATEGY_TVL_WRITE_SUCCESS = "DB_LATEST_STRATEGY_TVL_WRITE_SUCCESS";

async function run_pipeline() {
  // Setup argument parsing and input specs.
  const parser = new ArgumentParser({
    description: "Data collector for Aperture.",
  });
  parser.add_argument("-n", "--network", {
    help: "The blockchain network to operate on. Either mainnet or testnet.",
    required: false,
    default: "testnet",
    type: "str",
    choices: ["mainnet", "testnet"],
  });
  parser.add_argument("-q", "--qps", {
    help: "Number of rebalance per second.",
    required: false,
    default: 10,
    type: "int",
  });
  parser.add_argument("-hbs", "--hive_batch_size", {
    help: "Number of positions to query against Terra Hive.",
    required: false,
    default: 200,
    type: "int",
  });

  const {
    network,
    qps,
    hive_batch_size,
  } = parser.parse_args();
  blockchain_network = network;

  // Initialize metric counters.
  metrics[CONTRACT_QUERY_ERROR] = 0;
  metrics[DATA_COLLECTOR_START] = 0;
  metrics[TOTAL_POSITION_COVERED] = 0;
  metrics[GET_NEXT_POSITION_ID_FAILURE] = 0;
  metrics[GET_POSITION_MANAGER_FAILURE] = 0;
  metrics[GET_POSITION_CONTRACT_FAILURE] = 0;
  metrics[GET_POSITION_INFO_FAILURE] = 0;
  metrics[BATCH_GET_POSITION_INFO_FAILURE] = 0;
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
  var latest_strategy_tvl_table = "";
  var connection = undefined;
  var terra_hive_address = "";

  if (blockchain_network == "testnet") {
    terra_manager = TERRA_MANAGER_TESTNET;
    position_ticks_table = position_ticks_dev;
    strategy_tvl_table = strategy_tvl_dev;
    latest_strategy_tvl_table = latest_strategy_tvl_dev;
    connection = testnetTerra;
    terra_hive_address = terra_hive_address_dev;
  } else if (blockchain_network == "mainnet") {
    terra_manager = TERRA_MANAGER_MAINNET;
    position_ticks_table = position_ticks_prod;
    strategy_tvl_table = strategy_tvl_prod;
    latest_strategy_tvl_table = latest_strategy_tvl_prod;
    connection = mainnetTerraData;
    terra_hive_address = terra_hive_address_prod;
  } else {
    console.log(`Invalid network argument ${blockchain_network}`);
    return;
  }

  console.log(
    `Generating data for ${blockchain_network} with terra manager address: ${terra_manager}`
  );
  console.log(
    `Position ticks table: ${position_ticks_table}. Strategy tvl table: ${strategy_tvl_table}. Latest strategy tvl table: ${latest_strategy_tvl_table}.`
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
      terra_manager, {
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

  // Fetch position infos.
  const next_id = parseInt(next_position_res.next_position_id);
  var position_infos_promise = await getPositionInfos(
    next_id,
    qps,
    position_manager_addr,
    hive_batch_size,
    terra_hive_address,
  )

  const resolved_promises = position_infos_promise.filter(x => x.items[0].info.detailed_info !== null).map(x => {
    const {
      info,
      position
    } = x.items[0];
    // Process per-strategy level aggregate metrics.
    const uusd_value = big(info.detailed_info.uusd_value);
    const mirror_asset_addr = info.mirror_asset_cw20_addr;
    if (mirror_asset_addr in mAssetToTVL) {
      mAssetToTVL[mirror_asset_addr] =
        mAssetToTVL[mirror_asset_addr].add(uusd_value);
    } else {
      mAssetToTVL[mirror_asset_addr] = uusd_value;
    }
    return {
      position_id: position.position_id,
      uusd_value: uusd_value,
    };
  });

  console.log(`Total position ticks to send: ${resolved_promises.length}`);

  // Construct position ticks batch write request.
  var position_ticks_items = [];
  // Iterating over resolved promises.
  for (const [index, raw_item] of resolved_promises.entries()) {
    position_ticks_items.push({
      PutRequest: {
        Item: {
          position_id: {
            N: raw_item.position_id.toString()
          },
          timestamp_sec: {
            N: parseInt(new Date().getTime() / 1e3).toString(),
          },
          chain_id: {
            N: TERRA_CHAIN_ID.toString()
          },
          uusd_value: {
            N: raw_item.uusd_value.toString()
          },
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
    await write_latest_strategy_metrics(latest_strategy_tvl_table, strategy_id, tvl_uusd);
  }
}

function strategy_input(table_name, strategy_id, tvl_uusd){
  const input = {
    TableName: table_name,
    Item: {
      strategy_id: {
        S: strategy_id.toString()
      },
      tvl_uusd: {
        S: tvl_uusd.toString()
      },
      timestamp_sec: {
        N: parseInt(new Date().getTime() / 1e3).toString()
      },
    },
  };
  return input;
}

async function write_strategy_metrics(table_name, strategy_id, tvl_uusd) {
  const command = new PutItemCommand(strategy_input(table_name, strategy_id, tvl_uusd));
  try {
    await client.send(command);
    metrics[DB_STRATEGY_TVL_WRITE_SUCCESS]++
  } catch (err) {
    console.error(`Strategy TVL write failed with error:  ${err}`);
    metrics[DB_LATEST_STRATEGY_TVL_WRITE_FAILURE];
  }
}

async function write_latest_strategy_metrics(table_name, strategy_id, tvl_uusd) {
  const command = new PutItemCommand(strategy_input(table_name, strategy_id, tvl_uusd));
  try {
    await client.send(command);
    metrics[DB_LATEST_STRATEGY_TVL_WRITE_SUCCESS]++
  } catch (err) {
    console.error(`Latest Strategy : 'Strategy'} TVL write failed with error:  ${err}`);
    metrics[DB_LATEST_STRATEGY_TVL_WRITE_FAILURE];
  }
}

async function publishMetrics(metrics_and_count) {
  var metrics_data = [];
  for (var key in metrics_and_count) {
    const metric_data = {
      MetricName: key,
      Timestamp: new Date(),
      Dimensions: [{
        Name: "Network",
        Value: blockchain_network,
      }, ],
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

async function getPositionInfos(
  next_id,
  qps,
  position_manager_addr,
  hive_batch_size,
  terra_hive_address,
) {
  var all_position_infos = undefined;
  let hive_query_num_batches = Math.ceil(parseFloat(next_id) / hive_batch_size);
  try {
    all_position_infos = (
      await pool({
        collection: generateRangeArray(0, hive_query_num_batches),
        maxConcurrency: qps,
        task: async (batch_id) => {
          let start_position_id = batch_id * hive_batch_size;
          let end_position_id = Math.min(
            start_position_id + hive_batch_size,
            next_id
          );
          let hive_query = getPositionInfoQueries(
            generateRangeArray(start_position_id, end_position_id),
            position_manager_addr
          );

          let hive_response = await axios({
            method: "post",
            url: terra_hive_address,
            data: {
              query: hive_query,
            },
          });
          return Object.values(hive_response.data.data).map(
            (element) => element.contractQuery
          );
        },
      })
    ).flat();
    console.log("Querying position infos using Terra Hive.");
  } catch (error) {
    console.log(
      `Failed to query Terra Hive with error: ${error}.`
    );
    metrics[BATCH_GET_POSITION_INFO_FAILURE]++;
    metrics[HIVE_QUERY_ERROR]++;
    return;
  }
  return all_position_infos;
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
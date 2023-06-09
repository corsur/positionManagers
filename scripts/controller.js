import {
  CloudWatchClient,
  PutMetricDataCommand,
} from "@aws-sdk/client-cloudwatch";
import big from "big.js";
const { Big } = big;
import { ArgumentParser } from "argparse";
import {
  delay,
  DELTA_NEUTRAL_STRATEGY_ID,
  mainnetTerraController,
  mAssetMap,
  CR_SAFETY_MARGIN,
  MIRROR_MINT_MAINNET,
  MIRROR_MINT_TESTNET,
  MIRROR_ORACLE_MAINNET,
  MIRROR_ORACLE_TESTNET,
  TERRA_CHAIN_ID,
  TERRA_MANAGER_MAINNET,
  TERRA_MANAGER_TESTNET,
  testnetTerra,
} from "./utils/terra.js";
import {
  isTxError,
  MnemonicKey,
  MsgExecuteContract,
} from "@terra-money/terra.js";
import pool from "@ricokahler/pool";
import {
  getMAssetQuoteQueries,
  getMAssetRequiredCRQueries,
  getPositionInfoQueries,
} from "./utils/graphql_queries.js";
import { generateRangeArray } from "./utils/hive.js";
import axios from "axios";
import axiosRetry from "axios-retry";
import dotenv from "dotenv";

// Setup env variable loading.
dotenv.config();

// Configure retry mechanism global Axios instance.
axiosRetry(axios, { retries: 3, retryDelay: axiosRetry.exponentialDelay });

async function publishMetrics(metrics_and_count) {
  var metrics_data = [];
  for (var key in metrics_and_count) {
    const metric_data = {
      MetricName: key,
      Timestamp: new Date(),
      Dimensions: [
        {
          Name: "Network",
          Value: "Mainnet",
        },
      ],
      Unit: "Count",
      Value: metrics_and_count[key],
    };
    metrics_data.push(metric_data);
  }
  const metrics_to_publish = {
    MetricData: metrics_data,
    Namespace: "ApertureController",
  };
  await client.send(new PutMetricDataCommand(metrics_to_publish));
}

const HIVE_ENDPOINT = "http://hive-lb-1253409394.us-west-2.elb.amazonaws.com:8085/graphql";
const client = new CloudWatchClient({ region: "us-west-2" });
var metrics = {};
const CONTRACT_QUERY_ERROR = "CONTRACT_QUERY_ERROR";
const HIVE_QUERY_ERROR = "HIVE_QUERY_ERROR";
const CONTROLLER_START = "CONTROLLER_START";
const MIRROR_ORACLE_QUERY_FAILURE = "MIRROR_ORACLE_QUERY_FAILURE";
const MIRROR_MINT_QUERY_FAILURE = "MIRROR_MINT_QUERY_FAILURE";
const GET_NEXT_POSITION_ID_FAILURE = "GET_NEXT_POSITION_ID_FAILURE";
const GET_POSITION_MANAGER_FAILURE = "GET_POSITION_MANAGER_FAILURE";
const GET_POSITION_MANAGER_ADMIN_CONFIG_FAILURE =
  "GET_POSITION_MANAGER_ADMIN_CONFIG_FAILURE";
const NUM_PROCESSED_POSITION = "NUM_PROCESSED_POSITION";
const BATCH_GET_POSITION_INFO_FAILURE = "BATCH_GET_POSITION_INFO_FAILURE";
const QUERY_POSITION_CONTRACT_INFO_FAILURE =
  "QUERY_POSITION_CONTRACT_INFO_FAILURE";
const GET_POSITION_INFO_FAILURE = "GET_POSITION_INFO_FAILURE";
const SHOULD_REBALANCE_POSITION = "SHOULD_REBALANCE_POSITION";
const REBALANCE_FAILURE = "REBALANCE_FAILURE";
const REBALANCE_SUCCESS = "REBALANCE_SUCCESS";
const REBALANCE_CREATE_AND_SIGN_FAILURE = "REBALANCE_CREATE_AND_SIGN_FAILURE";
const REBALANCE_BROADCAST_FAILURE = "REBALANCE_BROADCAST_FAILURE";

async function run_pipeline() {
  const parser = new ArgumentParser({
    description: "Aperture Finance Controller",
  });

  parser.add_argument("-n", "--network", {
    help: "The blockchain network to operate on. Either mainnet or testnet.",
    required: true,
    type: "str",
    choices: ["mainnet", "testnet"],
  });
  parser.add_argument("-d", "--delta_tolerance", {
    help: "The delta neutral tolerance percentage to trigger rebalance.",
    required: true,
    type: "float",
  });
  parser.add_argument("-b", "--balance_tolerance", {
    help: "The balance tolerance percentage to trigger rebalance.",
    required: true,
    type: "float",
  });
  parser.add_argument("-t", "--time_tolerance", {
    help: "The maximum allowed time in seconds after last orcale update timestamp.",
    required: true,
    type: "int",
  });
  parser.add_argument("-q", "--qps", {
    help: "Number of rebalance per second.",
    required: false,
    default: 2,
    type: "int",
  });
  parser.add_argument("-bs", "--batch_size", {
    help: "Number of positions to include per tx.",
    required: false,
    default: 5,
    type: "int",
  });
  parser.add_argument("-hbs", "--hive_batch_size", {
    help: "Number of positions to query against Terra Hive.",
    required: false,
    default: 100,
    type: "int",
  });

  // Parse and validate.
  const {
    network,
    delta_tolerance,
    balance_tolerance,
    time_tolerance,
    qps,
    batch_size,
    hive_batch_size,
  } = parser.parse_args();

  var terra_manager = "";
  var connection = undefined;
  var mirror_oracle_addr = "";
  var mirror_mint_addr = "";

  if (network == "testnet") {
    terra_manager = TERRA_MANAGER_TESTNET;
    connection = testnetTerra;
    mirror_oracle_addr = MIRROR_ORACLE_TESTNET;
    mirror_mint_addr = MIRROR_MINT_TESTNET;
  } else if (network == "mainnet") {
    terra_manager = TERRA_MANAGER_MAINNET;
    connection = mainnetTerraController;
    mirror_oracle_addr = MIRROR_ORACLE_MAINNET;
    mirror_mint_addr = MIRROR_MINT_MAINNET;
  } else {
    console.log(`Invalid network argument ${parser.parse_args().network}`);
    return;
  }

  // Initialize metric counters.
  metrics[CONTRACT_QUERY_ERROR] = 0;
  metrics[HIVE_QUERY_ERROR] = 0;
  metrics[CONTROLLER_START] = 0;
  metrics[MIRROR_ORACLE_QUERY_FAILURE] = 0;
  metrics[MIRROR_MINT_QUERY_FAILURE] = 0;
  metrics[GET_NEXT_POSITION_ID_FAILURE] = 0;
  metrics[GET_POSITION_MANAGER_FAILURE] = 0;
  metrics[GET_POSITION_MANAGER_ADMIN_CONFIG_FAILURE] = 0;
  metrics[NUM_PROCESSED_POSITION] = 0;
  metrics[BATCH_GET_POSITION_INFO_FAILURE] = 0;
  metrics[QUERY_POSITION_CONTRACT_INFO_FAILURE] = 0;
  metrics[GET_POSITION_INFO_FAILURE] = 0;
  metrics[SHOULD_REBALANCE_POSITION] = 0;
  metrics[REBALANCE_FAILURE] = 0;
  metrics[REBALANCE_SUCCESS] = 0;
  metrics[REBALANCE_CREATE_AND_SIGN_FAILURE] = 0;
  metrics[REBALANCE_BROADCAST_FAILURE] = 0;

  // Send running signal to AWS CloudWatch.
  metrics[CONTROLLER_START]++;

  const wallet = connection.wallet(
    new MnemonicKey({
      mnemonic:
        "witness produce visit clock feature chicken rural trend sock play weird barrel excess edge correct weird toilet buffalo vocal sock early similar unhappy gospel",
    })
  );

  console.log(
    `Controller operating on ${
      parser.parse_args().network
    } with terra manager address: ${terra_manager}`
  );

  // Get next position id to establish limit.
  var next_position_res = undefined;
  try {
    next_position_res = await connection.wasm.contractQuery(terra_manager, {
      get_next_position_id: {},
    });
    console.log("next position id: ", next_position_res.next_position_id);
  } catch (error) {
    console.log(`Failed to get next position id with error: ${error}`);
    metrics[GET_NEXT_POSITION_ID_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }

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
      `Failed to get delta-neutral position manager with error: ${error}`
    );
    metrics[GET_POSITION_MANAGER_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }
  const position_manager_addr = delta_neutral_pos_mgr_res.manager_addr;

  const asset_timestamps_promise = getAssetTimestamp(
    connection,
    qps,
    mirror_oracle_addr
  );

  const asset_required_cr_promise = getAssetRequiredCR(
    connection,
    qps,
    mirror_mint_addr
  );

  // Fetch position infos.
  const next_id = parseInt(next_position_res.next_position_id);
  var position_infos_promise = getPositionInfos(
    next_id,
    qps,
    connection,
    position_manager_addr,
    hive_batch_size
  );

  var [asset_timestamps, asset_required_crs, position_infos] =
    await Promise.all([
      asset_timestamps_promise,
      asset_required_cr_promise,
      position_infos_promise,
    ]);

  asset_timestamps = asset_timestamps.reduce((acc, cur) => {
    if (cur && cur.token_addr) {
      acc[cur.token_addr] = cur;
    }
    return acc;
  }, {});

  asset_required_crs = asset_required_crs.reduce((acc, cur) => {
    if (cur && cur.token_addr) {
      acc[cur.token_addr] = cur.required_cr;
    }
    return acc;
  }, {});

  // An undefined position info indicates fetching errors.
  position_infos = position_infos.filter((ele) => ele != undefined);

  // Make rebalance decisions.
  var rebalance_infos = [];
  position_infos = position_infos.map((batch_position_info) => {
    const rebalance_info = handleRebalance(
      batch_position_info,
      asset_timestamps,
      asset_required_crs,
      delta_tolerance,
      balance_tolerance,
      time_tolerance,
      wallet
    );
    // If a position doesn't need rebalance, `handleRebalance` will return
    // undefined.
    if (rebalance_info) {
      rebalance_infos.push(rebalance_info);
    }
  });

  console.log(
    `Total number of positions to rebalance: ${rebalance_infos.length}`
  );

  console.dir(rebalance_infos, { depth: null });

  var num_included_positions = 0;
  var msgs_acc = [];
  var position_ids = [];
  var position_addrs = [];
  for (const [index, rebalance_info] of rebalance_infos.entries()) {
    const { msg, asset_name, position_id, position_addr, reason } =
      rebalance_info;
    num_included_positions++;
    msgs_acc.push(msg);
    position_ids.push(position_id);
    position_addrs.push(position_addr);

    // Send out tx if:
    //   1. We have accumulated enough positions.
    //   2. Or, we are at the last batch.
    if (
      num_included_positions == batch_size ||
      index == rebalance_infos.length - 1
    ) {
      // Prepend migration msg for the current batch.
      const migrate_msg = new MsgExecuteContract(
        /*sender=*/ wallet.key.accAddress,
        /*contract=*/ position_manager_addr,
        {
          migrate_position_contracts: {
            positions: [],
            position_contracts: position_addrs,
          },
        }
      );
      msgs_acc = [migrate_msg, ...msgs_acc];

      var tx = undefined;
      var attempts = 0;
      var simulationStatus = false;
      const maxAttempts = 3;
      var seq = await wallet.sequence();
      while (attempts < maxAttempts) {
        try {
          tx = await wallet.createAndSignTx({
            msgs: msgs_acc,
            sequence: seq,
          });
          console.log(
            `Succeeded to createAndSignTx for position ids: ${position_ids.join(
              ","
            )}`
          );
          // Mark simulation as successful.
          simulationStatus = true;
          break;
        } catch (error) {
          if (error.response && error.response.data) {
            const errorPrefix = "account sequence mismatch, expected ";
            const errorSuffix = ": incorrect account sequence: invalid request";
            const errorLog = `Failed to createAndSignTx with error: ${
              error.response.data.message
            } for position ids: ${position_ids.join(",")}`;
            // Only retry sequence mismatch for now.
            if (error.response.data.message.includes(errorPrefix)) {
              console.log(
                `Simulation failed due to sequence mismatch for position ids: ${position_ids.join(
                  ","
                )}. Retrying it again.`
              );
              attempts++;
              // Update seq.
              const e = error.response.data.message;
              for (const strToken of e
                .substring(0, e.indexOf(errorSuffix))
                .substr(errorPrefix.length)
                .split(",")) {
                // The first element is the expected sequence number.
                seq = parseInt(strToken);
                console.log(`Parsed expected sequence: ${seq}.`);
                // Delay some time to avoid continuous spamming.
                await delay(1000);
                break;
              }
              console.log(errorLog);
              continue;
            }
            console.log(errorLog);
            break;
          } else {
            console.log(
              `Failed to createAndSignTx with ${error} for position ids: ${position_ids.join(
                ","
              )}`
            );
            break;
          }
        }
      }

      if (!simulationStatus) {
        console.log(
          `Simulation failed. Skipping position ids: ${position_ids.join(",")}.`
        );
        metrics[REBALANCE_CREATE_AND_SIGN_FAILURE]++;
        console.log("\n");
        // Clear states.
        num_included_positions = 0;
        msgs_acc = [];
        position_ids = [];
        position_addrs = [];
        continue;
      }

      try {
        const response = await connection.tx.broadcast(tx);
        if (isTxError(response)) {
          metrics[REBALANCE_FAILURE]++;
          console.log(
            `Rebalance broadcast failed for position ids: ${position_ids.join(
              ","
            )}. code: ${response.code}, codespace: ${
              response.codespace
            }, raw_log: ${response.raw_log}`
          );
        } else {
          metrics[REBALANCE_SUCCESS]++;
          console.log(
            `Successfully initiated rebalance for position ${position_ids.join(
              ","
            )}.`
          );
        }
      } catch (error) {
        metrics[REBALANCE_BROADCAST_FAILURE]++;
        if (error.response && error.response.data) {
          console.log(
            `Broadcast tx failed with error: ${
              error.response.data.message
            } for position ids: ${position_ids.join(",")}`
          );
        } else {
          console.log(
            `Broadcast tx failed with error: ${error} for position ids: ${position_ids.join(
              ","
            )}`
          );
        }
      } finally {
        // Clear states.
        num_included_positions = 0;
        msgs_acc = [];
        position_ids = [];
        position_addrs = [];
      }
    }
  }
}

async function getPositionInfos(
  next_id,
  qps,
  connection,
  position_manager_addr,
  hive_batch_size
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
            url: HIVE_ENDPOINT,
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
    console.log("Using Terra Hive for position info queries.");
  } catch (error) {
    console.log(
      `Failed to query Terra Hive with error: ${error}. Falling back to Terra node.`
    );
    metrics[BATCH_GET_POSITION_INFO_FAILURE]++;
    metrics[HIVE_QUERY_ERROR]++;

    all_position_infos = await pool({
      collection: generateRangeArray(0, next_id),
      maxConcurrency: qps,
      task: async (position_id) => {
        metrics[NUM_PROCESSED_POSITION]++;
        var position_info = undefined;
        try {
          position_info = await connection.wasm.contractQuery(
            position_manager_addr,
            {
              batch_get_position_info: {
                positions: [
                  {
                    position_id: position_id.toString(),
                    chain_id: TERRA_CHAIN_ID,
                  },
                ],
              },
            }
          );
        } catch (error) {
          console.log(
            `Failed to batch get for position id ${position_id} with error: ${error}`
          );
          metrics[BATCH_GET_POSITION_INFO_FAILURE]++;
          metrics[CONTRACT_QUERY_ERROR]++;
        }
        return position_info;
      },
    });
    console.log("Using Terra node for position info queries.");
  }
  return all_position_infos;
}

function handleRebalance(
  batch_position_info,
  asset_timestamps,
  asset_required_crs,
  delta_tolerance,
  balance_tolerance,
  time_tolerance,
  wallet
) {
  metrics[NUM_PROCESSED_POSITION]++;

  // We always query batch API with batch size of one.
  const current_position = batch_position_info.items[0];
  const position_info = current_position.info;
  const position_id = current_position.position.position_id;
  const position_addr = current_position.contract;

  if (position_info.detailed_info == null) {
    console.log("Position id ", position_id, " is closed.");
    return undefined;
  }

  const mAssetName = mAssetMap[position_info.mirror_asset_cw20_addr];

  // Determine whether we should trigger rebalance.
  const { result, logging, reason } = shouldRebalance(
    position_info,
    asset_timestamps,
    asset_required_crs,
    delta_tolerance,
    balance_tolerance,
    time_tolerance
  );

  console.log(
    `position id ${position_id} ${mAssetName} with address ${position_addr}`
  );
  process.stdout.write(logging);

  if (!result) {
    console.log(`Skipping rebalance for position ${position_id}.\n`);
    return undefined;
  }

  // Rebalance.
  metrics[SHOULD_REBALANCE_POSITION]++;
  const msg = new MsgExecuteContract(
    /*sender=*/ wallet.key.accAddress,
    /*contract=*/ position_addr,
    {
      controller: {
        rebalance_and_reinvest: {},
      },
    }
  );
  // Add new line for logging.
  console.log("\n");

  return {
    msg: msg,
    asset_name: mAssetName,
    position_id: position_id,
    position_addr: position_addr,
    reason: reason,
  };
}

function shouldRebalance(
  position_info,
  asset_timestamps,
  asset_required_crs,
  delta_tolerance,
  balance_tolerance,
  time_tolerance
) {
  var logging = "";
  const detailed_info = position_info.detailed_info;
  // Check market hours.
  var mirror_res = undefined;
  if (asset_timestamps[position_info.mirror_asset_cw20_addr]) {
    mirror_res =
      asset_timestamps[position_info.mirror_asset_cw20_addr].mirror_res;
  } else {
    logging += `Error: missing Mirror Oracle for ${position_info.mirror_asset_cw20_addr}.\n`;
    return { result: false, logging: logging, reason: "NA" };
  }

  const last_updated_base_sec = mirror_res.last_updated;

  // Do not rebalance if oracle price timestamp is too old.
  const current_time = new Date();
  if (current_time - last_updated_base_sec * 1e3 > time_tolerance * 1e3) {
    logging += `Oracle price too old. Current time: ${current_time.toString()}. Oracle time: ${new Date(
      last_updated_base_sec * 1e3
    ).toString()}. Time tolerance is ${time_tolerance} seconds.\n`;
    return { result: false, logging: logging, reason: "NA" };
  }

  // Check if the current target CR range satisfies the current required CR for the mAsset.
  if (
    new Big(asset_required_crs[position_info.mirror_asset_cw20_addr])
      .plus(CR_SAFETY_MARGIN)
      .gt(detailed_info.target_collateral_ratio_range.min)
  ) {
    logging += "Should rebalance due to: TCR.min needs to be raised.\n";
    return { result: true, logging: logging, reason: "CRR" };
  }

  // Check if current CR is within range.
  logging += `Current CR: ${parseFloat(
    detailed_info.collateral_ratio
  )}. Position CR range is [${
    detailed_info.target_collateral_ratio_range.min
  }, ${detailed_info.target_collateral_ratio_range.max}]\n`;

  if (
    parseFloat(detailed_info.collateral_ratio) <
    parseFloat(detailed_info.target_collateral_ratio_range.min)
  ) {
    logging += "Should rebalance due to: CR too small.\n";
    return { result: true, logging: logging, reason: "CRL" };
  }

  if (
    parseFloat(detailed_info.collateral_ratio) >
      parseFloat(detailed_info.target_collateral_ratio_range.max) &&
    (new Big(detailed_info.unclaimed_short_proceeds_uusd_amount).eq(0) ||
      new Big(detailed_info.unclaimed_short_proceeds_uusd_amount).eq(
        detailed_info.claimable_short_proceeds_uusd_amount
      ))
  ) {
    logging += "Should rebalance due to: CR too big.\n";
    return { result: true, logging: logging, reason: "CRG" };
  }

  // Check delta-neutrality.
  const short_amount = new Big(detailed_info.state.mirror_asset_short_amount);
  const long_amount = new Big(detailed_info.state.mirror_asset_long_amount);
  logging += `delta percentage: ${
    short_amount.minus(long_amount).abs() / long_amount
  }\n`;

  if (
    short_amount.minus(long_amount).abs().div(long_amount).gt(delta_tolerance)
  ) {
    logging += "Should rebalance due to: Violating delta-neutral constraint.\n";
    return { result: true, logging: logging, reason: "DL" };
  }

  // Check balance.
  const uusd_value = new Big(detailed_info.uusd_value);
  const uusd_balance = new Big(
    detailed_info.claimable_short_proceeds_uusd_amount
  )
    .plus(detailed_info.claimable_mir_reward_uusd_value)
    .plus(detailed_info.claimable_spec_reward_uusd_value)
    .plus(detailed_info.state.uusd_balance);
  logging += `balance percentage: ${uusd_balance.div(uusd_value).toString()}\n`;
  const has_locked_proceeds =
    !new Big(detailed_info.unclaimed_short_proceeds_uusd_amount).eq(0) &&
    new Big(detailed_info.claimable_short_proceeds_uusd_amount).eq(0);
  if (
    uusd_balance.div(uusd_value).gt(balance_tolerance) &&
    !has_locked_proceeds
  ) {
    logging += "Should rebalance due to: Balance too big.\n";
    return { result: true, logging: logging, reason: "BAL" };
  }

  logging +=
    "Delta is neutral, CR looks okay and balance is not enough or eligible for rebalance.\n";
  return { result: false, logging: logging, reason: "NA" };
}

async function getAssetTimestamp(connection, qps, mirror_oracle_addr) {
  var mAsset_timestamps = undefined;
  try {
    const hive_query = getMAssetQuoteQueries(
      mirror_oracle_addr,
      Object.keys(mAssetMap)
    );
    let hive_response = await axios({
      method: "post",
      url: HIVE_ENDPOINT,
      data: {
        query: hive_query,
      },
    });

    mAsset_timestamps = Object.entries(hive_response.data.data).map(
      (element) => {
        const [token_addr, res] = element;
        return {
          token_addr: token_addr,
          mirror_res: res.contractQuery,
          name: mAssetMap[token_addr],
        };
      }
    );
    console.log("Using Terra Hive for mAsset quote queries.");
  } catch (error) {
    console.log(
      `Failed to Hive for Mirror Oracle with error ${error}. Fallback to Terra node.`
    );
    metrics[MIRROR_ORACLE_QUERY_FAILURE]++;
    metrics[HIVE_QUERY_ERROR]++;

    mAsset_timestamps = await pool({
      collection: Object.entries(mAssetMap),
      maxConcurrency: qps,
      task: async (entry) => {
        var mirror_res = undefined;
        const [token_addr, name] = entry;
        const mirror_max_retry = 3;
        var retry_count = 0;
        while (retry_count < mirror_max_retry) {
          try {
            mirror_res = await connection.wasm.contractQuery(
              mirror_oracle_addr,
              {
                price: {
                  asset_token: token_addr,
                },
              }
            );
            break;
          } catch (error) {
            console.log(`Failed to query Mirror Orcale with error: ${error}\n`);
            metrics[MIRROR_ORACLE_QUERY_FAILURE]++;
            metrics[CONTRACT_QUERY_ERROR]++;
            // Delay briefly to avoid throttling.
            delay(1000);
            retry_count++;
          }
        }
        return { token_addr: token_addr, mirror_res: mirror_res, name: name };
      },
    });
    console.log("Using Terra node for mAsset quote queries.");
  }
  return mAsset_timestamps;
}

async function getAssetRequiredCR(connection, qps, mirror_mint_addr) {
  var mAsset_required_CRs = undefined;
  try {
    const hive_query = getMAssetRequiredCRQueries(
      mirror_mint_addr,
      Object.keys(mAssetMap)
    );
    let hive_response = await axios({
      method: "post",
      url: HIVE_ENDPOINT,
      data: {
        query: hive_query,
      },
    });

    mAsset_required_CRs = Object.entries(hive_response.data.data).map(
      (element) => {
        const [token_addr, res] = element;
        return {
          token_addr: token_addr,
          required_cr: res.contractQuery.min_collateral_ratio,
        };
      }
    );
    console.log("Using Terra Hive for mAsset required CR queries.");
  } catch (error) {
    console.log(
      `Failed to Hive for Mirror Mint with error ${error}. Fallback to Terra node.`
    );
    metrics[MIRROR_MINT_QUERY_FAILURE]++;
    metrics[HIVE_QUERY_ERROR]++;

    mAsset_required_CRs = await pool({
      collection: Object.entries(mAssetMap),
      maxConcurrency: qps,
      task: async (entry) => {
        var mirror_res = undefined;
        const [token_addr, name] = entry;
        const mirror_max_retry = 3;
        var retry_count = 0;
        while (retry_count < mirror_max_retry) {
          try {
            mirror_res = await connection.wasm.contractQuery(mirror_mint_addr, {
              asset_config: {
                asset_token: token_addr,
              },
            });
            break;
          } catch (error) {
            console.log(`Failed to query Mirror Mint with error: ${error}\n`);
            metrics[MIRROR_MINT_QUERY_FAILURE]++;
            metrics[CONTRACT_QUERY_ERROR]++;
            // Delay briefly to avoid throttling.
            delay(1000);
            retry_count++;
          }
        }
        return {
          token_addr: token_addr,
          required_cr: mirror_res.min_collateral_ratio,
        };
      },
    });
    console.log("Using Terra node for mAsset required CR queries.");
  }
  return mAsset_required_CRs;
}

// Start.
try {
  await run_pipeline();
} catch (error) {
  console.log(
    `[Unknown Failure] Some part of the operations failed with error: ${error}`
  );
} finally {
  if (process.env.NODE_ENV === "production") {
    await publishMetrics(metrics);
  } else {
    console.log("Skip publishing metrics for dev env. See metrics below.");
    console.dir(metrics, { depth: null });
  }
  console.log("Rebalance script execution completed.");
}

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

const client = new CloudWatchClient({ region: "us-west-2" });
var metrics = {};
const CONTRACT_QUERY_ERROR = "CONTRACT_QUERY_ERROR";
const CONTROLLER_START = "CONTROLLER_START";
const MIRROR_ORACLE_QUERY_FAILURE = "MIRROR_ORACLE_QUERY_FAILURE";
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
    default: 10,
    type: "int",
  });
  parser.add_argument("-bs", "--batch_size", {
    help: "Number of positions to include per tx.",
    required: false,
    default: 5,
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
  } = parser.parse_args();

  var terra_manager = "";
  var connection = undefined;
  var mirror_oracle_addr = "";

  if (network == "testnet") {
    terra_manager = TERRA_MANAGER_TESTNET;
    connection = testnetTerra;
    mirror_oracle_addr = MIRROR_ORACLE_TESTNET;
  } else if (network == "mainnet") {
    terra_manager = TERRA_MANAGER_MAINNET;
    connection = mainnetTerraController;
    mirror_oracle_addr = MIRROR_ORACLE_MAINNET;
  } else {
    console.log(`Invalid network argument ${parser.parse_args().network}`);
    return;
  }

  // Initialize metric counters.
  metrics[CONTRACT_QUERY_ERROR] = 0;
  metrics[CONTROLLER_START] = 0;
  metrics[MIRROR_ORACLE_QUERY_FAILURE] = 0;
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

  // Fetch position infos.
  const next_id = parseInt(next_position_res.next_position_id);
  var position_infos_promise = getPositionInfos(
    next_id,
    qps,
    connection,
    position_manager_addr
  );

  var [asset_timestamps, position_infos] = await Promise.all([
    asset_timestamps_promise,
    position_infos_promise,
  ]);

  asset_timestamps = asset_timestamps.reduce((acc, cur) => {
    if (cur && cur.token_addr) {
      acc[cur.token_addr] = cur;
    }
    return acc;
  }, {});

  // An undefined position info indicates fetching errors.
  position_infos = position_infos.filter((ele) => ele != undefined);

  // Make rebalance decisions.
  var rebalance_infos = [];
  position_infos = position_infos.map((batch_position_info) => {
    const rebalance_info = handleRebalance(
      position_manager_addr,
      batch_position_info,
      asset_timestamps,
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
  var memos = [];
  var position_ids = [];
  for (const [index, rebalance_info] of rebalance_infos.entries()) {
    const { msgs, asset_name, position_id, reason } = rebalance_info;
    num_included_positions++;
    msgs_acc.push(...msgs);
    memos.push(`p:${position_id},a:${asset_name},r:${reason}`);
    position_ids.push(position_id);
    // Send out tx if:
    //   1. We have accumulated enough positions.
    //   2. Or, we are at the last batch.
    if (
      num_included_positions == batch_size ||
      index == promises_with_result.length - 1
    ) {
      var tx = undefined;
      try {
        tx = await wallet.createAndSignTx({
          msgs: msgs_acc,
          memo: memos.join(";"),
          sequence: await wallet.sequence(),
        });
      } catch (error) {
        console.log(
          `Failed to createAndSignTx with error ${error} for position ids: ${position_ids.join(
            ","
          )}`
        );
        metrics[REBALANCE_CREATE_AND_SIGN_FAILURE]++;
        console.log("\n");
        num_included_positions = 0;
        memos = [];
        msgs_acc = [];
        position_ids = [];
        continue;
      }

      console.log(
        `Succeeded to createAndSignTx for position ids: ${position_ids.join(
          ","
        )}`
      );

      try {
        const response = await connection.tx.broadcast(tx);
        if (isTxError(response)) {
          metrics[REBALANCE_FAILURE]++;
          console.log(
            `Rebalance broadcast failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
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
        console.log(
          `Broadcast tx failed with error ${error} for position ids: ${position_ids.join(
            ","
          )}`
        );
      } finally {
        // Clear states.
        num_included_positions = 0;
        memos = [];
        msgs_acc = [];
        position_ids = [];
      }
    }
  }
}

// Generates array [begin, begin + 1, begin + 2, ..., end - 1].
function generateRangeArray(begin, end) {
  if (begin >= end) return [];
  return [...Array(end - begin).keys()].map((num) => num + begin);
}

async function getPositionInfos(
  next_id,
  qps,
  connection,
  position_manager_addr
) {
  return await pool({
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
}

function handleRebalance(
  position_manager_addr,
  batch_position_info,
  asset_timestamps,
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

  var msgs = [];
  // Always migrate contract before rebalance and reinvest.
  msgs.push(
    new MsgExecuteContract(
      /*sender=*/ wallet.key.accAddress,
      /*contract=*/ position_manager_addr,
      {
        migrate_position_contracts: {
          positions: [
            {
              chain_id: TERRA_CHAIN_ID,
              position_id: position_id.toString(),
            },
          ],
          position_contracts: [],
        },
      }
    )
  );

  const mAssetName = mAssetMap[position_info.mirror_asset_cw20_addr];

  // Determine whether we should trigger rebalance.
  const { result, logging, reason } = shouldRebalance(
    position_info,
    asset_timestamps,
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
  msgs.push(
    new MsgExecuteContract(
      /*sender=*/ wallet.key.accAddress,
      /*contract=*/ position_addr,
      {
        controller: {
          rebalance_and_reinvest: {},
        },
      }
    )
  );
  // Add new line for logging.
  console.log("\n");

  return {
    msgs: msgs,
    asset_name: mAssetName,
    position_id: position_id,
    reason: reason,
  };
}

function shouldRebalance(
  position_info,
  asset_timestamps,
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

async function getAssetTimestamp(connection, qps, mirror_orcale_addr) {
  return await pool({
    collection: Object.entries(mAssetMap),
    maxConcurrency: qps,
    task: async (entry) => {
      var mirror_res = undefined;
      const [token_addr, name] = entry;
      const mirror_max_retry = 3;
      var retry_count = 0;
      while (retry_count < mirror_max_retry) {
        try {
          mirror_res = await connection.wasm.contractQuery(mirror_orcale_addr, {
            price: {
              asset_token: token_addr,
            },
          });
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
}

// Start.
try {
  await run_pipeline();
} catch (error) {
  console.log(`Some part of the operations failed with error: ${error}`);
} finally {
  await publishMetrics(metrics);
  console.log("Rebalance script execution completed.");
}

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

var sequence = -1;
async function initializeSequence(wallet) {
  const account_and_sequence = await wallet.accountNumberAndSequence();
  sequence = account_and_sequence.sequence;
}

function getAndIncrementSequence() {
  return sequence++;
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
const GET_POSITION_CONTRACT_FAILURE = "GET_POSITION_CONTRACT_FAILURE";
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
  metrics[GET_POSITION_CONTRACT_FAILURE] = 0;
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
  initializeSequence(wallet);

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

  var position_manager_admin_config_res = undefined;
  try {
    position_manager_admin_config_res = await connection.wasm.contractQuery(
      position_manager_addr,
      {
        get_admin_config: {},
      }
    );
  } catch (error) {
    console.log(
      `Failed to get position manager admin config with error: ${error}`
    );
    metrics[GET_POSITION_MANAGER_ADMIN_CONFIG_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }

  const delta_neutral_position_code_id =
    position_manager_admin_config_res.delta_neutral_position_code_id;

  var promises = [];
  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Trottle request.
    if (i % qps == 0) {
      await delay(1000);
    }
    let promise = maybeExecuteRebalance(
      connection,
      position_manager_addr,
      delta_neutral_position_code_id,
      i,
      mirror_oracle_addr,
      delta_tolerance,
      balance_tolerance,
      time_tolerance,
      wallet
    );
    promise.catch((err) => console.log("Caught unexpected error: ", err));
    promises.push(promise);
  }

  console.log(`Waiting for ${promises.length} requests to complete.`);
  const resolved_promises = await Promise.allSettled(promises);

  // Extract out promises with msgs to execute.
  var promises_with_result = [];
  for (const promise of resolved_promises) {
    if (promise.status == "fulfilled" && promise.value != undefined) {
      promises_with_result.push(promise);
    }
  }
  console.log(
    `Total number of positions to rebalance: ${promises_with_result.length}`
  );

  var num_included_positions = 0;
  var msgs_acc = [];
  var memos = [];
  var position_ids = [];
  for (const [index, resolved_promise] of promises_with_result.entries()) {
    const { msgs, asset_name, position_id, reason } = resolved_promise.value;
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

async function maybeExecuteRebalance(
  connection,
  position_manager_addr,
  delta_neutral_position_code_id,
  position_id,
  mirror_oracle_addr,
  delta_tolerance,
  balance_tolerance,
  time_tolerance,
  wallet
) {
  metrics[NUM_PROCESSED_POSITION]++;

  // Query position metadata.
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
    console.log(`Failed to get position contract address with error: ${error}`);
    metrics[GET_POSITION_CONTRACT_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }

  // Get position info.
  var position_info = undefined;
  try {
    position_info = await connection.wasm.contractQuery(position_addr, {
      get_position_info: {},
    });
  } catch (error) {
    console.log(`Failed to get position info with error: ${error}`);
    metrics[GET_POSITION_INFO_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }

  if (position_info.detailed_info == null) {
    console.log("Position id ", position_id, " is closed.");
    return;
  }

  // Obtain current code id for the position contract.
  var position_contract_info = undefined;
  try {
    position_contract_info = await connection.wasm.contractInfo(position_addr);
  } catch (error) {
    console.log(`Failed to query position contract info with error: ${error}`);
    metrics[QUERY_POSITION_CONTRACT_INFO_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return;
  }
  const current_code_id = position_contract_info.code_id;

  var msgs = [];
  // Migrate contract as needed.
  if (current_code_id !== delta_neutral_position_code_id) {
    console.log(
      `Potentially will migrate position ${position_id} from ${current_code_id} to ${delta_neutral_position_code_id}.`
    );
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
  }

  const mAssetName = mAssetMap[position_info.mirror_asset_cw20_addr];

  // Determine whether we should trigger rebalance.
  const { result, logging, reason } = await shouldRebalance(
    connection,
    position_info,
    mirror_oracle_addr,
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
    return;
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

async function shouldRebalance(
  connection,
  position_info,
  mirror_orcale_addr,
  delta_tolerance,
  balance_tolerance,
  time_tolerance
) {
  var logging = "";
  const detailed_info = position_info.detailed_info;
  // Check market hours.
  var mirror_res = undefined;
  try {
    mirror_res = await connection.wasm.contractQuery(mirror_orcale_addr, {
      price: {
        quote_asset: "uusd",
        base_asset: position_info.mirror_asset_cw20_addr,
      },
    });
  } catch (error) {
    logging += `Failed to query Mirror Orcale with error: ${error}\n`;
    metrics[MIRROR_ORACLE_QUERY_FAILURE]++;
    metrics[CONTRACT_QUERY_ERROR]++;
    return { result: false, logging: logging, reason: "NA" };
  }

  const last_updated_base_sec = mirror_res.last_updated_base;

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

// Start.
try {
  await run_pipeline();
} catch (error) {
  console.log(`Some part of the operations failed with error: ${error}`);
} finally {
  await publishMetrics(metrics);
  console.log("Rebalance script execution completed.");
}

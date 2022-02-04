import big from "big.js";
const { Big } = big;
import { ArgumentParser } from "argparse";
import {
  DELTA_NEUTRAL_STRATEGY_ID,
  mainnetTerra,
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

const delay = ms => new Promise(res => setTimeout(res, ms));

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

  // Parse and validate.
  const { network, delta_tolerance, balance_tolerance, time_tolerance, qps } =
    parser.parse_args();

  var terra_manager = "";
  var connection = undefined;
  var mirror_oracle_addr = "";

  if (network == "testnet") {
    terra_manager = TERRA_MANAGER_TESTNET;
    connection = testnetTerra;
    mirror_oracle_addr = MIRROR_ORACLE_TESTNET;
  } else if (network == "mainnet") {
    terra_manager = TERRA_MANAGER_MAINNET;
    connection = mainnetTerra;
    mirror_oracle_addr = MIRROR_ORACLE_MAINNET;
  } else {
    console.log(`Invalid network argument ${parser.parse_args().network}`);
    return;
  }

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
  const next_position_res = await connection.wasm.contractQuery(terra_manager, {
    get_next_position_id: {},
  });
  console.log("next position id: ", next_position_res.next_position_id);

  // Get delta neutral position manager.
  const delta_neutral_pos_mgr_res = await connection.wasm.contractQuery(
    terra_manager,
    {
      get_strategy_metadata: {
        strategy_id: DELTA_NEUTRAL_STRATEGY_ID,
      },
    }
  );
  const position_manager_addr = delta_neutral_pos_mgr_res.manager_addr;

  var promises = [];
  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Trottle request.
    if (i % qps == 0) {
      await delay(1000);
    }

    promises.push(
      maybeExecuteRebalance(
        connection,
        position_manager_addr,
        i,
        mirror_oracle_addr,
        delta_tolerance,
        balance_tolerance,
        time_tolerance,
        wallet
      )
    );
  }
  console.log(`Waiting for ${promises.length} requests to complete.`);
  await Promise.allSettled(promises);
}

async function maybeExecuteRebalance(
  connection,
  position_manager_addr,
  position_id,
  mirror_oracle_addr,
  delta_tolerance,
  balance_tolerance,
  time_tolerance,
  wallet
) {
  // Query position metadata.
  const position_addr = await connection.wasm.contractQuery(
    position_manager_addr,
    {
      get_position_contract_addr: {
        position: {
          chain_id: TERRA_CHAIN_ID,
          position_id: position_id.toString(),
        },
      },
    }
  );

  // Get position info.
  const position_info = await connection.wasm.contractQuery(position_addr, {
    get_position_info: {},
  });

  if (position_info.detailed_info == null) {
    console.log("Position id ", position_id, " is closed.");
    return;
  }

  // Determine whether we should trigger rebalance.
  const { result, logging } = await shouldRebalance(
    connection,
    position_info,
    mirror_oracle_addr,
    delta_tolerance,
    balance_tolerance,
    time_tolerance
  );

  console.log(`position id ${position_id} with address ${position_addr}`);
  process.stdout.write(logging);

  if (!result) {
    console.log(`Skipping rebalance for position ${position_id}.\n`);
    return;
  }

  // Rebalance.
  console.log(`Initiating rebalance for position ${position_id}.`);
  try {
    const tx = await wallet.createAndSignTx({
      msgs: [
        new MsgExecuteContract(
          /*sender=*/ wallet.key.accAddress,
          /*contract=*/ position_addr,
          {
            controller: {
              rebalance_and_reinvest: {},
            },
          }
        ),
      ],
      memo: `Rebalance position id ${position_id}.`,
      sequence: getAndIncrementSequence(),
    });
    const response = await connection.tx.broadcast(tx);
    if (isTxError(response)) {
      console.log(
        `Rebalance failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
      );
    } else {
      console.log(
        `Successfully initiated rebalance for position ${position_id}.`
      );
    }
  } catch (error) {
    console.log(
      `create or broadcast tx failed with error ${error} for position id: ${position_id}`
    );
  } finally {
    console.log("\n");
  }
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
  const mirror_res = await connection.wasm.contractQuery(mirror_orcale_addr, {
    price: {
      quote_asset: "uusd",
      base_asset: position_info.mirror_asset_cw20_addr,
    },
  });

  const last_updated_base_sec = mirror_res.last_updated_base;

  // Do not rebalance if oracle price timestamp is too old.
  const current_time = new Date();
  if (current_time - last_updated_base_sec * 1e3 > time_tolerance * 1e3) {
    logging += `Oracle price too old. Current time: ${current_time.toString()}. Oracle time: ${new Date(
      last_updated_base_sec * 1e3
    ).toString()}. Time tolerance is ${time_tolerance} seconds.\n`;
    return { result: false, logging: logging };
  }

  // Check if current CR is within range.
  logging += `Current CR: ${parseFloat(
    detailed_info.collateral_ratio
  )}. Position CR range is [${
    detailed_info.target_collateral_ratio_range.min
  }, ${detailed_info.target_collateral_ratio_range.max}]\n`;

  if (
    parseFloat(detailed_info.collateral_ratio) <
      parseFloat(detailed_info.target_collateral_ratio_range.min) ||
    parseFloat(detailed_info.collateral_ratio) >
      parseFloat(detailed_info.target_collateral_ratio_range.max)
  ) {
    logging += "Should rebalance due to: CR out of range.\n";
    return { result: true, logging: logging };
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
    return { result: true, logging: logging };
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
  if (
    uusd_balance.div(uusd_value).gt(balance_tolerance) &&
    new Big(detailed_info.unclaimed_short_proceeds_uusd_amount).eq(0)
  ) {
    logging += "Should rebalance due to: Balance too big.\n";
    return { result: true, logging: logging };
  }

  logging +=
    "Delta is neutral, CR looks okay and balance is not enough or eligible for rebalance.\n";
  return { result: false, logging: logging };
}

// Start.
await run_pipeline();

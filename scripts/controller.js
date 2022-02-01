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

  // Parse and validate.
  const { network, delta_tolerance, balance_tolerance } = parser.parse_args();

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

  for (var i = 0; i < parseInt(next_position_res.next_position_id); i++) {
    // Query position metadata.
    const position_addr = await connection.wasm.contractQuery(
      position_manager_addr,
      {
        get_position_contract_addr: {
          position: {
            chain_id: TERRA_CHAIN_ID,
            position_id: i.toString(),
          },
        },
      }
    );
    console.log("position addr: ", position_addr);

    // Get position info.
    const position_info = await connection.wasm.contractQuery(position_addr, {
      get_position_info: {},
    });

    if (position_info.detailed_info == null) {
      console.log("Position id ", i, " is closed.");
      continue;
    }

    // Determine whether we should trigger rebalance.
    if (
      !(await shouldRebalance(
        connection,
        position_info,
        mirror_oracle_addr,
        delta_tolerance,
        balance_tolerance
      ))
    ) {
      console.log(`Skipping rebalance for position ${i}.`);
      continue;
    }
    // Rebalance.
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
      memo: `Rebalance position id ${i}.`,
      sequence: getAndIncrementSequence(),
    });

    const response = await connection.tx.broadcast(tx);
    if (isTxError(response)) {
      console.log(
        `Rebalance failed. code: ${response.code}, codespace: ${response.codespace}, raw_log: ${response.raw_log}`
      );
    }
  }
}

async function shouldRebalance(
  connection,
  position_info,
  mirror_orcale_addr,
  delta_tolerance,
  balance_tolerance
) {
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
  if (new Date() - last_updated_base_sec * 1e3 > 60 * 1e3) {
    console.log("Oracle price too old.");
    return false;
  }

  // Check if current CR is within range.
  console.log(`CR: ${parseFloat(detailed_info.collateral_ratio)}`);
  if (
    parseFloat(detailed_info.collateral_ratio) <
      parseFloat(detailed_info.target_collateral_ratio_range.min) ||
    parseFloat(detailed_info.collateral_ratio) >
      parseFloat(detailed_info.target_collateral_ratio_range.max)
  ) {
    console.log("CR out of range.");
    return true;
  }

  // Check delta-neutrality.
  const short_amount = new Big(detailed_info.state.mirror_asset_short_amount);
  const long_amount = new Big(detailed_info.state.mirror_asset_long_amount);
  console.log(
    `delta percentage: ${short_amount.minus(long_amount).abs() / long_amount}`
  );

  if (
    short_amount.minus(long_amount).abs().div(long_amount).gte(delta_tolerance)
  ) {
    console.log("Violating delta-neutral constraint.");
    return true;
  }

  // Check balance.
  const uusd_value = new Big(detailed_info.uusd_value);
  const uusd_balance = new Big(
    detailed_info.claimable_short_proceeds_uusd_amount
  )
    .plus(detailed_info.claimable_mir_reward_uusd_value)
    .plus(detailed_info.claimable_spec_reward_uusd_value)
    .plus(detailed_info.state.uusd_balance);
  console.log(`balance percentage: ${uusd_balance.div(uusd_value).toString()}`);
  if (uusd_balance.div(uusd_value).gte(balance_tolerance)) {
    console.log("Reinvest balance.");
    return true;
  }

  return false;
}

// Start.
await run_pipeline();

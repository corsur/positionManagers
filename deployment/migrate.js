import {
  Coin,
  LCDClient,
  MnemonicKey,
  isTxError,
  MsgMigrateContract,
  MsgExecuteContract,
  MsgStoreCode,
} from "@terra-money/terra.js";
import * as fs from "fs";

const gasAdjustment = 1.2;
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
  gasPrices: {
    uusd: 0.1508,
  },
  gasAdjustment: gasAdjustment,
});

const mainnet = new LCDClient({
  URL: "https://lcd.terra.dev",
  chainID: "columbus-5",
  gasPrices: {
    uusd: 0.15,
  },
  gasAdjustment: gasAdjustment,
});

const test_wallet = testnet.wallet(
  new MnemonicKey({
    mnemonic:
      "plastic evidence song forest fence daughter nuclear road angry knife wing punch sustain suit resist vapor thrive diesel collect easily minimum thing cost phone",
  })
);

async function migrate_contract(contract_addr, new_code_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgMigrateContract(
        test_wallet.key.accAddress,
        contract_addr,
        new_code_id,
        {}),
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

async function store_new_position_code(position_manager) {
  const new_code_id = await store_code("../artifacts/delta_neutral_position-aarch64.wasm");
  console.log("Stored position contract code id: ", new_code_id);
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        test_wallet.key.accAddress,
        position_manager,
        {
          update_admin_config: {
            delta_neutral_position_code_id: new_code_id
          }
        },
        []),
    ],
    sequence: getAndIncrementSequence(),
  });
  const txResult = await testnet.tx.broadcast(tx);
  if (isTxError(txResult)) {
    throw new Error(
      `Migrate code failed. code: ${txResult.code}, codespace: ${txResult.codespace}, raw_log: ${txResult.raw_log}`
    );
  }
  console.log("Set new position contract code id: ", new_code_id);
}

async function open_delta_neutral_position(terra_manager_addr, ust_amount) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          "create_position": {
            "data": "ewogICAgInRhcmdldF9taW5fY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjMiLAogICAgInRhcmdldF9tYXhfY29sbGF0ZXJhbF9yYXRpbyI6ICIyLjciLAogICAgIm1pcnJvcl9hc3NldF9jdzIwX2FkZHIiOiAidGVycmExeXM0ZHd3emFlbmpnMmd5MDJtc2xtYzk2ZjI2N3h2cHNqYXQ3Z3giCn0=",
            "assets": [
              {
                "info": {
                  "native_token": {
                    "denom": "uusd"
                  }
                },
                "amount": (ust_amount * 1e6).toString()
              }
            ],
            "strategy": {
              "chain_id": 3,
              "strategy_id": "0"
            }
          }
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
      `open_delta_neutral_position failed. code: ${response.code}, codespace: ${response.codespace}`//, raw_log: ${response.raw_log}`
    );
  }
  console.log("Opened delta-neutral position with ust amount: ", ust_amount.toString());
}

async function close_position(terra_manager_addr, position_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ terra_manager_addr,
        {
          "execute_strategy": {
            "action": {
              "close_position": {
                 "recipient": test_wallet.key.accAddress
              }
            },
            "assets": [],
            "position": {
              "chain_id": 3,
              "position_id": position_id.toString()
            }
          }
        },
        []
      ),
    ],
    memo: "Close position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `close position failed. code: ${response.code}, codespace: ${response.codespace}`//, raw_log: ${response.raw_log}`
    );
  }
  console.log("Closed position id: ", position_id.toString());
}

async function rebalance_position(position_contract) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ position_contract,
        {
          "controller": {
            "rebalance_and_reinvest": {}
          }
        },
        []
      ),
    ],
    memo: "Rebalance position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `rebalance failed. code: ${response.code}, codespace: ${response.codespace}`//, raw_log: ${response.raw_log}`
    );
  }
  console.log("Rebalanced position: ", position_contract);
}

async function migrate_existing_position(position_manager_contract, position_id) {
  const tx = await test_wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(
        /*sender=*/ test_wallet.key.accAddress,
        /*contract=*/ position_manager_contract,
        {
          migrate_position_contracts: {
            positions: [
              {
                chain_id: 3,
                position_id: position_id.toString()
              }
            ],
            position_contracts: []
          }
        },
        []
      ),
    ],
    memo: "Migrate existing position",
    sequence: getAndIncrementSequence(),
  });

  const response = await testnet.tx.broadcast(tx);
  if (isTxError(response)) {
    throw new Error(
      `rebalance failed. code: ${response.code}, codespace: ${response.codespace}`//, raw_log: ${response.raw_log}`
    );
  }
  console.log("Migrated position: ", position_id);
}

await initializeSequence(test_wallet);
const position_manager = "terra13u309m5zk7q975vq0p2dw0yg37cmxsq8ar0jhg";
await store_new_position_code(position_manager);

const terra_manager_addr = "terra1ettwsfevaz65sqf269m9txs8mv923zas44aaj0";
await open_delta_neutral_position(terra_manager_addr, 3000000);
// await open_delta_neutral_position(terra_manager_addr, 1000);
// await open_delta_neutral_position(terra_manager_addr, 10000);
// await open_delta_neutral_position(terra_manager_addr, 12000000);

// await close_position(terra_manager_addr, 1);

// await migrate_existing_position(position_manager, 2);
// await close_position(terra_manager_addr, 1);
// await rebalance_position("terra1jjr5a4xw26s6enqzksyljy3fae3ppwvqqt75ez");

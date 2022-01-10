import {
  LCDClient,
  MnemonicKey,
  MsgExecuteContract,
  MsgInstantiateContract,
  MsgStoreCode,
  isTxError,
} from "@terra-money/terra.js";
import { DynamoDBClient, PutItemCommand} from "@aws-sdk/client-dynamodb";


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
  const terra_manager = "terra1pvq5zdh4frjh773nfhk2shqvv5jlm450v8a9yh";
  const delta_neutral_strategy_id = "0";
  const terra_chain_id = 3;

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
    const uusd_value = position_info.detailed_info.uusd_value;
    await write_to_dynamodb(i, new Date().getTime(), position_metadata_res.strategy_location.terra_chain, uusd_value);
  }
}

await run_pipeline();

async function write_to_dynamodb(position_id, timestamp_sec, chain_id, uusd_value) {
  const input = {
    TableName: "position_ticks",
    Item: {
      position_id: { N: position_id.toString() },
      timestamp_sec: { N: timestamp_sec.toString() },
      chain_id: {N: chain_id.toString()},
      uusd_value: {N: uusd_value.toString()},
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

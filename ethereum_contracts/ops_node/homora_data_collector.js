import { ethers, BigNumber } from "ethers";
import { ArgumentParser } from "argparse";
import {
  BatchWriteItemCommand,
  DynamoDBClient,
} from "@aws-sdk/client-dynamodb";

const parser = new ArgumentParser({
  description: "Homora data collector",
});

parser.add_argument("-n", "--network", {
  help: "URL to node",
  required: true,
  type: "str",
});

parser.add_argument("-p", "--precision", {
  help: "Number of decimal points",
  required: false,
  default: 2,
  type: "int",
});

parser.add_argument("-r", "--region", {
  help: "AWS region to use",
  required: false,
  default: "us-west-2",
  type: "str",
});

const VAULT_LIB_ABI = [
  "function getETHPx(address,address) view returns (uint256)",
];
const HOMORA_VAULT_ABI = [
  "function vaultState() view returns (uint256, uint256)",
  "function contractInfo() view returns (address, address, address, address, address)",
  "function getEquityETHValue() view returns (uint256)",
];
const USDC_TOKEN_ADDRESS = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";

const { network, region, precision } = parser.parse_args();
const provider = new ethers.providers.JsonRpcProvider(network);
const vaultLib = new ethers.Contract(
  "0x1fa02b2d6a771842690194cf62d91bdd92bfe28d",
  VAULT_LIB_ABI,
  provider
);

var vault_addrs = [
  { addr: "0x04C89607413713Ec9775E14b954286519d836FEf", id: 0, ticker: "AVAX" },
];
// List of vault contracts.
var vaults = [];

const dynamoClient = new DynamoDBClient({ region });

function createVaults() {
  for (const { addr, id, ticker } of vault_addrs) {
    console.log(`Creating homora vault for ${addr}`);
    const homoraVault = new ethers.Contract(addr, HOMORA_VAULT_ABI, provider);
    vaults.push({ vault: homoraVault, id: id, ticker: ticker });
  }
}

async function processData(vaultContract) {
  const oracleAddr = (await vaultContract.contractInfo())[2];
  console.log(`Using oracle address ${oracleAddr}`);

  // Compute prices for USD and ETH.
  const usdPriceETH = (await vaultLib.getETHPx(oracleAddr, USDC_TOKEN_ADDRESS))
    .mul(1e6)
    .div(BigNumber.from(2).pow(112));
  const ethPriceUSD =
    BigNumber.from("10")
      .pow(18)
      .mul(10 ** precision)
      .div(usdPriceETH)
      .toNumber() /
    10 ** precision;
  console.log(`USD price per AVAX ${usdPriceETH.toString()}`);
  console.log(`AVAX price per USD ${ethPriceUSD}`);

  // Calculate equity value denom in USD.
  const equityETHValue = await vaultContract.getEquityETHValue();
  const equityValueUSD = equityETHValue.div(usdPriceETH).mul(1e6);
  console.log(`Current vault TVL is ${equityETHValue} AVAX`);
  console.log(`Current vault TVL is ${equityValueUSD} USDC`);

  const totalShare = (await vaultContract.vaultState())[0];
  console.log(`Total share ${totalShare}`);
  // Scaled to 1e18.
  const usdPerShare = equityETHValue
    .div(totalShare)
    .mul(BigNumber.from(10).pow(18))
    .div(usdPriceETH);
  console.log(`usd per share: ${usdPerShare}`);
  return { ethPriceUSD, usdPerShare };
}

async function main() {
  console.log(`Query against URL: ${network}`);
  // await createPosition();
  createVaults();

  // Process data for each vault.
  var sentTokenPrice = false;
  for (const { vault, id, ticker } of vaults) {
    const { ethPriceUSD, usdPerShare } = await processData(vault);
    const timestampSec = parseInt(new Date().getTime() / 1e3);
    // Write usd per share data to DynamoDB.
    const shareItem = [
      {
        PutRequest: {
          Item: {
            vault_id: {
              N: id.toString(),
            },
            timestamp_sec: {
              N: timestampSec.toString(),
            },
            price: {
              N: usdPerShare.toString(),
            },
          },
        },
      },
    ];

    const tokenPriceItem = [
      {
        PutRequest: {
          Item: {
            ticker: {
              S: ticker,
            },
            timestamp_sec: {
              N: timestampSec.toString(),
            },
            price: {
              N: ethPriceUSD.toString(),
            },
          },
        },
      },
    ];

    // TODO(Gao): move token price update to a separate script in the future.
    // Use `sentTokenPrice` to avoid updating token price for every vault.
    var rawData = {
      RequestItems: {
        homora_performance: shareItem,
      },
    };
    if (!sentTokenPrice) {
      rawData.RequestItems["asset_prices"] = tokenPriceItem;
    }
    console.log(`Raw data: ${rawData}`);
    try {
      dynamoClient.send(new BatchWriteItemCommand(rawData));
    } catch (error) {
      console.log(`Failed to write with error ${error}`);
    }
  }
}

await main();

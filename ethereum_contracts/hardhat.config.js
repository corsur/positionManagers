require("@nomiclabs/hardhat-waffle");
require("@openzeppelin/hardhat-upgrades");
require("hardhat-gas-reporter");
require("hardhat-abi-exporter");

const {
  ETH_PRV_KEY_1,
  INFURA_URL_RINKERBY,
  INFURA_URL_ROPSTEN,
  ALCHEMY_URL_MAINNET,
  AVAX_MAINNET_FORK,
} = require("./constants");

// This is a sample Hardhat task. To learn how to create your own go to
// https://hardhat.org/guides/create-task.html
task("accounts", "Prints the list of accounts", async (taskArgs, hre) => {
  const accounts = await hre.ethers.getSigners();

  for (const account of accounts) {
    console.log(account.address);
  }
});

// You need to export an object to set up your config
// Go to https://hardhat.org/config/ to learn more

/**
 * @type import('hardhat/config').HardhatUserConfig
 */
module.exports = {
  solidity: {
    version: "0.8.13",
    settings: {
      optimizer: {
        enabled: true,
        // See https://docs.soliditylang.org/en/v0.8.12/internals/optimizer.html#optimizer-parameter-runs.
        runs: 2 ** 32 - 1,
      },
    },
  },
  networks: {
    hardhat: {
      // for avax
      forking: {
        url: AVAX_MAINNET_FORK,
        blockNumber: 16424563,
      },
      // for ethereum
      // forking: {
      //   url: ALCHEMY_URL_MAINNET,
      //   blockNumber: 15004700, // previously 14957690, 14247160
      // }
    },
    ropsten: {
      url: INFURA_URL_ROPSTEN,
      accounts: [ETH_PRV_KEY_1],
      timeout: 60000,
    },
    rinkeby: {
      url: INFURA_URL_RINKERBY,
      accounts: [ETH_PRV_KEY_1],
    },
  },
  paths: {
    sources: "./contracts",
    tests: "./test",
    cache: "./cache",
    artifacts: "./artifacts",
  },
  mocha: {
    timeout: 10000000,
  },
  abiExporter: {
    path: "./data/abi",
    runOnCompile: true,
    clear: true,
    flat: true,
    spacing: 2,
    pretty: true,
  },
};

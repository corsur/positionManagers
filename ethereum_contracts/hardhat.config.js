require("@nomiclabs/hardhat-ethers");
require("@nomiclabs/hardhat-waffle");
require("@openzeppelin/hardhat-upgrades");
require("hardhat-gas-reporter");
require("hardhat-abi-exporter");
require("hardhat-contract-sizer");

const {
  ETH_PRV_KEY_1,
  INFURA_RINKERBY_URL,
  INFURA_ROPSTEN_URL,
} = require("./constants");

// This is a sample Hardhat task. To learn how to create your own go to
// https://hardhat.org/guides/create-task.html
task("accounts", "Prints the list of accounts", async (taskArgs, hre) => {
  const accounts = await hre.ethers.getSigners();

  for (const account of accounts) {
    console.log(account.address);
  }
});

/**
 * @type import('hardhat/config').HardhatUserConfig
 */
module.exports = {
  solidity: {
    version: "0.8.9",
    settings: {
      optimizer: {
        enabled: true,
        // See https://docs.soliditylang.org/en/v0.8.12/internals/optimizer.html#optimizer-parameter-runs.
        // runs: 2 ** 32 - 1,
        runs: 1000,
      },
    },
  },
  networks: {
    localhost: {
      url: "http://0.0.0.0:8989/",
    },
    remote: {
      url: "http://dev.hyperfocal.tech:8989/",
    },
    hardhat: {
      // Please set up forking in individual .js test files following https://hardhat.org/hardhat-network/docs/guides/forking-other-networks#resetting-the-fork.
      // This gives us the flexibility to reset Hardhat Network using different fork settings at will.
    },
    ropsten: {
      url: INFURA_ROPSTEN_URL,
      accounts: [ETH_PRV_KEY_1],
      timeout: 60000,
    },
    rinkeby: {
      url: INFURA_RINKERBY_URL,
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
    flat: false,
    spacing: 2,
    pretty: true,
  },
};

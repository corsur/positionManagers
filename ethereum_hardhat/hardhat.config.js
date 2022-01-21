require("@nomiclabs/hardhat-waffle");
require('@openzeppelin/hardhat-upgrades');

const {
  ETH_PRV_KEY_1,
  INFURA_URL_RINKERBY,
  INFURA_URL_ROPSTEN,
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
  solidity: "0.8.4",
  networks: {
    hardhat: {},
    ropsten: {
      url: INFURA_URL_ROPSTEN,
      accounts: [ETH_PRV_KEY_1],
    },
    rinkeby: {
      url: INFURA_URL_RINKERBY,
      accounts: [ETH_PRV_KEY_1],
    },
  },
  paths: {
    sources: "./contracts",
    tests: "./test/unit",
    cache: "./cache",
    artifacts: "./artifacts",
  },
  mocha: {
    timeout: 200000,
  },
};

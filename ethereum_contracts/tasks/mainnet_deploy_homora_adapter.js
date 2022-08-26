const { task, types } = require("hardhat/config");
const { AVAX_MAINNET_URL } = require("../constants");
const { deployHomoraAdapter } = require("../utils/deploy");

// Read private variables from .env file.
require("dotenv").config();

task("deploy-homora-adapter", "To deploy Homora adapter contract")
  .addFlag("dryRun", "Use hardhat testnet work if true")
  .addParam(
    "homoraBank",
    "Address to Homora bank",
    undefined,
    types.string,
    false
  )
  .setAction(async (taskArgs, hre) => {
    if (taskArgs.dryRun) {
      await network.provider.request({
        method: "hardhat_reset",
        params: [
          {
            forking: {
              jsonRpcUrl: AVAX_MAINNET_URL,
              blockNumber: 19079166,
            },
          },
        ],
      });
    }

    const ethers = hre.ethers;
    const provider = ethers.provider;
    const wallet = new ethers.Wallet(process.env.EVM_PRIVATE_KEY, provider);

    console.log(
      `Deploying using account: ${
        wallet.address
      } with balance of ${await wallet.getBalance()}`
    );

    // Deploy Homora adapter contract.
    // `deployHomoraAdapter` already made sure to wait for `deployed()`.
    const homoraAdapter = await deployHomoraAdapter(
      ethers,
      wallet,
      taskArgs.homoraBank
    );
    console.log(`Deployed homora adapter at ${homoraAdapter.address}`);

    // Transfer ownership.
    console.log(
      `Transferring adapter's ownership from ${await homoraAdapter.owner()} to ${
        process.env.GNOSIS_SAFE_ADDR
      }`
    );

    await homoraAdapter.transferOwnership(process.env.GNOSIS_SAFE_ADDR);

    console.log(
      `Completed ownership transfer to ${await homoraAdapter.owner()}`
    );
  });

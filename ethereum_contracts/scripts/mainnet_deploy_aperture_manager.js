const { ethers } = require("hardhat");
const { AVAX_MAINNET_TOKEN_BRIDGE_ADDR } = require("../constants");
const { deployApertureManager } = require("../utils/deploy");

// Read private variables from .env file.
require("dotenv").config();

const provider = ethers.provider;
const wallet = new ethers.Wallet(process.env.EVM_PRIVATE_KEY, provider);

async function main() {
  console.log(`Using account: ${wallet.address}`);

  const balance = await wallet.getBalance();
  console.log(`Avax balance: ${balance}`);

  const apertureManager = await deployApertureManager(
    ethers,
    wallet,
    AVAX_MAINNET_TOKEN_BRIDGE_ADDR
  );
  console.log(`Deployed Aperture Manager at: ${apertureManager.address}`);

  // Transfer ownership.
  console.log(`Transferring ownership to ${process.env.GNOSIS_SAFE_ADDR}`);

  await apertureManager.transferOwnership(process.env.GNOSIS_SAFE_ADDR);

  console.log(
    `Completed ownership transfer to ${process.env.GNOSIS_SAFE_ADDR}`
  );
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });

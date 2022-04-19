const { deployEthereumManagerHardhat } = require("../utils/helpers");

async function main() {
  const ethereumManager = await deployEthereumManagerHardhat();
  console.log(`Contract Address: ${ethereumManager.address}`);
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });

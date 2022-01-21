const { ethers, upgrades } = require("hardhat");

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log(`Deploying contract with the account: ${deployer.address}`);

  const balance = await deployer.getBalance();
  console.log(`Account Balance: ${balance}`);

  const PROXY_ADDRESS = "0x479250E286eEC0F39e007C67491DfE15A99Ab789";

  const EthManager = await ethers.getContractFactory("EthereumManager");
  const ethManager = await upgrades.upgradeProxy(
    PROXY_ADDRESS,
    EthManager,
    { unsafeAllow: ["delegatecall"] }
  );
  await ethManager.deployed();
  console.log(`Contract Address: ${ethManager.address}`);
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });

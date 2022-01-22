const { ethers, upgrades } = require("hardhat");
const {
  getEmitterAddressTerra,
  hexToUint8Array,
} = require("@certusone/wormhole-sdk");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_MANAGER_ADDR,
} = require("../constants");

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log(`Deploying contract with the account: ${deployer.address}`);

  const balance = await deployer.getBalance();
  console.log(`Account Balance: ${balance}`);

  const consistencyLevel = 1;
  const EthereumManager = await ethers.getContractFactory("EthereumManager");
  const ethereumManager = await upgrades.deployProxy(
    EthereumManager,
    [
      consistencyLevel,
      ETH_UST_CONTRACT_ADDR,
      ETH_TOKEN_BRIDGE_ADDR,
      hexToUint8Array(
        await getEmitterAddressTerra(TERRA_MANAGER_ADDR)
      ),
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await ethereumManager.deployed();
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

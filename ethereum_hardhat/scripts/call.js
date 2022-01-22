const { ethers, upgrades } = require("hardhat");
const {
  getEmitterAddressTerra,
  hexToUint8Array,
} = require("@certusone/wormhole-sdk");
const {
  ETH_UST_CONTRACT_ADDR,
  ETH_TOKEN_BRIDGE_ADDR,
  TERRA_CROSSANCHOR_BRIDGE_ADDR,
} = require("../constants");

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log(`Using account: ${deployer.address}`);

  const balance = await deployer.getBalance();
  console.log(`Account Balance: ${balance}`);

  const PROXY_ADDRESS = "0x479250E286eEC0F39e007C67491DfE15A99Ab789"
  const EthManager = await ethers.getContractFactory("EthereumManager");
  const ethManager = EthManager.attach(PROXY_ADDRESS);

  console.log(`Contract Address: ${ethManager.address}`);

  const res = await ethManager.getPositions("0x8F826f2ed5eaf1B53A478ed3236D234122FE8312");
  console.log(`Response: ${res}`);
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });

const { ethers } = require("hardhat");

async function main() {
  const [deployer] = await ethers.getSigners();
  console.log(`Using account: ${deployer.address}`);

  const balance = await deployer.getBalance();
  console.log(`Account Balance: ${balance}`);

  const PROXY_ADDRESS = "0xADf3c19b650AFC370E323e08204Cb79Ce74bf7d3";
  const EthManager = await ethers.getContractFactory("EthereumManager");
  const ethManager = EthManager.attach(PROXY_ADDRESS);

  console.log(`Contract Address: ${ethManager.address}`);

  const res = await ethManager.getPositions(
    "0x689961608D2d7047F5411F9d9004D440449CbD27"
  );
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

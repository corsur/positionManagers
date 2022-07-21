const { expect } = require("chai");
const { ethers, upgrades } = require("hardhat");
const { homoraBankABI } = require("../test/abi/homoraBankABI");
const {
  USDC_TOKEN_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  HOMORA_BANK_ADDRESS,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  JOE_TOKEN_ADDRESS,
  WAVAX_USDC_POOL_ID,
} = require("../test/avax_constants");

// constants.
const provider = ethers.provider;
const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };
const mainWallet = new ethers.Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);
const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);
const ZERO_ADDR = "0x0000000000000000000000000000000000000000";

async function getImpersonatedSigner(accountToImpersonate) {
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}

async function whitelistContractAndAddCredit(contractAddressToWhitelist) {
  // Get impersonatedSigner as governor of homoraBank contract.
  const homoraBankGovernor = await homoraBank.governor(txOptions);
  const signer = await getImpersonatedSigner(homoraBankGovernor);

  // Transfer AVAX to the governor and check.
  await mainWallet.sendTransaction({
    to: homoraBankGovernor,
    value: ethers.utils.parseEther("100"),
  });
  // expect(await provider.getBalance(signer.address)).to.equal(BigInt(1e20));

  // Whitelist address and check.
  await homoraBank
    .connect(signer)
    .setWhitelistUsers([contractAddressToWhitelist], [true], txOptions);
  let res = await homoraBank.whitelistedUsers(
    contractAddressToWhitelist,
    txOptions
  );
  expect(res).to.equal(true);

  // Set credit to 100,000 USDC and 5,000 WAVAX.
  await homoraBank.connect(signer).setCreditLimits(
    [
      [contractAddressToWhitelist, USDC_TOKEN_ADDRESS, 1e11, ZERO_ADDR],
      [
        contractAddressToWhitelist,
        WAVAX_TOKEN_ADDRESS,
        ethers.BigNumber.from("1000000000000000000000"),
        ZERO_ADDR,
      ],
    ],
    txOptions
  );
}

var managerFactory = undefined;
var managerContract = undefined;
var strategyFactory = undefined;
var strategyContract = undefined;

async function main() {
  console.log(`Using account: ${mainWallet.address}`);

  const balance = await mainWallet.getBalance();
  console.log(`Account Balance: ${balance}`);

  // Aperture manager contract.
  managerFactory = await ethers.getContractFactory("EthereumManager");
  managerContract = await upgrades.deployProxy(
    managerFactory,
    [
      /*_consistencyLevel=*/ 1,
      /*_wormholeTokenBridge=*/ "0x0e082F06FF657D94310cB8cE8B0D9a04541d8052",
      /*_crossChainFeeBPS=*/ 0,
      /*_feeSink=*/ mainWallet.address,
      /*_curveSwap unused=*/ mainWallet.address,
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await managerContract.connect(mainWallet).deployed(txOptions);

  console.log("Aperture manager deployed at: ", managerContract.address);

  // HomoraPDNVault contract.
  strategyFactory = await ethers.getContractFactory("HomoraPDNVault");
  strategyContract = await strategyFactory
    .connect(mainWallet)
    .deploy(
      mainWallet.address,
      managerContract.address,
      "WAVAX-USDC TraderJoe",
      "L3x-WAVAXUSDC-TJ1",
      USDC_TOKEN_ADDRESS,
      WAVAX_TOKEN_ADDRESS,
      /*leverage=*/ 3,
      HOMORA_BANK_ADDRESS,
      TJ_SPELLV3_WAVAX_USDC_ADDRESS,
      JOE_TOKEN_ADDRESS,
      WAVAX_USDC_POOL_ID,
      txOptions
    );
  console.log("Homora PDN contract deployed at: ", strategyContract.address);
  await whitelistContractAndAddCredit(strategyContract.address);

  // Add strategy into Aperture manager.
  await managerContract.addStrategy(
    "Homora Delta-neutral",
    "1.0.0",
    strategyContract.address
  );
  console.log(
    "Added strategy: ",
    await managerContract.strategyIdToMetadata(0)
  );

  // Whitelist tokens for the strategy.
  await managerContract.updateIsTokenWhitelistedForStrategy(
    AVAX_CHAIN_ID,
    /*strategyId=*/ 0,
    USDC_TOKEN_ADDRESS,
    /*isWhitelisted=*/ true
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

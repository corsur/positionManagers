const { CHAIN_ID_AVAX } = require("@certusone/wormhole-sdk");
const { ethers } = require("hardhat");
const { BigNumber } = require("ethers");
const { AVAX_MAINNET_TOKEN_BRIDGE_ADDR } = require("../constants");
const { homoraBankABI } = require("../test/abi/homoraBankABI");
const {
  USDC_TOKEN_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  HOMORA_BANK_ADDRESS,
  WAVAX_USDC_POOL_ID,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  JOE_TOKEN_ADDRESS,
} = require("../test/avax_constants");
const {
  whitelistContractAndAddCredit,
  getImpersonatedSigner,
} = require("../utils/accounts");
const { deployHomoraPDNVault } = require("../utils/deploy");

// constants.
const provider = ethers.provider;
const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };
const mainWallet = new ethers.Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);

const ERC20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
  "function transfer(address _to, uint256 value) returns(bool)",
];

const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);

const USDC = new ethers.Contract(USDC_TOKEN_ADDRESS, ERC20ABI, provider);

async function getUSDC(addr, amount) {
  // Impersonate USDC holder.
  const signer = await getImpersonatedSigner(
    "0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541"
  );

  await USDC.connect(signer).transfer(addr, amount * 1e6, txOptions);
}

async function main() {
  console.log(`Using account: ${mainWallet.address}`);

  const balance = await mainWallet.getBalance();
  console.log(`Account Balance: ${balance}`);

  const { managerContract, strategyContract, homoraAdapter } =
    await deployHomoraPDNVault(
      ethers,
      mainWallet,
      {
        wormholeTokenBridgeAddr: AVAX_MAINNET_TOKEN_BRIDGE_ADDR,
        controllerAddr: mainWallet.address,
        tokenA: USDC_TOKEN_ADDRESS,
        tokenB: WAVAX_TOKEN_ADDRESS,
        homoraBankAddr: homoraBank.address,
        spellAddr: TJ_SPELLV3_WAVAX_USDC_ADDRESS,
        rewardTokenAddr: JOE_TOKEN_ADDRESS,
        poolId: WAVAX_USDC_POOL_ID,
      },
      txOptions
    );

  // Set up Homora adapter contract.
  await homoraAdapter.setCaller(strategyContract.address, true);
  await homoraAdapter.setTarget(strategyContract.address, true);
  await homoraAdapter.setTarget(USDC_TOKEN_ADDRESS, true);
  await homoraAdapter.setTarget(WAVAX_TOKEN_ADDRESS, true);
  await homoraAdapter.setTarget(JOE_TOKEN_ADDRESS, true);

  await strategyContract.connect(mainWallet).initializeConfig(
    3, // _leverageLevel
    9231, // _targetDebtRatio
    100, // _debtRatioWidth
    300, // _deltaThreshold
    [
      20, // withdrawFee
      1500, // harvestFee
      200, // managementFee
    ],
    [
      BigNumber.from(1000000).mul(1e6), // maxCapacity
      BigNumber.from(200000).mul(1e6), // maxOpenPerTx
      BigNumber.from(200000).mul(1e6), // maxWithdrawPerTx
    ],
    txOptions
  );
  console.log("Homora PDN contract initialized");

  await whitelistContractAndAddCredit(
    mainWallet,
    homoraBank,
    homoraAdapter.address,
    {
      tokenA: USDC_TOKEN_ADDRESS,
      amtA: 1e11,
      tokenB: WAVAX_TOKEN_ADDRESS,
      amtB: ethers.BigNumber.from("1000000000000000000000"),
    },
    txOptions
  );

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
    CHAIN_ID_AVAX,
    /*strategyId=*/ 0,
    USDC_TOKEN_ADDRESS,
    /*isWhitelisted=*/ true
  );

  // Send some USDC to wallet for testing.
  await getUSDC(mainWallet.address, 1e6);
  console.log(
    `Sent ${await USDC.balanceOf(mainWallet.address)} USDC to ${
      mainWallet.address
    }`
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

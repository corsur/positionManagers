const { expect, assert } = require("chai");
const { ethers } = require("hardhat");

const { homoraBankABI } = require("./abi/homoraBankABI.js");

const ERC20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
  "function transfer(address _to, uint256 value) returns(bool)",
];

const {
  HOMORA_BANK_ADDRESS,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  USDC_TOKEN_ADDRESS,
  JOE_TOKEN_ADDRESS,
  WAVAX_USDC_POOL_ID,
} = require("./avax_constants");

const provider = ethers.provider;
const WAVAX = new ethers.Contract(WAVAX_TOKEN_ADDRESS, ERC20ABI, provider);
const USDC = new ethers.Contract(USDC_TOKEN_ADDRESS, ERC20ABI, provider);
const JOE = new ethers.Contract(JOE_TOKEN_ADDRESS, ERC20ABI, provider);
const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);

const mainWallet = new ethers.Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);

const wallets = [
  new ethers.Wallet(
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    provider
  ),
  new ethers.Wallet(
    "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    provider
  ),
  new ethers.Wallet(
    "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    provider
  ),
  new ethers.Wallet(
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    provider
  ),
];

const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };
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
  expect(await provider.getBalance(signer.address)).to.equal(BigInt(1e20));

  // Whitelist address and check.
  await homoraBank
    .connect(signer)
    .setWhitelistUsers([contractAddressToWhitelist], [true], txOptions);
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

  // Check.
  let res = await homoraBank.whitelistedUsers(
    contractAddressToWhitelist,
    txOptions
  );
  expect(res).to.equal(true);
}

async function initialize(contract) {
  // Impersonate USDC holder.
  const signer = await getImpersonatedSigner(
    "0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541"
  );

  await USDC.connect(signer).transfer(
    wallets[0].address,
    1000 * 1e6,
    txOptions
  );
  await USDC.connect(signer).transfer(
    wallets[1].address,
    1000 * 1e6,
    txOptions
  );
}

// testing function for rebalance()
async function rebalanceTest(contract) {
  const usdc_deposit_amount = 300 * 1e6;
  await USDC.connect(wallets[0]).approve(
    contract.address,
    usdc_deposit_amount * 10,
    txOptions
  );

  // deposit 300 USDC
  await contract.connect(wallets[0]).deposit(usdc_deposit_amount, 0, txOptions);

  let usdc_expect = (300 * 1e6 * 3) / 2;
  let wavax_expect = await contract
    .connect(wallets[0])
    .getEquivalentTokenB(usdc_expect, txOptions);

  // check collateral
  let collSize = await contract.connect(wallets[0]).getCollateralSize(txOptions);
  let [usdc_hold, wavax_hold] = await contract
    .connect(wallets[0])
    .convertCollateralToTokens(collSize, txOptions);
  console.log("collateral: usdc: %d, wavax: %d", usdc_hold, wavax_hold);
  console.log("    expect: usdc: %d, wavax: %d", usdc_expect, wavax_expect);
  assert(
    Math.abs(usdc_expect - usdc_hold) / usdc_expect < 1e-6,
    "collateral USDC not equal to the expected amount"
  );
  assert(
    Math.abs(wavax_expect - wavax_hold) / wavax_expect < 1e-6,
    "collateral WAVAX not equal to the expected amount"
  );

  // check debt
  let [usdc_debt, wavax_debt] = await contract
    .connect(wallets[0])
    .currentDebtAmount(txOptions);
  let usdc_debt_expect = (300 * 1e6) / 2;
  let wavax_debt_expect = wavax_expect;
  console.log("current debt: usdc: %d, wavax: %d", usdc_debt, wavax_debt);
  console.log(" expect debt: usdc: %d, wavax: %d", usdc_debt_expect, wavax_debt_expect);
  assert(
    Math.abs(usdc_debt_expect - usdc_debt) / usdc_debt_expect < 1e-6,
    "USDC debt not equal to the expected amount"
  );
  assert(
    Math.abs(wavax_debt_expect - wavax_debt) / wavax_debt_expect < 1e-6,
    "WAVAX debt not equal to the expected amount"
  );

  // check if position state is healthy (no need to rebalance)
  try {
    await contract.connect(wallets[0]).rebalance(txOptions);
  } catch (err) {
    console.log(err);
  }

  // set delta-neutral threshold to 0 to force executing rebalance
  // await contract.connect(wallets[0]).setDNThreshold(0, txOptions);

}

describe.only("DeltaNeutralVault Initialization", function () {
  it("Initialize and whitelist DeltaNeutralVault contract", async function () {
    // DeltaNeutralVault contract
    const contractFactory = await ethers.getContractFactory(
      "DeltaNeutralVault"
    );
    const contract = await contractFactory
      .connect(mainWallet)
      .deploy(
        "WAVAX-USDC TraderJoe",
        "L3x-WAVAXUSDC-TJ1",
        USDC_TOKEN_ADDRESS,
        WAVAX_TOKEN_ADDRESS,
        3,
        HOMORA_BANK_ADDRESS,
        TJ_SPELLV3_WAVAX_USDC_ADDRESS,
        JOE_TOKEN_ADDRESS,
        WAVAX_USDC_POOL_ID,
        txOptions
      );
    await whitelistContractAndAddCredit(contract.address);
    await initialize(contract);

    await rebalanceTest(contract);
  });
});

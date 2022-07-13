const { expect } = require("chai");
const { ethers } = require("hardhat");

const { homoraBankABI } = require('./abi/homoraBankABI.js');

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
const homoraBank = new ethers.Contract(HOMORA_BANK_ADDRESS, homoraBankABI, provider);

const mainWallet = new ethers.Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80', provider);

const wallets = [
    new ethers.Wallet('0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d', provider),
    new ethers.Wallet('0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a', provider),
    new ethers.Wallet('0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6', provider),
    new ethers.Wallet('0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a', provider),
];

const txOptions = {gasPrice: 50000000000, gasLimit: 8500000};

async function getImpersonatedSigner(accountToImpersonate) {
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}

async function whitelistContract(contractAddressToWhitelist) {
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
  await homoraBank.connect(signer).setWhitelistUsers([contractAddressToWhitelist, ], [true,], txOptions);
  let res = await homoraBank.whitelistedUsers(contractAddressToWhitelist, txOptions);
  expect(res).to.equal(true);
}

async function initialize(contract) {
  // Impersonate USDC holder.
  const signer = await getImpersonatedSigner('0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541');

  await USDC.connect(signer).transfer(wallets[0].address, 1000 * 1e6, txOptions);
  await USDC.connect(signer).transfer(wallets[1].address, 1000 * 1e6, txOptions);

  await USDC.connect(wallets[0]).approve(contract.address, 1000 * 1e6, txOptions);
  await USDC.connect(wallets[1]).approve(contract.address, 1000 * 1e6, txOptions);

  // JOE holder
  // const signer = await getImpersonatedSigner('0x1a731b2299e22fbac282e7094eda41046343cb51');
  // await JOE.connect(signer).transfer(wallets[0].address, 1e12, txOptions);
  // await JOE.connect(wallets[0]).approve(contract.address, 1e12, txOptions);
}

describe.only("DeltaNeutralVault Initialization", function() {
  it("Initialize and whitelist DeltaNeutralVault contract", async function () {
    const contractFactory = await ethers.getContractFactory("DeltaNeutralVault");
    const contract = await contractFactory.connect(mainWallet).deploy(
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
    
    
    await whitelistContract(contract.address);
    await initialize(contract);
    
    const usdc_deposit_amount_0 = 300 * 1e6;
    const usdc_deposit_amount_1 = 10000 * 1e6;

    await USDC.connect(wallets[0]).approve(contract.address, usdc_deposit_amount_0*10, txOptions);
    // await USDC.connect(wallets[1]).approve(contract.address, usdc_deposit_amount_1*10, txOptions);
    
    await contract.connect(wallets[0]).deposit(usdc_deposit_amount_0, 0, txOptions);
    // await contract.connect(wallets[1]).deposit(usdc_deposit_amount_1, 0, txOptions);
    //await contract.connect(wallets[0]).withdraw(5000000, txOptions);

    await contract.connect(wallets[0]).rebalance(txOptions);
    await contract.connect(wallets[0]).reinvest(txOptions);
  });
});


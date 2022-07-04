const { expect } = require("chai");
const { ethers } = require("hardhat");

const { homoraBankABI } = require('./abi/homoraBankABI.js');
const SpellABI = [
  "function pairs(address, address) external view returns (address)",
  "function werc20() external view returns (address)",
];

const ERC20ABI = require('../data/abi/CErc20.json');

const { 
  HOMORA_BANK_ADDRESS,
  UNISWAP_SPELL,
  USDC_TOKEN,
  WETH_TOKEN,
  WSTAKING,
} = require("../constants");

const provider = ethers.provider;
const USDC = new ethers.Contract(USDC_TOKEN, ERC20ABI, provider);
const WETH = new ethers.Contract(WETH_TOKEN, ERC20ABI, provider);
const homoraBankContract = new ethers.Contract(HOMORA_BANK_ADDRESS, homoraBankABI, provider);
const spell = new ethers.Contract(UNISWAP_SPELL, SpellABI, provider);
const LP_TOKEN = spell.pairs(USDC_TOKEN, WETH_TOKEN);
const LP = new ethers.Contract(LP_TOKEN, ERC20ABI, provider);

const mainWallet = new ethers.Wallet('0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80', provider);

const wallets = [
    new ethers.Wallet('0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d', provider),
    new ethers.Wallet('0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a', provider),
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
  const homoraBankGovernor = await homoraBankContract.governor(txOptions);
  const signer = await getImpersonatedSigner(homoraBankGovernor);
  
  // Transfer ETH to the governor and check.
  await mainWallet.sendTransaction({
      to: homoraBankGovernor,
      value: ethers.utils.parseEther("100"),
    });
  expect(await provider.getBalance(signer.address)).to.equal(BigInt(1e20));

  // Whitelist address and check.
  await homoraBankContract.connect(signer).setWhitelistUsers([contractAddressToWhitelist, ], [true,], txOptions);
  let res = await homoraBankContract.whitelistedUsers(contractAddressToWhitelist, txOptions);
  expect(res).to.equal(true);
}

async function initialize(contract) {
  await USDC.connect(wallets[0]).approve(contract.address, 1000);
  await USDC.connect(wallets[1]).approve(contract.address, 1000);
  await USDC.connect(wallets[0]).transfer(wallets[1].address, 1000);
}

describe.only("DeltaNeutralVault Initialization", function() {
  it("Initialize and whitelist DeltaNeutralVault contract", async function () {
    const contractFactory = await ethers.getContractFactory("DeltaNeutralVault");
    const contract = await contractFactory.connect(mainWallet).deploy(
      "USDC-WETH UniSwap",
      "L3x-USDCWETH-UNS1",
      USDC_TOKEN,
      WETH_TOKEN,
      3,
      HOMORA_BANK_ADDRESS,
      UNISWAP_SPELL,
      HOMORA_BANK_ADDRESS,
      WSTAKING,
      );
    await whitelistContract(contract.address);
    await initialize(contract);
    await contract.connect(wallets[0]).deposit(1000, 0, txOptions);
    await contract.connect(wallets[1]).deposit(1000, 0, txOptions);
  });
});


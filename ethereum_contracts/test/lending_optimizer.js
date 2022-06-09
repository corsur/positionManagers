const { ethers } = require("hardhat");
const { expect } = require("chai");
const { Web3Provider } = require("@ethersproject/providers");
const { cTokenAbi } = require('./cTokenABI.js');

const erc20ABI = [
  // Read-Only Functions
  "function balanceOf(address owner) view returns (uint256)",
  // Authenticated Functions
  "function approve(address spender, uint256 value) returns (bool)",
];

const IMPERSONATE_ADDR = "0x2FAF487A4414Fe77e2327F0bf4AE2a264a776AD2";
const USDC_ADDR = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const CUSDC_ADDR = "0x39AA39c021dfbaE8faC545936693aC917d5E7563";

async function getImpersonatedSigner() {
  const accountToImpersonate = IMPERSONATE_ADDR;
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}

async function deployLendingOptimizer(signer) {
  const address = await signer.getAddress();
  console.log("Using impersonated wallet address:", address);

  const LendingOptimizer = await ethers.getContractFactory(
    "LendingOptimizer",
    signer
  );

  const lendingOptimizer = await LendingOptimizer.deploy();
  await lendingOptimizer.deployed();

  return lendingOptimizer;
}

async function testSupplyTokenToCompound(signer, lendingOptimizer) {
  const supplyAmount = 20 * 1e6; // $20
  const tokenContract = new ethers.Contract(USDC_ADDR, erc20ABI, signer);
  const cTokenContract = new ethers.Contract(CUSDC_ADDR, cTokenAbi, signer);

  await tokenContract.approve(lendingOptimizer.address, supplyAmount);

  const prevSignerBalance = await tokenContract.balanceOf(signer.address);
  const prevCompoundBalance = await tokenContract.balanceOf(CUSDC_ADDR);
  const prevCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);

  await lendingOptimizer.supplyTokenToCompound(supplyAmount);

  const afterSignerBalance = await tokenContract.balanceOf(signer.address);
  const afterCompoundBalance = await tokenContract.balanceOf(CUSDC_ADDR);
  const afterCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);

  const signerBalanceDelta = (afterSignerBalance - prevSignerBalance) / 1e6;
  const compoundBalanceDelta = (afterCompoundBalance - prevCompoundBalance) / 1e6;
  const exchangeRate = await cTokenContract.callStatic.exchangeRateCurrent();
  const cDelta = (afterCBalance - prevCBalance) * exchangeRate / 1e24;

  console.log("supplyTokenToCompound(supplyAmount) completed.");
  return [signerBalanceDelta, compoundBalanceDelta, cDelta];
}

describe.only("Lending optimizer supply to compound unit tests", function () {
  var signer = undefined;
  var lendingOptimizer = undefined;

  beforeEach("Setup before each test", async function () {
    signer = await getImpersonatedSigner();
    lendingOptimizer = await deployLendingOptimizer(signer);
  });

  it("Supply to compound balance check", async function () {
    const [signerBalanceDelta, compoundBalanceDelta, cDelta] = await testSupplyTokenToCompound(signer, lendingOptimizer);
    expect(signerBalanceDelta).to.equal(-20);
    expect(compoundBalanceDelta).to.equal(20);
    expect(Math.ceil(cDelta)).to.equal(20); // ceil because some decimals are lost with exchange rate multiplication
  });
});

const { ethers } = require("hardhat");
const { expect } = require("chai");
const { Web3Provider } = require("@ethersproject/providers");
const { cTokenAbi } = require('./abi/cTokenABI.js');
const { aTokenAbi } = require('./abi/aTokenABI.js');
const { cEthAbi } = require('./abi/cEthABI.js');

const erc20ABI = [
  // Read-Only Functions
  "function balanceOf(address owner) view returns (uint256)",
  // Authenticated Functions
  "function approve(address spender, uint256 value) returns (bool)",
];

const IMPERSONATE_ADDR = "0x2FAF487A4414Fe77e2327F0bf4AE2a264a776AD2";
const USDC_ADDR = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const CUSDC_ADDR = "0x39AA39c021dfbaE8faC545936693aC917d5E7563";
const AUSDC_ADDR = "0xBcca60bB61934080951369a648Fb03DF4F96263C";


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

  const lendingOptimizer = await upgrades.deployProxy(
    LendingOptimizer,
    [
      "0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5",
      "0x7d2768dE32b0b80b7a3454c06BdAc94A69DDc7A9",
      "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
      "0xcc9a0B7c43DC2a5F023Bb9b738E45B0Ef6B06E04",
    ], { unsafeAllow: ["delegatecall"], kind: "uups" }
  );

  await lendingOptimizer.deployed();

  return lendingOptimizer;
}

// no longer works because supplyTokenToCompound is private
async function testSupplyTokenToCompound(signer, lendingOptimizer) {
  const supplyAmount = 20 * 1e6; // $20
  const tokenContract = new ethers.Contract(USDC_ADDR, erc20ABI, signer);
  const cTokenContract = new ethers.Contract(CUSDC_ADDR, cTokenAbi, signer);

  await tokenContract.approve(lendingOptimizer.address, supplyAmount);

  const prevSignerBalance = await tokenContract.balanceOf(signer.address);
  const prevCompoundBalance = await tokenContract.balanceOf(CUSDC_ADDR);
  const prevCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);

  await lendingOptimizer.supplyTokenToCompound(USDC_ADDR, supplyAmount);

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

// no longer works because supplyTokenToAave is private
async function testSupplyTokenToAave(signer, lendingOptimizer) {
  const supplyAmount = 20 * 1e6; // $20
  const tokenContract = new ethers.Contract(USDC_ADDR, erc20ABI, signer);
  const aTokenContract = new ethers.Contract(AUSDC_ADDR, aTokenAbi, signer);

  await tokenContract.approve(lendingOptimizer.address, supplyAmount);

  const prevSignerBalance = await tokenContract.balanceOf(signer.address);
  const prevAaveBalance = await tokenContract.balanceOf(AUSDC_ADDR);
  const prevABalance = await aTokenContract.balanceOf(lendingOptimizer.address);

  await lendingOptimizer.supplyTokenToAave(USDC_ADDR, supplyAmount);

  const afterSignerBalance = await tokenContract.balanceOf(signer.address);
  const afterAaveBalance = await tokenContract.balanceOf(AUSDC_ADDR);
  const afterABalance = await aTokenContract.balanceOf(lendingOptimizer.address);

  const signerBalanceDelta = (afterSignerBalance - prevSignerBalance) / 1e6;
  const aaveBalanceDelta = (afterAaveBalance - prevAaveBalance) / 1e6;
  const aDelta = (afterABalance - prevABalance) / 1e6;

  console.log("supplyTokenToAave(supplyAmount) completed.");
  return [signerBalanceDelta, aaveBalanceDelta, aDelta];
}

async function testSupplyUSDC(signer, lendingOptimizer) {
  const amount = 20 * 1e6; // $20
  const tokenContract = new ethers.Contract(USDC_ADDR, erc20ABI, signer);
  const cTokenContract = new ethers.Contract(CUSDC_ADDR, cTokenAbi, signer);
  const aTokenContract = new ethers.Contract(AUSDC_ADDR, aTokenAbi, signer);

  await tokenContract.approve(lendingOptimizer.address, amount);

  const prevSignerBalance = await tokenContract.balanceOf(signer.address);
  const prevCompoundBalance = await tokenContract.balanceOf(CUSDC_ADDR);
  const prevCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);
  const prevAaveBalance = await tokenContract.balanceOf(AUSDC_ADDR);
  const prevABalance = await aTokenContract.balanceOf(lendingOptimizer.address);

  await lendingOptimizer.supply(USDC_ADDR, amount);

  const afterSignerBalance = await tokenContract.balanceOf(signer.address);
  const afterCompoundBalance = await tokenContract.balanceOf(CUSDC_ADDR);
  const afterCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);
  const afterAaveBalance = await tokenContract.balanceOf(AUSDC_ADDR);
  const afterABalance = await aTokenContract.balanceOf(lendingOptimizer.address);

  const signerBalanceDelta = (afterSignerBalance - prevSignerBalance) / 1e6;
  const compoundBalanceDelta = (afterCompoundBalance - prevCompoundBalance) / 1e6;
  const exchangeRate = await cTokenContract.callStatic.exchangeRateCurrent();
  const cDelta = (afterCBalance - prevCBalance) * exchangeRate / 1e24;
  const aaveBalanceDelta = (afterAaveBalance - prevAaveBalance) / 1e6;
  const aDelta = (afterABalance - prevABalance) / 1e6;

  console.log("supply(tokenAddr, amount) completed.");
  return [signerBalanceDelta, compoundBalanceDelta, cDelta, aaveBalanceDelta, aDelta];
}

async function testSupplyOthers(addr, signer, lendingOptimizer) {
  const tokenContract = new ethers.Contract(addr, erc20ABI, signer);
  await tokenContract.approve(lendingOptimizer.address, 1e8);
  await lendingOptimizer.supply(addr, 1e8);
  console.log("test completed for " + addr);
}

describe.only("LendingOptimizer supply unit tests", function () {
  var signer = undefined;
  var lendingOptimizer = undefined;

  beforeEach("Setup before each test", async function () {
    signer = await getImpersonatedSigner();
    lendingOptimizer = await deployLendingOptimizer(signer);

    await lendingOptimizer.mapToC("0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9", "0xe65cdB6479BaC1e22340E4E755fAE7E509EcD06c"); // AAVE
    await lendingOptimizer.mapToC("0x0D8775F648430679A709E98d2b0Cb6250d2887EF", "0x6C8c6b02E7b2BE14d4fA6022Dfd6d75921D90E4E"); // BAT
    await lendingOptimizer.mapToC("0x6B175474E89094C44Da98b954EedeAC495271d0F", "0x5d3a536E4D6DbD6114cc1Ead35777bAB948E3643"); // DAI
    await lendingOptimizer.mapToC("0x956F47F50A910163D8BF957Cf5846D573E7f87CA", "0x7713DD9Ca933848F6819F38B8352D9A15EA73F67"); // FEI
    await lendingOptimizer.mapToC("0x514910771AF9Ca656af840dff83E8264EcF986CA", "0xFAce851a4921ce59e912d19329929CE6da6EB0c7"); // LINK
    await lendingOptimizer.mapToC("0x9f8F72aA9304c8B593d555F12eF6589cC3A579A2", "0x95b4eF2869eBD94BEb4eEE400a99824BF5DC325b"); // MKR
    await lendingOptimizer.mapToC("0x0000000000085d4780B73119b644AE5ecd22b376", "0x12392F67bdf24faE0AF363c24aC620a2f67DAd86"); // TUSD
    await lendingOptimizer.mapToC("0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984", "0x35A18000230DA775CAc24873d00Ff85BccdeD550"); // UNI
    await lendingOptimizer.mapToC("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "0x39AA39c021dfbaE8faC545936693aC917d5E7563"); // USDC
    await lendingOptimizer.mapToC("0x8E870D67F660D95d5be530380D0eC0bd388289E1", "0x041171993284df560249B57358F931D9eB7b925D"); // USDP
    await lendingOptimizer.mapToC("0xdAC17F958D2ee523a2206206994597C13D831ec7", "0xf650C3d88D12dB855b8bf7D11Be6C55A4e07dCC9"); // USDT
    await lendingOptimizer.mapToC("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", "0xC11b1268C1A384e55C48c2391d8d480264A3A7F4"); // WBTC
    await lendingOptimizer.mapToC("0x0bc529c00C6401aEF6D220BE8C6Ea1667F6Ad93e", "0x80a2AE356fc9ef4305676f7a3E2Ed04e12C33946"); // YFI
    await lendingOptimizer.mapToC("0xE41d2489571d322189246DaFA5ebDe1F4699F498", "0xB3319f5D18Bc0D84dD1b4825Dcde5d5f7266d407"); // ZRX
  });

  // no longer works because supplyTokenToCompound is private
  it.skip("Supply to Compound balance check", async function () {
    const [signerBalanceDelta, compoundBalanceDelta, cDelta] = await testSupplyTokenToCompound(signer, lendingOptimizer);
    expect(signerBalanceDelta).to.equal(-20);
    expect(compoundBalanceDelta).to.equal(20);
    expect(Math.ceil(cDelta)).to.equal(20); // ceil because some decimals are lost with exchange rate multiplication
  });

  // no longer works because supplyTokenToAave is private
  it.skip("Supply to AAVE balance check", async function () {
    const [signerBalanceDelta, aaveBalanceDelta, aDelta] = await testSupplyTokenToAave(signer, lendingOptimizer);
    expect(signerBalanceDelta).to.equal(-20);
    expect(aaveBalanceDelta).to.equal(20);
    expect(aDelta).to.equal(20);
  });

  it("Supply optimize between Compound and Aave, USDC", async function () {
    // at block 14957690, compound interst rate was around 0.60% APY, aave was around 1.34% APY, date 6/13/2022
    // the calculated interest rate is quite accurate, good enough and can't test for other dates as historical interset rate data cannot be found
    const [signerBalanceDelta, compoundBalanceDelta, cDelta, aaveBalanceDelta, aDelta] = await testSupplyUSDC(signer, lendingOptimizer);
    expect(signerBalanceDelta).to.equal(-20);
    expect(compoundBalanceDelta).to.equal(0);
    expect(cDelta).to.equal(0);
    expect(aaveBalanceDelta).to.equal(20);
    expect(aDelta).to.equal(20);
  });

  it("Supply optimize between Compound and Aave, USDT", async function () {
    // at block 14957690, compound had higher APY than aave for USDT
    const amount = 20 * 1e6; // $20
    const tokenContract = new ethers.Contract("0xdAC17F958D2ee523a2206206994597C13D831ec7", erc20ABI, signer);
    const cTokenContract = new ethers.Contract("0xf650C3d88D12dB855b8bf7D11Be6C55A4e07dCC9", cTokenAbi, signer);

    await tokenContract.approve(lendingOptimizer.address, amount);

    const prevSignerBalance = await tokenContract.balanceOf(signer.address);
    const prevCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);

    await lendingOptimizer.supply("0xdAC17F958D2ee523a2206206994597C13D831ec7", amount);

    const afterSignerBalance = await tokenContract.balanceOf(signer.address);
    const afterCBalance = await cTokenContract.balanceOf(lendingOptimizer.address);

    const signerBalanceDelta = (afterSignerBalance - prevSignerBalance) / 1e6;
    const exchangeRate = await cTokenContract.callStatic.exchangeRateCurrent();
    const cDelta = (afterCBalance - prevCBalance) * exchangeRate / 1e24;

    expect(signerBalanceDelta).to.equal(-20);
    expect(Math.ceil(cDelta)).to.equal(20);
  });

  it("Supply rest of the tokens", async function () {
    await testSupplyOthers("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", signer, lendingOptimizer); // WBTC
    await testSupplyOthers("0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9", signer, lendingOptimizer); // AAVE
    await testSupplyOthers("0x0D8775F648430679A709E98d2b0Cb6250d2887EF", signer, lendingOptimizer); // BAT
    await testSupplyOthers("0x6B175474E89094C44Da98b954EedeAC495271d0F", signer, lendingOptimizer); // DAI
    // no FEI balance in impersonating account yet
    // await testSupplyOthers("0x956F47F50A910163D8BF957Cf5846D573E7f87CA", signer, lendingOptimizer); // FEI
    await testSupplyOthers("0x514910771AF9Ca656af840dff83E8264EcF986CA", signer, lendingOptimizer); // LINK
    await testSupplyOthers("0x9f8F72aA9304c8B593d555F12eF6589cC3A579A2", signer, lendingOptimizer); // MKR
    await testSupplyOthers("0x0000000000085d4780B73119b644AE5ecd22b376", signer, lendingOptimizer); // TUSD
    await testSupplyOthers("0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984", signer, lendingOptimizer); // UNI
    await testSupplyOthers("0x8E870D67F660D95d5be530380D0eC0bd388289E1", signer, lendingOptimizer); // USDP
    await testSupplyOthers("0x0bc529c00C6401aEF6D220BE8C6Ea1667F6Ad93e", signer, lendingOptimizer); // YFI
    await testSupplyOthers("0xE41d2489571d322189246DaFA5ebDe1F4699F498", signer, lendingOptimizer); // ZRX
  });

  it("Supply: invalid address", async function () {
    await testSupplyOthers("0x0", signer, lendingOptimizer);
    // await testSupplyOthers("0xe65cdB6479BaC1e22340E4E755fAE7E509EcD06c", signer, lendingOptimizer);
  });

  it("Supply ETH", async function () {
    const cETHContract = new ethers.Contract("0x4Ddc2D193948926D02f9B1fE9e1daa0718270ED5", cTokenAbi, signer);
    const aWETHContract = new ethers.Contract("0x030bA81f1c18d280636F32af80b9AAd02Cf0854e", aTokenAbi, signer);

    const prevSignerBalance = await signer.getBalance();
    const prevCEthBalance = await cETHContract.callStatic.balanceOf(lendingOptimizer.address);
    const prevAaveBalance = await aWETHContract.balanceOf(lendingOptimizer.address);

    await lendingOptimizer.supplyEth({ value: ethers.utils.parseUnits('1', 'ether') });

    const afterSignerBalance = await signer.getBalance();
    const afterCEthBalance = await cETHContract.callStatic.balanceOf(lendingOptimizer.address);
    const afterAaveBalance = await aWETHContract.balanceOf(lendingOptimizer.address);

    const exchangeRate = await cETHContract.callStatic.exchangeRateCurrent();
    // actual signer balance delta: 1.0130437946831012
    expect(Math.ceil((afterSignerBalance - prevSignerBalance) / 1e18)).to.equal(-1);
    expect((afterCEthBalance - prevCEthBalance) * exchangeRate / 1e36).to.equal(0);
    expect((afterAaveBalance - prevAaveBalance) / 1e18).to.equal(1);
  });

});
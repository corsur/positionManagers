const { ethers, upgrades } = require("hardhat");
const { expect } = require("chai");
const { cTokenAbi } = require('./abi/cTokenABI.js');
const { aTokenAbi } = require('./abi/aTokenABI.js');

const erc20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
];

const IMPERSONATE_OWNER_ADDR = "0x42d6Ce661bB2e5F5cc639E7BEFE74Ff9Fd649541";
const IMPERSONATE_USER_ADDR = "0x9f8c163cBA728e99993ABe7495F06c0A3c8Ac8b9"; // Binance C-Chain Hot Wallet
const USDC_ADDR = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";
const USDCE_ADDR = "0xA7D7079b0FEaD91F3e65f86E8915Cb59c1a4C664";

async function getImpersonatedSigner(addr) {
  const accountToImpersonate = addr;

  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });

  return await ethers.getSigner(accountToImpersonate);
}

describe.only("LendingOptimizer tests", function () {
  var owner = undefined;
  var user = undefined;
  var lendingOptimizer = undefined;

  beforeEach("Setup before each test", async function () {
    owner = await getImpersonatedSigner(IMPERSONATE_OWNER_ADDR);
    user = await getImpersonatedSigner(IMPERSONATE_USER_ADDR);

    const LendingOptimizer = await ethers.getContractFactory(
      "LendingOptimizer",
      owner
    );

    lendingOptimizer = await upgrades.deployProxy(
      LendingOptimizer,
      [
        "0x794a61358D6845594F94dc1DB02A252b5b4814aD", // _aavePoolAddr
        "0xa938d8536aEed1Bd48f548380394Ab30Aa11B00E", // _wethGateAddr
        "0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7", // _wavaxAddr
        "0x5C0401e81Bc07Ca70fAD469b451682c0d747Ef1c", // _qiAvaxAddr
      ], { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await lendingOptimizer.deployed();

    lendingOptimizer.addBenqiTokenMapping(USDC_ADDR, "0xB715808a78F6041E46d61Cb123C9B4A27056AE9C");
    lendingOptimizer.addBenqiTokenMapping(USDCE_ADDR, "0xBEb5d47A3f720Ec0a390d04b4d41ED7d9688bC7F"); // qiUSDC address
  });

  it.skip("Supply and withdraw token: Aave, Benqi", async function () {
    const amount = 1e6;
    const tokenAddr = USDC_ADDR;
    const token = new ethers.Contract(tokenAddr, erc20ABI, user);

    await token.approve(lendingOptimizer.address, amount);
    await lendingOptimizer.connect(user).supplyTokenAave(tokenAddr, amount);
    await lendingOptimizer.connect(user).withdrawTokenAave(tokenAddr, 8000);
    console.log("Aave token complete.");

    await token.approve(lendingOptimizer.address, amount);
    await lendingOptimizer.connect(user).supplyTokenBenqi(tokenAddr, amount);
    await lendingOptimizer.connect(user).withdrawTokenBenqi(tokenAddr, 8000);
    console.log("Benqi token complete.");
  });

  it("Supply and withdraw AVAX: Aave, Benqi", async function () {
    var prevBalance;
    var afterBalance;

    prevBalance = await user.getBalance();
    await lendingOptimizer.connect(user).supplyAvaxAave({ value: ethers.utils.parseUnits('1000000000', 'gwei') });
    await lendingOptimizer.connect(user).withdrawAvaxAave(8000);
    afterBalance = await user.getBalance();
    console.log(afterBalance - prevBalance);
    console.log("Aave avax complete.");

    prevBalance = await user.getBalance();
    await lendingOptimizer.connect(user).supplyAvaxBenqi({ value: ethers.utils.parseUnits('1000000000', 'gwei') });
    await lendingOptimizer.connect(user).withdrawAvaxBenqi(8000);
    afterBalance = await user.getBalance();
    console.log(afterBalance - prevBalance);
    console.log("Benqi avax complete.");
  });

});

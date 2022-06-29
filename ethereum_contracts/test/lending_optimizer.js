const { ethers, upgrades } = require("hardhat");
const { expect } = require("chai");

const erc20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
];

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

  const USDC = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";
  const USDCE = "0xA7D7079b0FEaD91F3e65f86E8915Cb59c1a4C664";
  const USDT = "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7";

  beforeEach("Setup before each test", async function () {
    owner = await getImpersonatedSigner("0x42d6Ce661bB2e5F5cc639E7BEFE74Ff9Fd649541");
    user = await getImpersonatedSigner("0x9f8c163cBA728e99993ABe7495F06c0A3c8Ac8b9"); // Binance C-Chain Hot Wallet

    const LendingOptimizer = await ethers.getContractFactory("LendingOptimizer", owner);
    lendingOptimizer = await upgrades.deployProxy(
      LendingOptimizer,
      [
        "0x794a61358D6845594F94dc1DB02A252b5b4814aD", // _aavePoolAddr
        "0xa938d8536aEed1Bd48f548380394Ab30Aa11B00E", // _wethGateAddr
        "0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7", // _wavaxAddr
        "0x5C0401e81Bc07Ca70fAD469b451682c0d747Ef1c", // _qiAvaxAddr
        "0xb3c68d69E95B095ab4b33B4cB67dBc0fbF3Edf56", // _ibAvaxAddr
        "0xC22F01ddc8010Ee05574028528614634684EC29e", // _tjAvaxAddr
      ], { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await lendingOptimizer.deployed();

    lendingOptimizer.addCompoundMapping(1, USDC, "0xB715808a78F6041E46d61Cb123C9B4A27056AE9C");
    lendingOptimizer.addCompoundMapping(2, USDC, "0xEc5Aa19566Aa442C8C50f3C6734b6Bb23fF21CD7");
    lendingOptimizer.addCompoundMapping(3, USDC, "0x29472D511808Ce925F501D25F9Ee9efFd2328db2");
    lendingOptimizer.addCompoundMapping(1, USDCE, "0xBEb5d47A3f720Ec0a390d04b4d41ED7d9688bC7F");
    lendingOptimizer.addCompoundMapping(2, USDCE, "0xe28965073C49a02923882B8329D3E8C1D805E832");
    lendingOptimizer.addCompoundMapping(3, USDCE, "0xEd6AaF91a2B084bd594DBd1245be3691F9f637aC");
    lendingOptimizer.addCompoundMapping(1, USDT, "0xd8fcDa6ec4Bdc547C0827B8804e89aCd817d56EF");
  });

  it("Supply and withdraw tokens", async function () {
    const amount = 1e6;
    const tokens = [USDC, USDCE, USDT];

    for (let i = 0; i < tokens.length; i++) {
      const token = new ethers.Contract(tokens[i], erc20ABI, user);
      await token.approve(lendingOptimizer.address, amount);
      const prevBalance = await token.balanceOf(user.address);
      await lendingOptimizer.connect(user).supplyToken(tokens[i], amount);
      const contractBalance = await lendingOptimizer.connect(user).tokenBalance(tokens[i]);
      await lendingOptimizer.connect(user).withdrawToken(tokens[i], 8000);
      const afterBalance = await token.balanceOf(user.address);
      expect(Math.round(contractBalance / 10) * 10).to.equal(1e6);
      expect((afterBalance - prevBalance) / 1e6).to.equal(-0.2);
      console.log("Test complete.");
    }
  });

  it("Supply and withdraw AVAX", async function () {
    const prevBalance = await user.getBalance();
    await lendingOptimizer.connect(user).supplyAvax({ value: ethers.utils.parseUnits('1000000000', 'gwei') });
    await lendingOptimizer.connect(user).avaxBalance();
    await lendingOptimizer.connect(user).withdrawAvax(8000);
    const afterBalance = await user.getBalance();
    expect(Math.round((afterBalance - prevBalance) / 1e17) / 10).to.equal(-0.2);
  });

});

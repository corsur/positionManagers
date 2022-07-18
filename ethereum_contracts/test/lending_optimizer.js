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

describe("LendingOptimizer tests", function () {
  var owner = undefined;
  var user = undefined;
  var optimizer = undefined;

  const USDC = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";
  const USDCE = "0xA7D7079b0FEaD91F3e65f86E8915Cb59c1a4C664";
  const USDT = "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7";
  const MIM = "0x130966628846BFd36ff31a822705796e8cb8C18D";

  beforeEach("Setup before each test", async function () {
    owner = await getImpersonatedSigner("0x42d6Ce661bB2e5F5cc639E7BEFE74Ff9Fd649541");
    user = await getImpersonatedSigner("0x9f8c163cBA728e99993ABe7495F06c0A3c8Ac8b9"); // Binance C-Chain Hot Wallet

    const LendingOptimizer = await ethers.getContractFactory("LendingOptimizer", owner);
    optimizer = await upgrades.deployProxy(
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
    await optimizer.deployed();

    optimizer.addCompoundMapping(1, USDC, "0xB715808a78F6041E46d61Cb123C9B4A27056AE9C");
    optimizer.addCompoundMapping(2, USDC, "0xEc5Aa19566Aa442C8C50f3C6734b6Bb23fF21CD7");
    optimizer.addCompoundMapping(3, USDC, "0x29472D511808Ce925F501D25F9Ee9efFd2328db2");
    optimizer.addCompoundMapping(1, USDCE, "0xBEb5d47A3f720Ec0a390d04b4d41ED7d9688bC7F");
    optimizer.addCompoundMapping(2, USDCE, "0xe28965073C49a02923882B8329D3E8C1D805E832");
    optimizer.addCompoundMapping(3, USDCE, "0xEd6AaF91a2B084bd594DBd1245be3691F9f637aC");
    optimizer.addCompoundMapping(1, USDT, "0xd8fcDa6ec4Bdc547C0827B8804e89aCd817d56EF");
    optimizer.addCompoundMapping(2, MIM, "0xbf1430d9eC170b7E97223C7F321782471C587b29");
    optimizer.addCompoundMapping(3, MIM, "0xcE095A9657A02025081E0607c8D8b081c76A75ea");
  });

  it("ERC-20", async function () {
    let amount = 1e6;
    const tokens = [USDC, USDCE, USDT, MIM];
    const tokenString = ["USDC", "USDCE", "USDT", "MIM"];

    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i] == MIM) amount = BigInt(1000000000000000000);
      const token = new ethers.Contract(tokens[i], erc20ABI, user);
      await token.approve(optimizer.address, amount);

      await optimizer.connect(user).supplyToken(tokens[i], amount);
      await optimizer.connect(user).tokenBalance(tokens[i]);

      await optimizer.optimizeToken(tokens[i]);
      await optimizer.connect(user).tokenBalance(tokens[i]);

      await optimizer.connect(user).withdrawToken(tokens[i], 8000);
      await optimizer.connect(user).tokenBalance(tokens[i]);

      console.log(tokenString[i] + " test complete.");
    }
  });

  it("AVAX", async function () {
    const prevBalance = await user.getBalance();
    await optimizer.connect(user).supplyAvax({ value: ethers.utils.parseUnits('1000000000', 'gwei') });
    await optimizer.connect(user).avaxBalance();

    await optimizer.optimizeAvax();
    await optimizer.connect(user).avaxBalance();

    await optimizer.connect(user).withdrawAvax(8000);
    await optimizer.connect(user).avaxBalance();
    const afterBalance = await user.getBalance();
    expect(Math.round((afterBalance - prevBalance) / 1e17) / 10).to.equal(-0.2);
  });

});

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
  let owner;
  let user1, user2;
  let optimizer;

  const USDC = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";
  const USDCE = "0xA7D7079b0FEaD91F3e65f86E8915Cb59c1a4C664";
  const USDT = "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7";
  const MIM = "0x130966628846BFd36ff31a822705796e8cb8C18D";

  beforeEach("Setup before each test", async function () {
    owner = await getImpersonatedSigner("0x42d6Ce661bB2e5F5cc639E7BEFE74Ff9Fd649541");
    user1 = await getImpersonatedSigner("0x9f8c163cBA728e99993ABe7495F06c0A3c8Ac8b9"); // Binance C-Chain Hot Wallet
    user2 = await getImpersonatedSigner("0x4aeFa39caEAdD662aE31ab0CE7c8C2c9c0a013E8");

    const LendingOptimizer = await ethers.getContractFactory("LendingOptimizer", owner);
    optimizer = await upgrades.deployProxy(
      LendingOptimizer,
      [
        "0x794a61358D6845594F94dc1DB02A252b5b4814aD", // _pool
        "0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7", // _wAvax
        "0xa938d8536aEed1Bd48f548380394Ab30Aa11B00E", // _wEthGateway
        "0x5C0401e81Bc07Ca70fAD469b451682c0d747Ef1c", // _qiAvax
        "0xb3c68d69E95B095ab4b33B4cB67dBc0fbF3Edf56", // _iAvax
        "0xC22F01ddc8010Ee05574028528614634684EC29e", // _jAvax
      ], { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await optimizer.deployed();

    optimizer.addCompoundMapping(2, USDC, "0xB715808a78F6041E46d61Cb123C9B4A27056AE9C");
    optimizer.addCompoundMapping(3, USDC, "0xEc5Aa19566Aa442C8C50f3C6734b6Bb23fF21CD7");
    optimizer.addCompoundMapping(4, USDC, "0x29472D511808Ce925F501D25F9Ee9efFd2328db2");
    optimizer.addCompoundMapping(2, USDCE, "0xBEb5d47A3f720Ec0a390d04b4d41ED7d9688bC7F");
    optimizer.addCompoundMapping(3, USDCE, "0xe28965073C49a02923882B8329D3E8C1D805E832");
    optimizer.addCompoundMapping(4, USDCE, "0xEd6AaF91a2B084bd594DBd1245be3691F9f637aC");
    optimizer.addCompoundMapping(2, USDT, "0xd8fcDa6ec4Bdc547C0827B8804e89aCd817d56EF");
    optimizer.addCompoundMapping(3, MIM, "0xbf1430d9eC170b7E97223C7F321782471C587b29");
    optimizer.addCompoundMapping(4, MIM, "0xcE095A9657A02025081E0607c8D8b081c76A75ea");
  });

  it("ERC-20", async function () {
    let amount1 = 1e6, amount2 = 1e7;
    const tokens = [USDC, USDT];
    const tokenString = ["USDC", "USDT"];

    for (let i = 0; i < tokens.length; i++) {
      await (new ethers.Contract(tokens[i], erc20ABI, user1)).approve(optimizer.address, amount1);
      await (new ethers.Contract(tokens[i], erc20ABI, user2)).approve(optimizer.address, amount2);

      await optimizer.connect(user1).supplyToken(tokens[i], amount1);
      await optimizer.connect(user2).supplyToken(tokens[i], amount2);

      console.log("After supply, user 1: " + await optimizer.connect(user1).tokenBalance(tokens[i]));
      console.log("After supply, user 2: " + await optimizer.connect(user2).tokenBalance(tokens[i]));

      await optimizer.optimizeToken(tokens[i]);
      await optimizer.connect(user1).withdrawToken(tokens[i], 8000);
      await optimizer.connect(user2).withdrawToken(tokens[i], 8000);
      console.log("After withdraw, user 1: " + await optimizer.connect(user1).tokenBalance(tokens[i]));
      console.log("After withdraw, user 2: " + await optimizer.connect(user2).tokenBalance(tokens[i]));

      console.log(tokenString[i] + " test complete.\n");
    }
  });

  it("AVAX", async function () {
    await optimizer.connect(user1).supplyAvax({ value: ethers.utils.parseUnits('1000000000', 'gwei') });
    await optimizer.connect(user2).supplyAvax({ value: ethers.utils.parseUnits('1000000000', 'gwei') });

    console.log("After supply, user 1: " + await optimizer.connect(user1).avaxBalance());
    console.log("After supply, user 2: " + await optimizer.connect(user2).avaxBalance());

    await optimizer.optimizeAvax();
    await optimizer.connect(user1).withdrawAvax(8000);
    await optimizer.connect(user2).withdrawAvax(8000);
    console.log("After withdraw, user 1: " + await optimizer.connect(user1).avaxBalance());
    console.log("After withdraw, user 2: " + await optimizer.connect(user2).avaxBalance());
  });

});

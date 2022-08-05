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
  let owner, user1, user2, optimizer;

  const USDC = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";
  const USDT = "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7";

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
        "0xa938d8536aEed1Bd48f548380394Ab30Aa11B00E", // _wEth
        "0x5C0401e81Bc07Ca70fAD469b451682c0d747Ef1c", // _qiAvax
        "0xb3c68d69E95B095ab4b33B4cB67dBc0fbF3Edf56", // _iAvax
        "0xC22F01ddc8010Ee05574028528614634684EC29e", // _jAvax
      ], { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await optimizer.deployed();

    optimizer.addCompoundMapping(2, USDC, "0xB715808a78F6041E46d61Cb123C9B4A27056AE9C");
    optimizer.addCompoundMapping(3, USDC, "0xEc5Aa19566Aa442C8C50f3C6734b6Bb23fF21CD7");
    optimizer.addCompoundMapping(4, USDC, "0x29472D511808Ce925F501D25F9Ee9efFd2328db2");
    optimizer.addCompoundMapping(2, USDT, "0xd8fcDa6ec4Bdc547C0827B8804e89aCd817d56EF");
  });

  it("ERC-20", async function () {
    const tokens = [USDC, USDT];
    const tokenString = ["USDC", "USDT"];

    for (let i = 0; i < tokens.length; i++) {
      await (new ethers.Contract(tokens[i], erc20ABI, user1)).approve(optimizer.address, 1e6);
      await (new ethers.Contract(tokens[i], erc20ABI, user2)).approve(optimizer.address, 1e7);

      await optimizer.connect(user1).supplyToken(tokens[i], 5e5);
      await optimizer.connect(user1).supplyToken(tokens[i], 5e5);
      await optimizer.connect(user2).supplyToken(tokens[i], 5e6);
      await optimizer.connect(user2).supplyToken(tokens[i], 5e6);

      console.log("user 1: " + await optimizer.connect(user1).getBalance(tokens[i]));
      console.log("user 2: " + await optimizer.connect(user2).getBalance(tokens[i]));

      await optimizer.optimizeToken(tokens[i]);

      await optimizer.connect(user1).withdrawToken(tokens[i], 8000);
      await optimizer.connect(user2).withdrawToken(tokens[i], 8000);
      await optimizer.connect(user1).withdrawToken(tokens[i], 1000);
      await optimizer.connect(user2).withdrawToken(tokens[i], 1000);

      console.log("user 1: " + await optimizer.connect(user1).getBalance(tokens[i]));
      console.log("user 2: " + await optimizer.connect(user2).getBalance(tokens[i]));

      console.log(tokenString[i] + " test complete.\n");
    }
  });

  it("AVAX", async function () {
    const WAVAX = "0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7";

    await optimizer.connect(user1).supplyAvax({ value: ethers.utils.parseUnits('500000000', 'gwei') });
    await optimizer.connect(user2).supplyAvax({ value: ethers.utils.parseUnits('500000000', 'gwei') });
    await optimizer.connect(user1).supplyAvax({ value: ethers.utils.parseUnits('500000000', 'gwei') });
    await optimizer.connect(user2).supplyAvax({ value: ethers.utils.parseUnits('500000000', 'gwei') });

    console.log("After supply, user 1: " + await optimizer.connect(user1).getBalance(WAVAX));
    console.log("After supply, user 2: " + await optimizer.connect(user2).getBalance(WAVAX) + "\n");

    await optimizer.optimizeAvax();

    await optimizer.connect(user1).withdrawAvax(8000);
    await optimizer.connect(user2).withdrawAvax(8000);
    await optimizer.connect(user1).withdrawAvax(1000);
    await optimizer.connect(user2).withdrawAvax(1000);

    console.log("After withdraw, user 1: " + await optimizer.connect(user1).getBalance(WAVAX));
    console.log("After withdraw, user 2: " + await optimizer.connect(user2).getBalance(WAVAX));
  });

});

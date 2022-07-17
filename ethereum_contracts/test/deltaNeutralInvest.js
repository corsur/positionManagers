const { DELTA_NEUTRAL } = require("../constants");

const { ethers, upgrades } = require("hardhat");
const { expect } = require("chai");
const { AbiCoder } = require("ethers/lib/utils");

// Ethereum mainnet constants.
const NASDEX_SWAP_FACTORY_ADDR = "0xa07dD2e9fa20C14C45A28978041b4c64e45f7f97";
const NASDEX_SWAP_ROUTER_ADDR = "0x270Ec6bE0C9D67370C2B247D5AF1CC0B7dED0d4a";
const NASDEX_MINT_ADDR = "0xB7957FE76c2fEAe66B57CF3191aFD26d99EC5599";
const NASDEX_LOCK_ADDR = "0xB7957FE76c2fEAe66B57CF3191aFD26d99EC5599";
const NASDEX_LONG_ADDR = "0xB7957FE76c2fEAe66B57CF3191aFD26d99EC5599";
const UNISWAP_SWAP_ROUTER_ADDR = "0x270Ec6bE0C9D67370C2B247D5AF1CC0B7dED0d4a";
const USDC_TOKEN_ADDR = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174";
const nTSLA_TOKEN_ADDR = "0x20796C1C7738992E598B81062b41f2E0b8A8c382";
const nasdex_asset_oracle_usdc_rate = 800;

const _pid = 0;
const _positionId = 0;
const _amountAMin = 0;
const _amountBMin = 0;
const _deadline = 0;

const erc20ABI = [
  // Read-Only Functions
  "function balanceOf(address owner) view returns (uint256)",
  // Authenticated Functions
  "function approve(address spender, uint256 value) returns (bool)",
];
let utf8Encode = new TextEncoder();

async function getImpersonatedSigner() {
  // This is an FTX wallet with ETH/USDC/BUSD balances.
  const accountToImpersonate = "0x9bdB521a97E95177BF252C253E256A60C3e14447";
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}
async function deployDeltaNeutralInvest(signer) {
  const address = await signer.getAddress();
  console.log(
    "Using impersonated wallet address:",
    address,
    await ethers.provider.getBalance(signer.address)
  );

  const DeltaNeutralInvest = await ethers.getContractFactory(
    "DeltaNeutralInvest"
  );

  const deltaNeutralInvest = await DeltaNeutralInvest.deploy(
    USDC_TOKEN_ADDR,
    NASDEX_SWAP_FACTORY_ADDR,
    NASDEX_SWAP_ROUTER_ADDR,
    NASDEX_MINT_ADDR
  );
  await deltaNeutralInvest.deployed();

  console.log("deltaNeutralInvest deployed at:", deltaNeutralInvest.address);

  return deltaNeutralInvest;
}

function getDeltaNeutralOpenRequest() {
  const deltaNeutralParams = {
    target_min_collateral_ratio: 2300,
    target_max_collateral_ratio: 2700,
    nasdex_asset_erc20_addr: nTSLA_TOKEN_ADDR,
  };

  return ethers.utils.defaultAbiCoder.encode(
    ["uint16", "uint16", "address"],
    [2300, 2700, nTSLA_TOKEN_ADDR]
  );
}
describe("Aperture DeltaNeutralInvest unit tests", function () {
  var signer = undefined;
  var deltaNeutralInvest = undefined;

  before("Setup before each test", async function () {
    signer = await getImpersonatedSigner();
  });
  it("testDeltaNeutralInvest", async function () {
    deltaNeutralInvest = await deployDeltaNeutralInvest(signer);

    const usdcAmount = 1 * 1000 * 1e6;
    const usdcContract = new ethers.Contract(USDC_TOKEN_ADDR, erc20ABI, signer);
    const nTslaContract = new ethers.Contract(
      nTSLA_TOKEN_ADDR,
      erc20ABI,
      signer
    );

    await usdcContract.approve(deltaNeutralInvest.address, usdcAmount);
    console.log("nTsla Balance:", usdcContract.balanceOf(signer.address));

    console.log("getDeltaNeutralOpenRequest()", getDeltaNeutralOpenRequest());
    await deltaNeutralInvest
      .connect(signer)
      .deltaNeutralInvest(
        usdcAmount,
        nasdex_asset_oracle_usdc_rate,
        getDeltaNeutralOpenRequest()
      );

    console.log(
      "nTsla Balance:",
      await nTslaContract.balanceOf(deltaNeutralInvest.address)
    );
  });

  it("testStartLongFarm", async function () {
    deltaNeutralInvest = await deployDeltaNeutralInvest(signer);

    const usdcAmount = 1 * 1000 * 1e6;
    const usdcContract = new ethers.Contract(USDC_TOKEN_ADDR, erc20ABI, signer);
    const nTslaContract = new ethers.Contract(
      nTSLA_TOKEN_ADDR,
      erc20ABI,
      signer
    );

    await usdcContract.approve(deltaNeutralInvest.address, usdcAmount);
    console.log("nTsla Balance:", usdcContract.balanceOf(signer.address));

    console.log("getDeltaNeutralOpenRequest()", getDeltaNeutralOpenRequest());
    await deltaNeutralInvest
      .connect(signer)
      .deltaNeutralInvest(
        usdcAmount,
        nasdex_asset_oracle_usdc_rate,
        getDeltaNeutralOpenRequest()
      );

    console.log(
      "nTsla Balance:",
      await nTslaContract.balanceOf(deltaNeutralInvest.address)
    );
    await deltaNeutralInvest
      .connect(signer)
      .startLongFarm(_pid, _positionId, _amountAMin, _amountBMin, _deadline);
  });
});

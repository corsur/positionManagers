const { DELTA_NEUTRAL } = require("../constants");
const {
  CHAIN_ID_TERRA,
  getEmitterAddressTerra,
  hexToUint8Array,
} = require("@certusone/wormhole-sdk");
const { getDeltaNeutralOpenRequest } = require("../utils/helpers");
const { ethers, upgrades } = require("hardhat");
const { expect } = require("chai");

// Ethereum mainnet constants.
const WORMHOLE_UST_TOKEN_ADDR = "0xa693B19d2931d498c5B318dF961919BB4aee87a5";
const WORMHOLE_TOKEN_BRIDGE_ADDR = "0x3ee18B2214AFF97000D974cf647E7C347E8fa585";
const CURVE_WHUST_3CRV_POOL_ADDR = "0xCEAF7747579696A2F0bb206a14210e3c9e6fB269";
const CURVE_BUSD_3CRV_POOL_ADDR = "0x4807862AA8b2bF68830e4C8dc86D0e9A998e085a";
const USDC_TOKEN_ADDR = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const BUSD_TOKEN_ADDR = "0x4Fabb145d64652a948d72533023f6E7A623C7C53";
const CURVE_3CRV_TOKEN_ADDR = "0x6c3F90f043a72FA612cbac8115EE7e52BDe6E490";

// Terra mainnet constants.
const TERRA_MANAGER_ADDR = "terra1ajkmy2c0g84seh66apv9x6xt6kd3ag80jmcvtz";

// Wormhole constants.
const TERRA_CHAIN_ID = 3;

const erc20ABI = [
  // Read-Only Functions
  "function balanceOf(address owner) view returns (uint256)",
  // Authenticated Functions
  "function approve(address spender, uint256 value) returns (bool)",
];

const curvePoolABI = [
  "function exchange_underlying(int128 i, int128 j, uint256 dx, uint256 min_dy) returns (uint256)",
];

const ZERO_ADDRESS = "0x0000000000000000000000000000000000000000";
async function getImpersonatedSigner() {
  // This is an FTX wallet with ETH/USDC/BUSD balances.
  const accountToImpersonate = "0x2FAF487A4414Fe77e2327F0bf4AE2a264a776AD2";
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}
async function deployCurveSwap(signer) {
  const address = await signer.getAddress();
  console.log("Using impersonated wallet address:", address);

  const CurveSwap = await ethers.getContractFactory("CurveSwap", signer);
  const curveSwap = await CurveSwap.deploy();

  await curveSwap.deployed();
  console.log("curveSwap.deployed at:", curveSwap.address);

  return curveSwap;
}
async function deployEthereumManager(signer, curveSwap) {
  const address = await signer.getAddress();
  console.log("Using impersonated wallet address:", address);

  const EthereumManager = await ethers.getContractFactory(
    "EthereumManager",
    signer
  );

  const ethereumManager = await upgrades.deployProxy(
    EthereumManager,
    [
      /*_consistencyLevel=*/ 1,
      WORMHOLE_TOKEN_BRIDGE_ADDR,
      /*_crossChainFeeBPS=*/ 0,
      /*_feeSink=*/ address,
      curveSwap,
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await ethereumManager.deployed();

  console.log("ethereumManager.deployed at:", ethereumManager.address);

  // Register Aperture Terra manager.
  await ethereumManager.updateApertureManager(
    TERRA_CHAIN_ID,
    hexToUint8Array(await getEmitterAddressTerra(TERRA_MANAGER_ADDR))
  );

  // Register strategy params.
  await ethereumManager.updateIsTokenWhitelistedForStrategy(
    TERRA_CHAIN_ID,
    DELTA_NEUTRAL,
    WORMHOLE_UST_TOKEN_ADDR,
    true
  );

  return ethereumManager;
}

async function testSwapAndDeltaNeutralInvest(
  signer,
  ethereumManager,
  curveSwap
) {
  // USDC has 6 decimals, so this is $1M.
  const usdcAmount = 1 * 1e6 * 1e6;
  const usdcContract = new ethers.Contract(USDC_TOKEN_ADDR, erc20ABI, signer);
  await usdcContract.approve(ethereumManager.address, usdcAmount);
  await curveSwap.updateCurveSwapRoute(
    /*fromToken=*/ USDC_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    /*route=*/ [[CURVE_WHUST_3CRV_POOL_ADDR, 2, 0, true]],
    /*tokens=*/ [USDC_TOKEN_ADDR],
    {}
  );

  // BUSD has 18 decimals, so this is $1M.
  const busdAmount = BigInt(1 * 1e6 * 1e18);
  const busdContract = new ethers.Contract(BUSD_TOKEN_ADDR, erc20ABI, signer);
  await busdContract.approve(ethereumManager.address, busdAmount);
  await curveSwap.updateCurveSwapRoute(
    /*fromToken=*/ BUSD_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    /*route=*/ [
      [CURVE_BUSD_3CRV_POOL_ADDR, 0, 1, false],
      [CURVE_WHUST_3CRV_POOL_ADDR, 1, 0, false],
    ],
    /*tokens=*/ [BUSD_TOKEN_ADDR, CURVE_3CRV_TOKEN_ADDR],
    {}
  );
  // Base64 encoding of DN params.
  const encodedPositionOpenData = getDeltaNeutralOpenRequest();
  await ethereumManager.swapTokenAndCreatePosition(
    /*fromToken=*/ USDC_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    usdcAmount,
    /*minAmountOut=*/ 0,
    DELTA_NEUTRAL,
    CHAIN_ID_TERRA,
    encodedPositionOpenData,
    {}
  );
  console.log("swapTokenAndCreatePosition(USDC) completed.");
  await ethereumManager.swapTokenAndCreatePosition(
    /*fromToken=*/ BUSD_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    busdAmount,
    /*minAmountOut=*/ 0,
    DELTA_NEUTRAL,
    CHAIN_ID_TERRA,
    encodedPositionOpenData,
    {}
  );
  console.log("swapTokenAndCreatePosition(BUSD) completed.");
}

async function testUSTDeltaNeutralInvest(signer, ethereumManager) {
  // Exchange $1M worth of USDC for whUST.
  const usdcAmount = BigInt(1 * 1e6 * 1e6);
  const usdcContract = new ethers.Contract(USDC_TOKEN_ADDR, erc20ABI, signer);
  await usdcContract.approve(CURVE_WHUST_3CRV_POOL_ADDR, usdcAmount);
  const curvePoolContract = new ethers.Contract(
    CURVE_WHUST_3CRV_POOL_ADDR,
    curvePoolABI,
    signer
  );
  await curvePoolContract.exchange_underlying(2, 0, usdcAmount, 0);

  // Approve EthereumManager to spend whUST.
  const whUSTContract = new ethers.Contract(
    WORMHOLE_UST_TOKEN_ADDR,
    erc20ABI,
    signer
  );
  const whUSTAmount = await whUSTContract.balanceOf(signer.address);
  await whUSTContract.approve(ethereumManager.address, whUSTAmount);

  // Base64 encoding of DN params.
  const encodedPositionOpenData = getDeltaNeutralOpenRequest();
  await ethereumManager.createPosition(
    CHAIN_ID_TERRA,
    DELTA_NEUTRAL,
    [[WORMHOLE_UST_TOKEN_ADDR, whUSTAmount]],
    encodedPositionOpenData,
    {}
  );
  console.log("createPosition(%d UST) completed.", whUSTAmount);
}

describe("Aperture Ethereum Manager unit tests", function () {
  var signer = undefined;
  var ethereumManager = undefined;
  var curveSwap = undefined;

  before("Setup before each test", async function () {
    signer = await getImpersonatedSigner();
    curveSwap = await deployCurveSwap(signer);
    ethereumManager = await deployEthereumManager(signer, curveSwap.address);
  });

  it("Should add/remove a strategy", async function () {
    await ethereumManager.addStrategy("New Strategy", "V1.0", curveSwap.address);
    expect((await ethereumManager.strategyIdToMetadata(0))[0]).to.equal(
      "New Strategy"
    );
    await ethereumManager.removeStrategy(0);
    expect((await ethereumManager.strategyIdToMetadata(0))[0]).to.equal("");
  });

  it("Should swap USDC/BUSD token and create position", async function () {
    await testSwapAndDeltaNeutralInvest(signer, ethereumManager, curveSwap);
  });

  it("Should deposit whUST to create position", async function () {
    await testUSTDeltaNeutralInvest(signer, ethereumManager);
  });

  it("Should update cross-chain fee", async function () {
    await ethereumManager.updateCrossChainFeeBPS(20);
    expect(await ethereumManager.CROSS_CHAIN_FEE_BPS()).to.equal(20);
  });

  it("Should not be able to set cross-chain fee above 100 bps", async function () {
    ethereumManager
      .updateCrossChainFeeBPS(101)
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'crossChainFeeBPS exceeds maximum allowed value of 100'"
          )
      );
  });

  it("Should update fee sink address", async function () {
    await ethereumManager.updateFeeSink(
      "0x16be88fa89e7ff500a5b6854faea2d9a4b2f7383"
    );
    expect(await ethereumManager.FEE_SINK()).to.equal(
      "0x16be88Fa89e7FF500A5B6854fAea2d9a4B2f7383"
    );
  });

  it("Should update fee sink address", async function () {
    ethereumManager
      .updateFeeSink("0x0000000000000000000000000000000000000000")
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'feeSink address must be non-zero'"
          )
      );
  });

  it("Non-owner should not have access to updateCrossChainFeeBPS", async function () {
    ethereumManager = ethereumManager.connect(
      (await ethers.getSigners())[2] // Use a different wallet.
    );

    return ethereumManager
      .updateCrossChainFeeBPS(2000)
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'Ownable: caller is not the owner'"
          )
      );
  });

  it("Non-owner should not have access to updateFeeSink", async function () {
    ethereumManager = ethereumManager.connect(
      (await ethers.getSigners())[2] // Use a different wallet.
    );

    return ethereumManager
      .updateFeeSink("0x16be88fa89e7ff500a5b6854faea2d9a4b2f7383")
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'Ownable: caller is not the owner'"
          )
      );
  });
});

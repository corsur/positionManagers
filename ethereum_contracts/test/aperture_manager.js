const {
  DELTA_NEUTRAL,
  ALCHEMY_ETHEREUM_MAINNET_URL,
  ETH_MAINNET_TOKEN_BRIDGE_ADDR,
} = require("../constants");
const {
  CHAIN_ID_TERRA,
  getEmitterAddressTerra,
  hexToUint8Array,
} = require("@certusone/wormhole-sdk");
const { getDeltaNeutralOpenRequest } = require("../utils/helpers");
const { ethers, upgrades } = require("hardhat");
const { expect } = require("chai");
const {
  deployApertureManager,
} = require("../utils/deploy");

// Ethereum mainnet constants.
const WORMHOLE_UST_TOKEN_ADDR = "0xa693B19d2931d498c5B318dF961919BB4aee87a5";
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
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
];

const curvePoolABI = [
  "function exchange_underlying(int128 i, int128 j, uint256 dx, uint256 min_dy) returns (uint256)",
];

async function testSwapAndDeltaNeutralInvest(
  signer,
  apertureManager
) {
  // USDC has 6 decimals, so this is $1M.
  const usdcAmount = 1 * 1e6 * 1e6;
  const usdcContract = new ethers.Contract(USDC_TOKEN_ADDR, erc20ABI, signer);
  await usdcContract.approve(apertureManager.address, usdcAmount);
  await apertureManager.updateCurveRoute(
    /*fromToken=*/ USDC_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    /*route=*/[[CURVE_WHUST_3CRV_POOL_ADDR, 2, 0, true]],
    /*tokens=*/[USDC_TOKEN_ADDR],
    {}
  );

  // BUSD has 18 decimals, so this is $1M.
  const busdAmount = BigInt(1 * 1e6 * 1e18);
  const busdContract = new ethers.Contract(BUSD_TOKEN_ADDR, erc20ABI, signer);
  await busdContract.approve(apertureManager.address, busdAmount);
  await apertureManager.updateCurveRoute(
    /*fromToken=*/ BUSD_TOKEN_ADDR,
    /*toToken=*/ WORMHOLE_UST_TOKEN_ADDR,
    /*route=*/[
      [CURVE_BUSD_3CRV_POOL_ADDR, 0, 1, false],
      [CURVE_WHUST_3CRV_POOL_ADDR, 1, 0, false],
    ],
    /*tokens=*/[BUSD_TOKEN_ADDR, CURVE_3CRV_TOKEN_ADDR],
    {}
  );
  // Base64 encoding of DN params.
  const encodedPositionOpenData = getDeltaNeutralOpenRequest();
  await apertureManager.swapTokenAndCreatePosition(
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
  await apertureManager.swapTokenAndCreatePosition(
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

async function testUSTDeltaNeutralInvest(signer, apertureManager) {
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

  // Approve ApertureManager to spend whUST.
  const whUSTContract = new ethers.Contract(
    WORMHOLE_UST_TOKEN_ADDR,
    erc20ABI,
    signer
  );
  const whUSTAmount = await whUSTContract.balanceOf(signer.address);
  await whUSTContract.approve(apertureManager.address, whUSTAmount);

  // Base64 encoding of DN params.
  const encodedPositionOpenData = getDeltaNeutralOpenRequest();
  await apertureManager.createPosition(
    CHAIN_ID_TERRA,
    DELTA_NEUTRAL,
    [[/*assetType=*/ 0, WORMHOLE_UST_TOKEN_ADDR, whUSTAmount]],
    encodedPositionOpenData,
    {}
  );
  console.log("createPosition(%d UST) completed.", whUSTAmount);
}

describe("Aperture manager tests on Ethereum Mainnet Fork", function () {
  var signer = undefined;
  var apertureManager = undefined;

  before("Setup before each test", async function () {
    await network.provider.request({
      method: "hardhat_reset",
      params: [
        {
          forking: {
            jsonRpcUrl: ALCHEMY_ETHEREUM_MAINNET_URL,
            blockNumber: 14247160
          },
        },
      ],
    });

    // This is an FTX wallet with ETH/USDC/BUSD balances.
    signer = await ethers.getImpersonatedSigner("0x2FAF487A4414Fe77e2327F0bf4AE2a264a776AD2");
    apertureManager = await deployApertureManager(signer, ETH_MAINNET_TOKEN_BRIDGE_ADDR);

    // Register Aperture Terra manager.
    await apertureManager.updateApertureManager(
      TERRA_CHAIN_ID,
      hexToUint8Array(await getEmitterAddressTerra(TERRA_MANAGER_ADDR))
    );

    // Register strategy params.
    await apertureManager.updateIsTokenWhitelistedForStrategy(
      TERRA_CHAIN_ID,
      DELTA_NEUTRAL,
      WORMHOLE_UST_TOKEN_ADDR,
      true
    );
  });

  it("Should add/remove a strategy", async function () {
    await apertureManager.addStrategy("New Strategy", "V1.0", signer.address);
    expect((await apertureManager.strategyIdToMetadata(0))[0]).to.equal(
      "New Strategy"
    );
    await apertureManager.removeStrategy(0);
    expect((await apertureManager.strategyIdToMetadata(0))[0]).to.equal("");
  });

  it("Should swap USDC/BUSD token and create position", async function () {
    await testSwapAndDeltaNeutralInvest(signer, apertureManager);
  });

  it("Should deposit whUST to create position", async function () {
    await testUSTDeltaNeutralInvest(signer, apertureManager);
  });

  it("Should update cross-chain fee context", async function () {
    await apertureManager.updateCrossChainFeeContext([/*feeBps=*/20, /*feeSink=*/signer.address]);
    expect((await apertureManager.crossChainContext())[1][0]).to.equal(20);
    expect((await apertureManager.crossChainContext())[1][1]).to.equal(signer.address);
  });

  it("Should not be able to set cross-chain fee above 100 bps", async function () {
    await expect(apertureManager.updateCrossChainFeeContext([/*feeBps=*/101, /*feeSink=*/signer.address])).to.be.revertedWith("feeBps too large");
  });

  it("Should not be able to update fee sink address to null", async function () {
    await expect(apertureManager.updateCrossChainFeeContext([/*feeBps=*/10, /*feeSink=*/"0x0000000000000000000000000000000000000000"]))
      .to.be.revertedWith("feeSink can't be null");
  });

  it("Non-owner should not have access to updateCrossChainFeeContext", async function () {
    await expect(apertureManager.connect(
      (await ethers.getSigners())[2] // Use a different wallet.
    ).updateCrossChainFeeContext([/*feeBps=*/20, /*feeSink=*/signer.address]))
      .to.be.revertedWith("Ownable: caller is not the owner");
  });
});

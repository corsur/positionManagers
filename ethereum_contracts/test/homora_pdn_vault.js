const { CHAIN_ID_AVAX } = require("@certusone/wormhole-sdk");
const { expect, assert } = require("chai");
const { BigNumber } = require("ethers");
const { ethers, upgrades } = require("hardhat");
const {
  AVAX_MAINNET_TOKEN_BRIDGE_ADDR,
  AVAX_MAINNET_URL,
} = require("../constants.js");
const {
  deployApertureManager,
  deployHomoraAdapter,
} = require("../utils/deploy.js");

const { homoraBankABI } = require("./abi/homoraBankABI.js");

const ERC20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
  "function transfer(address _to, uint256 value) returns(bool)",
];

const JOEABI = [
  "function swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] calldata path, address to, uint256 deadline) returns (uint256[] memory amounts)",
];

const {
  HOMORA_BANK_ADDRESS,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  USDC_TOKEN_ADDRESS,
  JOE_TOKEN_ADDRESS,
  JOE_ROUTER_ADDRESS,
  WAVAX_USDC_POOL_ID,
  AVAX_CHAIN_ID,
} = require("./avax_constants");

const provider = ethers.provider;
const WAVAX = new ethers.Contract(WAVAX_TOKEN_ADDRESS, ERC20ABI, provider);
const USDC = new ethers.Contract(USDC_TOKEN_ADDRESS, ERC20ABI, provider);
const JOE = new ethers.Contract(JOE_TOKEN_ADDRESS, ERC20ABI, provider);
const router = new ethers.Contract(JOE_ROUTER_ADDRESS, JOEABI, provider);
const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);
const leverageLevel = 3;
const usdcDepositAmount0 = 1000e6;
const avaxDepositAmount0 = 0;
const minEquityReceived0 = 0;
const usdcDepositAmount1 = 500e6;
const avaxDepositAmount1 = 0;
const minEquityReceived1 = 0;

const mainWallet = new ethers.Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);

const wallets = [
  new ethers.Wallet(
    "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    provider
  ),
  new ethers.Wallet(
    "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
    provider
  ),
  new ethers.Wallet(
    "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
    provider
  ),
  new ethers.Wallet(
    "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
    provider
  ),
];

const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };
const ZERO_ADDR = "0x0000000000000000000000000000000000000000";

async function getImpersonatedSigner(accountToImpersonate) {
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}

async function whitelistContractAndAddCredit(contractAddressToWhitelist) {
  // Get impersonatedSigner as governor of homoraBank contract.
  const homoraBankGovernor = await homoraBank.governor(txOptions);
  const signer = await getImpersonatedSigner(homoraBankGovernor);

  // Transfer AVAX to the governor and check.
  await mainWallet.sendTransaction({
    to: homoraBankGovernor,
    value: ethers.utils.parseEther("100"),
  });
  // expect(await provider.getBalance(signer.address)).to.equal(BigInt(1e20));

  // Whitelist address and check.
  await homoraBank
    .connect(signer)
    .setWhitelistUsers([contractAddressToWhitelist], [true], txOptions);
  let res = await homoraBank.whitelistedUsers(
    contractAddressToWhitelist,
    txOptions
  );
  expect(res).to.equal(true);

  // Set credit to 100,000 USDC and 5,000 WAVAX.
  await homoraBank.connect(signer).setCreditLimits(
    [
      [contractAddressToWhitelist, USDC_TOKEN_ADDRESS, 1e11, ZERO_ADDR],
      [
        contractAddressToWhitelist,
        WAVAX_TOKEN_ADDRESS,
        ethers.BigNumber.from("1000000000000000000000"),
        ZERO_ADDR,
      ],
    ],
    txOptions
  );
}

async function initialize() {
  // Impersonate USDC holder.
  let signer = await getImpersonatedSigner("0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541");
  await USDC.connect(signer).transfer(wallets[0].address, 5e6 * 1e6, txOptions);
  await USDC.connect(signer).transfer(wallets[1].address, 1000 * 1e6, txOptions);
}

// testing function to swap USDC into WAVAX
async function swapUSDC(contract, swapAmt = 1e6 * 1e6) {
  await USDC.connect(wallets[0]).approve(
    contract.address,
    1e8 * 1e6,
    txOptions
  );

  let wavaxBalance0 = await WAVAX.balanceOf(wallets[0].address);

  // console.log("Token price before swap");
  // await contract.connect(wallets[0]).queryTokenPrice(txOptions);
  await contract
    .connect(wallets[0])
    .swapExactTokensForTokens(
      swapAmt,
      0,
      [USDC_TOKEN_ADDRESS, WAVAX_TOKEN_ADDRESS],
      wallets[0].address,
      10 ** 12,
      txOptions
    );
  // console.log("Token price after swap");
  // await contract.connect(wallets[0]).queryTokenPrice(txOptions);

  let wavaxBalance1 = await WAVAX.balanceOf(wallets[0].address);

  return wavaxBalance1.sub(wavaxBalance0);
}

// testing function to swap WAVAX into USDC
async function swapWAVAX(contract, swapAmt) {
  await WAVAX.connect(wallets[0]).approve(
    contract.address,
    BigNumber.from(500000).mul("1000000000000000000"),
    txOptions
  );

  let usdcBalance0 = await USDC.balanceOf(wallets[0].address);

  // console.log("Token price before swap");
  // await contract.connect(wallets[0]).queryTokenPrice(txOptions);
  await contract
    .connect(wallets[0])
    .swapExactTokensForTokens(
      swapAmt,
      0,
      [WAVAX_TOKEN_ADDRESS, USDC_TOKEN_ADDRESS],
      wallets[0].address,
      10 ** 12,
      txOptions
    );
  // console.log("Token price after swap");
  // await contract.connect(wallets[0]).queryTokenPrice(txOptions);

  let usdcBalance1 = await USDC.balanceOf(wallets[0].address);

  return usdcBalance1.sub(usdcBalance0);
}

// testing swap functions
async function testSwap(strategyContract) {
  // Impersonate WAVAX holder and transfer.
  signer = await getImpersonatedSigner("0x0e082F06FF657D94310cB8cE8B0D9a04541d8052");
  await WAVAX.connect(signer).transfer(strategyContract.address, BigNumber.from(1).mul("1000000000000000000"), txOptions);

  // Impersonate USDC holder and transfer.
  signer = await getImpersonatedSigner("0x279f8940ca2a44c35ca3edf7d28945254d0f0ae6");
  await USDC.connect(signer).transfer(strategyContract.address, BigNumber.from(200000).mul(1e6), txOptions);

  let swapAmt = BigNumber.from(1);
  let recvAmt = await strategyContract
      .connect(wallets[0])
      .swap(swapAmt, USDC_TOKEN_ADDRESS, WAVAX_TOKEN_ADDRESS, txOptions);
  console.log("swap %s USDC to %d AVAX", swapAmt.toString(), recvAmt);

  recvAmt = await strategyContract
      .connect(wallets[0])
      .swapAVAX(swapAmt, USDC_TOKEN_ADDRESS, txOptions);
  console.log("swap %s AVAX to %d USDC", swapAmt.toString(), recvAmt);
}

// testing function for rebalance()
async function testRebalance(managerContract, strategyContract) {
  await deposit(managerContract, strategyContract);
  // check collateral
  let collSize = await strategyContract
    .connect(wallets[0])
    .getCollateralSize(txOptions);
  let [usdcHold, wavaxHold] = await strategyContract
    .connect(wallets[0])
    .convertCollateralToTokens(collSize, txOptions);
  console.log("collateral: usdc: %d, wavax: %d", usdcHold, wavaxHold);

  // check debt
  let [usdcDebt, wavaxDebt] = await strategyContract
    .connect(wallets[0])
    .getDebtAmounts(txOptions);
  console.log("current debt: usdc: %d, wavax: %d", usdcDebt, wavaxDebt);

  // check if position state is healthy (no need to rebalance)
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");

  // Flash swap USDC and rebalance (short)
  let swapAmt = BigNumber.from(3e6).mul(1e6);
  let recvAmt = await swapUSDC(router, swapAmt);
  console.log(
    "Swap %s USDC to %s AVAX",
    swapAmt.div(1e6).toString(),
    recvAmt.div("1000000000000000000").toString()
  );

  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("Slippage_Too_Large");
  // Swap back
  swapAmt = recvAmt;
  recvAmt = await swapWAVAX(router, swapAmt);
  console.log(
    "Swap %s AVAX to %s USDC",
    swapAmt.div("1000000000000000000").toString(),
    recvAmt.div(1e6).toString()
  );

  // Decrease leverage to trigger rebalance
  await strategyContract.connect(mainWallet).setConfig(
    2, // _leverageLevel
    7154, // _targetDebtRatio
    350, // _debtRatioWidth
    490, // _dnThreshold
    txOptions
  );
  console.log("Leverage changed to 2");
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("Slippage_Too_Large");

  // Increase slippage and rebalance again
  await strategyContract.connect(wallets[0])
      .rebalance(100, 0, txOptions);

  // expect to be in delta-neutral after rebalance
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");

  // Impersonate WAVAX holder.
  signer = await getImpersonatedSigner(
    "0x0e082F06FF657D94310cB8cE8B0D9a04541d8052"
  );
  await WAVAX.connect(signer).transfer(
    wallets[0].address,
    BigNumber.from(200000).mul("1000000000000000000"),
    txOptions
  );

  // Flash swap WAVAX and rebalance (long)
  recvAmt = await swapWAVAX(router, swapAmt);
  console.log(
    "Swap %s AVAX to %s USDC",
    swapAmt.div("1000000000000000000").toString(),
    recvAmt.div(1e6).toString()
  );
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("Slippage_Too_Large");

  // Swap back
  swapAmt = recvAmt;
  recvAmt = await swapUSDC(router, swapAmt);
  console.log(
    "Swap %s USDC to %s AVAX",
    swapAmt.div(1e6).toString(),
    recvAmt.div("1000000000000000000").toString()
  );
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");

  // Increase leverage to trigger rebalance
  await strategyContract.connect(mainWallet).setConfig(
    3, // _leverageLevel
    9231, // _targetDebtRatio
    100, // _debtRatioWidth
    300, // _dnThreshold
    txOptions
  );
  console.log("Leverage changed to 3");
  await strategyContract.connect(wallets[0])
      .rebalance(100, 0, txOptions);

  // expect to be in delta-neutral after rebalance
  await expect(
    strategyContract.connect(wallets[0])
        .rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");
}

async function testReinvest(managerContract, strategyContract) {
  await deposit(managerContract, strategyContract);

  let collateralBefore = await strategyContract
    .connect(wallets[0])
    .getCollateralSize(txOptions);
  console.log("Collateral Before reinvest: %d", collateralBefore);

  let reinvested = false;
  try {
    await strategyContract.connect(wallets[0]).reinvest(0, txOptions);
    reinvested = true;
  } catch (err) {
    await expect(
      strategyContract.connect(wallets[0]).reinvest(0, txOptions)
    ).to.be.revertedWith("Insufficient_Liquidity_Mint");
    reinvested = false;
  }

  let collateralAfter = await strategyContract
    .connect(wallets[0])
    .getCollateralSize(txOptions);

  console.log("Collateral before reinvest: %d", collateralBefore);
  console.log("Collateral after reinvest: %d", collateralAfter);
  if (reinvested) {
    expect(collateralAfter > collateralBefore).to.equal(true);
  } else {
    expect(collateralAfter == collateralBefore).to.equal(true);
  }
}

async function deposit(managerContract, strategyContract) {
  // Deposit 1000 USDC to vault from wallet 0.
  await USDC.connect(wallets[0]).approve(
    managerContract.address,
    usdcDepositAmount0
  );
  console.log("using wallet: ", wallets[0].address);

  // Craft open position data.
  let openPositionBytesArray = ethers.utils.arrayify(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "uint256", "uint256", "uint256"],
      [
        usdcDepositAmount0, // uint256 stableTokenDepositAmount
        avaxDepositAmount0, // uint256 assetTokenDepositAmount
        minEquityReceived0, // uint256 minEquityETH
        0 // uint256 minReinvestETH
      ]
    )
  );

  // Deposit 1000 USDC to vault from wallet 0.
  await managerContract
    .connect(wallets[0])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[/*assetType=*/ 0, USDC_TOKEN_ADDRESS, usdcDepositAmount0]],
      openPositionBytesArray,
      txOptions
    );
  console.log(`created position with ${usdcDepositAmount0} USDC`);

  // Deposit 500 USDC to vault from wallet 1.
  console.log("using wallet: ", wallets[1].address);
  await USDC.connect(wallets[1]).approve(
    managerContract.address,
    usdcDepositAmount1
  );
  let openPositionBytesArray1 = ethers.utils.arrayify(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "uint256", "uint256", "uint256"],
      [usdcDepositAmount1, avaxDepositAmount1, minEquityReceived1, 0]
    )
  );
  await managerContract
    .connect(wallets[1])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[/*assetType=*/ 0, USDC_TOKEN_ADDRESS, usdcDepositAmount1]],
      openPositionBytesArray1,
      txOptions
    );
  console.log(`created position with ${usdcDepositAmount1} USDC`);
}

async function testDepositAndWithdraw(
  managerContract,
  strategyContract,
  homoraAdapter
) {
  await deposit(managerContract, strategyContract);

  // Check whether it initiates a position in HomoraBank.
  var homoraBankPosId = (await strategyContract.homoraBankPosId()).toNumber();
  expect(homoraBankPosId).not.to.equal(0);

  // Check whether the adapter contract is the owner of the HomoraBank position.
  var res = await homoraBank.getPositionInfo(homoraBankPosId);
  expect(res.owner).to.equal(homoraAdapter.address);

  // // Colletral size of each wallet.
  var totalCollateralSize = res.collateralSize;
  var totalShareAmount = await strategyContract.totalShareAmount();
  var shareAmount0 = await strategyContract.positions(CHAIN_ID_AVAX, 0); // position id 0
  console.log("share amount 0: ", shareAmount0.toString());
  var shareAmount1 = await strategyContract.positions(CHAIN_ID_AVAX, 1); // position id 1
  console.log("share amount 1: ", shareAmount1.toString());
  var collSize0 = shareAmount0.mul(totalCollateralSize).div(totalShareAmount);
  var collSize1 = shareAmount1.mul(totalCollateralSize).div(totalShareAmount);

  [usdcAmount0, wavaxAmount0] =
    await strategyContract.convertCollateralToTokens(collSize0);
  [usdcAmount1, wavaxAmount1] =
    await strategyContract.convertCollateralToTokens(collSize1);

  var totalAmount0InUsdc = 2 * usdcAmount0;
  var totalAmount1InUsdc = 2 * usdcAmount1;

  expect(totalAmount0InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount0 * leverageLevel),
    100
  );
  expect(totalAmount1InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount1 * leverageLevel),
    100
  );

  // Withdraw half amount from vault for wallet 0.
  var withdrawAmount0 = shareAmount0.div(2);
  console.log("withdraw amount 0: ", withdrawAmount0.toString());
  var usdcBalance0 = await USDC.balanceOf(wallets[0].address);

  // First byte is Action enum.
  // 0 -> Open, 1 -> Increase, 2 -> Decrease, 3 -> Close (not yet supported).
  const encodedWithdrawData = ethers.utils.concat([
    new Uint8Array([2]),
    ethers.utils.arrayify(
      ethers.utils.defaultAbiCoder.encode(
        ["address", "uint256", "uint256", "uint256", "uint256"],
        [wallets[0].address, withdrawAmount0, 0, 0, 0]
      )
    ),
  ]);
  // console.log("encoded withdraw data: ", encodedWithdrawData);
  console.log(ethers.utils.hexlify(encodedWithdrawData));

  await managerContract
    .connect(wallets[0])
    .executeStrategy(
      /*positionId=*/ 0,
      /*assetInfos=*/[],
      encodedWithdrawData,
      txOptions
    );

  // var withdrawUsdcAmount0 =
  //   (await USDC.balanceOf(wallets[0].address)) - usdcBalance0;
  // expect(withdrawUsdcAmount0).to.be.closeTo(
  //   BigNumber.from(usdcDepositAmount0).div(2),
  //   100
  // );
}

describe.only("HomoraPDNVault Initialization", function () {
  var managerContract = undefined;
  var homoraAdapter = undefined;
  var strategyFactory = undefined;
  var strategyContract = undefined;

  beforeEach("Setup before each test", async function () {
    await network.provider.request({
      method: "hardhat_reset",
      params: [
        {
          forking: {
            jsonRpcUrl: AVAX_MAINNET_URL,
            blockNumber: 16681756,
          },
        },
      ],
    });

    // Aperture manager contract.
    managerContract = await deployApertureManager(
      mainWallet,
      AVAX_MAINNET_TOKEN_BRIDGE_ADDR
    );
    console.log("Aperture manager deployed at: ", managerContract.address);

    // Deploy Homora adapter contract.
    homoraAdapter = await deployHomoraAdapter(mainWallet);

    // HomoraPDNVault contract.
    library = await ethers.getContractFactory("VaultLib");
    vaultLib = await library.deploy();
    library = await ethers.getContractFactory("OracleLib");
    oracleLib = await library.deploy();
    strategyFactory = await ethers.getContractFactory("HomoraPDNVault", {
      libraries: {
        VaultLib: vaultLib.address,
        OracleLib: oracleLib.address
      },
    });
    strategyContract = await upgrades.deployProxy(
      strategyFactory,
      [
        managerContract.address,
        homoraAdapter.address,
        wallets[0].address,
        wallets[0].address,
        USDC_TOKEN_ADDRESS,
        WAVAX_TOKEN_ADDRESS,
        HOMORA_BANK_ADDRESS,
        TJ_SPELLV3_WAVAX_USDC_ADDRESS,
        JOE_TOKEN_ADDRESS,
        WAVAX_USDC_POOL_ID,
      ],
      { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await strategyContract.connect(mainWallet).deployed(txOptions);

    // Set up Homora adapter contract.
    await homoraAdapter.setCaller(strategyContract.address, true);
    await homoraAdapter.setTarget(strategyContract.address, true);
    await homoraAdapter.setTarget(homoraBank.address, true);
    await homoraAdapter.setTarget(USDC.address, true);
    await homoraAdapter.setTarget(WAVAX.address, true);
    await homoraAdapter.setTarget(JOE.address, true);

    console.log("Homora PDN contract deployed at: ", strategyContract.address);
    await strategyContract.connect(mainWallet).initializeConfig(
      3, // _leverageLevel
      9231, // _targetDebtRatio
      100, // _debtRatioWidth
      300, // _deltaThreshold
      [
          20, // withdrawFee
          1500, // harvestFee
          200 // managementFee
      ],
      [
          BigNumber.from(1000000).mul(1e6), // maxCapacity
          BigNumber.from(200000).mul(1e6), // maxOpenPerTx
          BigNumber.from(200000).mul(1e6) // maxWithdrawPerTx
      ],
      txOptions
    );
    console.log("Homora PDN contract initialized");

    await whitelistContractAndAddCredit(homoraAdapter.address);
    await initialize(strategyContract);

    // Add strategy into Aperture manager.
    await managerContract.addStrategy(
      "Homora Delta-neutral",
      "1.0.0",
      strategyContract.address
    );
    console.log(
      "Added strategy: ",
      await managerContract.strategyIdToMetadata(0)
    );

    // Whitelist tokens for the strategy.
    await managerContract.updateIsTokenWhitelistedForStrategy(
      AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      USDC_TOKEN_ADDRESS,
      /*isWhitelisted=*/ true
    );
  });

  it("Test swap functions", async function () {
    await testSwap(strategyContract);
  });

  it("HomoraPDNVault DepositAndWithdraw", async function () {
    await testDepositAndWithdraw(
      managerContract,
      strategyContract,
      homoraAdapter
    );
  });

  it("Deposit and test rebalance", async function () {
    await testRebalance(managerContract, strategyContract);
  });

  it("Deposit and test reinvest", async function () {
    await testReinvest(managerContract, strategyContract);
  });

  it("Should fail for doing unauthorized operations", async function () {
    await expect(
      strategyContract.connect(wallets[0]).setConfig(0, 0, 0, 0, txOptions)
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });
});

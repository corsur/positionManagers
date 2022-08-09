const { CHAIN_ID_AVAX } = require("@certusone/wormhole-sdk");
const { expect, assert } = require("chai");
const { BigNumber } = require("ethers");
const { ethers, upgrades } = require("hardhat");

const { homoraBankABI } = require("./abi/homoraBankABI.js");

const ERC20ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
  "function transfer(address _to, uint256 value) returns(bool)",
];

const {
  HOMORA_BANK_ADDRESS,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  USDC_TOKEN_ADDRESS,
  JOE_TOKEN_ADDRESS,
  WAVAX_USDC_POOL_ID,
  AVAX_CHAIN_ID,
} = require("./avax_constants");

const provider = ethers.provider;
const WAVAX = new ethers.Contract(WAVAX_TOKEN_ADDRESS, ERC20ABI, provider);
const USDC = new ethers.Contract(USDC_TOKEN_ADDRESS, ERC20ABI, provider);
const JOE = new ethers.Contract(JOE_TOKEN_ADDRESS, ERC20ABI, provider);
const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);
const leverageLevel = 3;

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
  const signer = await getImpersonatedSigner(
    "0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541"
  );

  await USDC.connect(signer).transfer(
    wallets[0].address,
    1000 * 1e6,
    txOptions
  );
  await USDC.connect(signer).transfer(
    wallets[1].address,
    1000 * 1e6,
    txOptions
  );
}

// testing function for rebalance()
async function testRebalance(contract) {
  const usdcDepositAmt = 300 * 1e6;
  await USDC.connect(wallets[0]).approve(
    contract.address,
    usdcDepositAmt * 10,
    txOptions
  );

  // deposit 300 USDC
  await contract.connect(wallets[0]).deposit(usdcDepositAmt, 0, txOptions);

  let usdcExpect = (300 * 1e6 * 3) / 2;
  let wavaxExpect = await contract
    .connect(wallets[0])
    .getEquivalentTokenB(usdcExpect, txOptions);

  // check collateral
  let collSize = await contract
    .connect(wallets[0])
    .getCollateralSize(txOptions);
  let [usdcHold, wavaxHold] = await contract
    .connect(wallets[0])
    .convertCollateralToTokens(collSize, txOptions);
  console.log("collateral: usdc: %d, wavax: %d", usdcHold, wavaxHold);
  console.log("    expect: usdc: %d, wavax: %d", usdcExpect, wavaxExpect);
  assert(
    Math.abs(usdcExpect - usdcHold) / usdcExpect < 1e-6,
    "collateral USDC not equal to the expected amount"
  );
  assert(
    Math.abs(wavaxExpect - wavaxHold) / wavaxExpect < 1e-6,
    "collateral WAVAX not equal to the expected amount"
  );

  // check debt
  let [usdcDebt, wavaxDebt] = await contract
    .connect(wallets[0])
    .currentDebtAmount(txOptions);
  let usdcDebtExpect = (300 * 1e6) / 2;
  let wavaxDebtExpect = wavaxExpect;
  console.log("current debt: usdc: %d, wavax: %d", usdcDebt, wavaxDebt);
  console.log(
    " expect debt: usdc: %d, wavax: %d",
    usdcDebtExpect,
    wavaxDebtExpect
  );
  assert(
    Math.abs(usdcDebtExpect - usdcDebt) / usdcDebtExpect < 1e-6,
    "USDC debt not equal to the expected amount"
  );
  assert(
    Math.abs(wavaxDebtExpect - wavaxDebt) / wavaxDebtExpect < 1e-6,
    "WAVAX debt not equal to the expected amount"
  );

  // check if position state is healthy (no need to rebalance)
  await expect(
    contract.connect(wallets[0]).rebalance(txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");

  // set delta-neutral threshold to 0 to force executing rebalance
  console.log("Temporarily set delta-neutral offset threshold to 0");
  await contract.connect(wallets[0]).setDNThreshold(0, txOptions);
  await contract.connect(wallets[0]).rebalance(txOptions);

  await contract.connect(wallets[0]).setDNThreshold(500, txOptions);
  // check if position state is healthy after rebalance
  await expect(
    contract.connect(wallets[0]).rebalance(txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");
}

async function testReinvest(contract) {
  const usdcDepositAmt = 300 * 1e6;
  await USDC.connect(wallets[0]).approve(
    contract.address,
    usdcDepositAmt * 10,
    txOptions
  );

  // deposit 300 USDC
  await contract.connect(wallets[0]).deposit(usdcDepositAmt, 0, txOptions);

  let collateralBefore = await contract
    .connect(wallets[0])
    .getCollateralSize(txOptions);

  let reinvested = false;
  try {
    await contract.connect(wallets[0]).reinvest(txOptions);
    reinvested = true;
  } catch (err) {
    await expect(
      contract.connect(wallets[0]).reinvest(txOptions)
    ).to.be.revertedWith("Insufficient liquidity minted");
    reinvested = false;
  }

  let collateralAfter = await contract
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

async function testNativeTokenSupport(managerContract, strategyContract) {
  const usdcDepositAmount0 = 1000e6;
  const avaxAmount = BigNumber.from("10000000000000000000");

   // Deposit 1000 USDC to vault from wallet 0.
   await USDC.connect(wallets[0]).approve(
    managerContract.address,
    usdcDepositAmount0
  );
  console.log("using wallet: ", wallets[0].address);

  // Craft open position data.
  const buffer = new ArrayBuffer(32 * 2); // two uint256.
  const view = new DataView(buffer);
  // This is an encoding hack.
  view.setUint32(28, usdcDepositAmount0);
  const openPositionBytesArray = new Uint8Array(buffer);
  console.log("print out uint8 array: ", openPositionBytesArray);
  
  txOptions["value"] = avaxAmount;

  // Deposit 1000 USDC to vault from wallet 0.
  await managerContract
    .connect(wallets[0])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[/*assetType=*/ 0, USDC_TOKEN_ADDRESS, usdcDepositAmount0], [/*assetType=*/ 1, ZERO_ADDR, avaxAmount]],
      openPositionBytesArray,
      txOptions
    );
      
    // Manager contract is supposed not to keep any native tokens.
    expect(await provider.getBalance(managerContract.address)).to.equal(0);
    // Strategy contract is designed to receive native tokens but not handle them.
    expect(await provider.getBalance(strategyContract.address)).to.equal(avaxAmount);
}

async function testDepositAndWithdraw(managerContract, strategyContract) {
  const usdcDepositAmount0 = 1000e6;
  const usdcDepositAmount1 = 500e6;

  // Deposit 1000 USDC to vault from wallet 0.
  await USDC.connect(wallets[0]).approve(
    managerContract.address,
    usdcDepositAmount0
  );
  console.log("using wallet: ", wallets[0].address);

  // Craft open position data.
  const buffer = new ArrayBuffer(32 * 2); // two uint256.
  const view = new DataView(buffer);
  // This is an encoding hack.
  view.setUint32(28, usdcDepositAmount0);
  const openPositionBytesArray = new Uint8Array(buffer);
  console.log("print out uint8 array: ", openPositionBytesArray);

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

  // Deposit 500 USDC to vault from wallet 1.
  await USDC.connect(wallets[1]).approve(
    managerContract.address,
    usdcDepositAmount1
  );
  view.setUint32(28, usdcDepositAmount1);
  const openPositionBytesArray1 = new Uint8Array(buffer);
  console.log("print out uint8 array: ", openPositionBytesArray1);
  await managerContract
    .connect(wallets[1])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[/*assetType=*/ 0, USDC_TOKEN_ADDRESS, usdcDepositAmount1]],
      openPositionBytesArray1,
      txOptions
    );

  // Check whether it initiates a position in HomoraBank.
  var homoraBankPosId = (await strategyContract.homoraBankPosId()).toNumber();
  expect(homoraBankPosId).not.to.equal(0);

  // Check whether the vault contract is the owner of the HomoraBank position.
  var res = await homoraBank.getPositionInfo(homoraBankPosId);
  expect(res.owner).to.equal(strategyContract.address);

  // // Colletral size of each wallet.
  var totalCollateralSize = res.collateralSize;
  var totalShareAmount = await strategyContract.totalCollShareAmount();
  var shareAmount0 = await strategyContract.positions(CHAIN_ID_AVAX, 0); // position id 0
  console.log("share amount 0: ", shareAmount0);
  var shareAmount1 = await strategyContract.positions(CHAIN_ID_AVAX, 1); // position id 1
  console.log("share amount 1: ", shareAmount1);
  var collSize0 = shareAmount0.mul(totalCollateralSize).div(totalShareAmount);
  var collSize1 = shareAmount1.mul(totalCollateralSize).div(totalShareAmount);

  [usdcAmount0, wavaxAmount0] =
    await strategyContract.convertCollateralToTokens(collSize0);
  [usdcAmount1, wavaxAmount1] =
    await strategyContract.convertCollateralToTokens(collSize1);

  var totalAmount0InUsdc = usdcAmount0.add(
    await strategyContract.getEquivalentTokenA(wavaxAmount0)
  );
  var totalAmount1InUsdc = usdcAmount1.add(
    await strategyContract.getEquivalentTokenA(wavaxAmount1)
  );

  expect(totalAmount0InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount0 * leverageLevel),
    100
  );
  expect(totalAmount1InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount1 * leverageLevel),
    100
  );

  console.log(
    ethers.utils.arrayify("0x689961608D2d7047F5411F9d9004D440449CbD27")
  );

  // Withdraw half amount from vault for wallet 0.
  var withdrawAmount0 = shareAmount0.div(2);
  console.log("withdraw amount 0: ", withdrawAmount0.toNumber());
  var usdcBalance0 = await USDC.balanceOf(wallets[0].address);

  // First byte is Action enum.
  // 0 -> Open, 1 -> Increase, 2 -> Decrease, 3 -> Close (not yet supported).
  const withdrawBuffer = new ArrayBuffer(1 + 32);
  const withdrawView = new DataView(withdrawBuffer);
  withdrawView.setUint8(0, 2); // Set action to be Decrease.
  withdrawView.setUint32(29, withdrawAmount0.toNumber()); // Hack: set the last 4 bytes to be withdraw amount.
  const encodedWithdrawData = ethers.utils.concat([
    new Uint8Array(withdrawBuffer),
    ethers.utils.zeroPad(ethers.utils.arrayify(wallets[0].address), 32),
  ]);

  console.log("encoded withdraw data: ", encodedWithdrawData);

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

describe("HomoraPDNVault Initialization", function () {
  var managerFactory = undefined;
  var managerContract = undefined;
  var strategyFactory = undefined;
  var strategyContract = undefined;

  beforeEach("Setup before each test", async function () {
    // Aperture manager contract.
    managerFactory = await ethers.getContractFactory("EthereumManager");
    managerContract = await upgrades.deployProxy(
      managerFactory,
      [
        /*_consistencyLevel=*/ 1,
        /*_wormholeTokenBridge=*/ "0x0e082F06FF657D94310cB8cE8B0D9a04541d8052",
        /*_crossChainFeeBPS=*/ 0,
        /*_feeSink=*/ mainWallet.address,
        /*_curveSwap unused=*/ mainWallet.address,
      ],
      { unsafeAllow: ["delegatecall"], kind: "uups" }
    );
    await managerContract.connect(mainWallet).deployed(txOptions);

    console.log("Aperture manager deployed at: ", managerContract.address);

    // HomoraPDNVault contract.
    strategyFactory = await ethers.getContractFactory("HomoraPDNVault");
    strategyContract = await strategyFactory
      .connect(mainWallet)
      .deploy(
        wallets[0].address,
        managerContract.address,
        "WAVAX-USDC TraderJoe",
        "L3x-WAVAXUSDC-TJ1",
        USDC_TOKEN_ADDRESS,
        WAVAX_TOKEN_ADDRESS,
        3,
        HOMORA_BANK_ADDRESS,
        TJ_SPELLV3_WAVAX_USDC_ADDRESS,
        JOE_TOKEN_ADDRESS,
        WAVAX_USDC_POOL_ID,
        txOptions
      );
    console.log("Homora PDN contract deployed at: ", strategyContract.address);
    await whitelistContractAndAddCredit(strategyContract.address);
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

  it("HomoraPDNVault DepositAndWithdraw", async function () {
    await testDepositAndWithdraw(managerContract, strategyContract);
  });

  it("EthereumManager NativeTokenSupport", async function () {
    await testNativeTokenSupport(managerContract, strategyContract);
  });

  // it("Deposit and test rebalance", async function () {
  //   await testRebalance(contract);
  // });

  // it("Deposit and test reinvest", async function () {
  //   await testReinvest(contract);
  // });

  // it("Should fail for doing unauthorized operations", async function () {
  //   await expect(
  //     contract.connect(mainWallet).setDNThreshold(0, txOptions)
  //   ).to.be.revertedWith("unauthorized ops");
  // });
});

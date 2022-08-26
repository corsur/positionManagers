const { CHAIN_ID_AVAX } = require("@certusone/wormhole-sdk");
const { expect } = require("chai");
const { BigNumber } = require("ethers");
const { ethers } = require("hardhat");
const { mine } = require("@nomicfoundation/hardhat-network-helpers");
const {
  AVAX_MAINNET_TOKEN_BRIDGE_ADDR,
  AVAX_MAINNET_URL,
} = require("../constants.js");
const { deployHomoraPDNVault } = require("../utils/deploy.js");

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
  LP_WAVAX_USDC_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  USDC_TOKEN_ADDRESS,
  JOE_TOKEN_ADDRESS,
  JOE_ROUTER_ADDRESS,
  WAVAX_USDC_POOL_ID,
  AVAX_CHAIN_ID,
} = require("./avax_constants");
const {
  getImpersonatedSigner,
  whitelistContractAndAddCredit,
} = require("../utils/accounts.js");

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
];

const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };

async function initialize() {
  // Impersonate USDC holder.
  let signer = await getImpersonatedSigner(
    "0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541"
  );
  await USDC.connect(signer).transfer(wallets[0].address, 5e6 * 1e6, txOptions);
  await USDC.connect(signer).transfer(
    wallets[1].address,
    1000 * 1e6,
    txOptions
  );
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

// testing function for rebalance()
async function testRebalance(managerContract, strategyContract, vaultLib) {
  await deposit(managerContract, strategyContract);
  const homoraPosId = (await strategyContract.homoraPosId()).toNumber();
  // check collateral
  let collSize = await vaultLib.getCollateralSize(
    homoraBank.address,
    homoraPosId,
    txOptions
  );
  let [usdcHold, wavaxHold] = await vaultLib.convertCollateralToTokens(
    collSize,
    LP_WAVAX_USDC_ADDRESS,
    USDC_TOKEN_ADDRESS
  );
  console.log("collateral: usdc: %d, wavax: %d", usdcHold, wavaxHold);

  // check debt
  let [usdcDebt, wavaxDebt] = await vaultLib.getDebtAmounts(
    homoraBank.address,
    homoraPosId,
    USDC_TOKEN_ADDRESS,
    WAVAX_TOKEN_ADDRESS
  );
  console.log("current debt: usdc: %d, wavax: %d", usdcDebt, wavaxDebt);

  // check if position state is healthy (no need to rebalance)
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");

  // Flash swap USDC and rebalance (short)
  let swapAmt = BigNumber.from(3e6).mul(1e6);
  let recvAmt = await swapUSDC(router, swapAmt);
  console.log(
    "Swap %s USDC to %s AVAX",
    swapAmt.div(1e6).toString(),
    recvAmt.div("1000000000000000000").toString()
  );

  await mine(1000, { interval: 2 });
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
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
  await mine(1000, { interval: 2 });
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
  ).to.be.revertedWith("Slippage_Too_Large");

  await mine(1000, { interval: 2 });
  // Increase slippage and rebalance again
  await strategyContract.connect(wallets[0]).rebalance(100, 0, txOptions);

  // expect to be in delta-neutral after rebalance
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
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
  await mine(1000, { interval: 2 });
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
  ).to.be.revertedWith("Slippage_Too_Large");

  // Swap back
  swapAmt = recvAmt;
  recvAmt = await swapUSDC(router, swapAmt);
  console.log(
    "Swap %s USDC to %s AVAX",
    swapAmt.div(1e6).toString(),
    recvAmt.div("1000000000000000000").toString()
  );
  await mine(1000, { interval: 2 });
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
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
  await strategyContract.connect(wallets[0]).rebalance(100, 0, txOptions);

  // expect to be in delta-neutral after rebalance
  await expect(
    strategyContract.connect(wallets[0]).rebalance(10, 0, txOptions)
  ).to.be.revertedWith("HomoraPDNVault_PositionIsHealthy");
}

async function testReinvest(managerContract, strategyContract, vaultLib) {
  await deposit(managerContract);

  const homoraPosId = (await strategyContract.homoraPosId()).toNumber();

  let collateralBefore = await vaultLib.getCollateralSize(
    homoraBank.address,
    homoraPosId,
    txOptions
  );
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

  let collateralAfter = await vaultLib.getCollateralSize(
    homoraBank.address,
    homoraPosId,
    txOptions
  );

  console.log("Collateral before reinvest: %d", collateralBefore);
  console.log("Collateral after reinvest: %d", collateralAfter);
  if (reinvested) {
    expect(collateralAfter > collateralBefore).to.equal(true);
  } else {
    expect(collateralAfter == collateralBefore).to.equal(true);
  }
}

async function deposit(managerContract) {
  // Deposit 1000 USDC to vault from wallet 0.
  await USDC.connect(wallets[0]).approve(
    managerContract.address,
    usdcDepositAmount0
  );
  console.log("using wallet: ", wallets[0].address);

  // Craft open position data.
  let openPositionBytesArray = ethers.utils.arrayify(
    ethers.utils.defaultAbiCoder.encode(
      ["uint256", "uint256"],
      [
        minEquityReceived0, // uint256 minEquityETH
        0, // uint256 minReinvestETH
      ]
    )
  );

  // Deposit 1000 USDC to vault from wallet 0.
  await managerContract
    .connect(wallets[0])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[USDC_TOKEN_ADDRESS, usdcDepositAmount0]],
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
      ["uint256", "uint256"],
      [minEquityReceived1, 0]
    )
  );
  await managerContract
    .connect(wallets[1])
    .createPosition(
      /*strategyChainId=*/ AVAX_CHAIN_ID,
      /*strategyId=*/ 0,
      [[USDC_TOKEN_ADDRESS, usdcDepositAmount1]],
      openPositionBytesArray1,
      txOptions
    );
  console.log(`created position with ${usdcDepositAmount1} USDC`);
}

async function testDepositAndWithdraw(
  managerContract,
  strategyContract,
  homoraAdapter,
  vaultLib
) {
  await deposit(managerContract, strategyContract);

  // Check whether it initiates a position in HomoraBank.
  var homoraPosId = (await strategyContract.homoraPosId()).toNumber();
  expect(homoraPosId).not.to.equal(0);

  // Check whether the adapter contract is the owner of the HomoraBank position.
  var res = await homoraBank.getPositionInfo(homoraPosId);
  expect(res.owner).to.equal(homoraAdapter.address);

  // Colletral size of each wallet.
  var totalCollateralSize = res.collateralSize;
  // [totalShareAmount, _] = await strategyContract.vaultState();
  totalShareAmount = (await strategyContract.vaultState())[0];
  var shareAmount0 = await strategyContract.positions(CHAIN_ID_AVAX, 0); // position id 0
  console.log("share amount 0: ", shareAmount0.toString());
  var shareAmount1 = await strategyContract.positions(CHAIN_ID_AVAX, 1); // position id 1
  console.log("share amount 1: ", shareAmount1.toString());
  var collSize0 = shareAmount0.mul(totalCollateralSize).div(totalShareAmount);
  var collSize1 = shareAmount1.mul(totalCollateralSize).div(totalShareAmount);

  [usdcAmount0, wavaxAmount0] = await vaultLib.convertCollateralToTokens(
    collSize0,
    LP_WAVAX_USDC_ADDRESS,
    USDC_TOKEN_ADDRESS
  );
  [usdcAmount1, wavaxAmount1] = await vaultLib.convertCollateralToTokens(
    collSize1,
    LP_WAVAX_USDC_ADDRESS,
    USDC_TOKEN_ADDRESS
  );

  var totalAmount0InUsdc = 2 * usdcAmount0;
  var totalAmount1InUsdc = 2 * usdcAmount1;

  expect(totalAmount0InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount0 * leverageLevel),
    300
  );
  expect(totalAmount1InUsdc).to.be.closeTo(
    BigNumber.from(usdcDepositAmount1 * leverageLevel),
    300
  );

  // Withdraw half amount from vault for wallet 0.
  var withdrawAmount0 = shareAmount0.div(2);
  console.log("withdraw amount 0: ", withdrawAmount0.toString());

  // First byte is Action enum.
  // 0 -> Open, 1 -> Increase, 2 -> Decrease, 3 -> Close (not yet supported).
  const encodedWithdrawData = ethers.utils.concat([
    new Uint8Array([2]),
    new Uint8Array([0, AVAX_CHAIN_ID]), // recipient chainId.
    ethers.utils.zeroPad(ethers.utils.arrayify(wallets[0].address), 32), // recipient address (padded to 32 bytes).
    ethers.utils.arrayify(
      ethers.utils.defaultAbiCoder.encode(
        ["uint256", "uint256", "uint256", "uint256"],
        [withdrawAmount0, 0, 0, 0]
      )
    ),
  ]);

  await managerContract
    .connect(wallets[0])
    .executeStrategy(
      /*positionId=*/ 0,
      /*assetInfos=*/ [],
      encodedWithdrawData,
      txOptions
    );
}

describe("HomoraPDNVault Initialization", function () {
  var managerContract = undefined;
  var homoraAdapter = undefined;
  var strategyContract = undefined;
  var vaultLib = undefined;

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

    ({ managerContract, strategyContract, homoraAdapter, vaultLib } =
      await deployHomoraPDNVault(
        ethers,
        mainWallet,
        {
          wormholeTokenBridgeAddr: AVAX_MAINNET_TOKEN_BRIDGE_ADDR,
          controllerAddr: wallets[0].address,
          tokenA: USDC_TOKEN_ADDRESS,
          tokenB: WAVAX_TOKEN_ADDRESS,
          homoraBankAddr: homoraBank.address,
          spellAddr: TJ_SPELLV3_WAVAX_USDC_ADDRESS,
          rewardTokenAddr: JOE_TOKEN_ADDRESS,
          poolId: WAVAX_USDC_POOL_ID,
        },
        txOptions
      ));

    // Set up Homora adapter contract.
    await homoraAdapter.setCaller(strategyContract.address, true);
    await homoraAdapter.setTarget(strategyContract.address, true);
    await homoraAdapter.setTarget(USDC.address, true);
    await homoraAdapter.setTarget(WAVAX.address, true);
    await homoraAdapter.setTarget(JOE.address, true);

    await strategyContract.connect(mainWallet).initializeConfig(
      3, // _leverageLevel
      9231, // _targetDebtRatio
      100, // _debtRatioWidth
      300, // _deltaThreshold
      [
        20, // withdrawFee
        1500, // harvestFee
        200, // managementFee
      ],
      [
        BigNumber.from(1000000).mul(1e6), // maxCapacity
        BigNumber.from(200000).mul(1e6), // maxOpenPerTx
        BigNumber.from(200000).mul(1e6), // maxWithdrawPerTx
      ],
      txOptions
    );
    console.log("Homora PDN contract initialized");

    await whitelistContractAndAddCredit(
      mainWallet,
      homoraBank,
      homoraAdapter.address,
      {
        tokenA: USDC_TOKEN_ADDRESS,
        amtA: 1e11,
        tokenB: WAVAX_TOKEN_ADDRESS,
        amtB: ethers.BigNumber.from("1000000000000000000000"),
      },
      txOptions
    );
    await initialize();

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
    await testDepositAndWithdraw(
      managerContract,
      strategyContract,
      homoraAdapter,
      vaultLib
    );
  });

  it("Deposit and test rebalance", async function () {
    await testRebalance(managerContract, strategyContract, vaultLib);
  });

  it("Deposit and test reinvest", async function () {
    await testReinvest(managerContract, strategyContract, vaultLib);
  });

  it("Should fail for doing unauthorized operations", async function () {
    await expect(
      strategyContract.connect(wallets[0]).setConfig(0, 0, 0, 0, txOptions)
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });

  it("Should fail if request pause using unauthorized addr", async function () {
    // Pause Homora strategy contract.
    await expect(
      strategyContract.connect(wallets[0]).pause()
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });

  it("Should fail if request unpause using unauthorized addr", async function () {
    // Pause Homora strategy contract using admin contract.
    await strategyContract.connect(mainWallet).pause();

    // Unpause using unauthorized contract.
    await expect(
      strategyContract.connect(wallets[0]).pause()
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });
  it("Should fail create position if contract is paused", async function () {
    // Pause Homora strategy contract using admin contract.
    await strategyContract.connect(mainWallet).pause();

    // Approve to spend.
    await USDC.connect(wallets[0]).approve(
      managerContract.address,
      usdcDepositAmount0
    );

    // Craft open position data.
    let openPositionBytesArray = ethers.utils.arrayify(
      ethers.utils.defaultAbiCoder.encode(
        ["uint256", "uint256"],
        [
          minEquityReceived0, // uint256 minEquityETH
          0, // uint256 minReinvestETH
        ]
      )
    );

    // Deposit 1000 USDC to vault from wallet 0.
    await expect(
      managerContract
        .connect(wallets[0])
        .createPosition(
          /*strategyChainId=*/ AVAX_CHAIN_ID,
          /*strategyId=*/ 0,
          [[USDC_TOKEN_ADDRESS, usdcDepositAmount0]],
          openPositionBytesArray,
          txOptions
        )
    ).to.be.revertedWith("Pausable: paused");
  });
});

const { expect } = require("chai");
const { ethers } = require("hardhat");
const { AVAX_MAINNET_URL } = require("../../constants.js");
const { homoraBankABI } = require("../abi/homoraBankABI.js");

const ERC20_ABI = [
  "function balanceOf(address owner) view returns (uint256)",
  "function approve(address spender, uint256 value) returns (bool)",
  "function transfer(address _to, uint256 value) returns(bool)",
];

const SPELL_ABI = [
  "function addLiquidityWMasterChef(address,address,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256),uint256)",
];

const {
  HOMORA_BANK_ADDRESS,
  TJ_SPELLV3_WAVAX_USDC_ADDRESS,
  WAVAX_TOKEN_ADDRESS,
  USDC_TOKEN_ADDRESS,
  WAVAX_USDC_POOL_ID,
} = require("../avax_constants");

const txOptions = { gasPrice: 50000000000, gasLimit: 8500000 };

const provider = ethers.provider;
const ownerWallet = new ethers.Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);
const tempWallet = new ethers.Wallet(
  "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
  provider
);

const USDC = new ethers.Contract(USDC_TOKEN_ADDRESS, ERC20_ABI, provider);
const homoraBank = new ethers.Contract(
  HOMORA_BANK_ADDRESS,
  homoraBankABI,
  provider
);

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

  // Send native token to cover for gas.
  await ownerWallet.sendTransaction({
    to: homoraBankGovernor,
    value: ethers.utils.parseEther("100"),
  });

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
      [
        contractAddressToWhitelist,
        USDC_TOKEN_ADDRESS,
        1e11,
        ethers.constants.AddressZero,
      ],
      [
        contractAddressToWhitelist,
        WAVAX_TOKEN_ADDRESS,
        ethers.BigNumber.from("1000000000000000000000"),
        ethers.constants.AddressZero,
      ],
    ],
    txOptions
  );
}

async function initialize() {
  // Impersonate big whale USDC holder.
  const signer = await getImpersonatedSigner(
    "0x42d6ce661bb2e5f5cc639e7befe74ff9fd649541"
  );

  await USDC.connect(signer).transfer(
    ownerWallet.address,
    5e6 * 1e6,
    txOptions
  );
}

// Deploy HomoraAdapter contract and return contract handle.
async function deployContract() {
  const adapterContractFactory = await ethers.getContractFactory(
    "HomoraAdapter"
  );
  return await adapterContractFactory
    .connect(ownerWallet)
    .deploy(HOMORA_BANK_ADDRESS);
}

describe("HomoraPDNVault Initialization", function () {
  var adapterContract = null;
  var iUSDC = null; // USDC interface.
  var iSushiSpell = null;
  beforeEach("Setup before each test", async function () {
    await network.provider.request({
      method: "hardhat_reset",
      params: [
        {
          forking: {
            jsonRpcUrl: AVAX_MAINNET_URL,
            blockNumber: 19079166,
          },
        },
      ],
    });
    await initialize();
    adapterContract = await deployContract();
    console.log(
      `Deployed HomoraAdapter at ${
        adapterContract.address
      } with owner as ${await adapterContract.owner()}`
    );

    // Set up interface for HomoraBank.
    iUSDC = new ethers.utils.Interface(ERC20_ABI);
    // Construct interface for SushiSwap themed Spell contract.
    iSushiSpell = new ethers.utils.Interface(SPELL_ABI);
  });

  it("Should be possible to retrieve USDC sent to it", async function () {
    // Set up adapter contract.
    await adapterContract.setCaller(ownerWallet.address, true);
    await adapterContract.setTarget(USDC.address, true);

    // Transfer upfront 100 USDC to adapter contract.
    const USDCToSend = 100 * 1e6;
    await USDC.connect(ownerWallet).transfer(
      adapterContract.address,
      USDCToSend
    );

    // Check adapter contract's USDC balance.
    expect(await USDC.balanceOf(adapterContract.address)).eq(USDCToSend);

    // Construct bytes to transfer USDC out of the adapter.
    const usdcBytes = iUSDC.encodeFunctionData("transfer", [
      ownerWallet.address,
      USDCToSend,
    ]);

    // Instruct HomoraAdapter to send the balance back.
    await adapterContract.doWork(USDC.address, /*value=*/ 0, usdcBytes);

    // Adapter's USDC balance should be zero know.
    expect(await USDC.balanceOf(adapterContract.address)).eq(0);
  });

  it("Should be able to call HomoraBank", async function () {
    const numUSDC = 400 * 1e6;

    // Set up adapter contract.
    await adapterContract.setCaller(ownerWallet.address, true);
    await adapterContract.setTarget(USDC.address, true);

    // Set up HomoraBank whitelist & credit limit.
    await whitelistContractAndAddCredit(adapterContract.address);

    // Transfer USDC to adapter contract.
    await USDC.connect(ownerWallet).transfer(adapterContract.address, numUSDC);

    // Use `doWork` to approve HomoraBank to spend USDC from adapter contract.
    const usdcBytes = iUSDC.encodeFunctionData("approve", [
      homoraBank.address,
      numUSDC,
    ]);
    await adapterContract.doWork(USDC.address, /*value=*/ 0, usdcBytes);

    // Construct low level bytes for the call to HomoraBank.
    const spellBytes = iSushiSpell.encodeFunctionData(
      "addLiquidityWMasterChef",
      [
        USDC_TOKEN_ADDRESS,
        WAVAX_TOKEN_ADDRESS,
        [400 * 1e6, 0, 0, 100, 0, 0, 0, 0],
        WAVAX_USDC_POOL_ID,
      ]
    );

    // Instruct adapter to call HomoraBank.
    await adapterContract.homoraExecute(
      /*positionId=*/ 0,
      TJ_SPELLV3_WAVAX_USDC_ADDRESS,
      /*value=*/ 0,
      spellBytes
    );
  });

  it("Should be able to retrieve native token from adapter", async function () {
    const numNativeToken = 400;

    // Set up adapter contract.
    await adapterContract.setCaller(ownerWallet.address, true);
    await adapterContract.setTarget(tempWallet.address, true);

    // Send some native token to adapter contract.
    await ownerWallet.sendTransaction(
      {
        to: adapterContract.address,
        value: ethers.utils.parseEther(numNativeToken.toString()),
      },
      txOptions
    );

    // Record balance of a fresh wallet.
    // We use a fresh wallet to avoid accounting gas fee from the main wallet.
    const beforeBalance = await tempWallet.getBalance();

    // Pull out native token to the fresh wallet.
    await adapterContract.doWork(tempWallet.address, numNativeToken, []);

    // Record balance after refund.
    const afterBalance = await tempWallet.getBalance();

    // Check for correct diff.
    expect(afterBalance.sub(beforeBalance)).to.eq(numNativeToken);
  });

  it("Should throw error for unauthorized caller", async function () {
    await expect(
      adapterContract.doWork(ownerWallet.address, /*value=*/ 0, /*data=*/ [])
    ).to.be.revertedWith("unauthorized caller");
  });

  it("Should throw error for unauthorized target", async function () {
    await adapterContract.setCaller(ownerWallet.address, true);
    await expect(
      adapterContract.doWork(ownerWallet.address, /*value=*/ 0, /*data=*/ [])
    ).to.be.revertedWith("unauthorized target");
  });

  it("Should throw error for unauthorized call to setCaller", async function () {
    await expect(
      adapterContract.connect(tempWallet).setCaller(tempWallet.address, true)
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });

  it("Should throw error for unauthorized call to setTarget", async function () {
    await expect(
      adapterContract.connect(tempWallet).setTarget(tempWallet.address, true)
    ).to.be.revertedWith("Ownable: caller is not the owner");
  });

  it("Should throw error for attempting to whitelist Homora bank as a target", async function () {
    await expect(
      adapterContract.connect(ownerWallet).setTarget(HOMORA_BANK_ADDRESS, true)
    ).to.be.revertedWith("Disallow generic call to Homora bank");
  });

  it("Should throw error for attempting to directly call Homora bank using doWork", async function () {
    await expect(
      adapterContract.connect(ownerWallet).doWork(HOMORA_BANK_ADDRESS, 0, [])
    ).to.be.revertedWith("unauthorized caller");
  });
});

const { task, types } = require("hardhat/config");
const { deployHomoraAdapter } = require("../utils/deploy");

// Read private variables from .env file.
require("dotenv").config();

task("deploy-homora-pdn-vault", "To deploy Homora PDN Vault")
  .addParam(
    "apertureManager",
    "Address for Aperture Manager",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "tokenA",
    "Address for token A",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "tokenB",
    "Address for token B",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "homoraBank",
    "Address for Homora Bank contract",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "spell",
    "Address for Homora spell contract",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "rewardToken",
    "Address for reward token",
    undefined, // default value
    types.string,
    false // isOptional
  )
  .addParam(
    "poolId",
    "Address for underlying pool id",
    undefined, // default value
    types.int,
    false // isOptional
  )
  .setAction(async (taskArgs, hre) => {
    const ethers = hre.ethers;
    const provider = ethers.provider;
    const wallet = new ethers.Wallet(process.env.EVM_PRIVATE_KEY, provider);

    console.log(
      `Deploying using account: ${
        wallet.address
      } with balance of ${await wallet.getBalance()}`
    );

    // Deploy Homora adapter contract.
    const homoraAdapter = await deployHomoraAdapter(ethers, wallet);
    console.log(`Deployed homora adapter at ${homoraAdapter.address}`);

    // Deploy various dependency libraries.
    var library = await ethers.getContractFactory("HomoraAdapterLib");
    const adapterLib = await library.connect(wallet).deploy();
    await adapterLib.deployed();
    console.log(`Deployed adapter lib at ${adapterLib.address}`);

    library = await ethers.getContractFactory("VaultLib", {
      libraries: {
        HomoraAdapterLib: adapterLib.address,
      },
    });
    const vaultLib = await library.connect(wallet).deploy();
    await vaultLib.deployed();
    console.log(`Deployed vault lib at ${vaultLib.address}`);

    // Deploy the actual Homora PDN strategy contract.
    const strategyFactory = await ethers.getContractFactory("HomoraPDNVault", {
      signer: wallet,
      libraries: {
        VaultLib: vaultLib.address,
      },
    });
    console.log(taskArgs);
    const strategyContract = await upgrades.deployProxy(
      strategyFactory,
      [
        taskArgs.apertureManager,
        homoraAdapter.address,
        process.env.GNOSIS_SAFE_ADDR, // fee collector address.
        wallet.address, // controller address.
        taskArgs.tokenA,
        taskArgs.tokenB,
        taskArgs.homoraBank,
        taskArgs.spell,
        taskArgs.rewardToken,
        taskArgs.poolId,
      ],
      { unsafeAllow: ["delegatecall"], kind: "uups" }
    );

    await strategyContract.connect(wallet).deployed();
    console.log(`HomoraPDNVault deployed at ${strategyContract.address}`);

    // Transfer ownership.
    console.log(`Transferring ownership to ${process.env.GNOSIS_SAFE_ADDR}`);

    await strategyContract.transferOwnership(process.env.GNOSIS_SAFE_ADDR);

    console.log(
      `Completed ownership transfer to ${process.env.GNOSIS_SAFE_ADDR}`
    );
  });

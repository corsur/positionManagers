const { ethers } = require("hardhat");

async function deployApertureManager(signer, wormholeTokenBridgeAddr) {
  const curveRouterLibFactory = await ethers.getContractFactory(
    "CurveRouterLib",
    signer
  );
  // The `deploy()` only broadcasts tx out. We still need to wait for its
  // inclusion by miners/validators first.
  const curveRouterLib = await curveRouterLibFactory.deploy();
  // Wait for tx to be included in the block.
  await curveRouterLib.deployed();
  console.log(`Deployed curve router at ${curveRouterLib.address}`);

  const crossChainLibFactory = await ethers.getContractFactory(
    "CrossChainLib",
    {
      libraries: {
        CurveRouterLib: curveRouterLib.address,
      },
      signer: signer,
    }
  );
  const crossChainLib = await crossChainLibFactory.deploy();
  // Wait for tx to be included in the block.
  await crossChainLib.deployed();
  console.log(`Deployed cross-chain lib at ${crossChainLib.address}`);

  const apertureManagerContractFactory = await ethers.getContractFactory(
    "ApertureManager",
    {
      libraries: {
        CurveRouterLib: curveRouterLib.address,
        CrossChainLib: crossChainLib.address,
      },
      signer: signer,
    }
  );

  const apertureManagerProxy = await upgrades.deployProxy(
    apertureManagerContractFactory,
    [
      [wormholeTokenBridgeAddr, /*consistencyLevel=*/ 15],
      [/*feeBps=*/ 100, /*feeSink=*/ signer.address],
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await apertureManagerProxy.deployed();

  return apertureManagerProxy;
}

async function deployHomoraAdapter(signer) {
  const homoraAdapterFactory = await ethers.getContractFactory(
    "HomoraAdapter",
    signer
  );

  return await homoraAdapterFactory.connect(signer).deploy();
}

async function deployHomoraPDNVault(signer, vaultConfig, txOptions) {
  const {
    wormholeTokenBridgeAddr,
    controllerAddr,
    tokenA,
    tokenB,
    homoraBankAddr,
    spellAddr,
    rewardTokenAddr,
    poolId,
  } = vaultConfig;
  // Aperture manager contract.
  const managerContract = await deployApertureManager(
    signer,
    wormholeTokenBridgeAddr
  );
  console.log("Aperture manager deployed at: ", managerContract.address);

  // Deploy Homora adapter contract.
  const homoraAdapter = await deployHomoraAdapter(signer);

  // HomoraPDNVault contract.
  var library = await ethers.getContractFactory("HomoraAdapterLib");
  const adapterLib = await library.deploy();
  library = await ethers.getContractFactory("VaultLib", {
    libraries: {
      HomoraAdapterLib: adapterLib.address,
    },
  });
  const vaultLib = await library.deploy();
  const strategyFactory = await ethers.getContractFactory("HomoraPDNVault", {
    libraries: {
      VaultLib: vaultLib.address,
    },
  });
  const strategyContract = await upgrades.deployProxy(
    strategyFactory,
    [
      managerContract.address,
      homoraAdapter.address,
      signer.address, // fee collector addr.
      controllerAddr,
      tokenA,
      tokenB,
      homoraBankAddr,
      spellAddr,
      rewardTokenAddr,
      poolId,
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await strategyContract.connect(signer).deployed(txOptions);
  console.log(`HomoraPDNVault deployed at ${strategyContract.address}`);
  return {
    managerContract: managerContract,
    strategyContract: strategyContract,
    homoraAdapter: homoraAdapter,
    vaultLib: vaultLib,
  };
}

module.exports = {
  deployApertureManager: deployApertureManager,
  deployHomoraAdapter: deployHomoraAdapter,
  deployHomoraPDNVault: deployHomoraPDNVault,
};

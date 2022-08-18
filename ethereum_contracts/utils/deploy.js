const { ethers } = require("hardhat");

async function deployApertureManager(signer, wormholeTokenBridgeAddr) {
  const curveRouterLib = await ethers.getContractFactory(
    "CurveRouterLib",
    signer
  );
  const curveRouterLibAddress = (await curveRouterLib.deploy()).address;

  const crossChainLib = await ethers.getContractFactory("CrossChainLib", {
    libraries: {
      CurveRouterLib: curveRouterLibAddress,
    },
    signer: signer,
  });
  const crossChainLibAddress = (await crossChainLib.deploy()).address;

  const apertureManagerContractFactory = await ethers.getContractFactory(
    "ApertureManager",
    {
      libraries: {
        CrossChainLib: crossChainLibAddress,
        CurveRouterLib: curveRouterLibAddress,
      },
      signer: signer,
    }
  );

  const apertureManagerProxy = await upgrades.deployProxy(
    apertureManagerContractFactory,
    [
      [wormholeTokenBridgeAddr, /*consistencyLevel=*/ 1],
      [/*feeBps=*/ 100, /*feeSink=*/ wormholeTokenBridgeAddr],
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

const { ethers } = require("hardhat");

async function deployCrossChain(signer, tokenBridgeAddr) {
  const address = await signer.getAddress();
  console.log("Using impersonated wallet address:", address);

  const CrossChain = await ethers.getContractFactory("CrossChain");
  const crossChain = await CrossChain.deploy(
    /*_consistencyLevel=*/ 1,
    tokenBridgeAddr,
    /*_crossChainFeeBPS=*/ 0,
    /*_feeSink=*/ address
  );

  await crossChain.deployed();
  console.log("crossChain.deployed at:", crossChain.address);
  return crossChain;
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

async function deployEthereumManager(signer, crossChain, curveSwap) {
  const address = await signer.getAddress();
  console.log("Using impersonated wallet address:", address);

  const EthereumManager = await ethers.getContractFactory(
    "EthereumManager",
    signer
  );

  const ethereumManager = await upgrades.deployProxy(
    EthereumManager,
    [crossChain, curveSwap],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await ethereumManager.deployed();

  console.log("ethereumManager.deployed at:", ethereumManager.address);
  return ethereumManager;
}

async function deployEthereumManagerSimple(signer, tokenBridgeAddr) {
  const crossChain = await deployCrossChain(signer, tokenBridgeAddr);
  const curveSwap = await deployCurveSwap(signer);
  return await deployEthereumManager(
    signer,
    crossChain.address,
    curveSwap.address
  );
}

module.exports = {
  deployCrossChain: deployCrossChain,
  deployCurveSwap: deployCurveSwap,
  deployEthereumManager: deployEthereumManager,
  deployEthereumManagerSimple: deployEthereumManagerSimple,
};

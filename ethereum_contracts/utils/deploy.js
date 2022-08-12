const { ethers } = require("hardhat");

async function deployApertureManager(signer, wormholeTokenBridgeAddr) {
  const curveRouterLib = await ethers.getContractFactory("CurveRouterLib", signer);
  const curveRouterLibAddress = (await curveRouterLib.deploy()).address;

  const crossChainContractFactory = await ethers.getContractFactory("CrossChain", signer);
  const crossChainContract = await crossChainContractFactory.deploy(
    /*_consistencyLevel=*/ 1,
    wormholeTokenBridgeAddr,
    /*_crossChainFeeBPS=*/ 0,
    /*_feeSink=*/ wormholeTokenBridgeAddr,
  );

  const ApertureManager = await ethers.getContractFactory(
    "ApertureManager",
    {
      libraries: { CurveRouterLib: curveRouterLibAddress },
      signer: signer
    }
  );

  const apertureManager = await upgrades.deployProxy(
    ApertureManager,
    [
      crossChainContract.address
    ],
    { unsafeAllow: ["delegatecall"], kind: "uups" }
  );
  await apertureManager.deployed();

  // Temporarily returns both contract addresses. Once "crossChainContract" is converted to a library, only "apertureManager" needs to be returned.
  return [apertureManager, crossChainContract];
}

module.exports = {
  deployApertureManager: deployApertureManager,
};

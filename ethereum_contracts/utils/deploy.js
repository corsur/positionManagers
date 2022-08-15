const { ethers } = require("hardhat");

const wormholeTokenBridgeABI = [
  "function wormhole() external view returns (address)",
];

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
      [
        wormholeTokenBridgeAddr,
        /*consistencyLevel=*/ 1,
      ],
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

module.exports = {
  deployApertureManager: deployApertureManager,
  deployHomoraAdapter: deployHomoraAdapter,
};

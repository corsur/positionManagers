const { ethers } = require("hardhat");
const { expect } = require("chai");
const { deployEthereumManagerHardhat } = require("../../utils/helpers");

describe.only("Ethereum Manager Unit Tests", function () {
  var ethereumManager = undefined;
  beforeEach("some description", async function () {
    ethereumManager = await deployEthereumManagerHardhat();
  });

  it("Should update cross-chain fee", async function () {
    await ethereumManager.updateCrossChainFeeBPS(1000);
    expect((await ethereumManager.getConfig()).crossChainFeeBPS).to.equal(1000);

    await ethereumManager.updateCrossChainFeeBPS(2000);
    expect((await ethereumManager.getConfig()).crossChainFeeBPS).to.equal(2000);
  });

  it("Should update fee sink address", async function () {
    await ethereumManager.updateFeeSink(
      "0x16be88fa89e7ff500a5b6854faea2d9a4b2f7383"
    );
    expect((await ethereumManager.getConfig()).feeSink).to.equal(
      "0x16be88Fa89e7FF500A5B6854fAea2d9a4B2f7383"
    );
  });

  it("Non-owner should not have access to updateCrossChainFeeBPS", async function () {
    ethereumManager = ethereumManager.connect(
      (await ethers.getSigners())[2] // Use a different wallet.
    );

    return ethereumManager
      .updateCrossChainFeeBPS(2000)
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'Ownable: caller is not the owner'"
          )
      );
  });

  it("Non-owner should not have access to updateFeeSink", async function () {
    ethereumManager = ethereumManager.connect(
      (await ethers.getSigners())[2] // Use a different wallet.
    );

    return ethereumManager
      .updateFeeSink("0x16be88fa89e7ff500a5b6854faea2d9a4b2f7383")
      .catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'Ownable: caller is not the owner'"
          )
      );
  });
});

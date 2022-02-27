const { ethers } = require("hardhat");
const { expect } = require("chai");
const { deployEthereumManagerHardhat } = require("../../utils/helpers");

describe("Ethereum Manager Unit Tests", function () {
  var ethereumManager = undefined;
  beforeEach("some description", async function () {
    ethereumManager = await deployEthereumManagerHardhat();
  });

  it("Should update cross-chain fee", async function () {
    await ethereumManager.updateCrossChainFeeBPS(100);
    expect((await ethereumManager.getConfig()).crossChainFeeBPS).to.equal(100);

    await ethereumManager.updateCrossChainFeeBPS(20);
    expect((await ethereumManager.getConfig()).crossChainFeeBPS).to.equal(20);
  });

  it("Should not be able to set cross-chain fee above 100 bps", async function () {
    ethereumManager.updateCrossChainFeeBPS(101).catch((error) =>
      expect(error)
        .to.be.an("error")
        .with.property(
          "message",
          "VM Exception while processing transaction: reverted with reason string 'crossChainFeeBPS exceeds maximum allowed value of 100'"
        )
    );
  });

  it("Should update fee sink address", async function () {
    await ethereumManager.updateFeeSink(
      "0x16be88fa89e7ff500a5b6854faea2d9a4b2f7383"
    );
    expect((await ethereumManager.getConfig()).feeSink).to.equal(
      "0x16be88Fa89e7FF500A5B6854fAea2d9a4B2f7383"
    );
  });

  it("Should update fee sink address", async function () {
    ethereumManager.updateFeeSink(
      "0x0000000000000000000000000000000000000000").catch((error) =>
        expect(error)
          .to.be.an("error")
          .with.property(
            "message",
            "VM Exception while processing transaction: reverted with reason string 'feeSink address must be non-zero'"
          )
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

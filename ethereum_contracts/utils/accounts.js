const { ethers } = require("hardhat");

async function getImpersonatedSigner(accountToImpersonate) {
  await hre.network.provider.request({
    method: "hardhat_impersonateAccount",
    params: [accountToImpersonate],
  });
  return await ethers.getSigner(accountToImpersonate);
}

async function whitelistContractAndAddCredit(
  wallet,
  homoraBank,
  contractAddressToWhitelist,
  tokenArgs,
  txOptions
) {
  const { tokenA, amtA, tokenB, amtB } = tokenArgs;
  // Get impersonatedSigner as governor of homoraBank contract.
  const homoraBankGovernor = await homoraBank.governor(txOptions);
  const signer = await getImpersonatedSigner(homoraBankGovernor);

  // Transfer AVAX to the governor, so that it has gas to execute tx later.
  await wallet.sendTransaction({
    to: homoraBankGovernor,
    value: ethers.utils.parseEther("100"),
  });

  // Whitelist address and check.
  await homoraBank
    .connect(signer)
    .setWhitelistUsers([contractAddressToWhitelist], [true], txOptions);
  await homoraBank.whitelistedUsers(
    contractAddressToWhitelist,
    txOptions
  );

  // Set credit to 100,000 USDC and 5,000 WAVAX.
  await homoraBank.connect(signer).setCreditLimits(
    [
      [contractAddressToWhitelist, tokenA, amtA, ethers.constants.AddressZero],
      [contractAddressToWhitelist, tokenB, amtB, ethers.constants.AddressZero],
    ],
    txOptions
  );
}

module.exports = {
  whitelistContractAndAddCredit: whitelistContractAndAddCredit,
  getImpersonatedSigner: getImpersonatedSigner,
};

const { ethers } = require("hardhat");
const {
  INFURA_URL_ROPSTEN,
  ETH_PRV_KEY_1,
} = require("../constants");

const ethProvider = new ethers.providers.JsonRpcProvider(INFURA_URL_ROPSTEN);
const ethWallet = new ethers.Wallet(ETH_PRV_KEY_1, ethProvider);

module.exports = {
  ethWallet: ethWallet,
};

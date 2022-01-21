const { ethers } = require("hardhat");
const { ETH_NODE_URL, ETH_PRV_KEY_1 } = require("../constants");

const ethProvider = new ethers.providers.JsonRpcProvider(ETH_NODE_URL);
const ethWallet = new ethers.Wallet(ETH_PRV_KEY_1, ethProvider);

module.exports = {
  ethWallet: ethWallet,
};

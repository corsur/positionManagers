const { getSignedVAA } = require("@certusone/wormhole-sdk");
const { WORMHOLE_RPC_HOST } = require("../constants");
const {
  NodeHttpTransport,
} = require("@improbable-eng/grpc-web-node-http-transport");

async function getSignedVAAWithRetry(emitterChain, emitterAddress, sequence) {
  process.stdout.write(`Fetching VAA...`);
  while (true) {
    try {
      const { vaaBytes } = await getSignedVAA(
        WORMHOLE_RPC_HOST,
        emitterChain,
        emitterAddress,
        sequence,
        {
          transport: NodeHttpTransport(),
        }
      );
      if (vaaBytes !== undefined) {
        process.stdout.write(`✅\n`);
        return vaaBytes;
      }
    } catch (e) {}
    process.stdout.write(".");
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

module.exports = {
  getSignedVAAWithRetry: getSignedVAAWithRetry,
};

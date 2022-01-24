# Aperture EVM contracts built using Hardhat

## Directory Overview

    |-- contracts // Contains all EVM contract code.
    |-- scripts // Directly runnable scripts to deploy, upgrade, etc.
    |-- test // Unit & integration tests.
    |-- utils // helper functions and utility fns.

## Environment Setup

```shell
npm install
```

## Deploy Prerequisite

1. Ensure the Ethereum wallet has enough balance for Wormhole wrapped UST. The default ETH wallet already has plenty of UST. In case running out of UST, you can run the script `scripts/send_ust_to_eth.js`, which transfers UST from `TERRA_WALLET_MNEMONIC` to `ETH_PRV_KEY_1` (refer to `constants.js` for actual wallet address used.)

2. Deployment of `EthereumManager` needs reference to `TerraManager`'s address. This package relies on the `TERRA_MANAGER_ADDR` from `constants.js`. To use a new `TerraManager`, please update the value in `constants.js`

   - To compile `TerraManager`, please follow the top level `README` instructions.
   - To deploy a new `TerraManager`, please refer to the parent directory `deployment` and follow the instructions there.

## Deploy

```shell
npx hardhat run scripts/deploy.js --network <network_name>
```

Where `network_name` can be any of the networks listed under `hardhat.config.js`. This will execute the script based on the specified blockchain network.

## Upgrade

```shell
npx hardhat run scripts/upgrade.js --network <network_name>
```

## Running Scripts in General

```shell
npx hardhat run scripts/deploy.js --network <network_name>
```

## Testing

General command to run all tests:

```shell
npx hardhat test --network <network_name>
```

Replace `network_name` with networks that you'd like to run the test with.

Tests can take up to 10 minutes to complete due to delay in the Wormhole guardians and the blockchain network. But there are a few ways to speed things up:

- For unit test not relying on any existing data on testnet or mainnet, use the hardhat network.

  ```shell
  npx hardhat test --network hardhat
  ```

- For tests relying on existing data on testnet or mainnet, but not dependent on Wormhome guardians, use the forking features from Hardhat to spin up a local node (the `--fork` arg should refer to the blockchain network you'd like to fork):
  ```shell
  npx hardhat node --fork https://ropsten.infura.io/v3/9b8f5bdca4a9470f94290a14c39a299b
  ```
  Next, run tests on top of it:
  ```shell
    npx hardhat test --network localhost
  ```

### Running Individual Test

Add `.only` to the `describe`:

```javascript
describe.only("EthereumManager integration test", function () {...}
```

## Other Basic Hardhat Tasks

```shell
npx hardhat accounts
npx hardhat compile
npx hardhat clean
npx hardhat test
npx hardhat node
node scripts/sample-script.js
npx hardhat help
```

# Aperture EVM contracts built using Hardhat

## Directory Overview

    |-- contracts // Contains all EVM contract code.
        |-- interfaces
        |-- libraries
        |-- periphery
    |-- scripts // Directly runnable scripts to deploy, upgrade, etc.
    |-- test // Unit & integration tests.
    |-- utils // helper functions and utility fns for tests and scripts.

## Project Overview

### General Project Description

This repository contains contracts for creating a delta-neutral strategy on top of [Homora V2](https://homora-v2.alphaventuredao.io/). Essentially, it hedges price exposure by borrowing and longing at the same time. The detailed investment strategy can be found in our Notion doc [here](https://aperture-finance.notion.site/Homora-Based-PDN-6d6b3b662b0d4d649c1c6ef379b25a45) and the derivation can be found under the rebalance design [page](https://aperture-finance.notion.site/Rebalance-Design-20052f109aa74fcab59b7dad7a1baf6c).

### Smart Contract Overview

- `LendingOptimizer.sol` can be ignored is not within the scope of this audit.
- `ApertureManager.sol` directly interacts with users and is responsible for priliminary asset/logic validation and then delegating to individual strategy contract.
- `HomoraPDNVault.sol` contains the actual business logic for creating and handling delta-neutral on top of Homora.
- `periphery/homoraAdapter.sol` is an immutable contract designed to call `HomoraBank`, which has a hard requirement on the immutability of contracts interact with it. It has very minimal design but with flexibility for actual logic contract `HomoraPDNVault` to operate through it.
- `libraries/VaultLib.sol` contains refactored logic from `HomoraPDNVault` to achieve smaller contract size better code encapsulation.

## Homora Resources:

- Homora design [doc](https://hackmd.io/@PhhCdDESRme9EK6zwT-9Pw/BJsYdyrw9#Alpha-Homora-contract-documentation).
- Homora Ethereum [repo](https://github.com/AlphaFinanceLab/alpha-homora-v2-contract/tree/master).
- Homora token optimal swap [derivation](https://blog.alphaventuredao.io/byot/).

## Dev Environment Setup

```shell
npm install
```

## Running Scripts & Tasks

To compile all files under contracts/:

```shell
npx hardhat compile
```

To run coverage:

```shell
npx hardhat coverage
```

To run a generic script:

```shell
npx hardhat run scripts/deploy.js --network <network_name>
```

## Testing

General command to run all tests:

```shell
npx hardhat test --network <network_name>
```

Replace `network_name` with networks that you'd like to run the test with.

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

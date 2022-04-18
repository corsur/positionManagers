# Aperture Finance

This monorepository contains the source code for the core smart contracts implementing Aperture Protocol, a cross-chain investment platform.

[![codecov](https://codecov.io/gh/Aperture-Finance/Aperture-Contracts/branch/protocol/graph/badge.svg?token=EOJNHFN2Y1)](https://codecov.io/gh/Aperture-Finance/Aperture-Contracts)

## Directory Overview

    |-- contracts // Contains all CosmWasm contracts and test.
        |-- delta_neutral_position // Core ∆-neutral strategy logic
        |-- delta_neutral_position_manager // Manage positions for all users.
        |-- anchor_earn_proxy // A wrapper of Anchor Earn.
        |-- terra_manager // Entry point on Terra and manage all strategies.
    |-- controller // Jobs to periodically trigger rebalance.
    |-- data_pipeline // Scripts to periodically push analysis data for web app and dashboard uses.
    |-- deployment // Scripts to deploy Terra contracts.
    |-- ethereum_hardhat // All EVM related contracts built using Hardhat.
    |-- packages // Terra packages
        |-- spectrum_protocol // This is copied from Spectrum Protocol's repo; this package is not uploaded to crates.io index.
        |-- aperture_common // Aperture common util libraries and data types.

## Development

### Environment Setup

- Rust v1.44.1+
- `wasm32-unknown-unknown` target
- Docker

1. Install `rustup` via https://rustup.rs/

2. Run the following:

```sh
rustup default stable
rustup target add wasm32-unknown-unknown
```

3. Make sure [Docker](https://www.docker.com/) is installed

### Test Coverage

Tests are automatically run and a coverage report is generated at each commit by GitHub Actions.

To manually generate a test coverage report, on an x64 Linux machine, run the following

```sh
sh run_test_coverage.sh
```

### Compiling

After making sure tests pass, you can compile each contract with the following:

```sh
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/{contract_module}.wasm .
ls -l {contract_module}.wasm
sha256sum {contract_module}.wasm
```

### Productionization

For production builds, run the following:

M1 Mac (arm64):

```
./build_terra_contracts_arm64.sh
```


Intel/AMD (amd64):

```sh
./build_terra_contracts_amd64.sh
```

This performs several optimizations which can significantly reduce the final size of the contract binaries, which will be available inside the `artifacts/` directory.
The arm64 and amd64 optimizers will produce different wasm byte codes; however, either one can be safely deployed to Terra networks for production.

Note that Docker does not support IPv6 out of the box on Mac, so switch to IPv4 when possible; otherwise you may receive a "service unavailable" error when Docker attempts to fetch images.

### Code-health tools

```
cargo fmt
cargo clippy -- -D warnings
```

# Design Overview

Aperture strives to be an on-chain App Store super charged with cross-chain features. It enables users of any supported chain to invest in opportunities available on all supported chains. For example, a Solana user of Aperture is able to invest funds in their Solana wallet into opportunities across Ethereum, Binance Smart Chain, Polygon, Terra, and Solana itself.

## Architecture

### Investment Position Representation

Each investment position on Aperture is represented as an integer position ID. For example, an Ethereum user initiating a Terra Anchor investment strategy will get back a position ID (e.g. 12345).

Note that each Aperture position is uniquely identified by the ordered pair (chain_id, position_index), where chain_id is an integer id for a supported chain (Ethereum, Terra, etc. and have the same value as Wormhole's [design](https://docs.wormholenetwork.com/wormhole/contracts)) and position_index is the postion ID.

### Interoperability

Aperture will deploy a Manager contract on each supported chain, whose responsibilities include:

- Communicate with Aperture Manager contracts on other chains to facilitate position creation / close and fund transfers.
- Manage investment positions on their respective chains.

For example, the Terra Manager is responsible for:

- Communicating with Managers on Ethereum / BSC / Solana and other supported chains.
- Managing Terra investment positions opened by users across all supported chains.

![alt text](https://drive.google.com/uc?export=download&id=17fAC9-q1ip1SVJ4_lum1SfdTizVRgv4X)

Communication among Aperture Managers will be initially performed using the Wormhole v2 generic message passing protocol. Later on, we have the option of switching to Chainlink’s CCIP when that launches with support for all desired chains, and if CCIP proves to have better security properties and/or faster consensus.

### Cross-chain Communication Design

    // --- Cross-chain instruction format --- //
      [uint128] position_id
      [uint16] target_chain_id
      [uint64] strategy_id
      [uint32] num_token_transferred
      [var_len] num_token_transferred * sizeof(uint64)
      [uint32] encoded_action_len
      [var_len] base64 encoding of params needed by action.

![alt text](https://drive.google.com/uc?export=download&id=19EHZ1xJvp0wpyMpF_EXwfmg2qbT1l34F)

As illustrated above, when user initiates invest action from source chain, Aperture does the following:
If the operation is to create a new position, create a new position id and persist the `<chain_id, position_id>` mapping on the source chain.
Craft instruction and sends it over through wormhole generic message passing and transfer any necessary tokens through token transfer protocol.

Once finality is achieved within the bridging solution’s network, Aperture’s controller will trigger relevant contract on destination chain to follow through the investment request. In a nutshell, Aperture Manager holds the reference to position managers, and positions managers take care of investment logic and bookkeeping. However, for certain type of strategy, position manager will delegate the actual strategy logic to strategy contract: this basically added another layer of indirection in exchange for flexibility and readability.

### How Does Wormhole & Interop Work?

Wormhole generic message passing identifies each message by the tuple (nonce, emitter_chain, emitter_address, sequence):

- nonce: this should be randomly generated by the Aperture Manager on the source chain, and should be included in the instruction. However, Wormhole is not using this nonce field yet.
- emitter_chain: chain id of the source chain.
- emitter_address: Wormhole message bridge address; this is constant and therefore does not need to be included in the message.
- sequence: a number generated by Wormhole message bridge contract; this should be included in the instruction.

The controller monitors the source chain node for events published by Aperture Manager, waits for finality, and then retrieves the signatures called VAA (Verified Action Approval) from Wormhole guardians by making an RPC to Wormhole server.

Once the VAA is retrieved, the controller should call Aperture Manager on the target chain with the VAA bytes. The Manager parses VAA to obtain the (nonce, emitter_chain, emitter_address, sequence) and interacts with the Wormhole bridge contract to retrieve the payload message instruction. The Manager should verify the following before fulfilling the instruction:
Message sender is the Aperture Manager address on the source chain.
Nonce and sequence values embedded in the instruction match the values parsed from VAA.

### Investment Position Manager Design

There are two types of position manager schemes. Lite position manager and full position manager.

- Lite position manager keeps mapping <<chain_id, position_id>, token_balance>. And it supports a list of functions to interact with the corresponding investment opportunity. For instance Anchor lite position manager keeps track of the aUST for each position and it can allow users to deposit or withdraw.
- Full position manager is slightly more involved. Instead of keeping track of balance directly, it delegates the actual logic to a contract associated with a particular position. This helps to maintain the bookkeeping for more complicated strategies.

![alt text](https://drive.google.com/uc?export=download&id=1B-65Ym_sDulaozvrCyJN_mf9ROQh6vu6)

### Terra Only Design

Initially, Aperture will launch on Terra first without the aforementioned cross-chain ability. Interoperability will be added incrementally based on the initial launch. For the initial version, Aperture will be having the following modules:

- Terra manager

  - Entry point for users to interact with any investment strategies. Users will pass in the investment type and investment action to the manager. Terra manager will take care of locating individual position managers and delegate the business logic to the position manager to handle.

  - If users wish to update existing investment positions, a position id can be passed in for Terra manager to operate on.

- Delta-neutral position manager
  - Is a full position manager
  - Handles bookkeeping and delegate requests to specific delta-neutral contract.
  - See Aperture's [GitBook](https://docs.aperture.finance/docs/aperture-invest+/delta-neutral-strategy-terra) for an overview; there are pages that go into detail about position opening, etc.
- Delta-neutral contract
  - Contains actual logic to carry out the delta-neutral strategy.
- Anchor Earn proxy
  - Is a lite position manager
  - Contract that contains business logic specific to Anchor Earn.
  - This simple strategy put deposited funds in Anchor Earn. This is intended to be offered to non-Terra users (for example, Avax/Polygon/Ethereum/BSC/Solana users can use this strategy to achieve cross-chain Anchor Earn investment).

To put the above modules together, we have the following localized setup:

![alt text](https://drive.google.com/uc?export=download&id=1xLw3hhRL8YeupAcmjJgn09pLIeRcff2p)

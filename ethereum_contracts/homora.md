# Aperture Delta Neutral Vault

This repo contains the preliminary code for contract to interact with Homora V2 on Avalanche. It contains the basic functionality of depsoit, withdraw, rebalance (albeit primitive at this point) and reinvest, along with unit tests.

## Directory Overview

    |-- contracts // Contains all EVM contract code.
    |-- test // Unit & integration tests.

## Pending Optimization & Modification

### Rebalance

The current rebalance logic implemented is primitive and costly. It will be replaced by a much more efficient rebalance mechanism in the next few weeks. The details of the derivation is included in this [Notion doc](https://aperture-finance.notion.site/Rebalance-Design-20052f109aa74fcab59b7dad7a1baf6c).

### Integration with Aperture Manager Contract

Aperture has a manager contract on each chain to keep track of all Aperture implemented strategies. So far, we have managers implemented on EVM, Solana and Cosmwasm chains. This repo's logic is not yet compatible with Aperture manager's overall architecture. However, it's easy to hook up the current strategy to Aperture manager fairly easily.

### Time-lock / Multi-sig Upgrade

Ideally, it's good to release contracts as immutable contracts. However, in reality, certain upgrades may be warranted to provide bug fixes and feature support. Aperture would like to work with Homora to build a safe and efficient product for DeFi users.

## Resources

- Design doc [link](https://aperture-finance.notion.site/Homora-Based-PDN-6d6b3b662b0d4d649c1c6ef379b25a45).

# Aperture Finance - Amadeus

## Development

To build:
```
cargo wasm
```

To run tests:
```
cargo test
```

Useful code-health tools:
```
cargo fmt
cargo clippy -- -D warnings
```

To produce optimized build for deployment:
```
cargo run-script optimize
```
This requires installation of cargo-run-script. One-time installation:
```
cargo install cargo-run-script
```

## Deployment

### Testnet (bombay-11)

The addresses below are for tequila-0004; the addresses for bombay-11 are unknown.
```json
{
  "anchor_ust_cw20_addr": "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
  "mirror_collateral_oracle_addr": "terra1q3ls6u2glsazdeu7dxggk8d04elnvmsg0ung6n",
  "mirror_lock_addr": "terra1pcxghd4dyf950mcs0kmlp7lvnrjsnl6qlfldwj",
  "mirror_mint_addr": "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w",
  "mirror_oracle_addr": "terra1uvxhec74deupp47enh7z5pk55f3cvcz8nj4ww9",
  "mirror_staking_addr": "terra1a06dgl27rhujjphsn4drl242ufws267qxypptx",
  "spectrum_mirror_farms_addr": "terra1hasdl7l6xtegnch8mjyw2g7mfh9nt3gtdtmpfu",
  "spectrum_staker_addr": "terra15nwqmmmza9y643apneg0ddwt0ekk38qdevnnjt",
  "terraswap_factory_addr": "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf",
}
```

### Mainnet (columbus-5)

These contract addresses should be identical to Columbus-4; we just need to wait for contract owners to migrate.

```json
{
  "anchor_ust_cw20_addr": "terra1hzh9vpxhsk8253se0vv5jj6etdvxu3nv8z07zu",
  "mirror_collateral_oracle_addr": "terra1pmlh0j5gpzh2wsmyd3cuk39cgh2gfwk6h5wy9j",
  "mirror_lock_addr": "terra169urmlm8wcltyjsrn7gedheh7dker69ujmerv2",
  "mirror_mint_addr": "terra1wfz7h3aqf4cjmjcvc6s8lxdhh7k30nkczyf0mj",
  "mirror_oracle_addr": "terra1t6xe0txzywdg85n6k8c960cuwgh6l8esw6lau9",
  "mirror_staking_addr": "terra17f7zu97865jmknk7p2glqvxzhduk78772ezac5",
  "spectrum_mirror_farms_addr": "terra1kehar0l76kzuvrrcwj5um72u3pjq2uvp62aruf",
  "spectrum_staker_addr": "terra1fxwelge6mf5l6z0rjpylzcfq9w9tw2q7tewaf5",
  "terraswap_factory_addr": "terra1ulgw0td86nvs4wtpsc80thv6xelk76ut7a7apj",
}
```


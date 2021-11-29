# Aperture Finance - Terra Private Beta

This monorepository contains the source code for the core smart contracts implementing Aperture Protocol on the [Terra](https://terra.money) blockchain.

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

### Unit / Integration Tests

Each contract contains Rust unit and integration tests embedded within the contract source directories. You can run:

```sh
cargo unit-test
cargo integration-test
```

### Compiling

After making sure tests pass, you can compile each contract with the following:

```sh
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/{contract_module}.wasm .
ls -l {contract_module}.wasm
sha256sum {contract_module}.wasm
```

#### Production

For production builds, run the following:

```sh
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.11.5
```

This performs several optimizations which can significantly reduce the final size of the contract binaries, which will be available inside the `artifacts/` directory.

## Development

To build:
```
cargo wasm
```

Useful code-health tools:
```
cargo fmt
cargo clippy -- -D warnings
```

## Deployment

### Testnet (bombay-12)

Test controller address "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g" is generated from the seed phrase in "seed.txt".

```json
{
  "controller": "terra1ads6zkvpq0dvy99hzj6dmk0peevzkxvvufd76g",
  "anchor_ust_cw20_addr": "terra1ajt556dpzvjwl0kl5tzku3fc3p3knkg9mkv8jl",
  "mirror_cw20_addr": "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u",
  "spectrum_cw20_addr": "terra1kvsxd94ue6f4rtchv2l6me5k07uh26s7637cza",
  "anchor_market_addr": "terra15dwd5mj8v59wpj0wvt233mf5efdff808c5tkal",
  "mirror_collateral_oracle_addr": "terra1q3ls6u2glsazdeu7dxggk8d04elnvmsg0ung6n",
  "mirror_lock_addr": "terra1pcxghd4dyf950mcs0kmlp7lvnrjsnl6qlfldwj",
  "mirror_mint_addr": "terra1s9ehcjv0dqj2gsl72xrpp0ga5fql7fj7y3kq3w",
  "mirror_oracle_addr": "terra1uvxhec74deupp47enh7z5pk55f3cvcz8nj4ww9",
  "mirror_staking_addr": "terra1a06dgl27rhujjphsn4drl242ufws267qxypptx",
  "spectrum_gov_addr": "terra1x3l2tkkwzzr0qsnrpy3lf2cm005zxv7pun26x4",
  "spectrum_mirror_farms_addr": "terra1hasdl7l6xtegnch8mjyw2g7mfh9nt3gtdtmpfu",
  "spectrum_staker_addr": "terra15nwqmmmza9y643apneg0ddwt0ekk38qdevnnjt",
  "terraswap_factory_addr": "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
}
```

### Mainnet (columbus-5)

```json
{
  "controller": "terra...",
  "anchor_ust_cw20_addr": "terra1hzh9vpxhsk8253se0vv5jj6etdvxu3nv8z07zu",
  "mirror_cw20_addr": "terra15gwkyepfc6xgca5t5zefzwy42uts8l2m4g40k6",
  "spectrum_cw20_addr": "terra1s5eczhe0h0jutf46re52x5z4r03c8hupacxmdr",
  "anchor_market_addr": "terra1sepfj7s0aeg5967uxnfk4thzlerrsktkpelm5s",
  "mirror_collateral_oracle_addr": "terra1pmlh0j5gpzh2wsmyd3cuk39cgh2gfwk6h5wy9j",
  "mirror_lock_addr": "terra169urmlm8wcltyjsrn7gedheh7dker69ujmerv2",
  "mirror_mint_addr": "terra1wfz7h3aqf4cjmjcvc6s8lxdhh7k30nkczyf0mj",
  "mirror_oracle_addr": "terra1t6xe0txzywdg85n6k8c960cuwgh6l8esw6lau9",
  "mirror_staking_addr": "terra17f7zu97865jmknk7p2glqvxzhduk78772ezac5",
  "spectrum_gov_addr": "terra1dpe4fmcz2jqk6t50plw0gqa2q3he2tj6wex5cl",
  "spectrum_mirror_farms_addr": "terra1kehar0l76kzuvrrcwj5um72u3pjq2uvp62aruf",
  "spectrum_staker_addr": "terra1fxwelge6mf5l6z0rjpylzcfq9w9tw2q7tewaf5",
  "terraswap_factory_addr": "terra1ulgw0td86nvs4wtpsc80thv6xelk76ut7a7apj"
}
```

## Testing with terrad

### Testnet (bombay-12)
mETH cw20 address: "terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"

A test contract has been instantiated at "terra1zcdpycechzx0l7x7yyg2f5tpg3pkszepnhm4ha".

```console
terrad tx wasm execute terra1zcdpycechzx0l7x7yyg2f5tpg3pkszepnhm4ha '{"delta_neutral_invest":{"collateral_ratio_in_percentage":"250","mirror_asset_cw20_addr":"terra1ys4dwwzaenjg2gy02mslmc96f267xvpsjat7gx"}}' "10000000000uusd" --from test --chain-id=bombay-12 --gas=auto --fees=2000000uusd --gas-adjustment=2
```

## Demo
```json
{
    "amadeus_addr": "terra1czsy798tyws9lh0a8rpx340625ety23vy5v4ml",
    "wormhole_token_bridge_addr": "terra10nmmwe8r3g99a9newtqa7a75xfgs2e8z87r2sf"
}
```
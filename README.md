# Aperture Finance

This monorepository contains the source code for the core smart contracts implementing Aperture Protocol, a cross-chain investment platform.

[![codecov](https://codecov.io/gh/Aperture-Finance/Aperture-Contracts/branch/protocol/graph/badge.svg?token=EOJNHFN2Y1)](https://codecov.io/gh/Aperture-Finance/Aperture-Contracts)

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

#### Production

For production builds, run the following:

M1 Mac (arm64):
```
./build_arm64.sh
```

OR

```sh
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer-arm64:0.12.4
```

Intel/AMD (amd64):
```sh
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.4
```

This performs several optimizations which can significantly reduce the final size of the contract binaries, which will be available inside the `artifacts/` directory.
The arm64 and amd64 optimizers will produce different wasm byte codes; however, either one can be safely deployed to Terra networks for production.

Note that Docker does not support IPv6 out of the box on Mac, so switch to IPv4 when possible; otherwise you may receive a "service unavailable" error when Docker attempts to fetch images.

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

TODO

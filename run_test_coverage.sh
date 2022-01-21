#!/bin/sh
cargo install cargo-tarpaulin
cargo +nightly tarpaulin --verbose --workspace --timeout 120 --out Xml --exclude spectrum-protocol --exclude-files msg_instantiate_contract_response.rs

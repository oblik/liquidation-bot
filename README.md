# Liquidation Bot

This project is an experimental Aave v3 liquidation bot targeting the Base network.  
It is written in Rust and uses the [alloy](https://github.com/alloy-rs/alloy) crates for
Ethereum connectivity instead of `ethers-rs`.

The current prototype demonstrates how to query a user's health factor from the
Aave pool contract deployed on Base.  Environment variables are used to specify
RPC connection details and the target account.

## Quick Start

1. Install Rust (1.70+).
2. Set up a `.env` file with at least `RPC_URL` pointing to a Base RPC endpoint
   and optional `TARGET_USER` with the user address to inspect.
3. Run:

```bash
cargo run --release
```

The program will print the raw values returned by `getUserAccountData` and flag
whether the provided user address is undercollateralized.

This repository is a starting point for a more complete liquidation bot which
will integrate flash loans and real-time monitoring as described in the project
notes.

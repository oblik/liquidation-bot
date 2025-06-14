# Liquidation Bot Roadmap

This document outlines the steps needed to turn the current prototype into a fully featured Aave v3 liquidation bot on the Base network. The plan is derived from the deep research notes.

## Current Status

- Rust workspace created using **alloy-rs** for Ethereum interactions.
- Prototype in `src/main.rs` connects to Base RPC, loads the L2Pool ABI, and prints the health factor of a target user.
- Basic README with quick-start instructions.

This confirms that on-chain calls through alloy work. Everything else remains to be implemented.

## Next Steps

### 1. Smart Contract for Flash Loan Liquidations
- Write a Solidity contract (`AaveLiquidator`) implementing `IFlashLoanReceiver`.
- Integrate with Aave's L2Pool flash loan function to borrow the debt asset.
- Execute `liquidationCall` using the packed parameters for Base.
- Include swap logic (e.g. via Uniswap) to convert seized collateral back to the debt asset within the same transaction.
- Restrict callable functions with an `onlyOwner` modifier and implement `withdraw()` to reclaim profits.

### 2. Rust Bindings and Execution Module
- Generate bindings for the new contract with alloy build tooling.
- Implement a `liquidate()` function in Rust to send transactions to the contract, estimating gas and using adjustable gas prices.
- Handle pending transaction tracking and confirmation.

### 3. Monitoring and Data Pipeline
- Subscribe to Aave Pool events (Borrow, Repay, Supply, Withdraw) over WebSockets to update user positions in real time.
- Periodically poll the Chainlink price oracle for volatile assets.
- Maintain an in-memory or database-backed list of users near the liquidation threshold.
- Optionally seed the watch list from The Graph's subgraph at startup.

### 4. Profitability Estimation
- Fetch liquidation bonus and close factor parameters from on-chain or static config.
- Calculate expected profit from repaying a user's debt after accounting for flash loan fees, estimated swap slippage, and gas cost.
- Skip opportunities that do not meet the minimum profitability threshold.

### 5. Concurrency and Task Orchestration
- Use Tokio tasks and channels to decouple monitoring from execution.
- Ensure no duplicate liquidation attempts occur for the same user.
- Support parallel liquidations across independent signers if needed.

### 6. Persistence and Logging
- Integrate `sqlx` with SQLite or Postgres to store user data and executed liquidations.
- Add structured logging with the `tracing` crate.
- Optionally expose Prometheus metrics for monitoring.

### 7. Deployment and Operations
- Containerize the bot with Docker and provide a sample `docker-compose` setup.
- Document environment variables (RPC endpoints, private key, DB URL, etc.).
- Implement alerting/heartbeat mechanism so the operator knows the bot is running.

### 8. Testing and Simulation
- Deploy contracts and run the bot on Base testnet or a local fork for end-to-end testing.
- Use Tenderly or anvil fork to simulate historical scenarios and verify profit calculations.
- Backtest using historical liquidation data (from Dune or The Graph) to tune parameters.

### 9. Continuous Improvement
- Track success and failure rates to refine gas pricing strategy and latency.
- Update asset mappings and ABI files whenever Aave upgrades or adds new reserves.
- Periodically withdraw accumulated profits to a cold wallet for security.

## Conclusion

Completing these milestones will transform the current simple health factor check into a production-grade liquidation bot. Each phase builds on the previous one, with the smart contract and monitoring system being the next major pieces to implement.

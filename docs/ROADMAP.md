# Liquidation Bot Roadmap

This document outlines the development status and next steps for the Aave v3 liquidation bot.

## Phase 1: Core Infrastructure (COMPLETED)

- [x] **Smart Contract**: `AaveLiquidator.sol` contract written and deployed to Base Sepolia testnet at `0x4818d1cb788C733Ae366D6d1D463EB48A0544528`. Includes flash loan receiver, L2Pool integration, and security features.
- [x] **Rust Bot Foundation**: Initial prototype in `src/main.rs` connecting to Base RPC and checking a single user's health factor.
- [x] **Configuration & Logging**: Initial setup for environment variables and `tracing`.

## Phase 2: Real-Time Monitoring (IN PROGRESS)

### Completed
- [x] **WebSocket Event Subscriptions**: Subscribes to all Aave Pool events (`Borrow`, `Repay`, `Supply`, etc.) for real-time data.
- [x] **Dynamic User Discovery**: The bot now automatically discovers and monitors any user who interacts with the Aave protocol.
- [x] **Database Persistence**: Integrated `sqlx` with SQLite (and PostgreSQL support) to store user positions, bot events, and liquidation history.
- [x] **Concurrent Architecture**: Re-architected into a multi-task `tokio` application for high-performance, non-blocking monitoring.
- [x] **Graceful Fallback**: The bot intelligently detects if a WebSocket connection is available and falls back to HTTP polling if not.

### Next Steps
- [ ] **Oracle Price Monitoring**: Integrate with Chainlink price feeds to monitor for price changes that affect health factors, providing a second trigger for liquidations beyond direct user actions.
- [ ] **Profitability Calculation**: Implement logic to accurately estimate the profitability of a liquidation opportunity, considering the liquidation bonus, flash loan fees, DEX swap slippage, and gas costs.
- [ ] **Liquidation Execution**: Implement the final logic to call the `AaveLiquidator` smart contract, sending a transaction to execute a liquidation when a profitable opportunity is found.

## Phase 3: Production Hardening & Optimization (UPCOMING)
- [ ] **Advanced Error Handling & Retries**: Implement more robust strategies for handling failed transactions and RPC endpoint issues.
- [ ] **Gas Price Strategy**: Develop a dynamic gas pricing model to ensure transactions are mined competitively without overpaying.
- [ ] **Multi-Asset & Multi-Collateral Logic**: Enhance profitability calculations to choose the best debt asset to repay and collateral to seize.
- [ ] **Testing & Simulation**: Create a framework for backtesting strategies against historical data and simulating liquidations on forked networks.
- [ ] **Containerization**: Provide a `Dockerfile` and `docker-compose` setup for easy deployment.
- [ ] **Alerting & Dashboards**: Integrate with services like Prometheus, Grafana, or messaging apps to provide real-time alerts and performance dashboards.
- [ ] **Security Audits**: Perform thorough security reviews of both the smart contract and the off-chain bot.
- [ ] **Continuous Improvement**: Ongoing work to refine strategies, update to new Aave versions, and manage profits securely.

## Conclusion

The core monitoring engine is now complete and functional. The next critical steps involve integrating price oracle data and building the profitability models to allow the bot to make informed, autonomous liquidation decisions.

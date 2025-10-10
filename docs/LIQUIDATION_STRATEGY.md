# Liquidation Bot Strategy

This document describes the end-to-end strategy the liquidation-bot codebase is designed to execute. It synthesizes the architecture, roadmap targets, and custom liquidation concepts that guide development efforts.

## Strategic Objective

Deliver a production-ready Base mainnet liquidation platform for Aave v3 that can discover, evaluate, and execute profitable liquidations faster than competing bots while maintaining operational resilience. The system combines a high-performance Rust bot, dedicated smart contracts, and persistent storage so the team can safely capture undercollateralized positions at scale.

## Core Pillars

1. **Real-Time Market Awareness** – Maintain continuous coverage of Aave positions and oracle feeds so health-factor degradations are detected within seconds. The monitoring stack relies on WebSocket subscriptions with HTTP polling and scanner fallbacks to ensure events and price changes are captured even when providers degrade.【F:docs/ARCHITECTURE.md†L7-L75】【F:docs/ROADMAP.md†L22-L63】
2. **Deterministic Decisioning** – Centralize profitability and risk evaluation in the Rust decision engine. Health-factor analysis, liquidation bonus calculations, and configurable profit thresholds ensure only high-confidence opportunities are executed.【F:docs/ARCHITECTURE.md†L76-L132】【F:docs/ROADMAP.md†L52-L79】
3. **Atomic Execution & Capital Efficiency** – Use the deployed flash-loan-enabled Solidity liquidator to repay debt, seize collateral, and unwind positions within a single transaction. Bundling swaps through Uniswap and leveraging a mix of treasury capital and flash liquidity maximizes per-block profit capture.【F:docs/custom_liquidation_strategy_concept.md†L1-L63】
4. **Operational Resilience** – Harden the bot with retry logic, circuit breakers, and telemetry so it can withstand node hiccups and extreme market volatility while meeting uptime, latency, and profitability targets defined in the roadmap.【F:docs/ROADMAP.md†L81-L147】【F:docs/ROADMAP.md†L193-L215】

## Strategic Workflow

1. **Discovery**
   - Subscribe to Aave Pool events (Borrow, Supply, Repay, Withdraw, LiquidationCall) and Chainlink price feeds via WebSocket channels.
   - Periodically backfill state via HTTP polling and scheduled scans to catch missed events.
   - Persist user positions, event history, and configuration metadata in SQLite/PostgreSQL for replayable analytics.【F:docs/ARCHITECTURE.md†L7-L132】【F:docs/ROADMAP.md†L52-L79】

2. **Evaluation**
   - Recalculate user health factors when collateral or price changes occur.
   - Run profitability simulations that account for liquidation bonuses, flash-loan fees, gas forecasts, and slippage protections.
   - Filter to high-liquidity markets (WETH, USDC, cbETH, etc.) to avoid adverse fills, and stage multi-market bundles when a borrower has multiple debts.【F:docs/ROADMAP.md†L52-L79】【F:docs/custom_liquidation_strategy_concept.md†L1-L79】

3. **Execution**
   - Route qualified opportunities through the Solidity liquidator which borrows via flash loans, repays protocol debt, seizes collateral, and swaps it atomically.
   - Prioritize gas bidding strategies suited to Base’s low-fee environment and submit transactions via private relays when MEV resistance is required.
   - Track confirmations, reconcile database state, and withdraw realized profits automatically.【F:docs/ARCHITECTURE.md†L27-L55】【F:docs/custom_liquidation_strategy_concept.md†L31-L63】

4. **Feedback & Hardening**
   - Record execution outcomes and performance metrics (latency, profit, gas efficiency) to feed dashboards and future optimizations.
   - Trigger circuit breakers and alerting workflows during abnormal volatility or persistent failures.
   - Iterate towards roadmap KPIs: 99.9% uptime, sub-5s liquidation latency, and >95% profitable executions.【F:docs/ROADMAP.md†L81-L147】【F:docs/ROADMAP.md†L193-L215】

## Differentiating Tactics

- **Atomic Multi-Liquidations** – Batch multiple debt repayments inside one flash-loan-funded transaction to clear entire positions and reduce frontrunning risk.【F:docs/custom_liquidation_strategy_concept.md†L1-L35】
- **Selective Asset Targeting** – Focus on deep-liquidity collateral markets and skip exotic assets where slippage can erase the liquidation incentive.【F:docs/custom_liquidation_strategy_concept.md†L35-L55】
- **Hybrid Capital Allocation** – Combine on-hand treasury assets with flash loans for rapid small liquidations and scalable large opportunities.【F:docs/custom_liquidation_strategy_concept.md†L55-L79】
- **Cross-Protocol Awareness** – Monitor multiple Base lending venues to sweep borrower risk across ecosystems, maximizing capital efficiency and competitive edge.【F:docs/custom_liquidation_strategy_concept.md†L79-L103】

## Alignment with the Roadmap

- **Phase 2 Completion** – Current codebase already supports real-time monitoring, profitability calculations, and automated execution, satisfying the Phase 2 completion criteria.【F:docs/ROADMAP.md†L22-L79】
- **Phase 3 Focus** – Active development is targeting production hardening (retry logic, circuit breakers), gas optimization (dynamic pricing, MEV protection), expanded asset coverage, and comprehensive testing frameworks.【F:docs/ROADMAP.md†L81-L179】
- **Success Metrics** – All enhancements aim at the roadmap KPIs: uptime, latency, profitability, and coverage goals measured across Base network assets.【F:docs/ROADMAP.md†L193-L215】

This strategy document should guide contributors in prioritizing work that tightens the feedback loop between monitoring, decisioning, and execution while building the resilience and optimizations required for sustained dominance in Base liquidations.

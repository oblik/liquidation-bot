# Designing a High-Performance Aave v3 Liquidation Bot on Base (Ethereum L2)

## Introduction

Building a DeFi liquidation bot for Aave v3 on the Base network (an Ethereum L2) requires in-depth knowledge of Aave’s liquidation mechanics, real-time on-chain monitoring, flash loan integration, and robust Rust development practices. This report provides a comprehensive blueprint – from understanding Aave v3 liquidation conditions and smart contract interfaces to implementing an atomic flash-loan-powered liquidation in Rust. We cover how to fetch live health factor data, design a modular bot architecture, integrate essential Rust libraries, and ensure 24/7 reliable operation on Base. Implementation-ready details (including code snippets and system diagrams) are provided to avoid superficial guidance.

Scope: We focus on the Aave v3 Base deployment (launched Aug 2023 ￼) and incorporate up-to-date considerations from 2024–2025 (e.g. Aave v3.3 improvements). Real-world Base network specifics – like fast 2s blocks and current liquidity profiles – are discussed where relevant. All content is technical, actionable, and oriented towards a Rust-based implementation.

## Aave v3 Liquidation Mechanics on Base

Health Factor & Liquidation Conditions: In Aave, each borrower has a health factor (HF) representing the ratio of collateral value to borrow value (adjusted by liquidation thresholds). If HF drops below 1.0, the position becomes undercollateralized and eligible for liquidation ￼ ￼. The health factor is calculated as total collateral value (in ETH terms) * liquidation threshold / total debt value ￼. A value under 1 indicates the debt exceeds the collateral’s allowable borrow, so anyone can liquidate that account.

Identifying Undercollateralized Positions: A liquidator must identify accounts where healthFactor < 1 ￼. The Aave v3 Pool contract provides a view function getUserAccountData(address user) that returns an account’s total collateral, total debt, and current HF ￼. For example, calling this on a user yields a tuple ending with the health factor (scaled by 1e18). If the returned HF is below 1e18 (i.e. <1.0), the account can be liquidated ￼ ￼. Listing 1 shows a snippet of using alloy-rs in Rust to check a user’s health factor via on-chain call:

```rust
use alloy::{
    providers::{Provider, ProviderBuilder},
    primitives::{Address, U256},
    signers::local::PrivateKeySigner,
    sol,
};
use std::env;

// Define the Aave Pool contract interface using the sol! macro
sol! {
    interface IAavePool {
        function getUserAccountData(address user) external view returns (
            uint256 totalCollateralBase,
            uint256 totalDebtBase,
            uint256 availableBorrowsBase,
            uint256 currentLiquidationThreshold,
            uint256 ltv,
            uint256 healthFactor
        );
    }
}
 
#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Load ENV (RPC_URL, PRIVATE_KEY, etc.)
    dotenv::dotenv().ok();
    let rpc_url = env::var("RPC_URL")?;
    let private_key = env::var("PRIVATE_KEY")?;
    
    // Create signer from private key
    let signer: PrivateKeySigner = private_key.parse()?;
    
    // Build provider with wallet
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(signer)
        .on_http(rpc_url.parse()?);
 
    // Aave V3 Pool (or L2Pool) contract address on Base:
    let pool_address: Address = "0x...PoolAddressOnBase...".parse()?;
    let pool = IAavePool::new(pool_address, &provider);
 
    // Query user account data to get health factor
    let user: Address = "0x...TargetUserAddress...".parse()?;
    let result = pool.getUserAccountData(user).call().await?;
    
    println!("User health factor: {}", result.healthFactor);
    if result.healthFactor < U256::from(10u128.pow(18)) {
        println!("Account {} is undercollateralized!", user);
    }
}
```

Liquidation Thresholds and Close Factors: Aave v3 improved upon v2 by sometimes allowing 100% of a debt to be liquidated at once. Specifically, if a position’s health factor is very low (at or below a protocol-defined threshold, e.g. HF ≤ 0.95), the liquidator can repay up to 100% of the debt in one go ￼. Otherwise, if 0.95 < HF < 1 (just below solvency), Aave limits a single liquidation transaction to at most 50% of the debt (the “close factor”) ￼. On Aave v3, this threshold (often HF = 0.95) differentiates small vs. large liquidations: for HF <= 0.95, up to 100% of debt can be repaid; for HF > 0.95 (but <1), up to 50% can be repaid ￼. The bot must calculate the allowable debt to cover accordingly. If we want to liquidate the maximum, we can even pass uint(-1) (max uint) as the debtToCover parameter to Aave’s liquidation function, which Aave interprets as “cover as much as allowed by close factor” ￼ ￼.

L2Pool vs. Pool Contracts: On Base (an Optimism-stack L2), Aave uses an optimized L2Pool contract for user actions. The L2Pool provides the same external functions as the normal Pool (supply, borrow, repay, liquidate, etc.), but with parameters packed into bytes32 to save calldata ￼ ￼. For example, L2Pool.liquidationCall(bytes32 args1, bytes32 args2) packs the collateral asset, debt asset, user address, debt amount, and a boolean into two 32-byte values ￼ ￼. On Base, our bot should interact with the L2Pool’s liquidationCall function (instead of the L1 Pool’s version) to minimize gas. Aave’s documentation confirms that on L2 networks (Optimism, Base, etc.), one should call L2Pool.liquidationCall() with encoded params ￼. We can use Aave’s provided L2Encoder library or contract to encode these parameters correctly ￼ ￼.

LiquidationCall Interface: The liquidation function signature on L2 is essentially:

function liquidationCall(bytes32 args1, bytes32 args2) external;

Where:
- args1 encodes uint16 collateralAssetId, uint16 debtAssetId, and the 160-bit user address ￼.
- args2 encodes uint128 debtToCover (truncated or max to cover allowed) and a boolean flag for receiveAToken (whether to receive collateral as aTokens or underlying) ￼.

On Aave v3 Base, each supported asset has an internal asset ID (0, 1, 2, …). For example, if WETH is asset 0 and USDC is asset 1 in the reserves list, to liquidate a USDC debt using WETH collateral, you might have collateralAssetId=0, debtAssetId=1. The bot can fetch the asset IDs from the protocol’s PoolAddressesProvider/reserves list, or simply hardcode a mapping of asset address to ID (since the set of assets is static per market). Using alloy-rs, we can either call the L2Encoder.encodeLiquidationCall() via a view call or implement the packing in Rust. Once the two bytes32 args are prepared, we send a transaction to L2Pool.liquidationCall(args1, args2) to perform the liquidation.

Liquidation Bonus: If the call succeeds, Aave will transfer a portion of the collateral to the liquidator as a reward, at a discount (the “liquidation bonus”). For example, if the bonus is 5%, the liquidator who repays debt gets collateral worth 105% of the repaid amount (so effectively a 5% profit in collateral value) ￼. Each asset’s bonus % is set by Aave governance based on risk; the bot should retrieve these parameters (from on-chain ProtocolDataProvider or static config) to compute expected profit from a given liquidation.

Example: Suppose on Base, a user has borrowed 1000 USDC against collateral 0.5 WETH. If WETH price drops such that the user’s HF falls to 0.9, the position is liquidatable. The bot could repay up to 50% of the debt (since 0.9 > 0.95 threshold) – say it repays 500 USDC. With a 5% bonus, the bot can claim ~$525 USDC worth of WETH. If 500 USDC of debt repayment results in 0.275 WETH seized (worth $525), the profit in theory is $25 in WETH (minus fees). The actual profit depends on gas and slippage when swapping that WETH to recover USDC or ETH.

LiquidationCall Execution Requirements: To call liquidationCall, the liquidator must supply the debt asset being repaid. You cannot pull the debt amount from the protocol – you need to possess (or borrow via flash loan) the required amount of USDC (in the example above) to repay the user’s loan ￼. The bot therefore needs access to liquidity for the debt asset, which is where flash loans come in (unless the bot maintains its own inventory of assets, which ties up capital). We cover flash loan integration in a later section. Also note the liquidator can choose to receive the collateral as aTokens or the underlying by the receiveAToken flag ￼. Typically, one would take the underlying token (e.g. actual WETH) rather than aTokens, so it can be immediately swapped or used.

Aave v3.3 Updates: As of Feb 2025, Aave v3.3 introduced some refinements to liquidation logic (like handling of insolvent positions via “burn” mechanism) ￼. For our bot, this mostly means the protocol can write off small dust debts via the ecosystem’s Safety Module, but the core liquidation process for liquidators remains the same. It’s still triggered by HF < 1 and executed via the Pool/L2Pool contract by third-party bots. The 100% close factor if HF < 0.95 rule remains in effect ￼. Our bot should be aware that in edge cases where a position is so small or underwater that no one liquidates it (unprofitable), Aave may handle it separately (via the “logging and burn” of bad debt). Those cases (e.g. HF << 1 but tiny value) are rare and usually occur if gas costs exceed collateral value ￼. The bot can safely ignore them or use a minimum profitability check (so it doesn’t waste gas on dust positions that Aave will eventually absorb).

## Real-Time Position Monitoring and Data Access

To act quickly on undercollateralized positions, the bot needs real-time data on borrower health. We employ a combination of on-chain event subscriptions, direct contract reads, and The Graph (GraphQL) queries to maintain an updated view of all pertinent positions.

On-Chain Event Subscriptions: Aave v3 emits events on every significant action – e.g. Supply(address user, …), Borrow(address user, …), Repay(address user, …), ReserveDataUpdated(asset, …), etc. By listening to these events on the Base network, the bot can maintain a map of user positions. Concretely, we can subscribe to Borrow events to catch when users take new loans (or increase debt), and Price Oracle updates to catch when collateral values change. Each time an event indicates a user’s borrow or collateral changed, we recalc or fetch their health factor. Aave does not emit a direct “HealthFactorChanged” event, but we can derive it:
- Borrow/Repay/Supply/Withdraw events: These indicate changes in a user’s debt or collateral. We parse the event to get the user address and the new balances (some events include the new total, others we might need to track deltas). After processing, we call getUserAccountData(user) to get the up-to-the-block HF.
- Oracle price events: If the price of a volatile asset drops, many positions’ HFs will drop even if no action was taken by the user. Aave v3 uses Chainlink oracles – the Base deployment uses a Price Oracle contract (e.g. Chainlink aggregator proxy) that likely emits a price update event whenever a feed updates. Our bot can subscribe to these oracle events (for assets used as collateral) and trigger HF recomputation for all users holding that collateral. This is complex if done on-chain for every user; instead, a strategy is to fetch from The Graph all users who have that asset as collateral and recompute or simply check their HF via contract. A simpler heuristic: price drops usually affect many accounts, so in practice the bot might just scan all active borrowers periodically in volatile times.

Using alloy-rs with WebSockets, we can subscribe to events. For example:

// Subscribe to Borrow events on Aave Pool using sol! macro
use alloy::{
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Filter,
    sol,
};

// Define the Borrow event using sol! macro
sol! {
    event Borrow(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint8 interestRateMode,
        uint256 borrowRate,
        uint16 indexed referralCode
    );
}

let ws_provider = ProviderBuilder::new()
    .on_ws(WsConnect::new(WSS_URL))
    .await?;

let filter = Filter::new()
    .address(pool_address)
    .event_signature(Borrow::SIGNATURE_HASH);

let sub = ws_provider.subscribe_logs(&filter).await?;
let mut stream = sub.into_stream();

while let Some(log) = stream.next().await {
    // Parse log into strongly-typed event using alloy
    if let Ok(borrow_event) = Borrow::decode_log(&log) {
        let user = borrow_event.user;
        // Now trigger health check for user
        check_health_and_maybe_liquidate(user).await?;
    }
}

This pseudocode subscribes to Borrow events from the Aave Pool. When a borrow occurs, we extract the borrower’s address and then call our check_health_and_maybe_liquidate logic to see if their HF fell below 1 (for example, if they borrowed an asset and their collateral was insufficient, though Aave normally wouldn’t allow borrow that immediately makes HF<1). In reality, most liquidations occur due to price changes, so monitoring oracle updates or simply periodically scanning is crucial.

Periodic On-Chain Scans: As a backup (or primary method in simpler implementation), the bot can periodically iterate over a list of “at-risk” borrowers and update their health factors. The set of all borrowers can be large, so we maintain a focused set: e.g. addresses with HF below a certain threshold (say <1.1) or with high borrow utilization. We can seed this list from historical data or from subgraph (see below), and update it over time (adding anyone who takes a large borrow).

The Graph Subgraph Queries: Aave v3 has subgraphs (community and Messari versions) that index all positions. We can leverage The Graph’s GraphQL API to query for users with low health. For Aave v3 on Base, the subgraph (e.g. aave-v3-base by Messari) likely provides a User entity with fields like totalCollateralETH, totalDebtETH, and maybe healthFactor (or we can compute HF = totalCollateralETH * avgLiquidationThreshold / totalDebtETH). If healthFactor is stored, a GraphQL query could directly get all users with healthFactor < 1. If not stored, we might query all users with debt and compute it off-chain. Example GraphQL query (hypothetical):

{
  users(where: { totalBorrowsETH_gt: 0 }) {
    id
    totalCollateralETH
    totalBorrowsETH
    liquidationThreshold
    healthFactor
  }
}

This could return a list of users and their health. We could filter or sort by healthFactor ascending to identify the most at-risk positions (healthFactor approaching 1). However, note that subgraph data is not real-time – there’s typically a slight delay (a few seconds to a minute) and healthFactor might be computed at time of indexing. The Aave docs caution that GraphQL “does not provide real-time calculated user data such as healthFactor,” and suggests computing it locally using an SDK ￼. So, a more reliable approach: use subgraph to get the list of all borrowers and their balances, but compute HF ourselves with fresh on-chain prices.

### Combining Approaches – Monitoring Pipeline
 A high-performance bot will combine event-driven updates with periodic checks:
- Startup: Query subgraph for all active borrowers and their collateral/debt balances. Store these in a local SQLite/Postgres DB (with schema: user, collateral assets & amounts, debt assets & amounts, lastHealth, etc).
- Event Handling: Use WebSocket subscriptions to Aave Pool events:
- On Borrow or Withdraw (collateral removal): immediately recompute that user’s HF (via on-chain call) – their HF likely worsened.
- On Repay or Supply: HF likely improved or risk mitigated, but we can update it.
- Optionally on LiquidationCall events: if another bot liquidates a user, update that user’s record (their debt will drop and collateral drop).
- Oracle/Price Feeds: If possible, subscribe to price feed updates. In absence of direct events, a simpler method is to poll the Chainlink oracle contract for each asset’s price periodically (e.g. every few seconds) and if a significant price change (beyond a threshold) is observed, trigger a scan of all users (or all users holding that asset as collateral). This can be optimized by focusing on assets with high volatility or large borrow markets.
- Periodic full scan: Perhaps every N blocks (e.g. every minute or when idle), iterate through the list of users with HF < 1.2 (a buffer) and refresh their HF via getUserAccountData. This ensures the bot doesn’t miss someone due to a missed event (e.g., if an oracle update wasn’t caught).

### GraphQL for Analytics
 The Graph is also useful for analytics and backtesting rather than real-time actions. For instance, we can query historical liquidation events or track how HF of certain users changed over time (to identify patterns). During development, you might use GraphQL to find “who is close to liquidation now?” for manual verification, but in production the bot should rely on direct on-chain reads for accuracy and speed.

### Aave Utilities SDK
 Aave provides a utility library (@aave/math-utils in JavaScript, and a GitHub aave-utilities repository) that can calculate health factor and other user data off-chain ￼. This might not have a Rust equivalent, but one can port the formula or simply use the on-chain call as we did. The formula for HF is repeated here for clarity:

\[ \text{HealthFactor} = \frac{\sum_{\text{collateral}} ( \text{collateral amount} \times \text{price} \times \text{ltv\_of\asset})}{\sum{\text{borrowed}} (\text{debt amount} \times \text{price})} \]
![HealthFactor Formula](docs/healthfactor-formula.png)

If this falls below 1, liquidation is allowed ￼. Our bot doesn’t need to manually compute this often if we trust getUserAccountData, but understanding it is useful for sanity checks and simulations.

### Performance Consideration
 Base network has 2-second block times (often ~2s per block) ￼. This means our bot must detect and act on liquidations extremely fast – potentially within a second or two of HF dropping below 1 – because competing bots will do the same. Using event subscriptions and prompt RPC calls is crucial. We should avoid heavy synchronous loops that could lag; instead utilize parallelism (Tokio tasks) to handle events concurrently. More on this in the architecture section.

## Flash Loan Integration for Atomic Liquidations

Liquidating profitably often requires flash loans to source capital for debt repayment. A flash loan allows the bot to borrow the needed funds within the same transaction, use them for repayment, and return them (with a small fee), all atomically. This avoids the bot needing to pre-hold large amounts of each asset.

Aave Flash Loans: Aave v3’s Pool contract includes a flashLoan() function that any contract can call, specifying: the receiver contract, the assets and amounts to borrow, and modes (for flash loan, interestRateMode is usually 0 since it’s paid back immediately) ￼ ￼. The caller must be a contract implementing the IFlashLoanReceiver interface (specifically, the contract should have an executeOperation() function that Aave will callback to during the flash loan) ￼. The sequence is:
	1.	Bot calls Aave’s flashLoan on the Pool, pointing to its own Liquidator contract as the receiver.
	2.	Aave transfers the requested assets to the Liquidator contract, then immediately calls executeOperation(address[] assets, uint256[] amounts, uint256[] premiums, address initiator, bytes params) on that contract.
	3.	In executeOperation, our contract contains the logic to perform the liquidation:
- Use the borrowed asset (e.g. USDC) to call L2Pool.liquidationCall(...) to repay the target user’s debt and receive collateral.
- Optionally, if collateral and debt assets differ, swap the collateral for the debt asset using a DEX (within the same contract call).
- Ensure after the swap we have at least amount + premium of the debt asset to return to Aave.
- Return true (or no error) from executeOperation to indicate success.
	4.	If the flash-loan + liquidation sequence is successful, the contract repays the flash loan plus a premium (premium is typically 0.09% of amount ￼ ￼ for Aave v3). Aave then closes out the flash loan and the transaction can complete. Any leftover collateral or asset in our contract at the end of executeOperation is our profit.

Flash Loan Example Flow: See diagram below — the bot triggers a flash loan and liquidation in one atomic transaction. 

![Flash Loan Example Flow](docs/flash-loan-example-flow.png)


The Liquidation Bot (Rust) sends a transaction to its Liquidator Contract (Solidity), which requests a flash loan from Aave, gets the funds, then calls liquidationCall. Aave’s pool transfers the collateral to the Liquidator contract, which then swaps it via a DEX on Base (like Uniswap) to get back the borrowed asset. Finally, the borrowed asset (plus fee) is returned to Aave, and any remainder stays in the contract (and can be swept to the bot’s wallet as profit). This entire sequence happens within one block, ensuring atomicity – if anything fails (e.g., not enough collateral swapped to repay loan), the transaction reverts entirely.

### Base Network Flash Loan Options
 On Base, Aave itself is an excellent source of flash liquidity for assets listed in the Aave market (WETH, USDC, DAI, etc.). If an asset isn’t available or liquid enough in Aave, the bot could use other protocols:
- Uniswap v3 Flash Swaps: Uniswap v3 on Base might allow flash swapping of one asset for another (by calling a swap with zero input and using the callback to provide output – effectively a flash loan of one side). This requires implementing the IUniswapV3FlashCallback.
- Balancer or others: If Base has a Balancer or other AMMs with a flash-loan feature, those could be tapped. However, as of early 2024, Base’s DeFi is growing; Aave is likely the primary source for large flash loans on Base itself.
- Cross-chain flash loan: Not applicable because flash loans can’t be teleported cross-chain easily within one transaction.

### Using Aave’s Own Flash Loan
 since our target is an Aave liquidation, we can often borrow the exact asset needed from Aave itself. We must ensure the Pool has enough liquidity of that asset (e.g., if trying to liquidate a huge position, make sure Aave’s pool can lend that much in a flash loan – otherwise consider splitting the liquidation or using multiple sources).

### Implementing the Flash Loan Contract
 We will write a Solidity contract, e.g. AaveLiquidator.sol, that the bot deploys on Base. Key components of this contract:
- It holds an owner (the bot’s EOA address) and perhaps some allowances to spend tokens.
- It implements IFlashLoanReceiver interface, specifically the executeOperation function that Aave will call.
- It has a public function (callable by the owner/bot) like liquidate(address user, address debtAsset, address collateralAsset, uint256 debtToCover, bool receiveATokens) which kicks off the flash loan. This function will:
- Construct the parameters for Aave flashLoan: receiver = this contract, asset = debtAsset, amount = debtToCover (or portion needed), mode = 0 (no debt, full repayment), and a params bytes that encodes the user, collateralAsset, receiveATokens info we need in the callback.
- Call Aave’s POOL.flashLoanSimple(address receiver, address asset, uint256 amount, bytes calldata params, uint16 referralCode).
- In executeOperation(address asset, uint256 amount, uint256 premium, address initiator, bytes calldata params) returns (bool):
- Decode params to get user, collateralAsset, receiveATokens (and any other info).
- Approve the Aave Pool to use amount of asset (debt asset) for liquidation (since liquidationCall will pull the debt repayment from our contract).
- Call POOL.liquidationCall(collateralAsset, debtAsset, user, amount, receiveATokens) if using the standard Pool interface on L1, or for L2 Pool, call L2Pool.liquidationCall(args1, args2) with encoded params. (Alternatively, use Aave’s Pool interface even on Base – since Base’s L2Pool eventually forwards to Pool’s logic – but using L2Pool is gas-optimal).
- After that, we now hold some amount of collateralAsset (or aToken if receiveATokens was true; we usually would set receiveATokens = false to get underlying).
- If collateralAsset != asset (e.g. we borrowed USDC and got WETH collateral), we need to swap. Use an AMM: e.g., call Uniswap V3 exactInputSingle to swap WETH for USDC. Our contract must have approval to swap on Uniswap and possibly need a handle to a router. (This adds complexity – alternatively, we could have the bot compute if it’s profitable assuming a swap price and decide whether to liquidate, but doing the swap on-chain ensures we end with the correct asset to repay.)
- After swap, check that we have at least amount + premium of the asset to repay flash loan. If not, revert (transaction will abort – which is fine, it means either slippage or price moved).
- If yes, approve the Pool to pull amount + premium of asset and return true.
- The Aave Pool will then deduct amount+premium, completing the flash loan. Our contract will be left with whatever extra collateral or swap output remains after repaying. That is the profit. We can emit an event or just leave it for the bot to withdraw.

### Security
Security & Optimization: Optimization The executeOperation should be carefully written to handle reentrancy and edge cases. Also, because anyone can call our liquidate() function (unless we restrict it), we likely restrict it with an onlyOwner so that only our bot can trigger flash loans (to prevent griefing or weird use by others) ￼ ￼. We also must handle scenarios like the collateral swap not fully covering the debt (in which case the tx reverts and no loss except gas), or if the liquidationCall only partially liquidates (if we requested more than available). It’s wise to add logic to parse the LiquidationCall event or the return values (Aave’s liquidationCall might return actual repaid amount and collateral liquidated) to know how much collateral we received; this helps determine how much to swap.

### Gas and Fees Flash loans charge a fee (0.05% to 0.09% typically on Aave v3). Our profit calculation must account for this. For example, borrowing 1000 USDC will cost a 0.09% fee = 0.9 USDC. If our liquidation bonus yields, say, 5% of 1000 = 50 USDC in collateral value, paying ~0.9 USDC fee still leaves ~49.1 USDC gross profit – so flash loan fees are usually small relative to liquidation bonus, but not negligible.

Additionally, doing the liquidation atomically means we only pay gas for one transaction (which is higher due to internal steps, but we avoid two separate transactions for borrowing and repaying). Atomicity is crucial to avoid price changes or competition invalidating our trade mid-way. By the time our transaction is mined, either the entire sequence succeeds and is profitable, or it reverts with no loss (except gas).

### Alternative Non-Flash Strategy In some cases, the bot might choose to use its own capital if available to repay debt (avoiding the flash loan fee). For instance, if the bot’s wallet has a large balance of stablecoins, it could directly call liquidationCall from the EOA after swapping collateral in a separate transaction. However, this is riskier (needs two transactions: one to swap or source funds, one to liquidate), giving competitors a chance to intervene. Thus, the flash loan approach is preferred for most scenarios for guaranteed atomic execution.

### Integration in Rust The Rust bot interacts with the flash loan contract primarily by triggering its liquidate() function with appropriate parameters. Using alloy-rs, we can generate Rust bindings for our Liquidator contract (via the sol! macro) and call it. For example:

// Assume we have defined the Liquidator contract using sol! macro earlier
let liquidator = Liquidator::new(liquidator_address, client.clone());
let tx = liquidator.liquidate(user, debt_asset, collateral_asset, debt_to_cover, false)
    .gas_price(U256::from(gwei))  // set gas if needed
    .send().await?;
println!("Liquidation TX sent: {:?}", tx.tx_hash());

The bot would need to fill in debt_to_cover (ideally uint(-1) to liquidate max) and asset addresses. It monitors the transaction until confirmation, then perhaps fetches the event or remaining balance to confirm profit. We might also use Tenderly or Hardhat for dry-running the transaction for simulation, but more on that in backtesting.

Example Scenario: A borrower on Base has 10000 DAI debt against 6 WETH collateral. A sudden drop in WETH price pushes HF to 0.85. Our bot sees this via an oracle event. It calculates that up to 100% of debt can be liquidated (HF < 0.95). It calls liquidator.liquidate(user, DAI, WETH, uint(-1), false). The Liquidator contract flash-borrows 10000 DAI from Aave, repays the user’s debt, and receives, say, ~6 WETH (with a 5% bonus, maybe Aave gives 6.3 WETH worth). The contract swaps 6 WETH for ~10200 DAI via an on-chain DEX (assuming minimal slippage). It returns 10000 + 9 DAI fee = 10009 DAI to Aave. ~191 DAI remains in the contract. The contract then sends this profit to the bot’s address (or the bot calls a function to withdraw it). The bot records a successful liquidation with ~191 DAI profit (minus gas cost). All this happened in one transaction.

Profitability Estimation & Gas Modeling

To decide whether to execute a liquidation, the bot must estimate profitability in real time. Two primary factors determine profit: the liquidation bonus (discount on collateral) and the transaction cost (gas + flash loan fee). A robust bot will only trigger liquidations likely to net positive profit after costs.

Collateral Bonus Calculation: Given a target position, the maximum collateral the bot can claim is determined by Aave’s parameters:
- Liquidation bonus: e.g. if collateral asset has a 8% bonus, liquidator gets collateral at effectively 92% of market value (8% extra).
- Amount of debt repaid: either 50% or 100% of debt (depending on HF). Let’s denote X = debtToCover (in asset value).

If we repay debt worth $X, we receive collateral worth $X * (1 + bonus)(in USD terms, using oracle prices). The **bonus portion** isX * bonus`. For instance, repaying $1000 with 5% bonus yields $1050 in collateral, so $50 is the gross bonus value ￼ ￼.

However, if the collateral’s actual market price differs from oracle or has slippage on selling, the real value may differ. The bot should consider the realizable value of the collateral:
- Check current DEX price or orderbook for selling the collateral asset for the debt asset or for a stablecoin. If the collateral is illiquid, a 5% oracle bonus might not translate to 5% profit – the act of selling could move the price or incur slippage.
- In many cases (especially large liquidations), liquidators are sophisticated and may route collateral to off-chain OTC or gradually sell. Our bot might choose to immediately swap on-chain (to minimize market risk), so we should factor in the expected DEX price impact. Using a price oracle for valuation is a baseline, but for safety, the bot could apply a haircut (e.g. assume you can only get 98% of oracle price for the collateral when selling).

Gas Cost Modeling: The bot can estimate gas for the liquidation transaction using Ethereum RPC’s estimateGas on the liquidate() call before sending ￼. A flash-loan liquidation is a fairly heavy transaction (multiple calls and a swap); on an L2 like Base, gas is cheaper than L1, but still we might use e.g. 1–2 million L2 gas. Base L2 gas costs involve an L1 data fee component, but typically much lower than mainnet fees. Let’s say a liquidation costs 1,500,000 gas. If Base’s gas price is 0.1 gwei (just an example) and 1 ETH ~ $1800, then cost = 1.5e6 * 0.1e-9 ETH = 1.5e-4 ETH = ~$0.27 – extremely cheap. In reality, under congestion or if Coinbase’s sequencer chooses, gas price could rise. But generally, L2 gas is low, so gas cost is often not the limiting factor on Base. This means even small liquidations might be profitable on Base whereas they wouldn’t on mainnet (where 1.5e6 gas could be $30+). However, during volatile times, if many liquidations are happening, a bot might bump the gas price higher to beat competitors. That can raise cost significantly.

Our bot should:
- Estimate gas for the transaction via eth_estimateGas each time, and multiply by a chosen gas price (plus the L1 data fee estimate). Alloy allows .estimate_gas().await on a prepared call.
- Monitor Base network’s gas price (priority fee) trends. We likely set a high priority fee to ensure inclusion. If our profit margin is thin, we can’t afford too high a fee; if profit is large, we can overpay gas to secure the win.
- Possibly implement logic to adjust gas price if a previous attempt failed or if competing transactions are noticed (though on Base, the mempool is not as openly competitive as Ethereum mainnet; currently a single sequencer can order freely, but using a high fee is still the standard way to signal urgency).

Transaction Inclusion on Base: Base’s 2-second blocks mean if a bot triggers a liquidation at the same time as another, both might land in subsequent blocks. Only one will succeed (the first to consume the opportunity). A strategy to improve success:
- Send the transaction with a high gas price (to be picked first by the sequencer).
- If possible, connect to the sequencer via a low-latency endpoint (Coinbase Cloud maybe) to minimize delay.
- Concurrency: The bot might pre-calculate multiple targets but should avoid trying to liquidate the same account in parallel. If it sees an account HF<1, it should focus and not double-submit or it could waste gas if the first tx already will handle it. If multiple accounts drop below 1 simultaneously (e.g. broad market crash), it may bundle or rapidly sequence transactions.

Profit Calculation Summary: For each candidate position, compute:
- collateral_bonus_value = (debt_to_repay_value) * bonus_percent (in USD or ETH terms). This is X * bonus.
- flash_fee = X * flash_fee_percent (e.g. 0.09%).
- gas_cost = gas_estimate * gas_price * ETH_price. On Base, also add the L1 data fee (which is typically small, maybe a few cents).
- Potential slippage loss on swapping collateral to repay. E.g. if we get collateral worth $X (plus bonus) but due to slippage we only realize 0.98 * that, factor a 2% reduction.

Thus approx profit = X * bonus - flash_fee - slippage_loss - gas_cost ￼ ￼. We execute if this > 0 with some margin. To be safe, many bots require an expected profit > 0 by a margin (say at least a few dollars) to cover estimation errors.

Corner cases: If bonus is small (say 5%) and the position is small, profit might be mere dollars. If gas is negligible on Base, it might still be worth it – but consider opportunity cost: focus on bigger fish first. Also note, if multiple collateral types are involved (Aave allows liquidating one collateral to cover one debt at a time), we should choose the collateral with the highest bonus or easiest to sell.

Base Network Considerations: Base’s fast blocks mean less time for off-chain or manual computations. The bot’s profit calc must be optimized (but a simple formula as above is fine). The sequencer model means there isn’t a complex priority gas auction, but we should still behave as if there is competition. Base currently doesn’t have MEV auctions like Flashbots on Ethereum (to our knowledge), so just sending the tx normally is the approach.

One consideration: because Base is young, some assets might have shallow liquidity on DEXes. For example, if collateral is a smaller-cap token, selling it could incur huge slippage or even fail. The bot must either:
- Limit itself to liquidations where collateral is a major asset (WETH, USDC, etc.), or
- Integrate a dex aggregator (0x, 1inch) that can find the best route or even route via bridging to mainnet (less likely in one tx due to time). Probably stick to major assets.

Gas Token and Cost: Gas on Base is paid in ETH (the Base chain’s native token is ETH essentially). Ensure the bot’s address and contract have sufficient ETH for gas at all times. The bot should track its ETH balance and not attempt liquidations if low on gas funds (or have an alert to top up).

For modelling, the bot can simulate the transaction on a fork (using alloy's call().await on the contract method without sending, or using something like Tenderly simulation API) to verify that after execution, the contract has positive balance leftover. This simulation can double-check profit but may be too slow for real-time use; more on simulation in backtesting.

In summary, profitable liquidation requires: bonus_value > gas_cost + flash_fee + slippage. Typically on Base, gas+fee are very small, so the key is bonus vs slippage. The bot should either integrate a pricing library to estimate slippage (e.g., query Uniswap pool reserves) or use a conservative fixed adjustment (like assume you only realize 99% of oracle value, or even specifically fetch current on-chain price from a swap contract).

## Safe Atomic Execution with Rust (alloy-rs)

With the mechanics established, implementing the bot in Rust involves leveraging the alloy-rs library for Ethereum interactions, orchestrating the flash loan contract calls, and handling transactions safely.

### Rust Ethereum Libraries (alloy-rs): The alloy ecosystem provides a complete toolkit to interact with EVM chains:
- Provider & Signer: We use ProviderBuilder to create providers that can connect to RPC nodes via HTTP, WebSocket, or IPC. Signers are integrated via the with_recommended_fillers() and wallet() methods, providing seamless transaction signing.
- Contract bindings: We can generate type-safe bindings for Aave’s contracts (Pool, L2Pool, and our Liquidator) using ABIs. With the sol! macro, for example, we write Solidity code directly in Rust to create type-safe structs with methods corresponding to contract functions.
- Calls vs Transactions: For read-only calls like getUserAccountData, we use contract.function_name().call().await. For state-changing operations, we use .send().await which returns a pending transaction builder that we can configure and await for inclusion.
- Event decoding: alloy can decode events using the sol! macro to strongly-type events. Alternatively, manually parse logs using the contract’s ABI.
- Async runtime: alloy is asynchronous, built on Tokio. We can concurrently listen to events and send tx.

### Atomic Liquidation Flow in Rust Putting it together, here’s how a liquidation attempt might look in Rust pseudocode:

```rust
async fn attempt_liquidation(user: Address, debt_asset: Address, collat_asset: Address) -> Result<()> {
    // 1. Determine how much debt to cover
    let user_data: UserAccountData = pool.get_user_account_data(user).call().await?;
    if user_data.health_factor >= U256::exp10(18) { return Ok(()); } // not liquidatable
    
    // debt to cover: use uint(-1) to maximize, or compute % if needed
    let debt_to_cover = U256::MAX;
    // 2. Estimate profitability
    let bonus = get_asset_bonus(collat_asset).await?; // e.g. 5% as 0.05
    let price_collat = oracle.get_asset_price(collat_asset).call().await?; 
    let price_debt = oracle.get_asset_price(debt_asset).call().await?;
    // assume repaying all debt or close factor portion:
    let debt_value = user_data.total_borrow_base; // in ETH wei (Base currency units)
    let bonus_value_eth = debt_value * (bonus); 
    // ... get gas estimation
    let gas_estimate = liquidator.liquidate(user, debt_asset, collat_asset, debt_to_cover, false)
                        .estimate_gas().await?;
    let gas_price = provider.get_gas_price().await? * 2; // bump x2 to outbid others
    let gas_cost_eth = gas_price * gas_estimate;
    // flash fee
    let flash_fee_percent = 0.0009; 
    let flash_fee_value_eth = debt_value * flash_fee_percent;
    // Compare values (in ETH) 
    if bonus_value_eth <= gas_cost_eth + flash_fee_value_eth {
        return Ok(()); // not profitable
    }
    // 3. Send liquidation transaction via our contract
    let tx = liquidator.liquidate(user, debt_asset, collat_asset, debt_to_cover, false)
              .gas_price(gas_price)
              .send().await?;
    println!("Sent liquidation tx: {}", tx.tx_hash());
    // Optionally wait for confirmation
    let receipt = tx.await_confirmations(1, provider).await?;
    if receipt.status == Some(1.into()) {
        println!("Liquidation succeeded, gas used: {}", receipt.gas_used.unwrap());
    }
    Ok(())
}

```
This outline shows synchronous steps for one target. In practice, our monitor loop will find a user and call attempt_liquidation. We may parallelize across different users, but ensure not to double-liquidate the same user concurrently.

### Ensuring Atomicity Ensuring Atomicity & Safety: Safety The liquidation either happens fully or not at all due to the flash loan atomic nature. From Rust’s perspective, we just fire one transaction. We should handle the scenario of transaction failure (receipt status 0). Failures could be due to someone else liquidating first, or slippage causing executeOperation revert. The bot should catch that and log it. It might also adjust strategy: e.g., if failed because someone else did it, we drop that target. If failed due to slippage (rare if we estimated well), maybe it was borderline profit and price moved – we might avoid retry unless conditions change.

### Concurrency and Rate-Limiting Using Tokio, we can spawn multiple tasks:
- One task listening for events and pushing potential users into a queue.
- One or more worker tasks pulling from that queue and running attempt_liquidation.
- A shared state (like a HashSet of users being processed) can prevent duplicate processing.

### Database Integration (sqlx) We can use sqlx (with PostgreSQL or SQLite) to maintain persistent state:
- A table of users with last known HF, collateral/debt, timestamp updated.
- A table of liquidations executed (for record-keeping and analysis).
- Using sqlx, we can easily parameterize queries and fetch results asynchronously. For example, to insert a liquidation event:

sqlx::query!("INSERT INTO liquidations (user, profit, tx_hash, timestamp) VALUES (?, ?, ?, ?)",
             user, profit, tx_hash, timestamp)
     .execute(&db_pool).await?;


- The DB is also useful for backtesting: we could store historical positions and run simulations.

### Error Handling Error Handling & Logging: Logging We should use robust error handling (the eyre crate or anyhow for easier error reports). Logging is important for a long-running bot. We might integrate with tracing crate to log info and debug messages (like “Liquidation executed for user X, profit Y”).

### Private Key Management We use dotenv to load the private key, which is then parsed by alloy. Dotenv allows keeping secrets out of code. In deployment, ensure the .env file is protected. The key is held in memory by the process – which is fine if the machine is secure. For extra security, one could use an HSM or a managed signing service, but that adds latency (likely not ideal for high-speed liquidations). A compromise is to run on a secure dedicated server and limit access.

### Receiving Profits After a successful liquidation, the profit typically sits in the Liquidator contract (as some ERC20 or ETH). We should build into the contract (or a separate method) the ability for the owner to claim the accumulated assets. For instance, a function withdraw(address token) that sends the token balance to the owner. Our Rust bot after each liquidation could call liquidator.withdraw(collateralAsset) to transfer any leftover collateral to the bot’s EOA. Alternatively, the executeOperation could directly transfer out the profit at the end. But keeping it until after ensures the atomic loan is fully repaid first.

### Safety We must ensure the contract handles only expected tokens. A potential risk is if the contract unexpectedly holds different tokens from prior liquidations (e.g., leftover of various assets). Our withdraw function should handle each asset type. We also should handle if multiple liquidations accumulate the same asset in the contract – don’t forget to withdraw periodically to not leave funds vulnerable in the contract.

### Testing the Integration We can test on a smaller scale:
- Use Base Goerli testnet with Aave v3 (if deployed) or use another network’s test deployment.
- Deploy the Liquidator contract and run the Rust bot against a known scenario (perhaps create a test position to liquidate).
- Use Tenderly fork of Base mainnet to simulate a real liquidation by impersonating our bot and contract – to ensure all steps work before risking real funds.

## Essential Rust Libraries Essential Rust Libraries & Frameworks Frameworks

Our Rust liquidation bot will leverage several key libraries and frameworks to achieve reliability and performance. Below we list the essential ones and illustrate their usage in this context:
- alloy-rs (alloy crate): Ethereum connectivity (RPC calls, contract interactions, signing). This is the core of our bot for interacting with Aave and other smart contracts.
#### Usage Example Using alloy, we create a provider and signer, then call contract methods as shown earlier. For instance, to decode an event using the sol! macro:

sol! {
    event LiquidationCall(
        address indexed collateralAsset,
        address indexed debtAsset,
        address indexed user,
        uint256 debtToCover,
        uint256 liquidatedCollateralAmount,
        address liquidator,
        bool receiveAToken
    );
}

// Later, given a Log:
if let Ok(liquidation_event) = LiquidationCall::decode_log(&log) {
    println!("User {} liquidated, collateral received: {}", 
             liquidation_event.user, liquidation_event.liquidatedCollateralAmount);
}

This demonstrates alloy's ability to strongly-type contract data using the sol! macro. Additionally, alloy provides built-in retry logic and connection management through the ProviderBuilder pattern.

- tokio: Asynchronous runtime and concurrency. Tokio allows our bot to handle multiple tasks (event listening, multiple liquidations) simultaneously in a thread-safe way.
#### Usage Example Spawning concurrent tasks:

tokio::spawn(async move {
    loop {
        // poll or subscribe for events
        if let Some(user) = check_next_unhealthy_user().await {
            task_queue.send(user).await;
        }
    }
});
tokio::spawn(async move {
    while let Some(user) = task_queue.recv().await {
        attempt_liquidation(user, ...).await.unwrap_or_else(|e| eprintln!("Liquidation error: {:?}", e));
    }
});

Here one task produces candidates, another consumes and liquidates. Tokio’s scheduling ensures high performance even if one task blocks briefly on network I/O. We also use #[tokio::main] to start the runtime (as shown in earlier code). Because of Tokio, we can handle hundreds of events per second if needed (Base might not produce that many liquidation opportunities, but good to be prepared).

- dotenv: Loading environment variables (e.g. RPC URL, private key, database URL) from a .env file. This is straightforward:

dotenv::dotenv().ok();
let rpc = env::var("RPC_URL")?;
let db_url = env::var("DATABASE_URL")?;
let pk = env::var("PRIVATE_KEY")?;

Keeping keys/config in env ensures we don’t hardcode secrets. In deployment, the .env file should be secured (file permissions or injected via secure pipeline).

- sqlx: Async SQL query library (supports Postgres, MySQL, SQLite, etc. with compile-time query checking). We can use Postgres for a robust solution or SQLite for simplicity (SQLite could suffice if we just want a local DB to track data; Postgres is better for remote access or if running multiple instances sharing a DB).
#### Usage Example Define a schema for storing user data:

// Using Postgres, for example:
let pool = PgPool::connect(&db_url).await?;
sqlx::query!("CREATE TABLE IF NOT EXISTS users (address TEXT PRIMARY KEY, health_factor NUMERIC, updated_at TIMESTAMP)")
    .execute(&pool).await?;
// Inserting or updating:
sqlx::query!("INSERT INTO users (address, health_factor, updated_at) VALUES ($1, $2, NOW()) 
              ON CONFLICT (address) DO UPDATE SET health_factor = EXCLUDED.health_factor, updated_at = NOW()",
              user.to_string(), health_factor_low_u256.to_string())
    .execute(&pool).await?;

This stores the latest health factor of each user. We could also log every time an account crosses below a threshold, or keep a history of liquidations. With sqlx, queries are type-checked at compile time (if enabled) and results are mapped to Rust types easily. The library handles connection pooling and is fully async, so it won’t block our event loop when writing to DB.

- serde: Serialization/deserialization, used for reading/writing JSON (e.g., if we call external APIs or parse GraphQL responses) or loading configurations. We might define structs to mirror GraphQL JSON data and derive Deserialize for them.
#### Usage Example If using The Graph’s HTTP API:

#[derive(Deserialize)]
struct GraphUser {
    id: String,
    totalCollateralETH: String,
    totalBorrowsETH: String,
    currentLiquidationThreshold: String
}
#[derive(Deserialize)]
struct GraphResponse { data: GraphData }
#[derive(Deserialize)]
struct GraphData { users: Vec<GraphUser> }

let client = reqwest::Client::new();
let res = client.post(GRAPHQL_ENDPOINT)
    .json(&serde_json::json!({ "query": QUERY_STRING }))
    .send().await?
    .json::<GraphResponse>().await?;
for user in res.data.users {
    let debt = user.totalBorrowsETH.parse::<f64>().unwrap();
    let collat = user.totalCollateralETH.parse::<f64>().unwrap();
    // compute HF and decide if to add to watchlist...
}

Here serde handles mapping the JSON into our struct. We also use reqwest (another great library, which could be considered essential for any HTTP interactions – e.g., calling Dune API or an external price feed). The bot might not need much external HTTP during operation, but for backtesting or alerts (like sending a message to a Discord webhook), reqwest is useful.

- Other Utilities:
- chrono for timestamp and time handling (e.g., logging when something happened, computing durations).
- clap or structopt if we want command-line argument parsing (e.g., to pass different config or run modes like --backtest).
- tracing for structured logging, which can be hooked to monitor services or just console output with timestamps and levels.
- dashmap or other concurrent map structures if we manage state in memory across threads safely (though simple Arc<Mutex<>> can suffice given the scale).

By using these libraries, we keep our code concise and high-level. For instance, alloy abstracts the raw RPC calls, so we don’t manually build transactions or RLP-encode data; we just call contract methods as if calling a Rust function. Tokio and friends allow the bot to be event-driven and responsive, rather than a single-threaded polling loop.

System Architecture Blueprint

A modular design will keep the bot maintainable and efficient. We propose splitting the bot into distinct components or modules that handle specific responsibilities:
- Monitoring Module: Responsible for tracking the state of the Aave market and identifying liquidation opportunities. This includes:
- Subscribing to events (via Provider<Ws> as discussed).
- Polling the subgraph or on-chain data periodically.
- Maintaining an in-memory (or database-backed) list of accounts with their last known health factor and a flag if they are currently below threshold.
- When an account transitions to below HF 1, this module triggers an action (e.g., sends the account to a task queue for liquidation). It might also handle cases where accounts recover (HF goes back above 1) – possibly removing them from immediate queue.
- Simulation & Profit Evaluation Module: Before actually executing, it’s wise to simulate or at least calculate the profitability. This module:
- Calculates current prices (from oracle or DEX) and uses the formulas described to estimate profit.
- Optionally, interfaces with a simulation service (like Tenderly or anvil) to simulate the transaction. For example, Tenderly’s API could be given the transaction data and return whether it would succeed and the post-state (how much profit). However, doing this in real-time might be too slow (adds latency of an API call). In high-speed situations, we likely rely on our own calculations.
- Perhaps a compromise is using eth_call with state overrides (alloy allows specifying state overrides to simulate a tx with modified state). For instance, we can simulate the liquidationCall by doing an eth_call on the Liquidator contract’s liquidate function with block = latest and it will not actually execute but tell us if it would revert (this is tricky because flashLoan involves an external call – we may simulate just the executeOperation portion by manually calling Pool.liquidationCall as a static call to see how much collateral we’d get; not straightforward without writing custom simulation code).
- In practice, this module may just do the formula and gas estimate check as shown, and ensure a buffer.
- Execution Module: This contains the logic to compose transactions and send them. It interacts with the Liquidator contract. It handles:
- Selecting which collateral to liquidate if multiple available. (Aave allows choosing which collateral asset to take – typically liquidators pick the one with highest bonus or best liquidity. In implementation, if a user has multiple collaterals, we might have to iterate possible collaterals. Aave V3’s liquidationCall(collateralAsset, debtAsset, user, ...) requires specifying one collateral asset to seize. So if user posted both WETH and USDC as collateral, and borrowed DAI, we choose one. Likely the one with more value or whichever yields profit. Our bot could attempt each if needed.)
- Crafting the transaction to the Liquidator contract (with appropriate gas and parameters).
- Broadcasting the transaction via the provider and optionally handling retries. If the network is congested or a transaction is stuck, this module might bump gas (e.g., by resending with higher gas price after a timeout if not mined – though careful with nonces).
- Receiving the transaction receipt and determining success or failure. On success, parse events or query contract balance to confirm outcome. On failure, log reason (maybe by decoding revert message or just noting it).
- This module should be isolated so that it can focus on the mechanics of sending transactions, separate from deciding what to liquidate.
- Persistence Module: Manages database operations (if using a DB). It will:
- Save events like “liquidation executed” with details (user, debt asset, collateral, amount, profit, timestamp).
- Save any critical state needed across restarts (e.g., a cache of users, though that can be reconstructed).
- Possibly store metrics for performance (like how fast we detected vs mined, etc.).
- The bot can function without a DB, but for long-term operation and analysis, storing data is invaluable. Using sqlx with a lightweight SQLite might be enough; or Postgres if multiple components or an external dashboard will connect.
- Alerting/Reporting Module: Even though fully automated, we want insight into bot’s performance and any issues. This could include:
- Sending notifications on certain events. For example, integrate with a service like Slack or Telegram via webhooks or use email. If a liquidation yields an unusually low profit or fails, notify maintainers.
- Periodic status reports, e.g., every hour, “X accounts liquidated in last hour for Y total profit”.
- Using the tracing crate, we can output JSON logs which can feed into monitoring systems.
- If using an error tracking system (like Sentry), integrate that to capture unexpected errors.

These modules can be organized in the codebase as separate Rust modules or simply as distinct tasks within main.rs. A rough architecture diagram would show the flow from Monitoring to Execution:
- Event Listener → (detect user HF<1) → Task Queue → Liquidation Executor → Liquidator Contract TX → Aave & Flash loan → result → Logger/DB.

The earlier sequence diagram (embedded above) covers the on-chain part. 

![Flash Loan Example Flow](docs/flash-loan-example-flow.png)

For off-chain architecture:

[WS Event Stream] -- Borrow/Price events --> [Monitoring Task] -- user HF calc --> [Liquidation Queue] --> [Executor Task] -- tx --> [Base RPC] -- result --> [DB & Alerts]

Each arrow is asynchronous message passing or function call.

Modularity Benefits: For example, we can unit-test the Profit Evaluation module by feeding it dummy data and checking if it correctly says an opportunity is profitable. We can test the Execution module on a testnet by simulating a liquidation. The Monitoring module can be tested by simulating event streams (alloy might allow a test provider that yields fake events).

Benchmarking Module: We may also include a component (perhaps not running all the time in production) to benchmark our bot’s latency and success rate. This could:
- Record timestamps for when an HF<1 was first observed vs when our transaction got mined. On Base, we aim for maybe under 2 seconds difference ideally.
- Track how often we lost to another liquidator (we try but someone else’s tx beat us).
- Measure the fraction of opportunities seized vs missed.

These stats help tune the bot (maybe increase gas or improve event detection). They can be stored or just logged.

Concurrent Liquidations: If multiple accounts are liquidatable, our architecture should handle parallel execution. The Liquidator contract can actually handle multiple at once if we coded it (for example, we could even design a single transaction to liquidate multiple users if we had a custom multi-call contract, but Aave’s flash loan would then be repaid at end after doing multiple liquidationCalls sequentially – however, that’s complex and not often needed unless bundling small ones to save gas). More practically, we run separate tasks for separate users. The Rust bot can definitely send multiple transactions concurrently (they’ll have different nonces if we use multiple signers or if previous TX confirmed to increment nonce). Using a single EOA signer, we have to be careful with nonce management: alloy by default will queue transactions and assign sequential nonces. We might allow one outstanding tx at a time per signer or use manual nonce assignment for more control. Because Base blocks are fast, waiting for one to be mined before sending next is not too bad. But if many opportunities at once, having a second or third signer (with its own Liquidator contract) could be a strategy to truly parallelize. This adds complexity so initial design might stick to one.

In summary, the architecture emphasizes separation of concerns (detect vs act vs record vs inform) and uses asynchronous messaging (like Tokio channels) to connect them. This results in a high-performance pipeline where each part can be optimized or replaced independently.

Deployment & Infrastructure

Deploying a liquidation bot for 24/7 operation on Base requires a robust infrastructure setup:
- Runtime Environment: The bot will run continuously on a server. Options include:
- Cloud VM (AWS, GCP, etc.) running a Linux instance. Ensure it has a stable network (low latency to Base’s RPC).
- Container (Docker): Containerizing the bot is convenient for portability. We can create a Docker image with Rust static binary or so. Using Docker allows quick redeployment and isolation. We’d still need to manage persistence (volume for database, or use an external DB).
- If using Docker, consider Kubernetes or Docker Compose if scaling multiple bots (though one instance is usually enough; multiple could conflict unless splitting responsibilities by asset or something).
- Regardless, monitor the process. Use something like systemd to auto-restart on crash, and potentially a watchdog that alerts if the process stops or uses too much memory.
- RPC Node & Redundancy: Reliable RPC access is critical. We should have:
- Primary RPC: possibly a private node for Base (running an Optimism node in Base mode). Running our own Base node might be non-trivial (Base is OP Stack, so similar to running an Optimism node; ensure it’s fully synced and has archival if needed for historical queries). If we can manage that, it gives low-latency and independence from rate limits.
- Secondary RPCs: subscription to a service like Alchemy or Infura (if they support Base) or Coinbase’s own Node service. QuickNode also supports Base as indicated by their documentation ￼. We can configure alloy with a Provider that wraps multiple endpoints: e.g., use connection pooling to load balance or fallback. At minimum, implement logic: if one RPC fails (timeout or error), switch to backup.
- WebSocket vs HTTP: For event subscriptions, WebSocket is ideal. Not all providers offer WS for Base; QuickNode does, Alchemy likely does. If our primary is a self-node, ensure WS is enabled. Have both HTTP and WS endpoints configured.
- Monitor RPC latency and maybe geographically choose a server close to the RPC provider’s servers. E.g., if using QuickNode (which might be in US East), host bot in US East for minimal ping.
- Secure Key Handling: The bot’s private key controls potentially large funds (especially as it accumulates profits). Security steps:
- The key should never be exposed. It’s loaded from env or a vault service. If on a VM, restrict access (no unnecessary users, use firewall).
- Do not log the private key or any critical secrets. Even logs should avoid printing raw addresses if not needed.
- Optionally use an encrypted keystore file (alloy can decrypt JSON keystore with a password). This way, the actual key is not in plain text on disk or env. You’d need to provide a passphrase on startup (which complicates auto-restart unless using something like AWS Secrets Manager).
- Hardware wallet integration: not feasible for an automated rapid bot (signing needs to be automated).
- One idea: use a multisig for storing large profits, while the bot’s key only keeps minimal working balance. The bot could periodically transfer profits to a cold wallet. That way, if the bot’s key is compromised, the damage is limited. However, the bot’s key still could be used by an attacker to grief liquidations or steal current funds. Given the risk profile, treating the bot key as hot but limiting its access is wise.
- Use OS-level security: ensure the machine is patched, only SSH accessible by you, maybe use fail2ban or keys (no password login), etc.
- High Availability: If the bot goes down, opportunities are missed. We can set up:
- Monitoring via an external service (Pingdom or a simple cron job that tries to ping an HTTP endpoint the bot exposes, etc.). Or simpler, if the bot sends periodic “I’m alive” alerts, you notice if they stop.
- Have a failover instance: Perhaps run two bots in parallel with some coordination. But if two independent bots with same strategy run, they might both try to liquidate the same thing (wasting gas for one). This could be mitigated by giving one bot priority (the other only acts if primary is down). Alternatively, run both but on different sets of assets or addresses (complex to shard).
- The simplest HA might be: a secondary bot process that monitors the primary and if primary stops for > X seconds, it takes over (maybe via a shared heartbeat in DB). This is probably overkill unless the value at stake is high.
- Scaling & Performance: For now, Base’s Aave market is modest compared to Ethereum mainnet. One bot can handle it. If in future Base or other networks multiply, one could extend this bot to multiple networks (e.g. same logic on Polygon, Arbitrum, etc.). In that case, it might be better to separate configuration per network and possibly run separate instances to avoid cross-talk (since each network has its own RPC and timing).
- Logging and Monitoring: Use logging (maybe output to file and rotate logs). Also consider using Prometheus + Grafana: instrument the bot to expose metrics (like number of liquidations, current backlog, etc.) for monitoring. For example, use prometheus crate to increment counters for “liquidations_executed_total” or gauge for “last_profit_value”. These can feed a Grafana dashboard to visualize bot performance over time.
- Upgrades and Maintenance: Keep the contract address of Aave’s Pool, L2Pool, and your Liquidator configurable (in case Aave deploys new Pool or upgrades something – though Aave uses proxy so address stable). If Aave adds new assets, update the asset ID mapping if using L2Pool encoding. Watch Aave governance forums for any parameter changes (like bonus, close factor) on Base – those could affect strategy.
- Security Considerations: Running a bot that moves funds is like running a small service with its own wallet. Be mindful of smart contract risk – our Liquidator contract should be thoroughly tested/audited to not have vulnerabilities (like reentrancy, though it mostly interacts with known protocols, but e.g., when swapping on DEX, ensure using a secure router call). Also ensure the contract’s funds can only be moved by our bot (owner). The contract should ideally not hold funds long – after each liquidation, transfer profit out.
- Testing on Mainnet (Dry-Run): We might deploy the system in “shadow mode” – listen and simulate without actually sending transactions, to see how it would perform. This can be done by logging supposed actions without actually calling send(). Compare with actual liquidations that occurred (via Dune or logs) to measure missed opportunities or false triggers, then refine before risking real tx fees.
- RPC Rate Limits: If using a public RPC, ensure we don’t hit limits. Subgraph queries also have rate limits (The Graph might throttle heavy queries). If our monitoring loop is too aggressive (polling too often), it could be an issue. Use batch requests or efficient filters when possible. For example, instead of calling getUserAccountData for 100 users sequentially, we could batch them using JSON-RPC batch (alloy supports sending batch calls). Or stagger them over multiple blocks.
- Redundancy in Contracts: Deploy two Liquidator contracts in case one runs out of gas or gets a problem? Generally not needed, but if we wanted to, we could have one for stablecoin debts and one for volatile debts to micro-optimize parameters.

In summary, treat the bot like a high-uptime trading system. Use best practices of DevOps: monitoring, alerts, secure secrets, and quick rollback if something goes wrong (keep old version handy, perhaps feature flags to turn off certain functionality without redeploying).

Backtesting and Benchmarking

Before risking real capital and to tune the bot’s parameters, backtesting on historical data is essential. We can leverage data from Dune Analytics, Tenderly, and The Graph to simulate how our bot would have performed and identify optimal strategies.

Historical Liquidation Data (Dune & The Graph): Dune Analytics provides an SQL query interface to blockchain data. There are existing Dune dashboards for Aave v3 liquidations ￼. We can write a Dune query for the Base Aave v3 market, for example:

SELECT block_time, liquidator, target as user, debt_asset, collateral_asset, debt_to_cover, profit
FROM aave_v3.base_liquidations
ORDER BY block_time;

This (pseudo-code) would list every liquidation event on Base with timestamp and details. Particularly, look at profit if the Dune view calculates it (some dashboards do compute profit by valuing collateral minus debt). If not directly available, we can compute it offline:
- For each liquidation event, retrieve debt_to_cover (amount of debt repaid) and liquidated_collateral_amount from the event. Multiply liquidated_collateral_amount by price of collateral at that block (from Chainlink price feed data, which Dune likely has) to get collateral value. Subtract debt_to_cover * price_of_debt to get value gained. That difference minus flash loan fee is profit (assuming the liquidator immediately sold collateral at oracle price).

By analyzing historical liquidations:
- We see which assets were typically involved, and the sizes.
- We may find instances of undercollateralized accounts that did not get liquidated (bad debt), possibly because not profitable. For example, the StackExchange question we saw indicated some small accounts with HF<1 were left due to too small profit ￼. We can verify such cases in data.
- We can calibrate a minimum profit threshold. If historically no one liquidates unless profit > $10 (just an example), that might suggest that threshold covers gas costs and risk. On Base, maybe even $1 profit gets taken because gas is so low – only data can tell.

### Simulating Our Bot on Historical Scenarios We can replay specific moments:
- Use Tenderly: Tenderly allows forking the chain at a past block and running transactions hypothetically. We could take a past block just before a known liquidation, input our Liquidator contract and simulate if we would have liquidated it (and how much profit we’d get). This validates that our approach yields the correct outcome. Tenderly can also output gas used, etc.
- We can integrate Tenderly API to programmatically do this for many events to get distribution of gas used and profits. Or simpler, use foundry (anvil) to fork and run a script that executes our contract call at that state.

### Benchmarking Reaction Time If we have timestamps from events (like when HF dropped below 1 vs when liquidation happened), we can gauge how fast liquidators typically act. If on average there’s 10 seconds delay, our goal could be <2 seconds. This informs how aggressive to be on monitoring frequency and gas price.

### Load Testing We can simulate a scenario with e.g. 50 accounts all dropping below HF 1 at once (maybe via a local fork and a script that manipulates prices). This would test if our bot can handle a burst (multiple nearly simultaneous liquidations). The bot’s design with queue and concurrency should manage it, but we might find bottlenecks (e.g. if we only allowed one tx at a time, we’d miss some – then we might adjust to allow multiple).

### Backtesting on Recorded Data We might feed our stored data (if we had a DB of historical states or we fetch via Graph) into a simulator written in Rust or Python. The simulator would iterate block by block (or event by event), use our strategy (like if HF<1, liquidate immediately with given gas cost model) and see what profit we’d make vs actual outcomes. This can reveal suboptimal choices, e.g., perhaps sometimes waiting yields better profit if price rebounds (though in liquidation, waiting is usually not beneficial – first come first served).

### Parameter Tuning Using the historical data, we can tweak:
- Gas price strategy: e.g., always bid a fixed high priority vs dynamic. If historically liquidations have low competition (only one bot), we could save cost by not overbidding. But if we see multiple liquidators in events (different liquidator addresses in short succession), that implies competition. On Ethereum mainnet, competition is fierce and they often backrun via Flashbots. On Base, currently fewer players but that may change.
- Profit threshold: ensure we don’t waste time on borderline cases. If data shows 90% of liquidations had >$5 profit, maybe set $1 as a threshold to be safe.
- Whether to use flash loans for every liquidation or occasionally use held funds: If fees are negligible this doesn’t matter. But one might simulate that if we had 100 ETH sitting, using it avoids flash fee. However, the complexity of ensuring that ETH is available in correct asset each time is high. Flash loans are easier and the fee is small, so likely always use flash.

### Benchmarking System Performance We should measure the bot’s internal performance:
- How long does it take from receiving an event to submitting a transaction? (we can log a timestamp at event receive and at tx send).
- Are there any slow database operations or blocking calls? We can use Tokio’s tracing to detect any spans that take too long.
- Memory and CPU usage: a liquidation bot is mostly I/O bound, but if we parse tons of events or handle big lists, be mindful of memory. For safety, run the bot on a machine with decent memory (e.g., 1-2 GB free for process).

### Continuous Improvement via Data Once the bot is live, continue collecting data on successes/failures. If a liquidation occurs and our bot missed it (maybe we were too slow or offline), analyze why:
- Did we detect it? Did our tx get beaten? If beaten, maybe need to increase gas or improve latency.
- If we didn’t detect: was it because we only rely on events and maybe the price drop wasn’t directly an event? Possibly incorporate direct price monitoring.
- If we didn’t attempt because our profit calc said not profitable but someone still did it (maybe they have a different cost structure or are okay with marginal profit), reconsider our model.

### Dune and Dashboard We could even create our own Dune dashboard for Aave on Base to watch current at-risk accounts (though with some delay). Or use The Graph to query in a script. But for automated backtesting, exporting data as CSV or JSON and processing in Python or Rust is effective.

For instance, we could use sqlx in a special mode to ingest historical events (if we have them from Dune as CSV) into our local DB and then run a simulation function on each event in time order.

### Tenderly Integration Example Using Tenderly’s API (if available to us) would involve sending the transaction data (from our Liquidator contract) to a fork. We might skip detailed steps here, but it’s an option.

In conclusion, backtesting ensures our implementation is ready for real-world chaos. We prioritize recent data (2024 crashes, etc.) because the DeFi landscape and Aave parameters can change. For example, if a certain asset was added in 2024, older data wouldn’t include it. Also, Base itself only existed from 2023, so focusing on mid-2023 to 2025 data covers the entirety of Aave on Base.

By following this comprehensive plan – understanding Aave v3’s liquidation process, continuously monitoring on-chain events, leveraging flash loans for atomic execution, rigorously estimating profits, utilizing Rust’s powerful libraries, designing a modular system, deploying securely, and validating against historical scenarios – we can implement a high-performance Aave v3 liquidation bot on Base that is both efficient and reliable in safeguarding the protocol’s health and earning consistent returns for the liquidator.

## References Aave Protocol documentation on liquidations and L2Pool ￼ ￼, flash loan mechanics ￼, community guidelines for liquidation strategy ￼ ￼, and real-time monitoring techniques ￼ have informed this design. These sources, along with on-chain data analyses, ensure our approach is grounded in up-to-date DeFi practices and tailored to the Base network’s environment.

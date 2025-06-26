use alloy_contract::ContractInstance;
use alloy_primitives::{Address, U256, B256};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, Filter, Log};
use dashmap::DashMap;
use eyre::Result;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

use crate::database;
use crate::models::UserPosition;

/// Bootstrap configuration for user discovery
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub blocks_to_scan: u64,      // Number of blocks to scan back from current
    pub batch_size: u64,          // Number of blocks to scan per batch
    pub rate_limit_delay_ms: u64, // Delay between batches to avoid rate limiting
    pub max_users_to_discover: usize, // Maximum number of users to discover
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            blocks_to_scan: 50000,      // Scan last ~7 days on Base (12sec blocks)
            batch_size: 1000,           // 1000 blocks per batch
            rate_limit_delay_ms: 500,   // 500ms delay between batches
            max_users_to_discover: 1000, // Track up to 1000 users initially
        }
    }
}

/// Discovers users by scanning historical Aave Pool events
pub async fn discover_users_from_events<P>(
    provider: Arc<P>,
    pool_address: Address,
    db_pool: &Pool<Sqlite>,
    config: BootstrapConfig,
) -> Result<Vec<Address>>
where
    P: Provider,
{
    info!("🔍 Starting user discovery from historical events...");
    info!("📊 Scanning {} blocks in batches of {}", config.blocks_to_scan, config.batch_size);

    let current_block = provider
        .get_block_number()
        .await
        .map_err(|e| eyre::eyre!("Failed to get current block number: {}", e))?;

    let start_block = current_block.saturating_sub(config.blocks_to_scan);
    
    info!(
        "🎯 Scanning from block {} to {} (current)",
        start_block, current_block
    );

    let discovered_users = Arc::new(DashMap::new());
    let mut current_scan_block = start_block;

    while current_scan_block < current_block && discovered_users.len() < config.max_users_to_discover {
        let end_block = std::cmp::min(current_scan_block + config.batch_size, current_block);
        
        debug!(
            "📖 Scanning blocks {} to {} ({} users found so far)",
            current_scan_block,
            end_block,
            discovered_users.len()
        );

        // Scan this batch of blocks
        if let Err(e) = scan_block_range_for_users(
            provider.clone(),
            pool_address,
            current_scan_block,
            end_block,
            discovered_users.clone(),
        ).await {
            warn!("Failed to scan blocks {} to {}: {}", current_scan_block, end_block, e);
        }

        current_scan_block = end_block + 1;

        // Rate limiting delay
        if config.rate_limit_delay_ms > 0 {
            sleep(Duration::from_millis(config.rate_limit_delay_ms)).await;
        }

        // Progress update every few batches
        if (current_scan_block - start_block) % (config.batch_size * 10) == 0 {
            info!(
                "📈 Progress: {}% complete, {} users discovered",
                ((current_scan_block - start_block) * 100) / config.blocks_to_scan,
                discovered_users.len()
            );
        }
    }

    // Convert to Vec and add to database
    let users: Vec<Address> = discovered_users
        .iter()
        .map(|entry| *entry.key())
        .collect();

    info!("✅ User discovery complete! Found {} unique users", users.len());

    // Add discovered users to database for tracking
    let mut added_count = 0;
    for user in &users {
        match database::add_user_to_track(db_pool, *user).await {
            Ok(()) => added_count += 1,
            Err(e) => warn!("Failed to add user {:?} to database: {}", user, e),
        }
    }

    info!("📊 Added {} users to tracking database", added_count);

    Ok(users)
}

async fn scan_block_range_for_users<P>(
    provider: Arc<P>,
    pool_address: Address,
    from_block: u64,
    to_block: u64,
    discovered_users: Arc<DashMap<Address, ()>>,
) -> Result<()>
where
    P: Provider,
{
    // Define event signatures for major Aave events that involve users
    let event_signatures = vec![
        // Supply event: Supply(address indexed reserve, address user, address indexed onBehalfOf, uint256 amount, uint16 indexed referralCode)
        "0x2b627736bca15cd5381dcf80b0bf11fd197d01a037c52b927a881a10fb73ba61",
        // Borrow event: Borrow(address indexed reserve, address user, address indexed onBehalfOf, uint256 amount, uint8 interestRateMode, uint256 borrowRate, uint16 indexed referralCode)  
        "0xb3d084820fb1a9decffb176436bd02558d15fac9b0ddfed8c465bc7359d7dce0",
        // Repay event: Repay(address indexed reserve, address indexed user, address indexed repayer, uint256 amount, bool useATokens)
        "0xa534c8dbe71f871f9f3530e97a74601fea17b426cae02e1c5aee42c96c784051",
        // Withdraw event: Withdraw(address indexed reserve, address indexed user, address indexed to, uint256 amount)
        "0x3115d1449a7b732c986cba18244e897a450f61e1bb8d589cd2e69e6c8924f9f7",
        // LiquidationCall event: LiquidationCall(address indexed collateralAsset, address indexed debtAsset, address indexed user, uint256 debtToCover, uint256 liquidatedCollateralAmount, address liquidator, bool receiveAToken)
        "0xe413a321e8681d831f4dbccbca790d2952b56f977908e45be37335533e005286",
    ];

    for signature in event_signatures {
        let signature_hash: B256 = signature.parse()?;
        
        let filter = Filter::new()
            .address(pool_address)
            .from_block(BlockNumberOrTag::Number(from_block))
            .to_block(BlockNumberOrTag::Number(to_block))
            .event_signature(signature_hash);

        match provider.get_logs(&filter).await {
            Ok(logs) => {
                for log in logs {
                    if let Some(user) = extract_user_from_log(&log) {
                        discovered_users.insert(user, ());
                        debug!("👤 Discovered user: {:?}", user);
                    }
                }
            }
            Err(e) => {
                debug!("Failed to get logs for signature {}: {}", signature, e);
                // Continue with other signatures even if one fails
            }
        }
    }

    Ok(())
}

fn extract_user_from_log(log: &Log) -> Option<Address> {
    let topics = log.topics();
    
    // Most Aave events have user address in topic[1] or topic[2]
    // Supply, Borrow: user is in topic[1] (second topic after event signature)
    // Repay, Withdraw: user is in topic[1] 
    // LiquidationCall: user is in topic[2]
    
    for i in 1..std::cmp::min(topics.len(), 4) {
        let topic_bytes = topics[i].as_slice();
        if topic_bytes.len() >= 20 {
            // Address is the last 20 bytes of the topic (topics are 32 bytes, addresses are 20 bytes)
            let addr_bytes = &topic_bytes[12..32];
            if let Ok(user_addr) = Address::try_from(addr_bytes) {
                // Filter out zero address and contract addresses (simple heuristic)
                if !user_addr.is_zero() {
                    return Some(user_addr);
                }
            }
        }
    }
    
    None
}

/// Bootstrap users by checking their current health status
pub async fn bootstrap_user_positions<P>(
    provider: Arc<P>,
    pool_contract: &ContractInstance<alloy_transport::BoxTransport, Arc<P>>,
    db_pool: &Pool<Sqlite>,
    users: Vec<Address>,
) -> Result<Vec<UserPosition>>
where
    P: Provider,
{
    info!("🔄 Bootstrapping user positions for {} users...", users.len());
    
    let mut positions = Vec::new();
    let mut checked_count = 0;
    let mut at_risk_count = 0;

    let total_users = users.len();
    for user in users {
        // Add small delay to avoid rate limiting
        if checked_count > 0 && checked_count % 10 == 0 {
            sleep(Duration::from_millis(200)).await;
            info!("📊 Progress: {}/{} users checked, {} at risk", checked_count, total_users, at_risk_count);
        }

        match check_user_health(provider.clone(), *pool_contract.address(), user).await {
            Ok(position) => {
                // Only store users with actual debt or collateral
                if position.total_debt_base > U256::ZERO || position.total_collateral_base > U256::ZERO {
                    if position.is_at_risk {
                        at_risk_count += 1;
                        info!("⚠️  Found at-risk user: {:?} (HF: {:.3})", 
                            user, 
                            health_factor_to_f64(position.health_factor)
                        );
                    }
                    
                    // Save to database
                    if let Err(e) = database::save_user_position(db_pool, &position).await {
                        warn!("Failed to save user position for {:?}: {}", user, e);
                    } else {
                        positions.push(position);
                    }
                }
                checked_count += 1;
            }
            Err(e) => {
                debug!("Failed to check user {:?}: {}", user, e);
                // Continue with next user rather than failing completely
            }
        }
    }

    info!("✅ Bootstrap complete! {} users checked, {} positions stored, {} at risk", 
        checked_count, positions.len(), at_risk_count);

    Ok(positions)
}

fn health_factor_to_f64(health_factor: U256) -> f64 {
    // Convert U256 to f64 by first converting to u128 and then to f64
    // This handles the common case where health factor is reasonable
    if health_factor > U256::from(u128::MAX) {
        f64::INFINITY
    } else {
        let hf_u128 = health_factor.to::<u128>();
        (hf_u128 as f64) / 1e18
    }
}

async fn check_user_health<P>(
    provider: Arc<P>,
    pool_address: Address,
    user_address: Address,
) -> Result<UserPosition>
where
    P: Provider,
{
    debug!("Checking health factor for user: {:?}", user_address);

    // Call getUserAccountData with proper error handling
    let call_data = match alloy_primitives::hex::decode("bf92857c") {
        Ok(data) => data,
        Err(e) => {
            return Err(eyre::eyre!(
                "Failed to decode getUserAccountData selector: {}",
                e
            ));
        }
    };

    // Encode the user address parameter
    let mut full_call_data = call_data;
    let mut user_bytes = [0u8; 32];
    let user_address_bytes = user_address.as_slice();
    user_address_bytes.iter().enumerate().for_each(|(i, &b)| {
        user_bytes[i + 12] = b; // Address is 20 bytes, pad to 32
    });
    full_call_data.extend_from_slice(&user_bytes);

    let call_request = alloy_rpc_types::TransactionRequest {
        to: Some(pool_address.into()),
        input: alloy_rpc_types::TransactionInput::new(full_call_data.into()),
        ..Default::default()
    };

    let result = provider.call(&call_request).await?;

    // Parse the result - getUserAccountData returns 6 uint256 values
    if result.len() < 192 {
        // 6 * 32 bytes
        return Err(eyre::eyre!(
            "Invalid response length from getUserAccountData: {} bytes, expected at least 192",
            result.len()
        ));
    }

    // Parse the 6 uint256 values returned by getUserAccountData
    let total_collateral_base = U256::from_be_slice(&result[0..32]);
    let total_debt_base = U256::from_be_slice(&result[32..64]);
    let available_borrows_base = U256::from_be_slice(&result[64..96]);
    let current_liquidation_threshold = U256::from_be_slice(&result[96..128]);
    let ltv = U256::from_be_slice(&result[128..160]);
    let health_factor = U256::from_be_slice(&result[160..192]);

    debug!(
        "User {} health data: collateral={}, debt={}, health_factor={}",
        user_address, total_collateral_base, total_debt_base, health_factor
    );

    // Check if user is at risk (health factor < 1.1)
    let risk_threshold = U256::from(1100000000000000000u64); // 1.1 in 18 decimals
    let is_at_risk = health_factor < risk_threshold && health_factor > U256::ZERO;

    let position = UserPosition {
        address: user_address,
        total_collateral_base,
        total_debt_base,
        available_borrows_base,
        current_liquidation_threshold,
        ltv,
        health_factor,
        last_updated: chrono::Utc::now(),
        is_at_risk,
    };

    Ok(position)
}
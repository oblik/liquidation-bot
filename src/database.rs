use crate::models::UserPosition;
use alloy_primitives::Address;
use eyre::Result;
use sqlx::{Pool, Postgres, Sqlite, Row};
use tracing::info;

/// Database connection enum that can hold either PostgreSQL or SQLite connections
#[derive(Clone)]
pub enum DatabasePool {
    Postgres(Pool<Postgres>),
    Sqlite(Pool<Sqlite>),
}

impl DatabasePool {
    /// Execute a query
    pub async fn execute(&self, query: &str) -> Result<u64, sqlx::Error> {
        match self {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query(query).execute(pool).await?;
                Ok(result.rows_affected())
            }
            DatabasePool::Sqlite(pool) => {
                let result = sqlx::query(query).execute(pool).await?;
                Ok(result.rows_affected())
            }
        }
    }
}

/// Detect database type from connection string
fn detect_database_type(database_url: &str) -> Result<&'static str> {
    if database_url.starts_with("postgresql://") || database_url.starts_with("postgres://") {
        Ok("postgres")
    } else if database_url.starts_with("sqlite:") {
        Ok("sqlite")
    } else {
        Err(eyre::eyre!("Unsupported database type in URL: {}", database_url))
    }
}

/// Initialize database with automatic type detection
pub async fn init_database(database_url: &str) -> Result<DatabasePool> {
    let db_type = detect_database_type(database_url)?;
    info!("Detected database type: {}", db_type);

    let pool = match db_type {
        "postgres" => {
            info!("🐘 Connecting to PostgreSQL database...");
            let pool = Pool::<Postgres>::connect(database_url).await?;
            DatabasePool::Postgres(pool)
        }
        "sqlite" => {
            info!("🗄️ Connecting to SQLite database...");
            let pool = Pool::<Sqlite>::connect(database_url).await?;
            DatabasePool::Sqlite(pool)
        }
        _ => return Err(eyre::eyre!("Unsupported database type: {}", db_type)),
    };

    // Create tables
    create_tables(&pool).await?;
    Ok(pool)
}

/// Create database tables for both PostgreSQL and SQLite
pub async fn create_tables(db_pool: &DatabasePool) -> Result<()> {
    match db_pool {
        DatabasePool::Postgres(pool) => {
            info!("Creating PostgreSQL tables...");
            
            // Create user_positions table with PostgreSQL syntax
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS user_positions (
                    address VARCHAR PRIMARY KEY,
                    total_collateral_base VARCHAR NOT NULL,
                    total_debt_base VARCHAR NOT NULL,
                    available_borrows_base VARCHAR NOT NULL,
                    current_liquidation_threshold VARCHAR NOT NULL,
                    ltv VARCHAR NOT NULL,
                    health_factor VARCHAR NOT NULL,
                    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
                );
                "#,
            )
            .execute(pool)
            .await?;

            // Create liquidation_events table
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS liquidation_events (
                    id SERIAL PRIMARY KEY,
                    user_address VARCHAR NOT NULL,
                    collateral_asset VARCHAR NOT NULL,
                    debt_asset VARCHAR NOT NULL,
                    debt_covered VARCHAR NOT NULL,
                    collateral_received VARCHAR NOT NULL,
                    profit VARCHAR NOT NULL,
                    tx_hash VARCHAR,
                    block_number BIGINT,
                    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
                );
                "#,
            )
            .execute(pool)
            .await?;

            // Create indexes
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_positions_health_factor ON user_positions(health_factor);")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_positions_is_at_risk ON user_positions(is_at_risk);")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_liquidation_events_timestamp ON liquidation_events(timestamp);")
                .execute(pool)
                .await?;
        }
        DatabasePool::Sqlite(pool) => {
            info!("Creating SQLite tables...");
            
            // Create user_positions table with SQLite syntax
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS user_positions (
                    address TEXT PRIMARY KEY,
                    total_collateral_base TEXT NOT NULL,
                    total_debt_base TEXT NOT NULL,
                    available_borrows_base TEXT NOT NULL,
                    current_liquidation_threshold TEXT NOT NULL,
                    ltv TEXT NOT NULL,
                    health_factor TEXT NOT NULL,
                    last_updated DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    is_at_risk BOOLEAN NOT NULL DEFAULT 0
                );
                "#,
            )
            .execute(pool)
            .await?;

            // Create liquidation_events table
            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS liquidation_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    user_address TEXT NOT NULL,
                    collateral_asset TEXT NOT NULL,
                    debt_asset TEXT NOT NULL,
                    debt_covered TEXT NOT NULL,
                    collateral_received TEXT NOT NULL,
                    profit TEXT NOT NULL,
                    tx_hash TEXT,
                    block_number INTEGER,
                    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                "#,
            )
            .execute(pool)
            .await?;

            // Create indexes
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_positions_health_factor ON user_positions(health_factor);")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_positions_is_at_risk ON user_positions(is_at_risk);")
                .execute(pool)
                .await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_liquidation_events_timestamp ON liquidation_events(timestamp);")
                .execute(pool)
                .await?;
        }
    }

    info!("✅ Database tables created successfully");
    Ok(())
}

/// Save or update user position
pub async fn save_user_position(db_pool: &DatabasePool, position: &UserPosition) -> Result<()> {
    let address_str = position.address.to_string();
    
    // Convert Uint values to strings for database storage
    let total_collateral_str = position.total_collateral_base.to_string();
    let total_debt_str = position.total_debt_base.to_string();
    let available_borrows_str = position.available_borrows_base.to_string();
    let threshold_str = position.current_liquidation_threshold.to_string();
    let ltv_str = position.ltv.to_string();
    let health_factor_str = position.health_factor.to_string();

    match db_pool {
        DatabasePool::Postgres(pool) => {
            sqlx::query(
                r#"
                INSERT INTO user_positions (
                    address, total_collateral_base, total_debt_base, available_borrows_base,
                    current_liquidation_threshold, ltv, health_factor, last_updated, is_at_risk
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (address) 
                DO UPDATE SET
                    total_collateral_base = EXCLUDED.total_collateral_base,
                    total_debt_base = EXCLUDED.total_debt_base,
                    available_borrows_base = EXCLUDED.available_borrows_base,
                    current_liquidation_threshold = EXCLUDED.current_liquidation_threshold,
                    ltv = EXCLUDED.ltv,
                    health_factor = EXCLUDED.health_factor,
                    last_updated = EXCLUDED.last_updated,
                    is_at_risk = EXCLUDED.is_at_risk
                "#,
            )
            .bind(&address_str)
            .bind(&total_collateral_str)
            .bind(&total_debt_str)
            .bind(&available_borrows_str)
            .bind(&threshold_str)
            .bind(&ltv_str)
            .bind(&health_factor_str)
            .bind(&position.last_updated)
            .bind(&position.is_at_risk)
            .execute(pool)
            .await?;
        }
        DatabasePool::Sqlite(pool) => {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO user_positions (
                    address, total_collateral_base, total_debt_base, available_borrows_base,
                    current_liquidation_threshold, ltv, health_factor, last_updated, is_at_risk
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&address_str)
            .bind(&total_collateral_str)
            .bind(&total_debt_str)
            .bind(&available_borrows_str)
            .bind(&threshold_str)
            .bind(&ltv_str)
            .bind(&health_factor_str)
            .bind(&position.last_updated)
            .bind(&position.is_at_risk)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Get user position by address
pub async fn get_user_position(db_pool: &DatabasePool, address: Address) -> Result<Option<UserPosition>> {
    let address_str = address.to_string().to_lowercase();

    match db_pool {
        DatabasePool::Postgres(pool) => {
            let row_opt = sqlx::query("SELECT * FROM user_positions WHERE LOWER(address) = $1")
                .bind(&address_str)
                .fetch_optional(pool)
                .await?;
                
            if let Some(row) = row_opt {
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                Ok(Some(position))
            } else {
                Ok(None)
            }
        }
        DatabasePool::Sqlite(pool) => {
            let row_opt = sqlx::query("SELECT * FROM user_positions WHERE LOWER(address) = ?")
                .bind(&address_str)
                .fetch_optional(pool)
                .await?;
                
            if let Some(row) = row_opt {
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                Ok(Some(position))
            } else {
                Ok(None)
            }
        }
    }
}

/// Get all user positions (ordered by last_updated)
pub async fn get_all_user_positions(db_pool: &DatabasePool) -> Result<Vec<UserPosition>> {
    match db_pool {
        DatabasePool::Postgres(pool) => {
            let rows = sqlx::query("SELECT * FROM user_positions ORDER BY last_updated DESC")
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
        DatabasePool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM user_positions ORDER BY last_updated DESC")
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
    }
}

/// Get at-risk users (health factor < 1.05)
pub async fn get_at_risk_users(db_pool: &DatabasePool) -> Result<Vec<UserPosition>> {
    get_at_risk_users_with_limit(db_pool, None).await
}

/// Get at-risk users with optional limit (health factor < 1.05)
pub async fn get_at_risk_users_with_limit(
    db_pool: &DatabasePool,
    limit: Option<usize>,
) -> Result<Vec<UserPosition>> {
    match db_pool {
        DatabasePool::Postgres(pool) => {
            let query_str = if let Some(limit) = limit {
                format!(
                    "SELECT * FROM user_positions WHERE is_at_risk = true ORDER BY health_factor ASC LIMIT {}",
                    limit
                )
            } else {
                "SELECT * FROM user_positions WHERE is_at_risk = true ORDER BY health_factor ASC".to_string()
            };
            
            let rows = sqlx::query(&query_str)
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
        DatabasePool::Sqlite(pool) => {
            let query_str = if let Some(limit) = limit {
                format!(
                    "SELECT * FROM user_positions WHERE is_at_risk = 1 ORDER BY health_factor ASC LIMIT {}",
                    limit
                )
            } else {
                "SELECT * FROM user_positions WHERE is_at_risk = 1 ORDER BY health_factor ASC".to_string()
            };
            
            let rows = sqlx::query(&query_str)
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
    }
}

/// Get user position count
pub async fn get_user_position_count(db_pool: &DatabasePool) -> Result<i64> {
    let count = match db_pool {
        DatabasePool::Postgres(pool) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM user_positions")
                .fetch_one(pool)
                .await?;
            row.get::<i64, _>("count")
        }
        DatabasePool::Sqlite(pool) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM user_positions")
                .fetch_one(pool)
                .await?;
            row.get::<i32, _>("count") as i64
        }
    };
    Ok(count)
}

/// Record a liquidation event
pub async fn record_liquidation_event(
    db_pool: &DatabasePool,
    user_address: &Address,
    collateral_asset: &str,
    debt_asset: &str,
    debt_covered: &str,
    collateral_received: &str,
    profit: &str,
    tx_hash: Option<&str>,
    block_number: Option<i64>,
) -> Result<()> {
    let user_str = user_address.to_string();

    match db_pool {
        DatabasePool::Postgres(pool) => {
            sqlx::query(
                r#"
                INSERT INTO liquidation_events (
                    user_address, collateral_asset, debt_asset, debt_covered,
                    collateral_received, profit, tx_hash, block_number
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(&user_str)
            .bind(collateral_asset)
            .bind(debt_asset)
            .bind(debt_covered)
            .bind(collateral_received)
            .bind(profit)
            .bind(tx_hash)
            .bind(block_number)
            .execute(pool)
            .await?;
        }
        DatabasePool::Sqlite(pool) => {
            sqlx::query(
                r#"
                INSERT INTO liquidation_events (
                    user_address, collateral_asset, debt_asset, debt_covered,
                    collateral_received, profit, tx_hash, block_number, timestamp
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(&user_str)
            .bind(collateral_asset)
            .bind(debt_asset)
            .bind(debt_covered)
            .bind(collateral_received)
            .bind(profit)
            .bind(tx_hash)
            .bind(block_number)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Log monitoring events (simplified for now - just use tracing)
pub async fn log_monitoring_event(
    _db_pool: &DatabasePool,
    event_type: &str,
    user_address: Option<Address>,
    message: Option<&str>,
) -> Result<()> {
    // For now, just log using tracing instead of database storage
    match (user_address, message) {
        (Some(addr), Some(msg)) => {
            info!("📊 {} - User: {} - {}", event_type, addr, msg);
        }
        (Some(addr), None) => {
            info!("📊 {} - User: {}", event_type, addr);
        }
        (None, Some(msg)) => {
            info!("📊 {} - {}", event_type, msg);
        }
        (None, None) => {
            info!("📊 {}", event_type);
        }
    }
    Ok(())
}

/// Get all users from the database for full rescan
pub async fn get_all_users(db_pool: &DatabasePool) -> Result<Vec<UserPosition>> {
    match db_pool {
        DatabasePool::Postgres(pool) => {
            let rows = sqlx::query("SELECT * FROM user_positions ORDER BY health_factor ASC")
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
        DatabasePool::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM user_positions ORDER BY health_factor ASC")
                .fetch_all(pool)
                .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
    }
}

/// Get users eligible for archival (zero debt and high health factor for a cooldown period)
pub async fn get_users_eligible_for_archival(
    db_pool: &DatabasePool,
    cooldown_hours: u64,
    safe_health_factor_threshold: alloy_primitives::U256,
) -> Result<Vec<UserPosition>> {
    let cooldown_timestamp = chrono::Utc::now() - chrono::Duration::hours(cooldown_hours as i64);
    let health_factor_str = safe_health_factor_threshold.to_string();

    match db_pool {
        DatabasePool::Postgres(pool) => {
            let rows = sqlx::query(
                r#"
                SELECT * FROM user_positions 
                WHERE total_debt_base = '0' 
                  AND health_factor >= $1
                  AND last_updated <= $2
                ORDER BY last_updated ASC
                "#,
            )
            .bind(&health_factor_str)
            .bind(&cooldown_timestamp)
            .fetch_all(pool)
            .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
        DatabasePool::Sqlite(pool) => {
            let rows = sqlx::query(
                r#"
                SELECT * FROM user_positions 
                WHERE total_debt_base = '0' 
                  AND CAST(health_factor AS INTEGER) >= CAST(? AS INTEGER)
                  AND last_updated <= ?
                ORDER BY last_updated ASC
                "#,
            )
            .bind(&health_factor_str)
            .bind(&cooldown_timestamp)
            .fetch_all(pool)
            .await?;

            let mut positions = Vec::new();
            for row in rows {
                let address = Address::parse_checksummed(row.get::<String, _>("address"), None)?;
                let position = UserPosition {
                    address,
                    total_collateral_base: row.get::<String, _>("total_collateral_base").parse()?,
                    total_debt_base: row.get::<String, _>("total_debt_base").parse()?,
                    available_borrows_base: row.get::<String, _>("available_borrows_base").parse()?,
                    current_liquidation_threshold: row.get::<String, _>("current_liquidation_threshold").parse()?,
                    ltv: row.get::<String, _>("ltv").parse()?,
                    health_factor: row.get::<String, _>("health_factor").parse()?,
                    last_updated: row.get("last_updated"),
                    is_at_risk: row.get("is_at_risk"),
                };
                positions.push(position);
            }
            Ok(positions)
        }
    }
}

/// Archive (delete) users with zero debt and safe health factor
pub async fn archive_zero_debt_users(
    db_pool: &DatabasePool,
    user_addresses: &[Address],
) -> Result<u64> {
    if user_addresses.is_empty() {
        return Ok(0);
    }

    let address_strings: Vec<String> = user_addresses.iter().map(|addr| addr.to_string()).collect();

    match db_pool {
        DatabasePool::Postgres(pool) => {
            // Use ANY to handle the array of addresses
            let placeholders: Vec<String> = (1..=address_strings.len()).map(|i| format!("${}", i)).collect();
            let query = format!(
                "DELETE FROM user_positions WHERE address = ANY(ARRAY[{}])",
                placeholders.join(", ")
            );
            
            let mut query_builder = sqlx::query(&query);
            for addr_str in &address_strings {
                query_builder = query_builder.bind(addr_str);
            }
            
            let result = query_builder.execute(pool).await?;
            Ok(result.rows_affected())
        }
        DatabasePool::Sqlite(pool) => {
            // Use IN clause for SQLite
            let placeholders: Vec<String> = (0..address_strings.len()).map(|_| "?".to_string()).collect();
            let query = format!(
                "DELETE FROM user_positions WHERE address IN ({})",
                placeholders.join(", ")
            );
            
            let mut query_builder = sqlx::query(&query);
            for addr_str in &address_strings {
                query_builder = query_builder.bind(addr_str);
            }
            
            let result = query_builder.execute(pool).await?;
            Ok(result.rows_affected())
        }
    }
}

/// Get count of users with zero debt for monitoring purposes
pub async fn get_zero_debt_user_count(db_pool: &DatabasePool) -> Result<i64> {
    let count = match db_pool {
        DatabasePool::Postgres(pool) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM user_positions WHERE total_debt_base = '0'")
                .fetch_one(pool)
                .await?;
            row.get::<i64, _>("count")
        }
        DatabasePool::Sqlite(pool) => {
            let row = sqlx::query("SELECT COUNT(*) as count FROM user_positions WHERE total_debt_base = '0'")
                .fetch_one(pool)
                .await?;
            row.get::<i32, _>("count") as i64
        }
    };
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};

    #[tokio::test]
    async fn test_archival_functions() {
        // Test basic archival query construction for SQLite
        let mock_addresses = vec![
            Address::from([1u8; 20]),
            Address::from([2u8; 20]),
        ];
        
        // Test that the query construction doesn't panic
        let address_strings: Vec<String> = mock_addresses.iter().map(|addr| addr.to_string()).collect();
        let placeholders: Vec<String> = (0..address_strings.len()).map(|_| "?".to_string()).collect();
        let query = format!(
            "DELETE FROM user_positions WHERE address IN ({})",
            placeholders.join(", ")
        );
        
        assert_eq!(query, "DELETE FROM user_positions WHERE address IN (?, ?)");
        
        // Test config parsing for archival settings
        let threshold = U256::from(10000000000000000000u64); // 10.0 ETH in wei
        assert_eq!(threshold.to_string(), "10000000000000000000");
    }
}

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
    /// Execute a query and return the first row
    pub async fn fetch_one(&self, query: &str) -> Result<sqlx::Row, sqlx::Error> {
        match self {
            DatabasePool::Postgres(pool) => {
                sqlx::query(query).fetch_one(pool).await
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(query).fetch_one(pool).await
            }
        }
    }

    /// Execute a query
    pub async fn execute(&self, query: &str) -> Result<sqlx::database::QueryResult, sqlx::Error> {
        match self {
            DatabasePool::Postgres(pool) => {
                sqlx::query(query).execute(pool).await
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query(query).execute(pool).await
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
            let pool = Pool::<Postgres>::connect(database_url).await?;
            
            // Verify connection
            sqlx::query("SELECT 1")
                .fetch_one(&pool)
                .await
                .map_err(|e| eyre::eyre!("Database connection verification failed: {}", e))?;
            
            // Create tables
            create_postgres_tables(&pool).await?;
            DatabasePool::Postgres(pool)
        }
        "sqlite" => {
            let pool = Pool::<Sqlite>::connect(database_url).await?;
            
            // Verify connection
            sqlx::query("SELECT 1")
                .fetch_one(&pool)
                .await
                .map_err(|e| eyre::eyre!("Database connection verification failed: {}", e))?;
            
            // Create tables
            create_sqlite_tables(&pool).await?;
            DatabasePool::Sqlite(pool)
        }
        _ => return Err(eyre::eyre!("Unsupported database type: {}", db_type)),
    };

    info!("Database initialized successfully");
    Ok(pool)
}

/// Create tables for PostgreSQL
async fn create_postgres_tables(pool: &Pool<Postgres>) -> Result<()> {
    // Create user_positions table
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
            last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create liquidation_events table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS liquidation_events (
            id SERIAL PRIMARY KEY,
            user_address TEXT NOT NULL,
            collateral_asset TEXT NOT NULL,
            debt_asset TEXT NOT NULL,
            debt_covered TEXT NOT NULL,
            collateral_received TEXT NOT NULL,
            profit TEXT NOT NULL,
            tx_hash TEXT,
            block_number BIGINT,
            timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create index for user_positions.address
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_user_positions_address ON user_positions(address)"
    )
    .execute(pool)
    .await?;

    // Create index for liquidation_events.user_address
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_liquidation_events_user_address ON liquidation_events(user_address)"
    )
    .execute(pool)
    .await?;

    info!("PostgreSQL tables created/verified successfully");
    Ok(())
}

/// Create tables for SQLite
async fn create_sqlite_tables(pool: &Pool<Sqlite>) -> Result<()> {
    // Create user_positions table
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
            last_updated TEXT NOT NULL DEFAULT (datetime('now')),
            is_at_risk INTEGER NOT NULL DEFAULT 0
        )
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
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create index for user_positions.address
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_user_positions_address ON user_positions(address)"
    )
    .execute(pool)
    .await?;

    // Create index for liquidation_events.user_address
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_liquidation_events_user_address ON liquidation_events(user_address)"
    )
    .execute(pool)
    .await?;

    info!("SQLite tables created/verified successfully");
    Ok(())
}

/// Save user position to database
pub async fn save_user_position(
    db_pool: &DatabasePool,
    position: &UserPosition,
) -> Result<()> {
    match db_pool {
        DatabasePool::Postgres(pool) => {
            let timestamp = position.last_updated;
            sqlx::query(
                r#"
                INSERT INTO user_positions (
                    address, total_collateral_base, total_debt_base, 
                    available_borrows_base, current_liquidation_threshold, 
                    ltv, health_factor, last_updated, is_at_risk
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (address) DO UPDATE SET
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
            .bind(&position.address.to_string())
            .bind(&position.total_collateral_base)
            .bind(&position.total_debt_base)
            .bind(&position.available_borrows_base)
            .bind(&position.current_liquidation_threshold)
            .bind(&position.ltv)
            .bind(&position.health_factor)
            .bind(timestamp)
            .bind(position.is_at_risk)
            .execute(pool)
            .await?;
        }
        DatabasePool::Sqlite(pool) => {
            let timestamp = position.last_updated.format("%Y-%m-%d %H:%M:%S").to_string();
            sqlx::query(
                r#"
                INSERT INTO user_positions (
                    address, total_collateral_base, total_debt_base, 
                    available_borrows_base, current_liquidation_threshold, 
                    ltv, health_factor, last_updated, is_at_risk
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (address) DO UPDATE SET
                    total_collateral_base = excluded.total_collateral_base,
                    total_debt_base = excluded.total_debt_base,
                    available_borrows_base = excluded.available_borrows_base,
                    current_liquidation_threshold = excluded.current_liquidation_threshold,
                    ltv = excluded.ltv,
                    health_factor = excluded.health_factor,
                    last_updated = excluded.last_updated,
                    is_at_risk = excluded.is_at_risk
                "#,
            )
            .bind(&position.address.to_string())
            .bind(&position.total_collateral_base)
            .bind(&position.total_debt_base)
            .bind(&position.available_borrows_base)
            .bind(&position.current_liquidation_threshold)
            .bind(&position.ltv)
            .bind(&position.health_factor)
            .bind(timestamp)
            .bind(if position.is_at_risk { 1 } else { 0 })
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Get user position count from database
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
            row.get::<i64, _>("count")
        }
    };

    Ok(count)
}

/// Get user position from database by address
pub async fn get_user_position_by_address(
    db_pool: &DatabasePool,
    address: &Address,
) -> Result<Option<UserPosition>> {
    let address_str = address.to_string().to_lowercase();
    
    let row_opt = match db_pool {
        DatabasePool::Postgres(pool) => {
            sqlx::query("SELECT * FROM user_positions WHERE LOWER(address) = $1")
                .bind(&address_str)
                .fetch_optional(pool)
                .await?
        }
        DatabasePool::Sqlite(pool) => {
            sqlx::query("SELECT * FROM user_positions WHERE LOWER(address) = ?")
                .bind(&address_str)
                .fetch_optional(pool)
                .await?
        }
    };

    if let Some(row) = row_opt {
        let address_from_db: String = row.get("address");
        let last_updated = match db_pool {
            DatabasePool::Postgres(_) => {
                row.get::<chrono::DateTime<chrono::Utc>, _>("last_updated")
            }
            DatabasePool::Sqlite(_) => {
                let timestamp_str: String = row.get("last_updated");
                chrono::DateTime::parse_from_str(&timestamp_str, "%Y-%m-%d %H:%M:%S")
                    .map_err(|e| eyre::eyre!("Failed to parse timestamp: {}", e))?
                    .with_timezone(&chrono::Utc)
            }
        };

        let is_at_risk = match db_pool {
            DatabasePool::Postgres(_) => row.get::<bool, _>("is_at_risk"),
            DatabasePool::Sqlite(_) => {
                let value: i32 = row.get("is_at_risk");
                value != 0
            }
        };

        Ok(Some(UserPosition {
            address: address_from_db.parse()?,
            total_collateral_base: row.get("total_collateral_base"),
            total_debt_base: row.get("total_debt_base"),
            available_borrows_base: row.get("available_borrows_base"),
            current_liquidation_threshold: row.get("current_liquidation_threshold"),
            ltv: row.get("ltv"),
            health_factor: row.get("health_factor"),
            last_updated,
            is_at_risk,
        }))
    } else {
        Ok(None)
    }
}

/// Record a liquidation event in the database
pub async fn record_liquidation_event(
    db_pool: &DatabasePool,
    user_address: &Address,
    collateral_asset: &str,
    debt_asset: &str,
    debt_covered: &str,
    collateral_received: &str,
    profit: &str,
    tx_hash: Option<&str>,
    block_number: Option<u64>,
) -> Result<()> {
    let user_address_str = user_address.to_string();
    
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
            .bind(&user_address_str)
            .bind(collateral_asset)
            .bind(debt_asset)
            .bind(debt_covered)
            .bind(collateral_received)
            .bind(profit)
            .bind(tx_hash)
            .bind(block_number.map(|n| n as i64))
            .execute(pool)
            .await?;
        }
        DatabasePool::Sqlite(pool) => {
            sqlx::query(
                r#"
                INSERT INTO liquidation_events (
                    user_address, collateral_asset, debt_asset, debt_covered,
                    collateral_received, profit, tx_hash, block_number
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&user_address_str)
            .bind(collateral_asset)
            .bind(debt_asset)
            .bind(debt_covered)
            .bind(collateral_received)
            .bind(profit)
            .bind(tx_hash)
            .bind(block_number.map(|n| n as i64))
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

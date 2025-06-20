use alloy_primitives::Address;
use chrono::Utc;
use eyre::Result;
use sqlx::{Pool, Row, Sqlite};
use tracing::info;
use crate::models::UserPosition;
use crate::monitoring::AssetPrice;

pub async fn init_database(database_url: &str) -> Result<Pool<Sqlite>> {
    let pool = sqlx::SqlitePool::connect(database_url).await?;

    // Create tables if they don't exist
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
            last_updated DATETIME NOT NULL,
            is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
        )
    "#,
    )
    .execute(&pool)
    .await?;

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
            timestamp DATETIME NOT NULL
        )
    "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS monitoring_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type TEXT NOT NULL,
            user_address TEXT,
            asset_address TEXT,
            health_factor TEXT,
            timestamp DATETIME NOT NULL,
            details TEXT
        )
    "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS asset_prices (
            asset_address TEXT PRIMARY KEY,
            price TEXT NOT NULL,
            decimals INTEGER NOT NULL,
            last_updated DATETIME NOT NULL,
            source TEXT NOT NULL DEFAULT 'chainlink'
        )
    "#,
    )
    .execute(&pool)
    .await?;

    info!("Database initialized successfully");
    Ok(pool)
}

pub async fn save_user_position(db_pool: &Pool<Sqlite>, position: &UserPosition) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO user_positions (
            address, total_collateral_base, total_debt_base, available_borrows_base,
            current_liquidation_threshold, ltv, health_factor, last_updated, is_at_risk
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#,
    )
    .bind(position.address.to_string())
    .bind(position.total_collateral_base.to_string())
    .bind(position.total_debt_base.to_string())
    .bind(position.available_borrows_base.to_string())
    .bind(position.current_liquidation_threshold.to_string())
    .bind(position.ltv.to_string())
    .bind(position.health_factor.to_string())
    .bind(position.last_updated)
    .bind(position.is_at_risk)
    .execute(db_pool)
    .await?;

    Ok(())
}

pub async fn log_monitoring_event(
    db_pool: &Pool<Sqlite>,
    event_type: &str,
    user_address: Option<Address>,
    details: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO monitoring_events (event_type, user_address, timestamp, details)
        VALUES (?, ?, ?, ?)
    "#,
    )
    .bind(event_type)
    .bind(user_address.map(|addr| addr.to_string()))
    .bind(Utc::now())
    .bind(details)
    .execute(db_pool)
    .await?;

    Ok(())
}

pub async fn get_at_risk_users(db_pool: &Pool<Sqlite>) -> Result<Vec<Address>> {
    let rows = sqlx::query(
        "SELECT address FROM user_positions WHERE is_at_risk = true OR total_debt_base != '0' ORDER BY health_factor ASC LIMIT 100"
    )
    .fetch_all(db_pool)
    .await?;

    let mut users = Vec::new();
    for row in rows {
        if let Ok(addr_str) = row.try_get::<String, _>("address") {
            if let Ok(addr) = addr_str.parse::<Address>() {
                users.push(addr);
            }
        }
    }

    Ok(users)
}

pub async fn save_asset_price(db_pool: &Pool<Sqlite>, asset_price: &AssetPrice) -> Result<()> {
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO asset_prices (
            asset_address, price, decimals, last_updated, source
        ) VALUES (?, ?, ?, ?, ?)
    "#,
    )
    .bind(asset_price.asset.to_string())
    .bind(asset_price.price.to_string())
    .bind(asset_price.decimals as i32)
    .bind(chrono::DateTime::from_timestamp(asset_price.last_updated as i64, 0)
        .unwrap_or_else(|| Utc::now()))
    .bind("chainlink")
    .execute(db_pool)
    .await?;

    Ok(())
}
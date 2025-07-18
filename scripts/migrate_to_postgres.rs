use eyre::Result;
use sqlx::{Pool, Postgres, Row, Sqlite};
use std::env;
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
struct UserPosition {
    address: String,
    total_collateral_base: String,
    total_debt_base: String,
    available_borrows_base: String,
    current_liquidation_threshold: String,
    ltv: String,
    health_factor: String,
    last_updated: chrono::DateTime<chrono::Utc>,
    is_at_risk: bool,
}

#[derive(Debug, Clone)]
struct LiquidationEvent {
    id: i64,
    user_address: String,
    collateral_asset: String,
    debt_asset: String,
    debt_covered: String,
    collateral_received: String,
    profit: String,
    tx_hash: Option<String>,
    block_number: Option<i64>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
struct MonitoringEvent {
    id: i64,
    event_type: String,
    user_address: Option<String>,
    asset_address: Option<String>,
    health_factor: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
    details: Option<String>,
}

#[derive(Debug, Clone)]
struct PriceFeed {
    id: i64,
    asset_address: String,
    asset_symbol: String,
    price: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// Initialize PostgreSQL database with schema
async fn init_postgres_schema(postgres_pool: &Pool<Postgres>) -> Result<()> {
    info!("Creating PostgreSQL schema...");

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
            last_updated TIMESTAMP NOT NULL,
            is_at_risk BOOLEAN NOT NULL DEFAULT FALSE
        )
        "#,
    )
    .execute(postgres_pool)
    .await?;

    // Create liquidation_events table with SERIAL instead of AUTOINCREMENT
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
            block_number INTEGER,
            timestamp TIMESTAMP NOT NULL
        )
        "#,
    )
    .execute(postgres_pool)
    .await?;

    // Create monitoring_events table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS monitoring_events (
            id SERIAL PRIMARY KEY,
            event_type TEXT NOT NULL,
            user_address TEXT,
            asset_address TEXT,
            health_factor TEXT,
            timestamp TIMESTAMP NOT NULL,
            details TEXT
        )
        "#,
    )
    .execute(postgres_pool)
    .await?;

    // Create price_feeds table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS price_feeds (
            id SERIAL PRIMARY KEY,
            asset_address TEXT NOT NULL,
            asset_symbol TEXT NOT NULL,
            price TEXT NOT NULL,
            timestamp TIMESTAMP NOT NULL
        )
        "#,
    )
    .execute(postgres_pool)
    .await?;

    // Create index for price feeds
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_asset_timestamp 
        ON price_feeds (asset_address, timestamp)
        "#,
    )
    .execute(postgres_pool)
    .await?;

    info!("PostgreSQL schema created successfully!");
    Ok(())
}

/// Migrate user positions from SQLite to PostgreSQL
async fn migrate_user_positions(
    sqlite_pool: &Pool<Sqlite>,
    postgres_pool: &Pool<Postgres>,
) -> Result<usize> {
    info!("Migrating user positions...");

    let rows = sqlx::query("SELECT * FROM user_positions")
        .fetch_all(sqlite_pool)
        .await?;

    let mut migrated_count = 0;

    for row in rows {
        let position = UserPosition {
            address: row.get("address"),
            total_collateral_base: row.get("total_collateral_base"),
            total_debt_base: row.get("total_debt_base"),
            available_borrows_base: row.get("available_borrows_base"),
            current_liquidation_threshold: row.get("current_liquidation_threshold"),
            ltv: row.get("ltv"),
            health_factor: row.get("health_factor"),
            last_updated: row.get("last_updated"),
            is_at_risk: row.get("is_at_risk"),
        };

        // Use ON CONFLICT instead of INSERT OR REPLACE
        sqlx::query(
            r#"
            INSERT INTO user_positions 
            (address, total_collateral_base, total_debt_base, available_borrows_base, 
             current_liquidation_threshold, ltv, health_factor, last_updated, is_at_risk)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
        .bind(&position.address)
        .bind(&position.total_collateral_base)
        .bind(&position.total_debt_base)
        .bind(&position.available_borrows_base)
        .bind(&position.current_liquidation_threshold)
        .bind(&position.ltv)
        .bind(&position.health_factor)
        .bind(&position.last_updated)
        .bind(&position.is_at_risk)
        .execute(postgres_pool)
        .await?;

        migrated_count += 1;
    }

    info!("Migrated {} user positions", migrated_count);
    Ok(migrated_count)
}

/// Migrate liquidation events from SQLite to PostgreSQL
async fn migrate_liquidation_events(
    sqlite_pool: &Pool<Sqlite>,
    postgres_pool: &Pool<Postgres>,
) -> Result<usize> {
    info!("Migrating liquidation events...");

    let rows = sqlx::query("SELECT * FROM liquidation_events")
        .fetch_all(sqlite_pool)
        .await?;

    let mut migrated_count = 0;

    for row in rows {
        let event = LiquidationEvent {
            id: row.get("id"),
            user_address: row.get("user_address"),
            collateral_asset: row.get("collateral_asset"),
            debt_asset: row.get("debt_asset"),
            debt_covered: row.get("debt_covered"),
            collateral_received: row.get("collateral_received"),
            profit: row.get("profit"),
            tx_hash: row.get("tx_hash"),
            block_number: row.get("block_number"),
            timestamp: row.get("timestamp"),
        };

        // PostgreSQL will auto-generate the ID
        sqlx::query(
            r#"
            INSERT INTO liquidation_events 
            (user_address, collateral_asset, debt_asset, debt_covered,
             collateral_received, profit, tx_hash, block_number, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(&event.user_address)
        .bind(&event.collateral_asset)
        .bind(&event.debt_asset)
        .bind(&event.debt_covered)
        .bind(&event.collateral_received)
        .bind(&event.profit)
        .bind(&event.tx_hash)
        .bind(&event.block_number)
        .bind(&event.timestamp)
        .execute(postgres_pool)
        .await?;

        migrated_count += 1;
    }

    info!("Migrated {} liquidation events", migrated_count);
    Ok(migrated_count)
}

/// Migrate monitoring events from SQLite to PostgreSQL
async fn migrate_monitoring_events(
    sqlite_pool: &Pool<Sqlite>,
    postgres_pool: &Pool<Postgres>,
) -> Result<usize> {
    info!("Migrating monitoring events...");

    let rows = sqlx::query("SELECT * FROM monitoring_events")
        .fetch_all(sqlite_pool)
        .await?;

    let mut migrated_count = 0;

    for row in rows {
        let event = MonitoringEvent {
            id: row.get("id"),
            event_type: row.get("event_type"),
            user_address: row.get("user_address"),
            asset_address: row.get("asset_address"),
            health_factor: row.get("health_factor"),
            timestamp: row.get("timestamp"),
            details: row.get("details"),
        };

        sqlx::query(
            r#"
            INSERT INTO monitoring_events 
            (event_type, user_address, asset_address, health_factor, timestamp, details)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&event.event_type)
        .bind(&event.user_address)
        .bind(&event.asset_address)
        .bind(&event.health_factor)
        .bind(&event.timestamp)
        .bind(&event.details)
        .execute(postgres_pool)
        .await?;

        migrated_count += 1;
    }

    info!("Migrated {} monitoring events", migrated_count);
    Ok(migrated_count)
}

/// Migrate price feeds from SQLite to PostgreSQL
async fn migrate_price_feeds(
    sqlite_pool: &Pool<Sqlite>,
    postgres_pool: &Pool<Postgres>,
) -> Result<usize> {
    info!("Migrating price feeds...");

    let rows = sqlx::query("SELECT * FROM price_feeds")
        .fetch_all(sqlite_pool)
        .await?;

    let mut migrated_count = 0;

    for row in rows {
        let feed = PriceFeed {
            id: row.get("id"),
            asset_address: row.get("asset_address"),
            asset_symbol: row.get("asset_symbol"),
            price: row.get("price"),
            timestamp: row.get("timestamp"),
        };

        sqlx::query(
            r#"
            INSERT INTO price_feeds 
            (asset_address, asset_symbol, price, timestamp)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(&feed.asset_address)
        .bind(&feed.asset_symbol)
        .bind(&feed.price)
        .bind(&feed.timestamp)
        .execute(postgres_pool)
        .await?;

        migrated_count += 1;
    }

    info!("Migrated {} price feeds", migrated_count);
    Ok(migrated_count)
}

/// Main migration function
pub async fn migrate_sqlite_to_postgres() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🚀 Starting SQLite to PostgreSQL migration...");

    // Get database URLs from environment
    let sqlite_url = env::var("SQLITE_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:liquidation_bot.db".to_string());
    
    let postgres_url = env::var("POSTGRES_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| eyre::eyre!("POSTGRES_DATABASE_URL or DATABASE_URL environment variable not set"))?;

    if !postgres_url.starts_with("postgresql://") && !postgres_url.starts_with("postgres://") {
        return Err(eyre::eyre!("DATABASE_URL must be a PostgreSQL connection string"));
    }

    info!("SQLite source: {}", sqlite_url);
    info!("PostgreSQL target: {}", postgres_url.split('@').collect::<Vec<_>>().first().unwrap_or(&"[hidden]"));

    // Connect to both databases
    info!("Connecting to SQLite database...");
    let sqlite_pool = sqlx::SqlitePool::connect(&sqlite_url).await?;

    info!("Connecting to PostgreSQL database...");
    let postgres_pool = sqlx::PgPool::connect(&postgres_url).await?;

    // Test connections
    sqlx::query("SELECT 1").fetch_one(&sqlite_pool).await?;
    sqlx::query("SELECT 1").fetch_one(&postgres_pool).await?;
    info!("✅ Database connections established");

    // Initialize PostgreSQL schema
    init_postgres_schema(&postgres_pool).await?;

    // Perform migration for each table
    let mut total_migrated = 0;

    match migrate_user_positions(&sqlite_pool, &postgres_pool).await {
        Ok(count) => total_migrated += count,
        Err(e) => {
            error!("Failed to migrate user positions: {}", e);
            return Err(e);
        }
    }

    match migrate_liquidation_events(&sqlite_pool, &postgres_pool).await {
        Ok(count) => total_migrated += count,
        Err(e) => {
            error!("Failed to migrate liquidation events: {}", e);
            return Err(e);
        }
    }

    match migrate_monitoring_events(&sqlite_pool, &postgres_pool).await {
        Ok(count) => total_migrated += count,
        Err(e) => {
            error!("Failed to migrate monitoring events: {}", e);
            return Err(e);
        }
    }

    match migrate_price_feeds(&sqlite_pool, &postgres_pool).await {
        Ok(count) => total_migrated += count,
        Err(e) => {
            error!("Failed to migrate price feeds: {}", e);
            return Err(e);
        }
    }

    info!("🎉 Migration completed successfully!");
    info!("📊 Total records migrated: {}", total_migrated);
    info!("💡 Don't forget to update your DATABASE_URL to use PostgreSQL");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    migrate_sqlite_to_postgres().await
}
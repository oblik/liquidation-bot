# PostgreSQL Migration Guide

This guide will help you migrate your liquidation bot from SQLite to PostgreSQL for better production performance and scalability.

## 🏃‍♂️ Quick Start Migration

### 1. Set Up PostgreSQL

#### Option A: Local PostgreSQL Installation

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install postgresql postgresql-contrib
sudo systemctl start postgresql
sudo systemctl enable postgresql
```

**macOS:**
```bash
brew install postgresql
brew services start postgresql
```

**Create Database and User:**
```bash
# Connect as postgres user
sudo -u postgres psql

# Create database and user
CREATE DATABASE liquidation_bot;
CREATE USER liquidation_user WITH PASSWORD 'your_secure_password';
GRANT ALL PRIVILEGES ON DATABASE liquidation_bot TO liquidation_user;
\q
```

#### Option B: Docker PostgreSQL

```bash
# Run PostgreSQL in Docker
docker run --name postgres-liquidation \
  -e POSTGRES_DB=liquidation_bot \
  -e POSTGRES_USER=liquidation_user \
  -e POSTGRES_PASSWORD=your_secure_password \
  -p 5432:5432 \
  -d postgres:15

# Verify it's running
docker ps | grep postgres-liquidation
```

#### Option C: Cloud PostgreSQL

Popular managed PostgreSQL services:
- **AWS RDS PostgreSQL**
- **Google Cloud SQL for PostgreSQL**
- **DigitalOcean Managed Databases**
- **Heroku Postgres**
- **Supabase**

### 2. Run the Migration

Set environment variables:
```bash
# Your current SQLite database (optional, defaults to sqlite:liquidation_bot.db)
export SQLITE_DATABASE_URL="sqlite:liquidation_bot.db"

# Your new PostgreSQL connection string
export POSTGRES_DATABASE_URL="postgresql://liquidation_user:your_secure_password@localhost/liquidation_bot"
```

Run the migration script:
```bash
cargo run --bin migrate_to_postgres
```

### 3. Update Your Configuration

Update your `.env` file:
```bash
# Old SQLite configuration
# DATABASE_URL=sqlite:liquidation_bot.db

# New PostgreSQL configuration
DATABASE_URL=postgresql://liquidation_user:your_secure_password@localhost/liquidation_bot
```

### 4. Test the Migration

Run your bot to verify everything works:
```bash
cargo run --release
```

## 📊 Migration Details

### What Gets Migrated

The migration script transfers all data from these SQLite tables to PostgreSQL:

1. **user_positions** - User health factors and position data
2. **liquidation_events** - Historical liquidation records
3. **monitoring_events** - Bot activity logs
4. **price_feeds** - Oracle price history

### Schema Differences

| Feature | SQLite | PostgreSQL |
|---------|--------|------------|
| Auto-increment | `INTEGER PRIMARY KEY AUTOINCREMENT` | `SERIAL PRIMARY KEY` |
| Timestamp | `DATETIME` | `TIMESTAMP` |
| Current time | `datetime('now')` | `NOW()` |
| Upsert | `INSERT OR REPLACE` | `INSERT ... ON CONFLICT DO UPDATE` |

### Migration Safety

The migration script:
- ✅ **Non-destructive** - Does not modify your original SQLite database
- ✅ **Transactional** - Uses database transactions for consistency
- ✅ **Resumable** - Can be run multiple times safely
- ✅ **Validated** - Tests connections before starting migration

## 🔧 Advanced Configuration

### Connection String Examples

#### Basic Connection
```bash
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot
```

#### With SSL (Recommended for production)
```bash
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot?sslmode=require
```

#### With Connection Pooling
```bash
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot?pool_max_conns=10&pool_min_conns=2
```

#### Cloud Examples

**AWS RDS:**
```bash
DATABASE_URL=postgresql://username:password@myinstance.cluster-xyz.us-east-1.rds.amazonaws.com:5432/liquidation_bot?sslmode=require
```

**Google Cloud SQL:**
```bash
DATABASE_URL=postgresql://username:password@10.1.2.3:5432/liquidation_bot?sslmode=require
```

### Performance Tuning

#### PostgreSQL Configuration

Add these settings to your `postgresql.conf` for better performance:

```conf
# Memory
shared_buffers = 256MB
work_mem = 4MB
maintenance_work_mem = 64MB

# Checkpoints
checkpoint_completion_target = 0.9
wal_buffers = 16MB

# Query planner
random_page_cost = 1.1  # For SSD storage
effective_cache_size = 1GB
```

#### Database Indexes

The migration automatically creates these indexes:
```sql
CREATE INDEX idx_asset_timestamp ON price_feeds (asset_address, timestamp);
```

For high-volume production use, consider additional indexes:
```sql
-- Index for user lookups
CREATE INDEX idx_user_positions_health_factor ON user_positions (health_factor);
CREATE INDEX idx_user_positions_at_risk ON user_positions (is_at_risk);

-- Index for liquidation events
CREATE INDEX idx_liquidation_events_timestamp ON liquidation_events (timestamp);
CREATE INDEX idx_liquidation_events_user ON liquidation_events (user_address);

-- Index for monitoring events
CREATE INDEX idx_monitoring_events_timestamp ON monitoring_events (timestamp);
CREATE INDEX idx_monitoring_events_type ON monitoring_events (event_type);
```

## 🚨 Troubleshooting

### Common Issues

#### "Connection refused"
```bash
# Check if PostgreSQL is running
sudo systemctl status postgresql

# Check if the port is open
netstat -ln | grep 5432

# Check authentication
psql -h localhost -U liquidation_user -d liquidation_bot
```

#### "Database does not exist"
```bash
# Create the database
sudo -u postgres createdb liquidation_bot
```

#### "Permission denied"
```bash
# Grant permissions
sudo -u postgres psql
GRANT ALL PRIVILEGES ON DATABASE liquidation_bot TO liquidation_user;
```

#### "SSL connection required"
Add `?sslmode=disable` for local development (not recommended for production):
```bash
DATABASE_URL=postgresql://username:password@localhost/liquidation_bot?sslmode=disable
```

### Migration Failures

#### Partial migration
The migration script is idempotent - you can run it multiple times:
```bash
cargo run --bin migrate_to_postgres
```

#### Data validation
Verify your data was migrated correctly:
```bash
# Check record counts
psql -h localhost -U liquidation_user -d liquidation_bot -c "
SELECT 
    'user_positions' as table_name, COUNT(*) as records FROM user_positions
UNION ALL
SELECT 
    'liquidation_events' as table_name, COUNT(*) as records FROM liquidation_events
UNION ALL
SELECT 
    'monitoring_events' as table_name, COUNT(*) as records FROM monitoring_events
UNION ALL
SELECT 
    'price_feeds' as table_name, COUNT(*) as records FROM price_feeds;
"
```

### Performance Issues

#### Slow queries
Enable query logging in PostgreSQL:
```conf
# In postgresql.conf
log_statement = 'all'
log_min_duration_statement = 1000  # Log queries taking > 1 second
```

#### Connection limits
Increase connection limits if needed:
```conf
# In postgresql.conf
max_connections = 200
```

## 🔄 Rolling Back

If you need to revert to SQLite:

1. Stop your bot
2. Update your `.env` file:
   ```bash
   DATABASE_URL=sqlite:liquidation_bot.db
   ```
3. Restart your bot

Your original SQLite database remains unchanged during migration.

## 📈 Production Recommendations

### Security
- ✅ Use SSL connections (`sslmode=require`)
- ✅ Use strong passwords
- ✅ Restrict network access to database
- ✅ Regular security updates
- ✅ Use connection pooling

### Monitoring
- ✅ Set up database monitoring (pgAdmin, DataDog, etc.)
- ✅ Monitor connection counts
- ✅ Track query performance
- ✅ Set up alerts for failures

### Backup
- ✅ Schedule regular backups:
  ```bash
  pg_dump -h localhost -U liquidation_user liquidation_bot > backup_$(date +%Y%m%d).sql
  ```
- ✅ Test backup restoration
- ✅ Store backups securely

### High Availability
- ✅ Consider read replicas for analytics
- ✅ Set up automated failover
- ✅ Use managed database services for production

## 🎯 Next Steps

After successful migration:

1. **Monitor Performance** - Check if queries are faster
2. **Optimize Queries** - Add indexes for frequently accessed data
3. **Scale Resources** - Adjust CPU/memory based on usage
4. **Set Up Monitoring** - Use tools like pgAdmin or Grafana
5. **Plan Backups** - Implement automated backup strategy

The migration maintains full compatibility with your existing bot code while providing the scalability and performance benefits of PostgreSQL.
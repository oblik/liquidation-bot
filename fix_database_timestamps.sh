#!/bin/bash

# Fix timestamp columns in PostgreSQL database
# This script changes TIMESTAMP columns to TIMESTAMPTZ to match the Rust DateTime<Utc> type

set -e

echo "🔧 Fixing timestamp columns in PostgreSQL database..."

# Check if DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
    echo "❌ ERROR: DATABASE_URL environment variable is not set"
    echo "Please set it to your PostgreSQL connection string, e.g.:"
    echo "export DATABASE_URL='postgresql://username:password@localhost/liquidation_bot'"
    exit 1
fi

# Check if it's a PostgreSQL URL
if [[ ! "$DATABASE_URL" =~ ^postgresql:// ]] && [[ ! "$DATABASE_URL" =~ ^postgres:// ]]; then
    echo "❌ ERROR: DATABASE_URL must be a PostgreSQL connection string"
    echo "Current DATABASE_URL: $DATABASE_URL"
    exit 1
fi

echo "📊 Connected to database: $DATABASE_URL"

# Run the SQL fix script
psql "$DATABASE_URL" << 'EOF'
-- Fix timestamp columns in PostgreSQL database
-- This script changes TIMESTAMP columns to TIMESTAMPTZ to match the Rust DateTime<Utc> type

BEGIN;

-- Fix user_positions table
ALTER TABLE user_positions 
ALTER COLUMN last_updated TYPE TIMESTAMPTZ USING last_updated AT TIME ZONE 'UTC';

-- Fix liquidation_events table (if it exists)
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'liquidation_events') THEN
        ALTER TABLE liquidation_events 
        ALTER COLUMN timestamp TYPE TIMESTAMPTZ USING timestamp AT TIME ZONE 'UTC';
    END IF;
END $$;

-- Fix monitoring_events table (if it exists)
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'monitoring_events') THEN
        ALTER TABLE monitoring_events 
        ALTER COLUMN timestamp TYPE TIMESTAMPTZ USING timestamp AT TIME ZONE 'UTC';
    END IF;
END $$;

-- Fix price_feeds table (if it exists)
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'price_feeds') THEN
        ALTER TABLE price_feeds 
        ALTER COLUMN timestamp TYPE TIMESTAMPTZ USING timestamp AT TIME ZONE 'UTC';
    END IF;
END $$;

COMMIT;

-- Verify the changes
\echo 'Checking user_positions table:'
SELECT column_name, data_type, is_nullable 
FROM information_schema.columns 
WHERE table_name = 'user_positions' AND column_name = 'last_updated';

\echo 'Checking liquidation_events table (if exists):'
SELECT column_name, data_type, is_nullable 
FROM information_schema.columns 
WHERE table_name = 'liquidation_events' AND column_name = 'timestamp';

\echo '✅ Timestamp columns have been fixed!'
EOF

echo "✅ Database timestamp columns have been successfully updated!"
echo "🚀 You can now run your liquidation bot again"
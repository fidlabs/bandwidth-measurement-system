#!/bin/bash
set -e

# ============================================================================
# Initialize Local Services
# ============================================================================
# This script inserts the local development service configurations into the
# database. It is idempotent and safe to run multiple times.
# ============================================================================

if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

SQL_FILE="scripts/sql/local_services.sql"

if [ -z "$DATABASE_URL" ]; then
    echo "Error: DATABASE_URL environment variable is not set"
    exit 1
fi

echo "Initializing local services..."
psql "$DATABASE_URL" -f "$SQL_FILE" -v ON_ERROR_STOP=1
echo "Local services initialized successfully!"

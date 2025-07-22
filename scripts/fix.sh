#!/bin/bash
set -e

echo "🔧 Auto-formatting code..."
cargo fmt
echo "✅ Code formatted"

echo ""
echo "📎 Running Clippy with auto-fix (where possible)..."
# Note: --fix requires clean working directory
if git diff --quiet && git diff --staged --quiet; then
    cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged
    echo "✅ Applied automatic Clippy fixes"
else
    echo "⚠️  Working directory not clean. Skipping auto-fix to prevent data loss."
    echo "   Commit or stash changes first, then run this script again."
    cargo clippy --all-targets --all-features
fi

echo ""
echo "🎯 Running final check..."
./scripts/check.sh

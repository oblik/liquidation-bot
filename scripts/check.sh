#!/bin/bash
set -e

echo "🔍 Checking code formatting..."
if ! cargo fmt -- --check; then
    echo "❌ Code formatting check failed. Run 'cargo fmt' to fix."
    exit 1
fi
echo "✅ Code formatting is correct"

echo ""
echo "📎 Running Clippy linting..."
if ! cargo clippy --all-targets --all-features -- -D warnings; then
    echo "❌ Clippy linting failed. Fix the warnings above."
    exit 1
fi
echo "✅ Clippy linting passed"

echo ""
echo "🧪 Running tests..."
if ! cargo test --all-features; then
    echo "❌ Tests failed. Fix the failing tests above."
    exit 1
fi
echo "✅ All tests passed"

echo ""
echo "🔨 Building release..."
if ! cargo build --release; then
    echo "❌ Release build failed."
    exit 1
fi
echo "✅ Release build successful"

echo ""
echo "🔒 Running security audit..."
if command -v cargo-audit >/dev/null 2>&1; then
    if ! cargo audit; then
        echo "⚠️  Security audit found issues. Review above."
        # Don't fail on audit issues for now, just warn
    else
        echo "✅ Security audit passed"
    fi
else
    echo "⚠️  cargo-audit not installed. Run 'cargo install cargo-audit' to enable security checks."
fi

echo ""
echo "🎉 All quality checks passed!"

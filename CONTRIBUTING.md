# Contributing to Liquidation Bot

Thank you for your interest in contributing to the Liquidation Bot project! This document outlines the development workflow, code standards, and tools we use to maintain high code quality.

## 🛠️ Development Setup

### Prerequisites

- **Rust 1.70+**: Install via [rustup](https://rustup.rs/)
- **Foundry**: For smart contract development - `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- **Git**: Version control
- **Base mainnet RPC access**: For testing and development

### Initial Setup

1. **Clone the repository**:
   ```bash
   git clone <repository-url>
   cd liquidation-bot
   ```

2. **Install Rust components**:
   ```bash
   rustup component add rustfmt clippy
   ```

3. **Build the project**:
   ```bash
   cargo build
   ```

4. **Run tests**:
   ```bash
   cargo test
   ```

## 📏 Code Quality Standards

We maintain high code quality through automated linting and formatting tools. All contributions must pass these checks.

### Formatting with rustfmt

We use `rustfmt` to ensure consistent code formatting across the project.

**Configuration**: See `rustfmt.toml` for our formatting rules.

**Commands**:
```bash
# Check if code is properly formatted
cargo fmt -- --check

# Auto-format your code
cargo fmt

# Format specific file
rustfmt src/main.rs
```

**Key formatting rules**:
- Maximum line length: 100 characters
- Use trailing commas in vertical layouts
- Group imports by: std, external crates, internal modules
- Use consistent brace styles and indentation

### Linting with Clippy

We use `clippy` for static analysis and idiomatic Rust suggestions.

**Configuration**: See `clippy.toml` for our linting rules.

**Commands**:
```bash
# Run clippy on all targets
cargo clippy --all-targets --all-features

# Run clippy with extra strictness
cargo clippy --all-targets --all-features -- -D warnings

# Run clippy on tests only
cargo clippy --tests --all-features
```

**Key linting rules**:
- **Financial Safety**: Warnings for float arithmetic, cast precision loss
- **Security**: Deny `panic!`, `unimplemented!`, `unreachable!`
- **Performance**: Warn on inefficient string operations, unnecessary clones
- **Documentation**: Require error and panic documentation for public APIs

### Pre-commit Checks

Before committing, ensure your code passes all checks:

```bash
# Run the full check suite
./scripts/check.sh

# Or run individual checks:
cargo fmt -- --check          # Formatting
cargo clippy -- -D warnings   # Linting  
cargo test                     # Tests
cargo build --release         # Build
```

## 🔄 Development Workflow

### Branch Strategy

- **`main`**: Production-ready code
- **`develop`**: Integration branch for features
- **`feature/*`**: Individual feature branches
- **`hotfix/*`**: Critical bug fixes

### Pull Request Process

1. **Create a feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following the code standards above

3. **Test thoroughly**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

4. **Commit with clear messages**:
   ```bash
   git commit -m "feat: add profitable liquidation detection
   
   - Implement profit calculation including gas fees
   - Add slippage protection for DEX swaps
   - Include comprehensive unit tests"
   ```

5. **Push and create PR**:
   ```bash
   git push origin feature/your-feature-name
   ```

6. **Ensure CI passes**: All GitHub Actions checks must pass

## 🧪 Testing Guidelines

### Test Commands

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_profitable_liquidation

# Run with all features
cargo test --all-features
```

## 📊 Continuous Integration

Our CI pipeline runs on every push and pull request and includes:

1. **Lint and Format**: Code formatting and Clippy linting
2. **Test**: All unit and integration tests
3. **Build**: Debug and release builds
4. **Security**: Dependency vulnerability scanning

All jobs must pass for pull requests to be merged.

## 🔧 Quick Commands

```bash
# Format code
cargo fmt

# Run linting
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test

# Build release
cargo build --release

# Security audit
cargo audit
```

For detailed development guidelines, testing strategies, and security considerations, see the full project documentation.

Thank you for contributing! 🚀

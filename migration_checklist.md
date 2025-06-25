# Base Mainnet Migration Checklist

## Current Status: ⚠️ PARTIALLY COMPLETE

Your liquidation bot migration from Base Sepolia to mainnet is **incomplete**. Here's what needs to be finished:

## ❌ CRITICAL MISSING STEPS

### 1. Contract Deployment Missing
- **Issue**: No `deployments/base.json` file exists
- **Evidence**: Only `deployments/base-sepolia.json` found
- **Fix**: Deploy contract to mainnet:
  ```bash
  npm run deploy --network base
  ```

### 2. Environment Configuration Missing  
- **Issue**: No `.env` file exists
- **Fix**: Create environment file:
  ```bash
  cp .env.mainnet.example .env
  # Edit .env and add your private key (without 0x prefix)
  ```

### 3. Rust Code Still References Base Sepolia
Files that need updating:

#### `src/liquidation/assets.rs`
- Function name: `init_base_sepolia_assets()` → `init_base_mainnet_assets()`
- Comments reference "Base Sepolia testnet"
- DAI asset is disabled with "not available on Base Sepolia" comments

#### `src/bot.rs` (Line 93)
- Still calls `liquidation::init_base_sepolia_assets()`

#### `src/liquidation/mod.rs` (Line 5)
- Exports `init_base_sepolia_assets`

#### `src/liquidation/opportunity.rs` (Line 145)
- Calls `assets::init_base_sepolia_assets()`

#### `src/main.rs` (Line 39)
- Contains Sepolia-specific WebSocket check

#### `src/monitoring/oracle.rs` (Lines 17, 31, 44, 80, 86, 87)
- Multiple "Base Sepolia testnet" references
- Sepolia-specific logic

#### `src/monitoring/scanner.rs` (Line 191)
- "Base Sepolia WETH address" comment

#### `src/liquidation/executor.rs` (Line 202)
- "Base Sepolia pool" verification comment

## ✅ ALREADY COMPLETED

1. ✅ Migration scripts (`migrate-to-mainnet.sh`)
2. ✅ Environment template (`.env.mainnet.example`)
3. ✅ Migration documentation
4. ✅ Hardhat mainnet configuration
5. ✅ Deploy script with mainnet addresses

## 🚀 QUICK FIX STEPS

### Step 1: Deploy Contract
```bash
# Make sure you have a .env file with your private key
npm run deploy --network base
```

### Step 2: Update Environment
```bash
# Copy template and edit with your private key
cp .env.mainnet.example .env
# Edit PRIVATE_KEY in .env
# Update LIQUIDATOR_CONTRACT with deployed address from Step 1
```

### Step 3: Update Rust Code
- Rename `init_base_sepolia_assets()` to `init_base_mainnet_assets()`
- Update all function calls and references
- Update comments from "Base Sepolia" to "Base Mainnet"
- Enable DAI asset (available on mainnet)

### Step 4: Test
```bash
cargo build --release
cargo run --bin liquidation-bot
```

## 🔍 VERIFICATION CHECKLIST

- [ ] `deployments/base.json` exists with mainnet contract address
- [ ] `.env` file exists with valid PRIVATE_KEY and LIQUIDATOR_CONTRACT
- [ ] No grep results for "sepolia" in Rust source files
- [ ] Bot starts successfully without Sepolia references
- [ ] WebSocket connects to `wss://mainnet.base.org`

## ⚡ ASSET CONFIGURATION UPDATES NEEDED

The mainnet should enable all assets including DAI:

**Base Mainnet Assets:**
- WETH: `0x4200000000000000000000000000000000000006`
- USDC: `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`
- cbETH: `0x2Ae3F1Ec7F1F5012CFEab0185bfc7aa3cf0DEc22`  
- DAI: `0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb` (enable on mainnet)

**Aave V3 Base Mainnet:**
- Pool: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
- AddressesProvider: `0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e`
- SwapRouter: `0x2626664c2603336E57B271c5C0b26F421741e481`

## 🛡️ SECURITY NOTES

- Mainnet uses real ETH - start with small amounts
- Monitor gas costs (higher than testnet)
- Set appropriate profit thresholds for mainnet
- Keep private keys secure and never commit to git

---

**Next Action:** Run the deployment command and update the Rust code references.
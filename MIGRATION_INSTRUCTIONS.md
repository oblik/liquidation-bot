# Migration from Base Sepolia to Base Mainnet

## Overview
This guide will help you migrate your liquidation bot from Base Sepolia testnet to Base mainnet to access higher liquidity and real liquidation opportunities.

## Current Status
- **Sepolia Contract**: `0x501B7f2E7162B59F11E986025202aAaA9d80a1AE` 
- **Funded Wallet**: `0x502B5e67464f1D4506FFC71b6c7762312474EA30`
- **Target Network**: Base Mainnet (Chain ID: 8453)

## Prerequisites
1. **Funded Base Mainnet Wallet**: Your wallet `0x502B5e67464f1D4506FFC71b6c7762312474EA30` should have ETH for gas fees
2. **Private Key Access**: You'll need the private key for deployment
3. **MetaMask/Wallet Access**: To export your private key securely

## Step 1: Get Your Private Key
**⚠️ SECURITY WARNING**: Never share your private key with anyone!

### From MetaMask:
1. Open MetaMask and select your funded account
2. Click the three dots next to your account name
3. Select "Account Details" → "Show Private Key"
4. Enter your wallet password
5. Copy the private key (without the 0x prefix)

## Step 2: Set Up Environment
1. Copy the environment template:
   ```bash
   cp .env.mainnet.example .env
   ```

2. Edit `.env` file and replace `YOUR_PRIVATE_KEY_HERE_WITHOUT_0x_PREFIX` with your actual private key

## Step 3: Deploy to Base Mainnet
Run the deployment command:
```bash
npm run deploy
```

This will:
- Deploy the AaveLiquidator contract to Base mainnet
- Use the correct Aave V3 addresses for Base:
  - Pool: `0xA238Dd80C259a72e81d7e4664a9801593F98d1c5`
  - AddressesProvider: `0x2f39d218133AFaB8F2B819B1066c7E434Ad94E9e`
  - SwapRouter: `0x2626664c2603336E57B271c5C0b26F421741e481`

## Step 4: Update Bot Configuration
After deployment, update your `.env` file with the new contract address that will be printed in the deployment output.

## Step 5: Start the Bot
```bash
# Build the Rust bot
cargo build --release

# Run the bot
cargo run --bin liquidation-bot
```

## Network Information
- **Network**: Base Mainnet
- **Chain ID**: 8453
- **RPC URL**: https://mainnet.base.org
- **Block Explorer**: https://basescan.org/
- **Currency**: ETH

## Expected Benefits
1. **Higher Liquidity**: Access to real DeFi protocols with significant TVL
2. **Real Liquidations**: Actual liquidation opportunities vs limited testnet activity
3. **Revenue Generation**: Earn real profits from successful liquidations
4. **Better Testing**: Test with real market conditions and user behavior

## Troubleshooting

### Deployment Issues
- Ensure your wallet has sufficient ETH for gas fees (≥0.01 ETH recommended)
- Check that your private key is correct and doesn't include "0x" prefix
- Verify you're connected to Base mainnet (Chain ID 8453)

### Bot Issues
- Confirm the LIQUIDATOR_CONTRACT address in .env matches the deployed contract
- Check RPC_URL is set to https://mainnet.base.org
- Verify your private key has permission to call the contract

### Network Issues
- Base mainnet RPC: https://mainnet.base.org
- If RPC fails, try alternative endpoints or check Base network status

## Security Best Practices
1. **Environment Files**: Never commit .env files to version control
2. **Private Keys**: Store securely, never share publicly  
3. **Gas Limits**: Monitor gas usage on mainnet vs testnet
4. **Monitoring**: Set up alerts for bot activity and wallet balance

## Next Steps After Migration
1. Monitor bot performance for the first few hours
2. Adjust profit thresholds based on mainnet gas costs
3. Set up proper logging and monitoring
4. Consider implementing additional safety checks for mainnet operations

## Files Modified in This Migration
- `hardhat.config.js`: Already configured for Base mainnet
- `scripts/deploy.js`: Already has Base mainnet Aave addresses
- `.env`: New environment configuration for mainnet
- `src/config.rs`: Will read new environment variables

## Support
- **Base Documentation**: https://docs.base.org/
- **Aave V3 Base Docs**: https://docs.aave.com/developers/deployed-contracts/v3-mainnet/base
- **Basescan Explorer**: https://basescan.org/
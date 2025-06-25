#!/bin/bash

echo "🚀 Base Mainnet Migration Script"
echo "================================"
echo

# Check if .env exists
if [ -f ".env" ]; then
    echo "⚠️  .env file already exists. Please backup or remove it first."
    echo "   Current .env will be backed up to .env.backup.$(date +%s)"
    mv .env .env.backup.$(date +%s)
fi

# Copy template
echo "📋 Creating .env file from template..."
cp .env.mainnet.example .env

echo "✅ Environment template created!"
echo
echo "🔑 NEXT STEPS:"
echo "1. Get your private key from MetaMask (Account Details → Show Private Key)"
echo "2. Edit .env file and replace YOUR_PRIVATE_KEY_HERE_WITHOUT_0x_PREFIX"
echo "3. Run: npm run deploy"
echo "4. Update .env with the deployed contract address"
echo "5. Run: cargo run --bin liquidation-bot"
echo
echo "⚠️  SECURITY REMINDER:"
echo "   - Never share your private key"
echo "   - The .env file is in .gitignore for security"
echo "   - Keep your private key safe and secure"
echo
echo "📖 For detailed instructions, see: MIGRATION_INSTRUCTIONS.md"
echo

# Check if private key is set
echo "🔍 Checking current .env configuration..."
if grep -q "YOUR_PRIVATE_KEY_HERE_WITHOUT_0x_PREFIX" .env; then
    echo "❌ Private key not set in .env file"
    echo "   Please edit .env and add your private key before deploying"
else
    echo "✅ Private key appears to be configured"
    echo "🚀 Ready to deploy! Run: npm run deploy"
fi

echo
echo "🌐 Base Mainnet Information:"
echo "   Chain ID: 8453"
echo "   RPC URL: https://mainnet.base.org"
echo "   Explorer: https://basescan.org/"
echo "   Your Wallet: 0x502B5e67464f1D4506FFC71b6c7762312474EA30"
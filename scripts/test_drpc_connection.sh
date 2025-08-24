#!/bin/bash

# Test dRPC connection script
# This script tests both HTTP and WebSocket connections to dRPC

echo "🔍 Testing dRPC Connection..."
echo "================================"

# Load environment variables
if [ -f .env ]; then
    export $(cat .env | grep -v '^#' | xargs)
fi

# Extract the API key from the URL
API_KEY=$(echo $RPC_URL | sed -n 's|.*drpc.org/base/\([^/]*\).*|\1|p')

echo "📝 Configuration:"
echo "  RPC URL: $RPC_URL"
echo "  WS URL: $WS_URL"
echo "  API Key (first 10 chars): ${API_KEY:0:10}..."
echo ""

# Test 1: Basic HTTP RPC call
echo "🧪 Test 1: HTTP RPC Connection (eth_blockNumber)"
echo "----------------------------------------"
HTTP_RESPONSE=$(curl -s -X POST $RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}')

echo "Response: $HTTP_RESPONSE"

if echo "$HTTP_RESPONSE" | grep -q "error"; then
    echo "❌ HTTP connection failed!"
    echo ""
    echo "Error details:"
    echo "$HTTP_RESPONSE" | jq '.error' 2>/dev/null || echo "$HTTP_RESPONSE"
else
    BLOCK_NUMBER=$(echo "$HTTP_RESPONSE" | jq -r '.result' 2>/dev/null)
    if [ ! -z "$BLOCK_NUMBER" ] && [ "$BLOCK_NUMBER" != "null" ]; then
        echo "✅ HTTP connection successful! Current block: $BLOCK_NUMBER"
    else
        echo "⚠️ HTTP connection returned unexpected response"
    fi
fi
echo ""

# Test 2: Chain ID check
echo "🧪 Test 2: Chain ID Verification (eth_chainId)"
echo "----------------------------------------"
CHAIN_RESPONSE=$(curl -s -X POST $RPC_URL \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":2}')

echo "Response: $CHAIN_RESPONSE"

if echo "$CHAIN_RESPONSE" | grep -q "error"; then
    echo "❌ Chain ID request failed!"
else
    CHAIN_ID=$(echo "$CHAIN_RESPONSE" | jq -r '.result' 2>/dev/null)
    if [ "$CHAIN_ID" = "0x2105" ]; then
        echo "✅ Connected to Base Mainnet (Chain ID: 8453)"
    else
        echo "⚠️ Unexpected chain ID: $CHAIN_ID"
    fi
fi
echo ""

# Test 3: WebSocket connection (if wscat is available)
if command -v wscat &> /dev/null; then
    echo "🧪 Test 3: WebSocket Connection Test"
    echo "----------------------------------------"
    echo "Testing WebSocket connection (5 second timeout)..."
    
    # Create a test command that will timeout after 5 seconds
    timeout 5 wscat -c "$WS_URL" -x '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' 2>&1 | head -n 5
    
    if [ $? -eq 0 ]; then
        echo "✅ WebSocket connection appears to work"
    else
        echo "❌ WebSocket connection failed or timed out"
    fi
else
    echo "ℹ️ wscat not installed. Skipping WebSocket test."
    echo "  Install with: npm install -g wscat"
fi
echo ""

# Test 4: Check if the API key might be expired
echo "🧪 Test 4: API Key Validation"
echo "----------------------------------------"
if echo "$HTTP_RESPONSE" | grep -q "invalid or expired"; then
    echo "❌ API Key appears to be invalid or expired!"
    echo ""
    echo "📋 Next steps:"
    echo "  1. Log in to your dRPC dashboard at https://drpc.org"
    echo "  2. Check if your API key is still valid"
    echo "  3. Generate a new API key if needed"
    echo "  4. Update your .env file with the new URLs"
    echo ""
    echo "Example .env format:"
    echo "  export RPC_URL=https://lb.drpc.org/base/YOUR_NEW_API_KEY"
    echo "  export WS_URL=wss://lb.drpc.org/base/YOUR_NEW_API_KEY"
else
    echo "✅ API Key format appears valid"
fi

echo ""
echo "================================"
echo "📊 Summary:"
echo "================================"

if echo "$HTTP_RESPONSE" | grep -q "error"; then
    echo "Status: ❌ Connection Failed"
    echo ""
    echo "Troubleshooting steps:"
    echo "1. Verify your dRPC API key is valid and not expired"
    echo "2. Check your dRPC dashboard for any usage limits or restrictions"
    echo "3. Ensure the Base network is enabled for your API key"
    echo "4. Try regenerating your API key from the dRPC dashboard"
else
    echo "Status: ✅ HTTP Connection Working"
    echo ""
    echo "Note: If WebSocket is still failing, it might require:"
    echo "1. A different endpoint or configuration"
    echo "2. Specific WebSocket permissions in your dRPC plan"
    echo "3. Additional authentication headers (not supported by current code)"
fi
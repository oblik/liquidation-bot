import dotenv from 'dotenv';
import { ethers } from 'ethers';

dotenv.config();

// Network configurations
const NETWORK_CONFIGS = {
  mainnet: {
    name: 'Ethereum Mainnet',
    chainId: 1,
    poolAddress: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', // Aave V3 Pool
    rpcUrl: process.env.MAINNET_RPC_URL || 'https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY'
  },
  arbitrum: {
    name: 'Arbitrum One',
    chainId: 42161,
    poolAddress: '0x794a61358D6845594F94dc1DB02A252b5b4814aD', // Aave V3 Pool
    rpcUrl: process.env.ARBITRUM_RPC_URL || 'https://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY'
  },
  optimism: {
    name: 'Optimism',
    chainId: 10,
    poolAddress: '0x794a61358D6845594F94dc1DB02A252b5b4814aD', // Aave V3 Pool
    rpcUrl: process.env.OPTIMISM_RPC_URL || 'https://opt-mainnet.g.alchemy.com/v2/YOUR_API_KEY'
  },
  polygon: {
    name: 'Polygon',
    chainId: 137,
    poolAddress: '0x794a61358D6845594F94dc1DB02A252b5b4814aD', // Aave V3 Pool
    rpcUrl: process.env.POLYGON_RPC_URL || 'https://polygon-mainnet.g.alchemy.com/v2/YOUR_API_KEY'
  },
  base: {
    name: 'Base',
    chainId: 8453,
    poolAddress: '0xA238Dd80C259a72e81d7e4664a9801593F98d1c5', // Aave V3 Pool on Base
    rpcUrl: process.env.BASE_RPC_URL || 'https://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY'
  },
  avalanche: {
    name: 'Avalanche',
    chainId: 43114,
    poolAddress: '0x794a61358D6845594F94dc1DB02A252b5b4814aD', // Aave V3 Pool
    rpcUrl: process.env.AVALANCHE_RPC_URL || 'https://api.avax.network/ext/bc/C/rpc'
  },
  // Testnets
  sepolia: {
    name: 'Sepolia Testnet',
    chainId: 11155111,
    poolAddress: '0x6Ae43d3271ff6888e7Fc43Fd7321a503ff738951', // Aave V3 Pool on Sepolia
    rpcUrl: process.env.SEPOLIA_RPC_URL || 'https://eth-sepolia.g.alchemy.com/v2/YOUR_API_KEY'
  },
  goerli: {
    name: 'Goerli Testnet',
    chainId: 5,
    poolAddress: '0x7b5C526B7F8dfdff278b4a3e045083FBA4028790', // Aave V3 Pool on Goerli
    rpcUrl: process.env.GOERLI_RPC_URL || 'https://eth-goerli.g.alchemy.com/v2/YOUR_API_KEY'
  }
};

/**
 * Load configuration from environment variables
 */
export async function loadConfig() {
  const network = process.env.NETWORK || 'mainnet';
  const networkConfig = NETWORK_CONFIGS[network.toLowerCase()];
  
  if (!networkConfig) {
    throw new Error(`Unsupported network: ${network}. Supported networks: ${Object.keys(NETWORK_CONFIGS).join(', ')}`);
  }
  
  // Override with custom values if provided
  const config = {
    network: networkConfig.name,
    chainId: networkConfig.chainId,
    poolAddress: process.env.POOL_ADDRESS || networkConfig.poolAddress,
    rpcUrl: process.env.RPC_URL || networkConfig.rpcUrl,
    
    // Monitoring settings
    queryHistorical: process.env.QUERY_HISTORICAL === 'true',
    historicalBlocks: parseInt(process.env.HISTORICAL_BLOCKS || '1000'),
    statsInterval: parseInt(process.env.STATS_INTERVAL || '60000'), // milliseconds
    
    // Logging settings
    logLevel: process.env.LOG_LEVEL || 'info',
    logToFile: process.env.LOG_TO_FILE !== 'false', // Default true
    logDir: process.env.LOG_DIR || './logs',
    
    // Optional filters
    watchedUsers: process.env.WATCHED_USERS ? process.env.WATCHED_USERS.split(',') : [],
    watchedAssets: process.env.WATCHED_ASSETS ? process.env.WATCHED_ASSETS.split(',') : [],
    minDebtThreshold: process.env.MIN_DEBT_THRESHOLD ? ethers.parseEther(process.env.MIN_DEBT_THRESHOLD) : 0n
  };
  
  // Validate RPC URL
  if (config.rpcUrl.includes('YOUR_API_KEY')) {
    console.warn('⚠️  Warning: Using default RPC URL. Please set your RPC URL in the .env file for better performance.');
  }
  
  return config;
}

/**
 * Get token symbol from address (you can expand this mapping)
 */
export const TOKEN_SYMBOLS = {
  // Mainnet tokens
  '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2': 'WETH',
  '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48': 'USDC',
  '0xdAC17F958D2ee523a2206206994597C13D831ec7': 'USDT',
  '0x6B175474E89094C44Da98b954EedeAC495271d0F': 'DAI',
  '0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599': 'WBTC',
  '0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9': 'AAVE',
  
  // Base tokens
  '0x4200000000000000000000000000000000000006': 'WETH',
  '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913': 'USDC',
  '0x50c5725949A6F0c72E6C4a641F24049A917DB0Cb': 'DAI',
  
  // Add more token mappings as needed
};

export default loadConfig;
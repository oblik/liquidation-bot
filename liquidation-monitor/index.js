import { ethers } from 'ethers';
import dotenv from 'dotenv';
import chalk from 'chalk';
import { createLogger } from './utils/logger.js';
import { formatLiquidationEvent, formatEther, formatAddress } from './utils/formatters.js';
import { loadConfig } from './config/config.js';
import L2PoolABI from './abi/L2Pool.json' assert { type: 'json' };

// Load environment variables
dotenv.config();

// Initialize logger
const logger = createLogger();

// Statistics tracking
const stats = {
  totalLiquidations: 0,
  totalDebtCovered: 0n,
  totalCollateralLiquidated: 0n,
  liquidationsByAsset: new Map(),
  liquidationsByUser: new Map(),
  startTime: Date.now()
};

/**
 * Main monitoring function
 */
async function startMonitoring() {
  try {
    // Load configuration
    const config = await loadConfig();
    
    logger.info(chalk.cyan('🚀 Starting Aave V3 Liquidation Monitor'));
    logger.info(chalk.gray(`Network: ${config.network}`));
    logger.info(chalk.gray(`Pool Address: ${config.poolAddress}`));
    logger.info(chalk.gray(`RPC URL: ${config.rpcUrl.substring(0, 30)}...`));
    
    // Initialize provider
    const provider = new ethers.JsonRpcProvider(config.rpcUrl);
    
    // Verify connection
    const network = await provider.getNetwork();
    logger.info(chalk.green(`✅ Connected to network: ${network.name} (chainId: ${network.chainId})`));
    
    // Get current block
    const currentBlock = await provider.getBlockNumber();
    logger.info(chalk.gray(`Current block: ${currentBlock}`));
    
    // Initialize contract
    const poolContract = new ethers.Contract(
      config.poolAddress,
      L2PoolABI,
      provider
    );
    
    // Define event filter for LiquidationCall
    const liquidationFilter = poolContract.filters.LiquidationCall();
    
    // Handle liquidation events
    const handleLiquidation = async (
      collateralAsset,
      debtAsset,
      user,
      debtToCover,
      liquidatedCollateralAmount,
      liquidator,
      receiveAToken,
      event
    ) => {
      try {
        // Update statistics
        stats.totalLiquidations++;
        stats.totalDebtCovered += debtToCover;
        stats.totalCollateralLiquidated += liquidatedCollateralAmount;
        
        // Track by asset
        const assetKey = `${collateralAsset}-${debtAsset}`;
        stats.liquidationsByAsset.set(
          assetKey,
          (stats.liquidationsByAsset.get(assetKey) || 0) + 1
        );
        
        // Track by user
        stats.liquidationsByUser.set(
          user,
          (stats.liquidationsByUser.get(user) || 0) + 1
        );
        
        // Get block timestamp
        const block = await event.getBlock();
        const timestamp = new Date(block.timestamp * 1000);
        
        // Get transaction details
        const tx = await event.getTransaction();
        const receipt = await event.getTransactionReceipt();
        
        // Calculate gas cost
        const gasCost = receipt.gasUsed * receipt.gasPrice;
        
        // Prepare liquidation data
        const liquidationData = {
          eventNumber: stats.totalLiquidations,
          timestamp: timestamp.toISOString(),
          blockNumber: block.number,
          transactionHash: tx.hash,
          user: user,
          liquidator: liquidator,
          collateralAsset: collateralAsset,
          debtAsset: debtAsset,
          debtToCover: formatEther(debtToCover),
          liquidatedCollateralAmount: formatEther(liquidatedCollateralAmount),
          receiveAToken: receiveAToken,
          gasUsed: receipt.gasUsed.toString(),
          gasPrice: formatEther(receipt.gasPrice, 9), // Gwei
          gasCost: formatEther(gasCost),
          logIndex: event.index
        };
        
        // Log formatted event
        logger.info(chalk.yellow('⚡ LIQUIDATION DETECTED'));
        logger.info(formatLiquidationEvent(liquidationData));
        
        // Log to file with full details
        logger.info('Liquidation Event Details:', liquidationData);
        
        // Display running statistics
        displayStats();
        
      } catch (error) {
        logger.error('Error processing liquidation event:', error);
      }
    };
    
    // Listen for liquidation events
    logger.info(chalk.cyan('👂 Listening for liquidation events...'));
    poolContract.on(liquidationFilter, handleLiquidation);
    
    // Also query historical events if configured
    if (config.queryHistorical) {
      logger.info(chalk.cyan('📜 Querying historical events...'));
      const fromBlock = currentBlock - (config.historicalBlocks || 1000);
      const historicalEvents = await poolContract.queryFilter(
        liquidationFilter,
        fromBlock,
        currentBlock
      );
      
      logger.info(chalk.gray(`Found ${historicalEvents.length} historical liquidation events`));
      
      // Process historical events
      for (const event of historicalEvents) {
        await handleLiquidation(
          ...event.args,
          event
        );
      }
    }
    
    // Set up periodic statistics display
    setInterval(displayStats, config.statsInterval || 60000); // Every minute by default
    
    // Handle graceful shutdown
    process.on('SIGINT', () => {
      logger.info(chalk.red('\n📊 Final Statistics:'));
      displayStats();
      logger.info(chalk.red('👋 Shutting down liquidation monitor...'));
      process.exit(0);
    });
    
  } catch (error) {
    logger.error(chalk.red('❌ Error starting monitor:'), error);
    process.exit(1);
  }
}

/**
 * Display current statistics
 */
function displayStats() {
  const runtime = Date.now() - stats.startTime;
  const hours = Math.floor(runtime / 3600000);
  const minutes = Math.floor((runtime % 3600000) / 60000);
  
  console.log(chalk.cyan('\n📊 Liquidation Statistics:'));
  console.log(chalk.gray('─'.repeat(50)));
  console.log(chalk.white(`Total Liquidations: ${stats.totalLiquidations}`));
  console.log(chalk.white(`Total Debt Covered: ${formatEther(stats.totalDebtCovered)} ETH`));
  console.log(chalk.white(`Total Collateral Liquidated: ${formatEther(stats.totalCollateralLiquidated)} ETH`));
  console.log(chalk.white(`Unique Users Liquidated: ${stats.liquidationsByUser.size}`));
  console.log(chalk.white(`Unique Asset Pairs: ${stats.liquidationsByAsset.size}`));
  console.log(chalk.white(`Runtime: ${hours}h ${minutes}m`));
  
  if (stats.totalLiquidations > 0) {
    const avgDebt = stats.totalDebtCovered / BigInt(stats.totalLiquidations);
    const avgCollateral = stats.totalCollateralLiquidated / BigInt(stats.totalLiquidations);
    console.log(chalk.white(`Average Debt per Liquidation: ${formatEther(avgDebt)} ETH`));
    console.log(chalk.white(`Average Collateral per Liquidation: ${formatEther(avgCollateral)} ETH`));
    
    // Show top liquidated asset pairs
    if (stats.liquidationsByAsset.size > 0) {
      console.log(chalk.yellow('\nTop Liquidated Asset Pairs:'));
      const sortedPairs = Array.from(stats.liquidationsByAsset.entries())
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5);
      
      for (const [pair, count] of sortedPairs) {
        const [collateral, debt] = pair.split('-');
        console.log(chalk.gray(`  ${formatAddress(collateral)} -> ${formatAddress(debt)}: ${count} liquidations`));
      }
    }
  }
  
  console.log(chalk.gray('─'.repeat(50)));
}

// Start the monitor
startMonitoring().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
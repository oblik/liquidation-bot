import { ethers } from 'ethers';
import chalk from 'chalk';
import { TOKEN_SYMBOLS } from '../config/config.js';

/**
 * Format wei to ether with specified decimals
 */
export function formatEther(value, decimals = 18) {
  try {
    if (typeof value === 'bigint') {
      return ethers.formatUnits(value, decimals);
    }
    return ethers.formatUnits(value.toString(), decimals);
  } catch (error) {
    return '0.0';
  }
}

/**
 * Format address with optional ENS lookup
 */
export function formatAddress(address, short = true) {
  if (!address) return 'Unknown';
  
  // Check if we have a known token symbol
  const symbol = TOKEN_SYMBOLS[address];
  if (symbol) {
    return `${symbol} (${short ? shortenAddress(address) : address})`;
  }
  
  return short ? shortenAddress(address) : address;
}

/**
 * Shorten an address for display
 */
export function shortenAddress(address) {
  if (!address) return '';
  return `${address.substring(0, 6)}...${address.substring(address.length - 4)}`;
}

/**
 * Format a liquidation event for console display
 */
export function formatLiquidationEvent(event) {
  const lines = [];
  
  lines.push(chalk.gray('─'.repeat(60)));
  lines.push(chalk.white(`📅 Time: ${event.timestamp}`));
  lines.push(chalk.white(`📦 Block: ${event.blockNumber}`));
  lines.push(chalk.white(`🔗 Tx: ${shortenAddress(event.transactionHash)}`));
  lines.push('');
  
  lines.push(chalk.cyan('👤 User (Liquidated): ') + chalk.red(formatAddress(event.user)));
  lines.push(chalk.cyan('💰 Liquidator: ') + chalk.green(formatAddress(event.liquidator)));
  lines.push('');
  
  lines.push(chalk.cyan('📊 Assets:'));
  lines.push(`  ${chalk.yellow('Collateral:')} ${formatAddress(event.collateralAsset)}`);
  lines.push(`  ${chalk.yellow('Debt:')} ${formatAddress(event.debtAsset)}`);
  lines.push('');
  
  lines.push(chalk.cyan('💵 Amounts:'));
  lines.push(`  ${chalk.yellow('Debt Covered:')} ${event.debtToCover} ETH`);
  lines.push(`  ${chalk.yellow('Collateral Liquidated:')} ${event.liquidatedCollateralAmount} ETH`);
  
  // Calculate liquidation bonus (approximate)
  try {
    const debtValue = parseFloat(event.debtToCover);
    const collateralValue = parseFloat(event.liquidatedCollateralAmount);
    if (debtValue > 0) {
      const bonus = ((collateralValue - debtValue) / debtValue * 100).toFixed(2);
      lines.push(`  ${chalk.yellow('Liquidation Bonus:')} ~${bonus}%`);
    }
  } catch (error) {
    // Ignore calculation errors
  }
  
  lines.push('');
  lines.push(chalk.cyan('⛽ Gas:'));
  lines.push(`  ${chalk.yellow('Gas Used:')} ${event.gasUsed}`);
  lines.push(`  ${chalk.yellow('Gas Price:')} ${event.gasPrice} Gwei`);
  lines.push(`  ${chalk.yellow('Gas Cost:')} ${event.gasCost} ETH`);
  
  lines.push('');
  lines.push(`${chalk.cyan('🔄 Receive aToken:')} ${event.receiveAToken ? chalk.green('Yes') : chalk.red('No')}`);
  lines.push(chalk.gray('─'.repeat(60)));
  
  return lines.join('\n');
}

/**
 * Format large numbers with commas
 */
export function formatNumber(num) {
  return num.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/**
 * Format percentage
 */
export function formatPercentage(value, decimals = 2) {
  return `${(value * 100).toFixed(decimals)}%`;
}

/**
 * Format time duration
 */
export function formatDuration(milliseconds) {
  const seconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  
  if (days > 0) {
    return `${days}d ${hours % 24}h ${minutes % 60}m`;
  } else if (hours > 0) {
    return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
  } else if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  } else {
    return `${seconds}s`;
  }
}

/**
 * Format USD value
 */
export function formatUSD(value) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(value);
}

/**
 * Parse and validate Ethereum address
 */
export function isValidAddress(address) {
  try {
    ethers.getAddress(address);
    return true;
  } catch {
    return false;
  }
}

/**
 * Get block explorer URL for transaction
 */
export function getExplorerUrl(txHash, chainId) {
  const explorers = {
    1: 'https://etherscan.io/tx/',
    10: 'https://optimistic.etherscan.io/tx/',
    137: 'https://polygonscan.com/tx/',
    42161: 'https://arbiscan.io/tx/',
    8453: 'https://basescan.org/tx/',
    43114: 'https://snowtrace.io/tx/',
    11155111: 'https://sepolia.etherscan.io/tx/',
    5: 'https://goerli.etherscan.io/tx/'
  };
  
  const baseUrl = explorers[chainId] || 'https://etherscan.io/tx/';
  return `${baseUrl}${txHash}`;
}
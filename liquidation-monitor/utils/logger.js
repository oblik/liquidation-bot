import winston from 'winston';
import DailyRotateFile from 'winston-daily-rotate-file';
import chalk from 'chalk';
import path from 'path';
import { fileURLToPath } from 'url';
import { dirname } from 'path';
import fs from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Create a Winston logger instance with console and file transports
 */
export function createLogger() {
  const logDir = process.env.LOG_DIR || path.join(dirname(__dirname), 'logs');
  
  // Ensure log directory exists
  if (!fs.existsSync(logDir)) {
    fs.mkdirSync(logDir, { recursive: true });
  }
  
  // Custom format for console output
  const consoleFormat = winston.format.printf(({ level, message, timestamp, ...metadata }) => {
    let msg = `${timestamp} [${level}]: ${message}`;
    
    if (Object.keys(metadata).length > 0) {
      msg += ` ${JSON.stringify(metadata, null, 2)}`;
    }
    
    return msg;
  });
  
  // Create transports array
  const transports = [
    // Console transport with colors
    new winston.transports.Console({
      format: winston.format.combine(
        winston.format.colorize(),
        winston.format.timestamp({ format: 'YYYY-MM-DD HH:mm:ss' }),
        consoleFormat
      )
    })
  ];
  
  // Add file transports if enabled
  if (process.env.LOG_TO_FILE !== 'false') {
    // Daily rotating file for all logs
    transports.push(
      new DailyRotateFile({
        filename: path.join(logDir, 'liquidations-%DATE%.log'),
        datePattern: 'YYYY-MM-DD',
        zippedArchive: true,
        maxSize: '20m',
        maxFiles: '14d',
        format: winston.format.combine(
          winston.format.timestamp(),
          winston.format.json()
        )
      })
    );
    
    // Separate file for liquidation events only
    transports.push(
      new DailyRotateFile({
        filename: path.join(logDir, 'events-%DATE%.log'),
        datePattern: 'YYYY-MM-DD',
        zippedArchive: true,
        maxSize: '20m',
        maxFiles: '30d',
        level: 'info',
        format: winston.format.combine(
          winston.format.timestamp(),
          winston.format.json()
        ),
        filter: (info) => info.message && info.message.includes('LIQUIDATION')
      })
    );
    
    // Error log file
    transports.push(
      new winston.transports.File({
        filename: path.join(logDir, 'errors.log'),
        level: 'error',
        format: winston.format.combine(
          winston.format.timestamp(),
          winston.format.json()
        )
      })
    );
  }
  
  // Create logger
  const logger = winston.createLogger({
    level: process.env.LOG_LEVEL || 'info',
    transports: transports,
    exitOnError: false
  });
  
  // Add method to log liquidation events specifically
  logger.liquidation = function(data) {
    this.info('LIQUIDATION_EVENT', data);
  };
  
  return logger;
}

/**
 * Format log message with color based on severity
 */
export function formatLogMessage(level, message) {
  const colors = {
    error: chalk.red,
    warn: chalk.yellow,
    info: chalk.blue,
    debug: chalk.gray,
    success: chalk.green
  };
  
  const color = colors[level] || chalk.white;
  return color(message);
}

export default createLogger;
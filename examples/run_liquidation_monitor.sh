#!/bin/bash

# Example script to run the liquidation monitor with various configurations

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Aave Liquidation Monitor - Example Usage${NC}"
echo "========================================="

# Check if the binary is built
if [ ! -f "./target/release/liquidation-monitor" ]; then
    echo -e "${YELLOW}Building liquidation-monitor...${NC}"
    cargo build --release --bin liquidation-monitor
    if [ $? -ne 0 ]; then
        echo -e "${RED}Failed to build liquidation-monitor${NC}"
        exit 1
    fi
fi

# Function to show menu
show_menu() {
    echo ""
    echo "Select an option:"
    echo "1) Start real-time monitoring (default settings)"
    echo "2) Start monitoring with file logging"
    echo "3) Start monitoring with custom RPC"
    echo "4) Analyze historical liquidations"
    echo "5) Generate configuration file"
    echo "6) Start with verbose logging"
    echo "7) Exit"
    echo ""
}

# Main menu loop
while true; do
    show_menu
    read -p "Enter your choice [1-7]: " choice
    
    case $choice in
        1)
            echo -e "${GREEN}Starting real-time monitoring...${NC}"
            ./target/release/liquidation-monitor monitor
            ;;
        2)
            echo -e "${GREEN}Starting monitoring with file logging...${NC}"
            LOG_FILE="liquidations_$(date +%Y%m%d_%H%M%S).jsonl"
            echo -e "${YELLOW}Logging to: $LOG_FILE${NC}"
            ./target/release/liquidation-monitor --log-file "$LOG_FILE" monitor
            ;;
        3)
            read -p "Enter RPC URL: " RPC_URL
            echo -e "${GREEN}Starting monitoring with custom RPC...${NC}"
            ./target/release/liquidation-monitor --rpc-url "$RPC_URL" monitor
            ;;
        4)
            read -p "Enter starting block number: " FROM_BLOCK
            read -p "Enter ending block number (or press Enter for latest): " TO_BLOCK
            echo -e "${GREEN}Analyzing historical liquidations...${NC}"
            if [ -z "$TO_BLOCK" ]; then
                ./target/release/liquidation-monitor historical --from-block "$FROM_BLOCK"
            else
                ./target/release/liquidation-monitor historical --from-block "$FROM_BLOCK" --to-block "$TO_BLOCK"
            fi
            ;;
        5)
            echo -e "${GREEN}Generating configuration file...${NC}"
            ./target/release/liquidation-monitor generate-config liquidation-config.json
            echo -e "${YELLOW}Configuration saved to: liquidation-config.json${NC}"
            ;;
        6)
            echo -e "${GREEN}Starting with verbose logging...${NC}"
            ./target/release/liquidation-monitor --verbose monitor
            ;;
        7)
            echo -e "${GREEN}Exiting...${NC}"
            exit 0
            ;;
        *)
            echo -e "${RED}Invalid option. Please try again.${NC}"
            ;;
    esac
    
    # Add a pause before showing menu again (except for exit)
    if [ "$choice" != "7" ]; then
        echo ""
        read -p "Press Enter to continue..."
    fi
done
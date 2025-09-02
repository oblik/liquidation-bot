#!/bin/bash

# Script to format JSONL files for better readability

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <jsonl_file>"
    echo "Example: $0 liquidations.jsonl"
    exit 1
fi

INPUT_FILE="$1"
OUTPUT_FILE="${INPUT_FILE%.jsonl}_formatted.json"

echo "Formatting $INPUT_FILE to $OUTPUT_FILE..."

# Convert JSONL to formatted JSON array
echo "[" > "$OUTPUT_FILE"
first=true
while IFS= read -r line; do
    if [ "$first" = true ]; then
        first=false
    else
        echo "," >> "$OUTPUT_FILE"
    fi
    echo "$line" | jq '.' >> "$OUTPUT_FILE"
done < "$INPUT_FILE"
echo "]" >> "$OUTPUT_FILE"

echo "Done! Formatted output saved to: $OUTPUT_FILE"

# Optional: Display with syntax highlighting if available
if command -v bat &> /dev/null; then
    echo ""
    echo "Preview with syntax highlighting:"
    bat "$OUTPUT_FILE"
elif command -v less &> /dev/null; then
    echo ""
    echo "Opening in less..."
    less "$OUTPUT_FILE"
fi

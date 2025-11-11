#!/bin/bash

# VT220 Feature Test Script
# This script sends various VT220 escape sequences to test the terminal emulator

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}VT220 Terminal Emulator Test Script${NC}"
echo "=========================================="

# Function to send escape sequences with description
send_sequence() {
    local desc="$1"
    local seq="$2"
    echo -e "${YELLOW}Testing: $desc${NC}"
    printf "$seq"
    sleep 1
    echo
}

# Wait a bit for terminal to initialize
sleep 2

echo -e "\n${GREEN}1. Testing Device Attributes Response${NC}"
echo "=========================================="
# This should trigger the device attributes response
send_sequence "Device Attributes Request" "\033[c"

echo -e "\n${GREEN}2. Testing Character Set Selection${NC}"
echo "=========================================="
send_sequence "DEC Special Graphics Character Set (Line drawing)" "\033(0"
echo "Line drawing characters: ┌┐└┘├┤┬┴┼─│"
send_sequence "US ASCII Character Set" "\033(B"
echo "Back to normal ASCII"

echo -e "\n${GREEN}3. Testing Line Attributes${NC}"
echo "=========================================="
send_sequence "Double Height Top (Top half)" "\033#3"
echo "THIS LINE SHOULD BE DOUBLE HEIGHT (TOP)"
send_sequence "Double Height Bottom (Bottom half)" "\033#4"
echo "THIS LINE SHOULD BE DOUBLE HEIGHT (BOTTOM)"
send_sequence "Double Width" "\033#6"
echo "THIS LINE SHOULD BE DOUBLE WIDTH"
send_sequence "Single Width/Height" "\033#5"
echo "Back to normal single width/height"

echo -e "\n${GREEN}4. Testing Scrolling Regions${NC}"
echo "=========================================="
# Set scrolling region from line 5 to 15
send_sequence "Set scrolling region (lines 5-15)" "\033[5;15r"
echo "Scrolling region set from line 5 to 15"
echo "Line 1 - Outside region"
echo "Line 2 - Outside region" 
echo "Line 3 - Outside region"
echo "Line 4 - Outside region"
echo "Line 5 - Inside region (top)"
echo "Line 6 - Inside region"
echo "Line 7 - Inside region"
echo "Line 8 - Inside region"
echo "Line 9 - Inside region"
echo "Line 10 - Inside region"
echo "Line 11 - Inside region"
echo "Line 12 - Inside region"
echo "Line 13 - Inside region"
echo "Line 14 - Inside region"
echo "Line 15 - Inside region (bottom)"
echo "Line 16 - Outside region"
echo "Line 17 - Outside region"

# Reset scrolling region
send_sequence "Reset scrolling region" "\033[r"

echo -e "\n${GREEN}5. Testing Text Attributes${NC}"
echo "=========================================="
send_sequence "Bold text" "\033[1mThis is BOLD\033[0m"
send_sequence "Italic text" "\033[3mThis is ITALIC\033[0m"
send_sequence "Underline text" "\033[4mThis is UNDERLINED\033[0m"
send_sequence "Blink text" "\033[5mThis is BLINKING\033[0m"
send_sequence "Reverse video" "\033[7mThis is REVERSE VIDEO\033[0m"

echo -e "\n${GREEN}6. Testing Colors${NC}"
echo "=========================================="
send_sequence "Foreground Colors" "\033[31mRed \033[32mGreen \033[33mYellow \033[34mBlue \033[35mMagenta \033[36mCyan \033[0m"
send_sequence "Background Colors" "\033[41mRed \033[42mGreen \033[43mYellow \033[44mBlue \033[45mMagenta \033[46mCyan \033[0m"
send_sequence "Bright Colors" "\033[91mBright Red \033[92mBright Green \033[93mBright Yellow \033[0m"

echo -e "\n${GREEN}7. Testing Cursor Movement${NC}"
echo "=========================================="
send_sequence "Save cursor position" "\033[s"
echo "Cursor saved - moving around..."
send_sequence "Move cursor down 2 lines" "\033[2B"
send_sequence "Move cursor right 10 spaces" "\033[10C"
echo "New position text"
send_sequence "Restore cursor position" "\033[u"
echo "Back to saved position"

echo -e "\n${GREEN}8. Testing Screen Operations${NC}"
echo "=========================================="
send_sequence "Clear screen" "\033[2J"
echo "Screen should be cleared"
send_sequence "Cursor to home" "\033[H"
echo "Cursor at home position"

echo -e "\n${GREEN}9. Testing Line Drawing with DEC Special Graphics${NC}"
echo "=========================================="
send_sequence "Activate DEC Special Graphics" "\033(0"
# Draw a box using line drawing characters
echo "lqqqqqqqqqqk"
echo "x          x"
echo "x   TEST   x"
echo "x          x"
echo "mqqqqqqqqqqj"
send_sequence "Back to ASCII" "\033(B"

echo -e "\n${GREEN}10. Testing Combined Features${NC}"
echo "=========================================="
send_sequence "Double height + bold + color" "\033#3\033[1m\033[31mCOMBINED: Double Height Red Bold\033[0m"
send_sequence "Double width + underline" "\033#6\033[4mCOMBINED: Double Width Underlined\033[0m"

echo -e "\n${GREEN}Test Complete!${NC}"
echo "=============="
echo "Check the terminal output for:"
echo "- Proper device identification response"
echo "- Line drawing characters in the box"
echo "- Double height/width lines"
echo "- Colors and text attributes"
echo "- Scrolling region behavior"
echo "- Cursor save/restore functionality"

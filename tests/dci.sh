#!/bin/bash

# Function to print a separator
print_sep() {
    echo -e "\n----------------------------------------"
    echo "$1"
    echo "----------------------------------------"
}

echo "Starting Device Control (DCI) & DCS Tests..."
echo "NOTE: Watch your terminal emulator's DEBUG logs to verify receipt."
sleep 1

# 1. Test DC1 (XON) - 0x11
print_sep "Testing DC1 (XON) [\x11]"
printf "Sending DC1... "
printf "\x11"
echo "Sent."

# 2. Test DC2 - 0x12
print_sep "Testing DC2 [\x12]"
printf "Sending DC2... "
printf "\x12"
echo "Sent."

# 3. Test DC3 (XOFF) - 0x13
print_sep "Testing DC3 (XOFF) [\x13]"
printf "Sending DC3... "
printf "\x13"
echo "Sent."

# 4. Test DC4 - 0x14
print_sep "Testing DC4 [\x14]"
printf "Sending DC4... "
printf "\x14"
echo "Sent."

# 5. Test DCS (Device Control String)
# This tests the parsing logic for DCS we discussed earlier
# Format: ESC P (DCS) + params + q (Sixel/Data) + Payload + ESC \ (ST)
print_sep "Testing DCS (Sixel-style sequence)"
printf "Sending DCS... "
# \x1b = ESC
# P    = DCS Start
# 0;1;q = Params + Final Byte 'q'
# "TEST_PAYLOAD" = The data inside
# \x1b\\ = ST (String Terminator)
printf "\x1bP0;1;qTEST_PAYLOAD\x1b\\"
echo "Sent."

print_sep "Tests Complete."
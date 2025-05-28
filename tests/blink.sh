#!/bin/bash

echo "Testing cursor blink modes:"

# Slow blink (SGR 5)
echo -e "\033[5mCursor should be blinking SLOWLY now\033[0m"
sleep 4

# Rapid blink (SGR 6)
echo -e "\033[6mCursor should be blinking RAPIDLY now\033[0m"
sleep 4

# Reset to normal
echo -e "\033[0mCursor should be back to normal now\033[0m"
sleep 2

# Test visibility toggling
echo "Testing visibility toggling:"
for i in {1..5}; do
    echo -ne "\033[?25l"
    sleep 0.3
    echo -ne "\033[?25h"
    sleep 0.3
done

echo "Blink test complete"
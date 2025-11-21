#!/bin/bash

# This script generates OSC 8 escape sequences to test hyperlink rendering
# in a terminal emulator.

# OSC 8 Format:
# \e]8;id=...;URL=...;\e\nTEXT\e]8;;\e

# Set colors for better visibility in the terminal
CYAN='\033[36m'
GREEN='\033[32m'
RESET='\033[0m'

echo -e "${CYAN}--- Hyperlink Test Examples (OSC 8) ---${RESET}"
echo

# 1. Simple Google Link
URL="https://www.google.com"
ID="google_search"
TEXT="Google Search"
echo -e "${GREEN}Testing Simple Link:${RESET}"
echo -e "\e]8;id=${ID};URL=${URL};\e${TEXT}\e]8;;\e"
echo

# 2. Wikipedia Link with a different ID
URL="https://en.wikipedia.org/wiki/Terminal_emulator"
ID="wiki_term"
TEXT="Wikipedia Terminal Emulator Page"
echo -e "${GREEN}Testing Long URL Link:${RESET}"
echo -e "\e]8;id=${ID};URL=${URL};\e${TEXT}\e]8;;\e"
echo

# 3. File path link (Note: This depends on the OS/terminal supporting file:// protocol)
URL="file:///etc/hosts"
ID="local_hosts"
TEXT="Local Hosts File"
echo -e "${GREEN}Testing File Path Link:${RESET}"
echo -e "\e]8;id=${ID};URL=${URL};\e${TEXT}\e]8;;\e"
echo

# 4. Link embedded in text
URL="https://www.example.com"
ID="embedded_link"
TEXT="an important piece of information"
echo -e "${GREEN}Testing Embedded Link:${RESET}"
echo -e "Click here for \e]8;id=${ID};URL=${URL};\e${TEXT}\e]8;;\e demonstration."
echo

# 5. Link with SGR (Styling) applied. The link styling should override or combine with SGR.
URL="https://www.rust-lang.org"
ID="rust_lang"
TEXT="The Rust Programming Language"
echo -e "${GREEN}Testing Link with Bold SGR (should be bold + link style):${RESET}"
echo -e "\033[1m\e]8;id=${ID};URL=${URL};\e${TEXT}\e]8;;\e\\033[0m"
echo

URL="https://www.example.com"
DISPLAY_TEXT="Click here to visit Example.com"

# Print the hyperlink using OSC 8 escape sequences
echo -e "\e]8;;${URL}\e\\${DISPLAY_TEXT}\e]8;;\e\\"

echo -e "${CYAN}---------------------------------------${RESET}"

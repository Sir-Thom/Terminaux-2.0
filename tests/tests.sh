echo  "Running tests..."
echo -e "\e[?1049hHello alternate buffer!\e[?1049l"
./tests/24-bit-color.sh
./tests/blink.sh
./tests/colors.sh
./tests/return.sh

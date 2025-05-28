#!/bin/bash

echo "Testing return line (\r) support:"
echo -n "This message will be overwritten in 1 seconds if supported..."
sleep 1
echo -e "\rThis text appears at the line start if supported.          "
#test new line
echo -e "This message will be in a new line.:\nif supported. this will be on a newline\n "
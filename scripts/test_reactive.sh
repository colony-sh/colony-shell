#!/bin/bash
# Test script for reactive shell features
#
# This script sends commands to the colony shell to demonstrate
# the reactive tuple emission system

echo "Testing reactive environment features..."
echo ""
echo "Commands to test:"
echo "1. export TEST=hello    (should show env_change tuple)"
echo "2. export TEST=world    (should show change with old value)"
echo "3. unset TEST           (should show unset with Null)"
echo "4. cd /tmp              (should show dir_change tuple)"
echo "5. pwd                  (verify we're in /tmp)"
echo ""
echo "Run: ./target/debug/colony"
echo "Then type the commands above one by one"

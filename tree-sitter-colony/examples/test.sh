#!/usr/bin/env colony

# Test script for Tree-sitter parser

# Simple commands
echo "Hello, Colony!"
ls -la /tmp

# Pipelines
echo "testing" | grep test
ls *.txt | sort | uniq

# Variable expansion
NAME="World"
echo "Hello, $NAME"

# Parameter expansion
echo "${HOME:-/tmp}"
echo "${#NAME}"

# Compound commands
if [ -f test.txt ]; then
    cat test.txt
else
    echo "File not found"
fi

# For loop
for file in *.txt; do
    echo "Processing: $file"
    cat "$file"
done

# While loop
count=0
while [ $count -lt 5 ]; do
    echo "Count: $count"
    count=$((count + 1))
done

# Case statement
case "$1" in
    start)
        echo "Starting..."
        ;;
    stop)
        echo "Stopping..."
        ;;
    *)
        echo "Usage: $0 {start|stop}"
        ;;
esac

# Function definition
greet() {
    echo "Hello, $1!"
}

greet "Colony"

# Background job
sleep 10 &

# Command list
make && make test || echo "Build failed"

# Colony extensions (future)
# var x = 42
# const PI = 3.14159
# proc calculate(a, b) {
#     return a + b
# }

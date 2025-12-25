#!/bin/colony
# Tutorial Mode Demo Script
# Run with: colony --tutorial scripts/tutorial_demo.sh
# Or with teach mode: colony --tutorial --teach scripts/tutorial_demo.sh

# === Basic Variables ===
NAME="World"
GREETING="Hello, $NAME!"
echo "$GREETING"

# === Parameter Expansion ===
UNSET_VAR=""
echo "Default value: ${UNSET_VAR:-default_value}"
echo "Length of NAME: ${#NAME}"

# === Command Substitution ===
TODAY=$(date +%Y-%m-%d)
echo "Today is $TODAY"

# === Arithmetic ===
COUNT=5
RESULT=$((COUNT * 2 + 1))
echo "Result: $RESULT"

# === Conditionals ===
if [ -d /tmp ]; then
    echo "/tmp exists and is a directory"
else
    echo "/tmp does not exist"
fi

# === For Loop ===
echo "Counting:"
for i in 1 2 3; do
    echo "  $i"
done

# === Pipeline ===
echo "Files in current directory:"
ls -la | head -5

# === Redirection ===
echo "Writing to temp file..."
echo "Hello from tutorial" > /tmp/tutorial_test.txt
cat /tmp/tutorial_test.txt

# === Function Definition ===
greet() {
    local recipient="$1"
    echo "Greetings, $recipient!"
}

greet "Tutorial User"

# === Case Statement ===
FRUIT="apple"
case $FRUIT in
    apple)
        echo "It's an apple"
        ;;
    banana|orange)
        echo "It's yellow or orange"
        ;;
    *)
        echo "Unknown fruit"
        ;;
esac

# === Tilde Expansion ===
echo "Home directory: ~"

# === Glob Patterns ===
echo "Shell scripts in this directory:"
ls *.sh 2>/dev/null || echo "  (none found)"

# === Exit Status ===
true
echo "Exit status of 'true': $?"
false
echo "Exit status of 'false': $?"

# === Cleanup ===
rm -f /tmp/tutorial_test.txt

echo "Tutorial demo complete!"

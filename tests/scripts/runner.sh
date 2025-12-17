#!/usr/bin/env bash
# Test runner for Colony shell compatibility tests
# Compares colony output against bash output and .right files

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COLONY_BIN="${COLONY_BIN:-$SCRIPT_DIR/../../target/debug/colony}"
BASH_BIN="${BASH_BIN:-/bin/bash}"

# Temp directory for test outputs
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Run Colony shell compatibility tests against bash test suite.

OPTIONS:
    --help              Show this help message
    --test <name>       Run specific test (e.g., bash-compat/01_builtins)
    --category <cat>    Run specific category (e.g., builtins)
    --verbose           Show detailed output
    --compare-only      Only compare against bash, skip .right files
    --colony-only       Only test Colony variants

EXAMPLES:
    $0                              # Run all tests
    $0 --test bash-compat/01_builtins  # Run single test
    $0 --category builtins          # Run all builtin tests
    $0 --verbose                    # Show all output

EOF
    exit 0
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $*"
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $*"
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $*"
}

run_test() {
    local test_file="$1"
    local test_name="$2"
    local right_file="${test_file%.tests}.right"

    log_info "Running: $test_name"

    # Check if .tests file exists
    if [[ ! -f "$test_file" ]]; then
        log_skip "$test_name: .tests file not found"
        ((SKIPPED++))
        return
    fi

    # Run with bash
    local bash_output="$TEMP_DIR/bash_output.txt"
    local bash_exit=0
    if ! "$BASH_BIN" "$test_file" > "$bash_output" 2>&1; then
        bash_exit=$?
    fi

    # Run with colony
    local colony_output="$TEMP_DIR/colony_output.txt"
    local colony_exit=0
    if [[ ! -f "$COLONY_BIN" ]]; then
        log_skip "$test_name: colony binary not found at $COLONY_BIN"
        ((SKIPPED++))
        return
    fi

    if ! "$COLONY_BIN" "$test_file" > "$colony_output" 2>&1; then
        colony_exit=$?
    fi

    # Compare exit codes
    if [[ $bash_exit -ne $colony_exit ]]; then
        log_fail "$test_name: Exit code mismatch (bash=$bash_exit, colony=$colony_exit)"
        if [[ -n "${VERBOSE:-}" ]]; then
            echo "  Bash output:"
            head -20 "$bash_output" | sed 's/^/    /'
            echo "  Colony output:"
            head -20 "$colony_output" | sed 's/^/    /'
        fi
        ((FAILED++))
        return
    fi

    # Compare outputs
    if ! diff -u "$bash_output" "$colony_output" > "$TEMP_DIR/diff.txt" 2>&1; then
        log_fail "$test_name: Output differs from bash"
        if [[ -n "${VERBOSE:-}" ]]; then
            echo "  Diff:"
            head -40 "$TEMP_DIR/diff.txt" | sed 's/^/    /'
        fi
        ((FAILED++))
        return
    fi

    # If .right file exists, compare against it too
    if [[ -f "$right_file" && -z "${COMPARE_ONLY:-}" ]]; then
        if ! diff -u "$right_file" "$colony_output" > "$TEMP_DIR/diff_right.txt" 2>&1; then
            log_fail "$test_name: Output differs from .right file"
            if [[ -n "${VERBOSE:-}" ]]; then
                echo "  Diff:"
                head -40 "$TEMP_DIR/diff_right.txt" | sed 's/^/    /'
            fi
            ((FAILED++))
            return
        fi
    fi

    log_success "$test_name"
    ((PASSED++))
}

find_tests() {
    local base_dir="$1"
    local pattern="${2:-.tests}"

    find "$base_dir" -type f -name "*$pattern" | sort
}

run_all_tests() {
    local base_dir="${1:-$SCRIPT_DIR/bash-compat}"

    log_info "Discovering tests in $base_dir..."

    local test_files=()
    while IFS= read -r test_file; do
        test_files+=("$test_file")
    done < <(find_tests "$base_dir")

    if [[ ${#test_files[@]} -eq 0 ]]; then
        log_skip "No test files found in $base_dir"
        return
    fi

    log_info "Found ${#test_files[@]} test file(s)"
    echo

    for test_file in "${test_files[@]}"; do
        local test_name="${test_file#$SCRIPT_DIR/}"
        run_test "$test_file" "$test_name"
    done
}

print_summary() {
    local total=$((PASSED + FAILED + SKIPPED))

    echo
    echo "================================================"
    echo "Test Summary"
    echo "================================================"
    echo -e "Total:   $total"
    echo -e "${GREEN}Passed:  $PASSED${NC}"
    echo -e "${RED}Failed:  $FAILED${NC}"
    echo -e "${YELLOW}Skipped: $SKIPPED${NC}"
    echo "================================================"

    if [[ $FAILED -gt 0 ]]; then
        exit 1
    fi
}

main() {
    local specific_test=""
    local category=""

    while [[ $# -gt 0 ]]; do
        case $1 in
            --help)
                usage
                ;;
            --test)
                specific_test="$2"
                shift 2
                ;;
            --category)
                category="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=1
                shift
                ;;
            --compare-only)
                COMPARE_ONLY=1
                shift
                ;;
            --colony-only)
                COLONY_ONLY=1
                shift
                ;;
            *)
                echo "Unknown option: $1"
                usage
                ;;
        esac
    done

    log_info "Colony Shell Test Runner"
    log_info "Colony: $COLONY_BIN"
    log_info "Bash:   $BASH_BIN"
    echo

    if [[ -n "$specific_test" ]]; then
        # Run single test
        local test_file="$SCRIPT_DIR/$specific_test"
        if [[ ! "$test_file" =~ \.tests$ ]]; then
            test_file="${test_file}.tests"
        fi
        run_test "$test_file" "$specific_test"
    elif [[ -n "$category" ]]; then
        # Run category
        run_all_tests "$SCRIPT_DIR/bash-compat/*${category}*"
    elif [[ -n "${COLONY_ONLY:-}" ]]; then
        # Run only Colony variants
        run_all_tests "$SCRIPT_DIR/colony-variants"
    else
        # Run all tests
        run_all_tests "$SCRIPT_DIR/bash-compat"
        if [[ -d "$SCRIPT_DIR/colony-variants" ]]; then
            run_all_tests "$SCRIPT_DIR/colony-variants"
        fi
    fi

    print_summary
}

main "$@"

#!/usr/bin/env bash
# Colony Shell Spec Audit Tool
# Compares specification against implementation using Claude
#
# Usage:
#   ./scripts/audit-spec.sh [part-number]     # Audit specific part
#   ./scripts/audit-spec.sh all               # Audit all parts
#   ./scripts/audit-spec.sh --generate-prompt # Generate prompt file only
#
# Requires: ANTHROPIC_API_KEY environment variable (or use with Claude Code)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SPEC_DIR="$PROJECT_ROOT/docs/spec"
CRATES_DIR="$PROJECT_ROOT/crates"
OUTPUT_DIR="$PROJECT_ROOT/target/audit"

# Mapping of spec parts to relevant crates
declare -A SPEC_TO_CRATES=(
    ["1-architecture"]="all"
    ["2-lexical-structure"]="colony-parser"
    ["3-type-system"]="colony-parser"
    ["4-ast-definitions"]="colony-parser"
    ["5-execution"]="colony-exec"
    ["6-pipelines"]="colony-exec"
    ["7-channels"]="colony-exec"
    ["8-buffers"]="colony-shell"
    ["9-agents"]="colony-agent"
    ["10-coordination"]="colony-shell"
    ["11-mcp"]="colony-agent"
    ["12-special-files"]="colony-shell"
    ["13-interactive"]="colony-shell"
    ["14-builtins"]="colony-exec"
    ["15-grammar"]="colony-parser"
    ["16-context"]="colony-shell"
)

# System prompt for Claude
SYSTEM_PROMPT='You are a technical auditor comparing a software specification against its implementation.

Your task is to analyze alignment between spec and code, identifying:

1. **Implemented** - Features fully implemented matching spec
2. **Partial** - Features partially implemented or diverging from spec
3. **Missing** - Features specified but not implemented
4. **Extra** - Implementation features not in spec (may need spec update)
5. **Inconsistent** - Naming, structure, or behavior differences

For each finding, provide:
- Category (Implemented/Partial/Missing/Extra/Inconsistent)
- Spec reference (section number)
- Code reference (file:line if applicable)
- Description
- Severity (info/warning/error)
- Recommendation

Output format: Structured markdown report with summary statistics.

Be thorough but concise. Focus on semantic alignment, not cosmetic differences.'

gather_crate_summary() {
    local crate="$1"
    local crate_dir="$CRATES_DIR/$crate"

    if [[ ! -d "$crate_dir" ]]; then
        echo "# Crate $crate: NOT FOUND"
        return
    fi

    echo "# Crate: $crate"
    echo ""
    echo "## Structure"
    echo '```'
    find "$crate_dir/src" -name "*.rs" -type f 2>/dev/null | sed "s|$crate_dir/||" | sort
    echo '```'
    echo ""
    echo "## Public API (pub items)"
    echo '```rust'
    grep -rh "^pub " "$crate_dir/src" 2>/dev/null | head -100 || true
    echo '```'
    echo ""
    echo "## Key Types"
    echo '```rust'
    grep -rh "^pub struct\|^pub enum\|^pub trait" "$crate_dir/src" 2>/dev/null | head -50 || true
    echo '```'
    echo ""
}

gather_all_crates() {
    for crate_dir in "$CRATES_DIR"/*/; do
        crate=$(basename "$crate_dir")
        gather_crate_summary "$crate"
        echo "---"
        echo ""
    done
}

generate_audit_prompt() {
    local part="$1"
    local spec_file="$SPEC_DIR/$part.md"

    if [[ ! -f "$spec_file" ]]; then
        echo "Error: Spec file not found: $spec_file" >&2
        return 1
    fi

    local crates="${SPEC_TO_CRATES[$part]:-}"

    echo "# Spec Audit Request"
    echo ""
    echo "## Specification: Part ${part%%-*} - ${part#*-}"
    echo ""
    echo '```markdown'
    cat "$spec_file"
    echo '```'
    echo ""
    echo "## Implementation"
    echo ""

    if [[ "$crates" == "all" ]]; then
        gather_all_crates
    elif [[ -n "$crates" ]]; then
        for crate in $crates; do
            gather_crate_summary "$crate"
        done
    else
        echo "No crates mapped for this spec part."
    fi

    echo ""
    echo "## Audit Request"
    echo ""
    echo "Compare the specification above against the implementation and provide an alignment report."
}

run_audit_with_api() {
    local prompt_file="$1"
    local output_file="$2"

    if [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
        echo "Warning: ANTHROPIC_API_KEY not set. Generating prompt file only." >&2
        echo "You can run this prompt through Claude Code or the API manually." >&2
        return 1
    fi

    # Call Anthropic API
    curl -s https://api.anthropic.com/v1/messages \
        -H "Content-Type: application/json" \
        -H "x-api-key: $ANTHROPIC_API_KEY" \
        -H "anthropic-version: 2023-06-01" \
        -d @- <<EOF > "$output_file"
{
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 8192,
    "system": $(echo "$SYSTEM_PROMPT" | jq -Rs .),
    "messages": [
        {
            "role": "user",
            "content": $(cat "$prompt_file" | jq -Rs .)
        }
    ]
}
EOF

    # Extract response text
    jq -r '.content[0].text // .error.message // "Unknown error"' "$output_file"
}

audit_part() {
    local part="$1"

    mkdir -p "$OUTPUT_DIR"

    local prompt_file="$OUTPUT_DIR/${part}-prompt.md"
    local result_file="$OUTPUT_DIR/${part}-audit.json"
    local report_file="$OUTPUT_DIR/${part}-report.md"

    echo "Generating audit prompt for $part..."
    generate_audit_prompt "$part" > "$prompt_file"

    echo "Prompt saved to: $prompt_file"
    echo "Prompt size: $(wc -c < "$prompt_file") bytes"

    if run_audit_with_api "$prompt_file" "$result_file"; then
        echo "Audit complete. Report saved to: $report_file"
    else
        echo ""
        echo "To run this audit manually:"
        echo "  1. Open $prompt_file"
        echo "  2. Paste into Claude with the system prompt from this script"
        echo "  3. Save the response to $report_file"
    fi
}

audit_all() {
    for spec_file in "$SPEC_DIR"/*.md; do
        part=$(basename "$spec_file" .md)
        [[ "$part" == "0-overview" ]] && continue  # Skip overview
        audit_part "$part"
        echo ""
    done

    # Generate summary
    echo "# Spec Audit Summary" > "$OUTPUT_DIR/SUMMARY.md"
    echo "" >> "$OUTPUT_DIR/SUMMARY.md"
    echo "Generated: $(date -Iseconds)" >> "$OUTPUT_DIR/SUMMARY.md"
    echo "" >> "$OUTPUT_DIR/SUMMARY.md"
    echo "## Parts Audited" >> "$OUTPUT_DIR/SUMMARY.md"
    ls "$OUTPUT_DIR"/*-report.md 2>/dev/null | while read f; do
        echo "- $(basename "$f" -report.md)" >> "$OUTPUT_DIR/SUMMARY.md"
    done

    echo "Summary saved to: $OUTPUT_DIR/SUMMARY.md"
}

# Main
case "${1:-}" in
    "all")
        audit_all
        ;;
    "--generate-prompt")
        part="${2:-1-architecture}"
        generate_audit_prompt "$part"
        ;;
    "--help"|"-h"|"")
        echo "Colony Shell Spec Audit Tool"
        echo ""
        echo "Usage:"
        echo "  $0 <part>              Audit specific part (e.g., 5-execution)"
        echo "  $0 all                 Audit all spec parts"
        echo "  $0 --generate-prompt <part>  Output prompt to stdout"
        echo ""
        echo "Available parts:"
        for spec_file in "$SPEC_DIR"/*.md; do
            echo "  $(basename "$spec_file" .md)"
        done
        ;;
    *)
        audit_part "$1"
        ;;
esac

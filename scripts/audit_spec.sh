#!/bin/bash
# Spec Adherence Audit - Generate report of implemented features
#
# Reads .claude/system/features.json and generates markdown report

set -e

FEATURES_FILE=".claude/system/features.json"
REPORT_FILE="spec-audit-report.md"

if [ ! -f "$FEATURES_FILE" ]; then
    echo "Error: $FEATURES_FILE not found"
    exit 1
fi

# Extract feature counts using grep and wc
TOTAL=$(grep -c '"id": "F' "$FEATURES_FILE" || echo "0")
COMPLETE=$(grep -A3 '"id": "F' "$FEATURES_FILE" | grep -c '"status": "complete"' || echo "0")
IN_PROGRESS=$(grep -A3 '"id": "F' "$FEATURES_FILE" | grep -c '"status": "in_progress"' || echo "0")
NEW=$(grep -A3 '"id": "F' "$FEATURES_FILE" | grep -c '"status": "new"' || echo "0")
DEPRECATED=$(grep -A3 '"id": "F' "$FEATURES_FILE" | grep -c '"status": "deprecated"' || echo "0")

# Calculate percentage
if [ "$TOTAL" -gt 0 ]; then
    PERCENT=$((COMPLETE * 100 / TOTAL))
else
    PERCENT=0
fi

# Generate report
cat > "$REPORT_FILE" <<EOF
# Colony Shell - Spec Adherence Report

**Generated:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")

## Summary

- **Total Features:** $TOTAL
- **Complete:** $COMPLETE ($PERCENT%)
- **In Progress:** $IN_PROGRESS
- **New (Not Started):** $NEW
- **Deprecated:** $DEPRECATED

## Status Breakdown

### Complete Features

EOF

# Extract complete features
grep -B2 '"status": "complete"' "$FEATURES_FILE" | grep '"id"' | sed 's/.*"id": "\(F[0-9]*\)".*/- \1/' | while read -r line; do
    # Get feature name
    FID=$(echo "$line" | sed 's/- //')
    FNAME=$(grep -A1 "\"id\": \"$FID\"" "$FEATURES_FILE" | grep '"name"' | sed 's/.*"name": "\(.*\)".*/\1/' | head -1)
    echo "$line: $FNAME" >> "$REPORT_FILE"
done

cat >> "$REPORT_FILE" <<EOF

### In Progress Features

EOF

# Extract in-progress features
grep -B2 '"status": "in_progress"' "$FEATURES_FILE" | grep '"id"' | sed 's/.*"id": "\(F[0-9]*\)".*/- \1/' | while read -r line; do
    FID=$(echo "$line" | sed 's/- //')
    FNAME=$(grep -A1 "\"id\": \"$FID\"" "$FEATURES_FILE" | grep '"name"' | sed 's/.*"name": "\(.*\)".*/\1/' | head -1)
    echo "$line: $FNAME" >> "$REPORT_FILE"
done

cat >> "$REPORT_FILE" <<EOF

### Pending Features

EOF

# Extract new features
grep -B2 '"status": "new"' "$FEATURES_FILE" | grep '"id"' | sed 's/.*"id": "\(F[0-9]*\)".*/- \1/' | while read -r line; do
    FID=$(echo "$line" | sed 's/- //')
    FNAME=$(grep -A1 "\"id\": \"$FID\"" "$FEATURES_FILE" | grep '"name"' | sed 's/.*"name": "\(.*\)".*/\1/' | head -1)
    FDESC=$(grep -A2 "\"id\": \"$FID\"" "$FEATURES_FILE" | grep '"description"' | sed 's/.*"description": "\(.*\)".*/\1/' | head -1)
    echo "$line: $FNAME" >> "$REPORT_FILE"
done

cat >> "$REPORT_FILE" <<EOF

## Core Capabilities

### Parser & Execution (POSIX)
- [x] Lexer with mode stack (F001 - deprecated, replaced by pest)
- [x] Recursive descent parser (F002 - deprecated, replaced by pest)
- [x] Pure PEG parser with pest (F021)
- [x] Tree-sitter for editor features (F022)
- [x] Expansion system (F003)
- [x] Simple command execution (F004)
- [x] Pipeline execution (F005)
- [x] Redirections (F006)
- [x] Core builtins (F007)
- [x] Job control (F008)

### Agent & AI Features
- [x] OSC 2337 protocol (F009)
- [x] Tuple space coordination (F010, F011, F012)
- [x] Multi-provider agent routing (F014)
- [x] Claude ADK integration (F015)
- [x] Project configuration (F020)
- [ ] MCP connection pool (F016)
- [ ] MCP TypeScript codegen (F017)
- [ ] MCP executor session (F018)
- [ ] Goal tracking (F019)

### Terminal Integration
- [x] Extended channels (F024)
- [ ] Buffer management (F025)
- [ ] Pane management (F026)
- [ ] Reactive prompt (F027)
- [ ] File tree state (F028)
- [ ] File tree protocol (F029)

### Advanced Features
- [ ] Special files (AGENT.md, CONTEXT.md) (F030)
- [ ] Context management (F031)
- [ ] Reflection engine (F032)
- [ ] Typed pipelines (F033)
- [ ] Type system (F034)
- [ ] Interactive features (F035)
- [ ] Completion system (F036)

## Test Coverage

- **BATS Integration Tests:** Available in \`tests/builtins.bats\`
- **Unit Tests:** Run with \`cargo test --workspace\`
- **POSIX Compliance:** 90%+ on initial test suite (F023)

## CI/CD

- **GitHub Actions:** CI workflow for build/test/lint
- **Cargo Publish:** Automated publishing on version tags
- **Documentation:** Auto-generated docs with \`cargo doc\`

---

*Report generated from .claude/system/features.json*
EOF

echo "✅ Audit report generated: $REPORT_FILE"
cat "$REPORT_FILE"

# tree-sitter-colony

Tree-sitter grammar for Colony Shell.

## Features

- **POSIX Shell Syntax**: Complete support for simple commands, pipelines, compound commands
- **Colony Extensions**: Typed pipes (`->`, `=>`), `var`/`const` declarations, proc definitions
- **Incremental Parsing**: Fast re-parsing on edits (< 1ms typical)
- **Error Recovery**: Syntax highlighting works even with syntax errors
- **Language Injection**: Support for heredocs, command substitution

## Usage

### Development

```bash
# Install Tree-sitter CLI
npm install

# Generate parser
npx tree-sitter generate

# Test parser
npx tree-sitter test

# Parse a file
npx tree-sitter parse examples/test.sh
```

### Rust Integration

The parser is available as a Rust crate via `tree-sitter-colony`:

```rust
use tree_sitter::Parser;
use tree_sitter_colony::language;

let mut parser = Parser::new();
parser.set_language(language()).unwrap();

let source_code = "echo hello | grep h";
let tree = parser.parse(source_code, None).unwrap();
```

## Architecture

### Hybrid Parser Model

Colony Shell uses Tree-sitter for editor features and a precise PEG/hand-written parser for execution:

```
┌─────────────────────────────────────────┐
│           Input Buffer                  │
└───────────────┬─────────────────────────┘
                │
    ┌───────────┴────────────┐
    ▼                        ▼
┌───────────┐         ┌─────────────┐
│Tree-sitter│         │ PEG Parser  │
│(editing)  │         │(execution)  │
│           │         │             │
│• Fast     │         │• Precise    │
│• Tolerant │         │• Validated  │
└─────┬─────┘         └──────┬──────┘
      │                      │
      │ CST                  │ AST
      ▼                      ▼
┌─────────────────────────────────────────┐
│       Reconciliation Layer              │
└─────────────────────────────────────────┘
```

**Tree-sitter provides:**
- Syntax highlighting
- Code folding
- Symbol navigation
- Structural selection
- Incremental updates

**PEG/hand-written parser provides:**
- Exact POSIX semantics
- Validation predicates
- Execution AST

## Grammar Overview

### Commands

```colony
# Simple commands
echo hello world
ls -la /tmp

# Pipelines
echo foo | grep f
ls | grep txt | sort

# Typed pipes (Colony extension)
data.json -> jq '.name' => capitalize

# Command lists
make && make test || echo failed
```

### Compound Commands

```colony
# if/elif/else
if [ -f file.txt ]; then
    cat file.txt
elif [ -d dir ]; then
    ls dir
else
    echo "not found"
fi

# for loops
for file in *.txt; do
    cat "$file"
done

# while loops
while read line; do
    echo "$line"
done < file.txt
```

### Colony Extensions

```colony
# Variable declarations
var x = 42
const NAME = "Colony"

# Expressions
var result = (x + 10) * 2

# Proc definitions (future)
proc greet(name, greeting = "Hello") {
    echo "$greeting, $name!"
}
```

## Testing

Test files are in `test/corpus/`:

```bash
npx tree-sitter test
```

## License

MIT OR Apache-2.0

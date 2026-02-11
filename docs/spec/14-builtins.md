# Part 14: Builtins Reference

## 14.1 Overview

Colony provides builtin commands in several categories:
- **Core Shell**: Navigation, environment, job control
- **Variables**: Declaration, manipulation
- **Control Flow**: Conditionals, loops, functions
- **Channels**: Extended channel operations
- **Agents**: Agent definition and execution
- **Coordination**: Tuple space, messaging
- **MCP**: Model Context Protocol integration
- **UI**: Pane and buffer management

## 14.2 Core Shell Builtins

### cd - Change Directory

```
cd [dir]
cd -
cd ~
```

| Argument | Description |
|----------|-------------|
| `dir` | Directory to change to |
| `-` | Previous directory ($OLDPWD) |
| `~` | Home directory |

**Options:** `-P` (physical), `-L` (logical, default)

---

### pwd - Print Working Directory

```
pwd [-P|-L]
```

---

### echo - Print Arguments

```
echo [-n] [-e] [args...]
```

| Option | Description |
|--------|-------------|
| `-n` | No trailing newline |
| `-e` | Enable escape sequences |

---

### printf - Formatted Print

```
printf format [args...]
```

Format: `%s` (string), `%d` (int), `%f` (float), `%x` (hex), `%%` (literal)

---

### exit - Exit Shell

```
exit [code]
```

---

### source / . - Execute Script

```
source file [args...]
```

---

### exec - Replace Shell

```
exec command [args...]
```

---

### eval - Evaluate String

```
eval string
```

---

### type - Command Type

```
type [-a] name...
```

Output: alias, builtin, function, proc, agent, file, keyword

---

### which - Locate Command

```
which [-a] name...
```

---

### history - Command History

```
history [n]
history -c          # Clear
history -d offset   # Delete
history -w [file]   # Write
history -r [file]   # Read
```

---

### alias / unalias

```
alias [name[=value]...]
unalias [-a] name...
```

---

### jobs / fg / bg - Job Control

```
jobs [-l] [-p]
fg [job_spec]
bg [job_spec]
```

Job specs: `%1`, `%+`, `%-`, `%name`

---

### wait - Wait for Jobs

```
wait [job_spec...]
wait -n             # Any job
```

---

### kill - Send Signal

```
kill [-signal] pid...
kill -l [signal]
```

Signals: TERM(15), KILL(9), HUP(1), INT(2), STOP(19), CONT(18)

---

### trap - Signal Handler

```
trap 'commands' signal...
trap - signal...    # Reset
trap -l             # List
```

---

### umask - File Mode Mask

```
umask [-S] [mode]
```

---

### ulimit - Resource Limits

```
ulimit [-SHa] [-c|-d|-f|-n|-s|-t|-v] [limit]
```

---

## 14.3 Variable Builtins

### var - Declare Variable

```
var name[: Type] [= value]
```

```colony
var count = 0
var name: Str = "Alice"
var items: List[Int] = [1, 2, 3]
```

---

### const - Declare Constant

```
const name[: Type] = value
```

---

### setvar - Set Existing Variable

```
setvar name = value
```

---

### export - Export Variable

```
export name[=value]...
export -n name...   # Un-export
export -p           # Print
```

---

### unset - Unset Variable

```
unset [-v] [-f] name...
```

---

### local - Local Variable

```
local name[=value]...
```

---

### readonly - Read-only Variable

```
readonly name[=value]...
```

---

### declare - Declare with Attributes

```
declare [-aAilrx] name[=value]
```

| Option | Description |
|--------|-------------|
| `-a` | Array |
| `-A` | Associative array |
| `-i` | Integer |
| `-r` | Readonly |
| `-x` | Export |

---

## 14.4 Control Flow Builtins

### if - Conditional

```colony
if condition { ... }
if condition { ... } else { ... }
if condition { ... } elif condition { ... } else { ... }
```

---

### for - Loop

```colony
for var in items { ... }
for (init; cond; update) { ... }
for i, item in enumerate(items) { ... }
```

---

### while / until

```colony
while condition { ... }
until condition { ... }
```

---

### match - Pattern Match

```colony
match value {
    pattern => result
    pattern if guard => result
    _ => default
}
```

---

### case - Pattern Branch

```colony
case value in
    pattern) commands ;;
    *) default ;;
esac
```

---

### break / continue

```
break [n]
continue [n]
```

---

### return

```
return [value]
```

---

### fn - Define Function

```colony
fn name(params) -> Type { expr }
```

Pure function, expression-based.

---

### proc - Define Procedure

```colony
proc name(params) [type -> type] { body }
```

Procedure with pipeline type annotations.

---

### test / [

```
test expression
[ expression ]
```

**File:** `-e` (exists), `-f` (file), `-d` (dir), `-r` (read), `-w` (write), `-x` (exec)
**String:** `-z` (empty), `-n` (non-empty), `=`, `!=`
**Numeric:** `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge`

---

### true / false

```
true    # Exit 0
false   # Exit 1
```

---

## 14.5 Channel Builtins

### context - Context Channel

```colony
context [--type TYPE] message
context:add TYPE message
context:show [--type TYPE] [--last N]
context:search pattern
context:export [--format FORMAT]
context:clear [--type TYPE]
context:summarize
```

Types: `observation`, `decision`, `action`, `artifact`, `error`, `question`, `seed`

---

### llm - LLM Channel

```colony
llm:user message
llm:system message
llm:assistant message
llm:show [--last N]
llm:get [index]
llm:branch name
llm:checkout name
llm:branches
llm:checkpoint [name]
llm:restore name
llm:truncate N
llm:clear
llm:export [--format FORMAT]
llm:chat [prompt]
```

---

### tool - Tool Channel

```colony
tool:log name args result
tool:show [--last N]
tool:search tool_name
tool:stats
```

---

### diagnostic - Diagnostic Channel

```colony
diagnostic:enable
diagnostic:disable
diagnostic:show [--level LEVEL] [--category CAT]
diagnostic:clear
```

Levels: `debug`, `info`, `warn`, `error`

---

## 14.6 Agent Builtins

```colony
agent name { ... }              # Define agent
agent:run name [args...]        # Execute agent
agent:spawn name [args...] &    # Background agent
agent:wait handle               # Wait for completion
agent:result handle             # Get result
agent:cancel handle             # Cancel agent
agent:list                      # List agents
agent:show name                 # Show definition
agent:sessions [name]           # List sessions
agent:continue session msg      # Continue session
agent:export session [--format] # Export conversation
```

---

## 14.7 Tuple Space Builtins

```colony
ts:out tuple [--expires DUR]    # Add tuple
ts:in pattern [--timeout DUR]   # Take (blocking)
ts:rd pattern [--timeout DUR]   # Read (blocking)
ts:inp pattern                  # Take (non-blocking)
ts:rdp pattern                  # Read (non-blocking)
ts:query pattern                # Query all
ts:count pattern                # Count matches
ts:clear                        # Clear space
ts:info                         # Statistics
```

---

## 14.8 Messaging Builtins

```colony
msg:send recipient body         # Send message
msg:receive [--timeout DUR]     # Receive (blocking)
msg:poll                        # Check (non-blocking)
msg:broadcast recipients body   # Multi-send
msg:request recip body [--timeout] # Request-response
msg:reply message_id body       # Reply
msg:sessions                    # List sessions
```

---

## 14.9 Lock Builtins

```colony
lock:acquire name [--timeout]   # Acquire lock
lock:release name               # Release lock
lock:try name                   # Try acquire
lock:with name { commands }     # Lock block
lock:info name                  # Lock status
```

---

## 14.10 MCP Builtins

```colony
mcp:connect name --command CMD  # Connect stdio
mcp:connect name --url URL      # Connect HTTP
mcp:disconnect name             # Disconnect
mcp:servers                     # List servers
mcp:tools [server]              # List tools
mcp:call [server:]tool [args]   # Call tool
mcp:resources [server]          # List resources
mcp:read server uri             # Read resource
mcp:refresh [server]            # Refresh lists
mcp:info server                 # Server info
```

---

## 14.11 Pane/Buffer Builtins

```colony
pane:layout preset              # Apply layout
pane:split-h [--ratio R]        # Split horizontal
pane:split-v [--ratio R]        # Split vertical
pane:focus pane_id              # Focus pane
pane:focus-next                 # Next pane
pane:focus-prev                 # Previous pane
pane:close [pane_id]            # Close pane
pane:zoom [pane_id]             # Toggle zoom
pane:assign pane channel        # Assign channel
pane:presets                    # List presets

buffer:clear [channel]          # Clear buffer
buffer:save [channel] > file    # Save to file
buffer:export [--ansi] > file   # Export with styles
```

---

## 14.12 Utility Builtins

### read - Read Input

```
read [-p prompt] [-t timeout] [-s] [-n count] var...
```

---

### sleep - Sleep

```
sleep duration    # 1s, 100ms, 2m
```

---

### time - Time Command

```
time command [args...]
```

---

### set - Set Options

```
set [option] [value]
set -o option     # Enable
set +o option     # Disable
```

---

### shopt - Shell Options

```
shopt [-s|-u] option...
```

Options: `extglob`, `nullglob`, `dotglob`, `nocaseglob`, `failglob`

---

### hash - Command Hash

```
hash [-r] [name...]
```

---

### enable - Enable Builtins

```
enable [-n] name...
```

---

### builtin - Run Builtin

```
builtin name [args...]
```

---

### command - Run Command

```
command [-pv] name [args...]
```

---

### getopts - Parse Options

```
getopts optstring name [args...]
```

---

### shift - Shift Arguments

```
shift [n]
```

---

### pushd / popd / dirs

```
pushd [dir]
popd
dirs [-v]
```

---

*Continue to Part 15: Formal Grammar*

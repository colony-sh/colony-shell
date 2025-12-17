# Colony Architecture: Where Does the File Tree Live?

## The Question

Colony is both a **shell** and a **terminal emulator**. The file tree pane could live in either layer, or span both. This decision has significant implications for:

- Agent integration and context awareness
- Compatibility with external shells/terminals
- Rendering performance and capabilities
- Code organization and complexity

---

## Option 1: Terminal-Side (Like Warp)

The terminal emulator owns the file tree entirely. The shell is unaware of it.

```
┌─────────────────────────────────────────────────────────────┐
│                     COLONY TERMINAL                         │
│  ┌─────────────────┬─────────────────────────────────────┐  │
│  │                 │                                     │  │
│  │   File Tree     │         PTY Output                  │  │
│  │   (Terminal)    │         (Any Shell)                 │  │
│  │                 │                                     │  │
│  │  - Rendering    │    $ ls                             │  │
│  │  - State        │    file1.txt file2.txt              │  │
│  │  - Watching     │    $ █                              │  │
│  │  - Git status   │                                     │  │
│  │                 │                                     │  │
│  └─────────────────┴─────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### How It Works

```rust
// Terminal owns everything
pub struct ColonyTerminal {
    pty: Pty,                        // Runs any shell (bash, zsh, colony-shell)
    tree_pane: FileTreePane,         // Terminal-owned
    tree_watcher: FileSystemWatcher, // Terminal-owned
    git_cache: GitStatusCache,       // Terminal-owned
    renderer: WgpuRenderer,
}

impl ColonyTerminal {
    fn handle_tree_action(&mut self, action: TreeAction) {
        match action {
            TreeAction::CdTo(path) => {
                // Inject cd command into PTY
                self.pty.write(format!("cd {}\n", shell_quote(&path)));
            }
            TreeAction::Edit(path) => {
                // Inject $EDITOR command
                let editor = std::env::var("EDITOR").unwrap_or("vim".into());
                self.pty.write(format!("{} {}\n", editor, shell_quote(&path)));
            }
            // ...
        }
    }
    
    fn sync_with_shell_cwd(&mut self) {
        // Parse OSC 7 (current directory) escape sequences from shell
        // Update tree root to match
    }
}
```

### Pros

| Advantage | Why It Matters |
|-----------|----------------|
| **Works with any shell** | Users can use bash, zsh, fish, nushell AND get the tree pane |
| **Clean separation** | Terminal handles UI, shell handles commands |
| **Simpler shell** | Colony shell doesn't need TUI code |
| **Rich rendering** | Terminal already has GPU renderer, graphics protocols |
| **Warp-proven model** | Warp does this successfully |

### Cons

| Disadvantage | Why It Matters |
|--------------|----------------|
| **Agent blindness** | Shell-side agents don't know what's selected in the tree |
| **No tuple space integration** | Tree state can't participate in distributed state |
| **Duplicate watching** | If shell also watches files (for env reload), wasteful |
| **Limited shell integration** | Tree can only inject text into PTY, not call shell functions |
| **OSC parsing fragility** | Relying on escape sequences for cwd sync is brittle |

### Agent Context Problem

```rust
// In terminal-side model, agents in the shell have no tree context
impl ShellAgent {
    fn get_context(&self) -> AgentContext {
        AgentContext {
            cwd: self.shell.cwd(),
            history: self.shell.history(),
            environment: self.shell.env(),
            // tree_selection: ???  // Agent can't see this!
            // visible_files: ???   // Agent can't see this!
        }
    }
}
```

---

## Option 2: Shell-Side (Pure Shell)

The shell owns the file tree. Terminal just renders PTY output.

```
┌─────────────────────────────────────────────────────────────┐
│                     COLONY TERMINAL                         │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                                                         ││
│  │                    PTY Output                           ││
│  │              (Colony Shell renders tree)                ││
│  │                                                         ││
│  │  ┌─────────────┬──────────────────────────────────┐    ││
│  │  │ File Tree   │  $ cargo build                   │    ││
│  │  │ (Shell TUI) │    Compiling...                  │    ││
│  │  │             │  $ █                             │    ││
│  │  └─────────────┴──────────────────────────────────┘    ││
│  │                                                         ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### Pros

| Advantage | Why It Matters |
|-----------|----------------|
| **Full agent context** | Agents know exactly what user is looking at |
| **Tuple space integration** | Tree state can sync across sessions |
| **Direct shell integration** | Tree actions can call shell functions directly |
| **Single source of truth** | One file watcher, one git cache |
| **Works in any terminal** | Colony shell in iTerm2, Alacritty, etc. still has tree |

### Cons

| Disadvantage | Why It Matters |
|--------------|----------------|
| **Only works with Colony shell** | bash/zsh users get nothing |
| **Complex shell** | Shell becomes a full TUI application |
| **Limited rendering** | Escape sequences less capable than native GPU rendering |
| **Terminal duplication** | Shell reinvents terminal UI primitives |
| **Input handling complexity** | Shell must handle mouse, focus, resize |

---

## Option 3: Hybrid (Recommended) ⭐

The shell owns the **state**, the terminal owns the **rendering**. A protocol synchronizes them.

```
┌─────────────────────────────────────────────────────────────┐
│                     COLONY TERMINAL                         │
│  ┌─────────────────┬─────────────────────────────────────┐  │
│  │                 │                                     │  │
│  │   File Tree     │         PTY                         │  │
│  │   (Rendered by  │    ┌─────────────────────────────┐  │  │
│  │    Terminal)    │    │    COLONY SHELL             │  │  │
│  │                 │    │                             │  │  │
│  │   ◄─── Sync ───►│    │  - Tree STATE               │  │  │
│  │                 │    │  - File watching            │  │  │
│  │   State from    │    │  - Git status               │  │  │
│  │   shell via     │    │  - Agent context            │  │  │
│  │   protocol      │    │  - Tuple space              │  │  │
│  │                 │    │                             │  │  │
│  │                 │    └─────────────────────────────┘  │  │
│  └─────────────────┴─────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### The Protocol

Define a **Tree Sync Protocol** using escape sequences (OSC) or a side channel:

```rust
// Shell -> Terminal: State updates
pub enum TreeStateUpdate {
    /// Full tree snapshot (on connect)
    Snapshot(TreeSnapshot),
    
    /// Incremental updates
    NodeAdded { parent: PathBuf, node: FileNodeInfo },
    NodeRemoved { path: PathBuf },
    NodeModified { path: PathBuf, changes: NodeChanges },
    
    /// Selection/UI state
    CursorMoved { path: PathBuf },
    SelectionChanged { paths: Vec<PathBuf> },
    MarksChanged { paths: Vec<PathBuf> },
    FilterChanged { filter: Option<String> },
    
    /// Expansion state
    Expanded { path: PathBuf },
    Collapsed { path: PathBuf },
}

// Terminal -> Shell: User interactions
pub enum TreeUserAction {
    /// Navigation
    Navigate { path: PathBuf },
    Expand { path: PathBuf },
    Collapse { path: PathBuf },
    
    /// Selection
    Select { path: PathBuf },
    ToggleMark { path: PathBuf },
    
    /// Operations (shell executes these)
    Open { path: PathBuf },
    Delete { paths: Vec<PathBuf> },
    Rename { from: PathBuf, to: String },
    Create { parent: PathBuf, name: String, is_dir: bool },
    Move { sources: Vec<PathBuf>, dest: PathBuf },
    Copy { sources: Vec<PathBuf>, dest: PathBuf },
    
    /// Filter
    SetFilter { pattern: String },
    ClearFilter,
}
```

### Protocol Encoding Options

**Option A: Custom OSC Sequences**

```rust
// Shell sends tree updates as OSC sequences
// OSC 7777 ; <json-payload> ST

impl TreeProtocolEncoder {
    fn send(&self, update: TreeStateUpdate) {
        let json = serde_json::to_string(&update).unwrap();
        let encoded = base64::encode(&json);
        print!("\x1b]7777;{}\x1b\\", encoded);
    }
}
```

**Option B: Side Channel (Unix Socket)**

```rust
// More bandwidth, doesn't pollute PTY stream

pub struct TreeSideChannel {
    socket: UnixStream,  // /tmp/colony-tree-{pid}.sock
}
```

**Option C: Hybrid (Recommended)**

```rust
// Use OSC for small updates (selection, cursor)
// Use side channel for bulk data (full tree snapshot)

pub struct TreeProtocol {
    // Small, frequent updates via OSC (low latency)
    osc_encoder: OscEncoder,
    
    // Large, infrequent updates via socket (high bandwidth)
    socket: Option<UnixStream>,
}
```

### Pros

| Advantage | Why It Matters |
|-----------|----------------|
| **Best of both worlds** | Shell has context, terminal has rendering |
| **Full agent integration** | Agents know tree state (it's in the shell) |
| **Tuple space works** | Tree state can sync across sessions |
| **GPU rendering** | Terminal does what it's good at |
| **Graceful degradation** | Shell works in other terminals (just no tree pane) |
| **Separation of concerns** | Clear boundary between state and presentation |

### Cons

| Disadvantage | Why It Matters |
|--------------|----------------|
| **Protocol complexity** | Must design and maintain sync protocol |
| **State duplication** | Terminal caches shell's tree state |
| **Sync bugs possible** | State can diverge if protocol has issues |
| **Only full experience in Colony terminal** | Other terminals don't render the tree |

---

## Option 4: MCP-Based Integration

Use MCP (Model Context Protocol) as the integration layer. The shell exposes tree state as MCP resources, terminal consumes them.

```rust
// === SHELL AS MCP SERVER ===

impl ColonyShell {
    fn register_tree_mcp(&mut self, server: &mut McpServer) {
        // Resource: Current tree state
        server.register_resource(Resource {
            uri: "tree://state".into(),
            name: "File Tree State".into(),
            handler: Box::new(|| {
                serde_json::to_value(&self.tree_state)
            }),
        });
        
        // Tools for tree operations
        server.register_tool(Tool {
            name: "tree_navigate".into(),
            description: "Navigate to a path in the file tree".into(),
            handler: Box::new(|params| {
                let path = params["path"].as_str().unwrap();
                self.tree_state.navigate(Path::new(path));
                Ok(json!({"success": true}))
            }),
        });
    }
}
```

### Pros

| Advantage | Why It Matters |
|-----------|----------------|
| **Standard protocol** | MCP is well-defined, has tooling |
| **AI agents work naturally** | Same protocol for terminal and AI agents |
| **Extensible** | Add new tree capabilities as MCP tools |
| **Already building MCP** | Colony already uses MCP for agents |

### Cons

| Disadvantage | Why It Matters |
|--------------|----------------|
| **Overhead** | MCP is JSON-RPC, more verbose than binary protocol |
| **Latency** | Request/response model less efficient for streaming |

---

## Recommendation: Hybrid with MCP Inspiration

Combine the **Hybrid approach** (Option 3) with **MCP-style semantics** (Option 4):

1. **Shell owns state** — TreeState, FileWatcher, GitCache, AgentContext
2. **Binary protocol for terminal** — Fast, efficient sync via side channel
3. **MCP tools for external access** — AI agents, plugins, remote clients use MCP
4. **Graceful degradation** — Works (sans tree pane) in other terminals

```
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│   ┌──────────────────────────────────────────────────────────────┐    │
│   │                      COLONY SHELL                             │    │
│   │                                                               │    │
│   │   TreeState ◄─── FileWatcher                                  │    │
│   │       │              │                                        │    │
│   │       ▼              ▼                                        │    │
│   │   AgentContext    GitCache                                    │    │
│   │       │                                                       │    │
│   │       ├──────────► MCP Server (for AI agents, plugins)        │    │
│   │       │                                                       │    │
│   │       └──────────► TreeProtocol (for terminal)                │    │
│   │                         │                                     │    │
│   └─────────────────────────┼─────────────────────────────────────┘    │
│                             │                                          │
│                    Binary Side Channel                                 │
│                             │                                          │
│   ┌─────────────────────────▼─────────────────────────────────────┐   │
│   │                    COLONY TERMINAL                             │   │
│   │                                                                │   │
│   │   TreeStateCache ◄─── TreeProtocol                             │   │
│   │         │                                                      │   │
│   │         ▼                                                      │   │
│   │   TreeRenderer ───► GPU (wgpu)                                 │   │
│   │                                                                │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### File Structure

```
colony/
├── colony-shell/
│   ├── src/
│   │   ├── tree/
│   │   │   ├── state.rs        # TreeState, FileNode
│   │   │   ├── watcher.rs      # FileSystemWatcher
│   │   │   ├── git.rs          # GitStatusCache
│   │   │   ├── protocol.rs     # TreeProtocol encoder
│   │   │   └── mcp.rs          # MCP tool/resource definitions
│   │   ├── agent/
│   │   │   └── context.rs      # AgentContext includes tree
│   │   └── ...
│
├── colony-terminal/
│   ├── src/
│   │   ├── tree/
│   │   │   ├── cache.rs        # TreeStateCache (mirror)
│   │   │   ├── renderer.rs     # TreeRenderer (GPU)
│   │   │   ├── input.rs        # Mouse/keyboard handling
│   │   │   └── protocol.rs     # TreeProtocol decoder
│   │   ├── renderer/
│   │   │   └── wgpu.rs         # GPU rendering
│   │   └── ...
│
├── colony-tree-protocol/        # Shared protocol crate
│   ├── src/
│   │   ├── messages.rs         # TreeStateUpdate, TreeUserAction
│   │   ├── encoding.rs         # Binary encoding
│   │   └── lib.rs
│
└── colony-core/                 # Shared types
    ├── src/
    │   ├── tree_types.rs       # FileNode, GitStatus, etc.
    │   └── ...
```

---

## Decision Matrix

| Criterion | Terminal-Side | Shell-Side | Hybrid | MCP |
|-----------|---------------|------------|--------|-----|
| Agent context | ❌ None | ✅ Full | ✅ Full | ✅ Full |
| Tuple space | ❌ No | ✅ Yes | ✅ Yes | ✅ Yes |
| GPU rendering | ✅ Native | ⚠️ Limited | ✅ Native | ✅ Native |
| Works with bash/zsh | ✅ Yes | ❌ No | ⚠️ Degraded | ⚠️ Degraded |
| Works in other terminals | N/A | ✅ Yes | ⚠️ No tree | ⚠️ No tree |
| Complexity | Low | Medium | High | High |
| Latency | ✅ Low | ✅ Low | ✅ Low | ⚠️ Medium |

**Recommendation: Hybrid** — gives you everything Colony needs:
- Full agent context (tree state in shell)
- GPU rendering (terminal does what it's good at)  
- Tuple space integration (sync across sessions)
- Clean architecture (protocol boundary)

---

## Implementation Phases

### Phase 1: Shell-Side State
- Implement TreeState in colony-shell
- File watching with notify
- Git status integration
- Agent context includes tree

### Phase 2: Protocol Design
- Define TreeStateUpdate and TreeUserAction messages
- Implement binary encoding (bincode/msgpack)
- Side channel via Unix socket

### Phase 3: Terminal Renderer
- TreeStateCache mirrors shell state
- GPU-accelerated tree rendering
- Mouse/keyboard input handling
- Send actions back to shell

### Phase 4: MCP Bridge
- Expose tree as MCP resources
- Tree operations as MCP tools
- External clients (AI agents, plugins) can access

### Phase 5: Fallback TUI
- Optional escape-sequence-based tree for other terminals
- Reduced functionality but still useful
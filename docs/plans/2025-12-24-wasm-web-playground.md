# Colony Web Playground - WASM Browser Implementation

**Status:** Design Document  
**Created:** 2025-12-24  
**Author:** Claude + mcfearsome  
**Priority:** Phase 2 (Post-Launch Feature)

## Executive Summary

Build a browser-based Colony Shell playground that runs entirely client-side via WebAssembly. Users can learn shell scripting, experiment with commands, and run the Tutorial Mode without installing anything. This serves as both a marketing tool and educational platform.

## Goals

1. **Zero Installation** - Run Colony in any modern browser
2. **Educational Focus** - Tutorial Mode as primary use case
3. **Realistic Experience** - Actual shell execution, not simulation
4. **Offline Capable** - PWA with ServiceWorker caching
5. **Monetization Ready** - Premium features, course integration

## Non-Goals

1. NOT a production shell replacement
2. NOT network-capable (no curl, ssh, etc.)
3. NOT full POSIX compliance (practical subset)
4. NOT mobile-first (desktop browsers primary target)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Browser Tab                                                            │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Web UI (TypeScript + React/Solid)                                │  │
│  │  ├── Terminal Emulator (xterm.js)                                 │  │
│  │  ├── File Tree Panel                                              │  │
│  │  ├── Tutorial Mode Panes                                          │  │
│  │  └── Editor (Monaco/CodeMirror)                                   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                              │                                          │
│                              ▼                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Colony WASM Core (Main Thread)                                   │  │
│  │  ├── colony-parser.wasm                                           │  │
│  │  ├── colony-expand.wasm                                           │  │
│  │  ├── colony-vfs.wasm (Virtual Filesystem)                         │  │
│  │  └── colony-syscall.wasm (Syscall Router)                         │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                              │                                          │
│         ┌────────────────────┼────────────────────┐                     │
│         ▼                    ▼                    ▼                     │
│  ┌─────────────┐      ┌─────────────┐      ┌─────────────┐              │
│  │  Worker 1   │      │  Worker 2   │      │  Worker N   │              │
│  │  (pid 1)    │      │  (pid 2)    │      │  (pid N)    │              │
│  │  colony.wasm│      │  grep.wasm  │      │  cat.wasm   │              │
│  └─────────────┘      └─────────────┘      └─────────────┘              │
│         │                    │                    │                     │
│         └────────────────────┴────────────────────┘                     │
│                              │                                          │
│                    SharedArrayBuffer (Pipes)                            │
│                              │                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Storage Layer                                                    │  │
│  │  ├── Origin Private File System (OPFS) - Fast, Persistent         │  │
│  │  ├── IndexedDB - Fallback, Metadata                               │  │
│  │  └── localStorage - Settings, Preferences                         │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Component Design

### 1. Virtual Filesystem (`colony-vfs`)

A POSIX-like filesystem abstraction that works entirely in-browser.

#### Storage Backends

| Backend | Speed | Persistence | Size Limit | Use Case |
|---------|-------|-------------|------------|----------|
| **MemFS** | Fastest | None | RAM | Temp files, pipes |
| **OPFS** | Fast | Yes | Quota (~10%+ of disk) | Primary storage |
| **IndexedDB** | Slow | Yes | Quota | Fallback, metadata |

#### API Design

```rust
// crates/colony-vfs/src/lib.rs

pub trait FileSystem: Send + Sync {
    // File operations
    fn open(&self, path: &Path, flags: OpenFlags) -> Result<Fd>;
    fn close(&self, fd: Fd) -> Result<()>;
    fn read(&self, fd: Fd, buf: &mut [u8]) -> Result<usize>;
    fn write(&self, fd: Fd, buf: &[u8]) -> Result<usize>;
    fn seek(&self, fd: Fd, offset: i64, whence: Whence) -> Result<u64>;
    fn fstat(&self, fd: Fd) -> Result<Stat>;
    fn ftruncate(&self, fd: Fd, length: u64) -> Result<()>;
    
    // Path operations
    fn stat(&self, path: &Path) -> Result<Stat>;
    fn lstat(&self, path: &Path) -> Result<Stat>;  // No symlink follow
    fn readlink(&self, path: &Path) -> Result<PathBuf>;
    fn unlink(&self, path: &Path) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn link(&self, target: &Path, linkpath: &Path) -> Result<()>;
    fn symlink(&self, target: &Path, linkpath: &Path) -> Result<()>;
    
    // Directory operations
    fn mkdir(&self, path: &Path, mode: Mode) -> Result<()>;
    fn rmdir(&self, path: &Path) -> Result<()>;
    fn readdir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    
    // Permissions (simplified - no real users in browser)
    fn chmod(&self, path: &Path, mode: Mode) -> Result<()>;
    fn access(&self, path: &Path, mode: AccessMode) -> Result<bool>;
}

pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: Mode,
    pub nlink: u64,
    pub uid: u32,      // Always 1000 in browser
    pub gid: u32,      // Always 1000 in browser
    pub size: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub file_type: FileType,
}

pub enum FileType {
    Regular,
    Directory,
    Symlink,
    CharDevice,  // /dev/null, /dev/zero, /dev/urandom
    Fifo,        // Named pipes
}
```

#### Inode Structure

```rust
pub struct Inode {
    pub ino: u64,
    pub file_type: FileType,
    pub mode: Mode,
    pub nlink: u32,
    pub size: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub data: InodeData,
}

pub enum InodeData {
    Regular(Vec<u8>),                    // File contents (MemFS)
    RegularOpfs(OpfsHandle),             // OPFS file handle
    Directory(HashMap<String, u64>),     // name -> child inode
    Symlink(PathBuf),                    // Target path
    CharDevice(DeviceType),              // Special device
    Fifo(Arc<Pipe>),                     // Pipe buffer
}
```

#### Initial Filesystem Layout

```
/
├── bin/                    # WASM "executables"
│   ├── cat.wasm
│   ├── echo.wasm
│   ├── ls.wasm
│   ├── grep.wasm
│   ├── head.wasm
│   ├── tail.wasm
│   ├── wc.wasm
│   ├── sort.wasm
│   ├── uniq.wasm
│   ├── tr.wasm
│   ├── cut.wasm
│   ├── sed.wasm
│   ├── awk.wasm
│   ├── mkdir.wasm
│   ├── rm.wasm
│   ├── cp.wasm
│   ├── mv.wasm
│   ├── touch.wasm
│   ├── chmod.wasm
│   ├── test.wasm           # [ and [[ implementation
│   ├── true.wasm
│   ├── false.wasm
│   ├── sleep.wasm
│   ├── date.wasm
│   ├── env.wasm
│   ├── printenv.wasm
│   ├── seq.wasm
│   ├── yes.wasm
│   ├── tee.wasm
│   ├── xargs.wasm
│   └── colony.wasm         # The shell itself
├── dev/
│   ├── null                # Discards writes, EOF on read
│   ├── zero                # Infinite zeros on read
│   ├── urandom             # Pseudo-random bytes
│   ├── stdin -> /proc/self/fd/0
│   ├── stdout -> /proc/self/fd/1
│   └── stderr -> /proc/self/fd/2
├── etc/
│   ├── passwd              # Fake passwd for getpwnam()
│   ├── group               # Fake group file
│   └── profile             # Login shell profile
├── home/
│   └── user/               # User's home directory ($HOME)
│       ├── .bashrc
│       └── .profile
├── proc/
│   └── self/
│       ├── fd/             # Open file descriptors (virtual)
│       ├── cwd -> /home/user
│       └── environ         # Environment variables
├── tmp/                    # Temporary files (MemFS, cleared on reload)
└── usr/
    └── share/
        └── colony/
            ├── tutorials/  # Built-in tutorial scripts
            │   ├── 01-basics.sh
            │   ├── 02-variables.sh
            │   ├── 03-pipelines.sh
            │   └── ...
            └── examples/   # Example scripts
```

---

### 2. Process Emulation (`colony-process`)

Emulate Unix process model using Web Workers.

#### Process Table

```rust
// crates/colony-process/src/lib.rs

pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub sid: u32,           // Session ID
    pub state: ProcessState,
    pub exit_code: Option<i32>,
    
    // File descriptors
    pub fd_table: FdTable,
    
    // Environment
    pub env: HashMap<String, String>,
    pub cwd: PathBuf,
    pub umask: Mode,
    
    // Execution
    pub argv: Vec<String>,
    pub worker: Option<WorkerHandle>,
    
    // Resource tracking
    pub children: Vec<u32>,
    pub start_time: Instant,
}

pub enum ProcessState {
    Running,
    Sleeping,       // Waiting for I/O
    Stopped,        // SIGSTOP/SIGTSTP
    Zombie,         // Exited, waiting for parent to wait()
}

pub struct ProcessTable {
    processes: HashMap<u32, Process>,
    next_pid: AtomicU32,
    
    // Process groups for job control
    process_groups: HashMap<u32, Vec<u32>>,  // pgid -> [pids]
    
    // Sessions
    sessions: HashMap<u32, Session>,
}
```

#### File Descriptor Table

```rust
pub struct FdTable {
    descriptors: HashMap<Fd, FileDescriptor>,
    next_fd: Fd,
}

pub struct FileDescriptor {
    pub flags: FdFlags,      // O_CLOEXEC, etc.
    pub target: FdTarget,
}

pub enum FdTarget {
    File {
        inode: u64,
        position: u64,
        flags: OpenFlags,
    },
    Pipe {
        pipe: Arc<Pipe>,
        direction: PipeDirection,
    },
    Terminal {
        // PTY emulation for xterm.js integration
    },
    Closed,
}

pub enum PipeDirection {
    Read,
    Write,
}
```

#### Fork Implementation

```rust
impl ProcessTable {
    pub fn fork(&mut self, parent_pid: u32) -> Result<ForkResult> {
        let parent = self.processes.get(&parent_pid)
            .ok_or(Error::NoSuchProcess)?;
        
        let child_pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
        
        // Clone process state (COW for fd_table)
        let child = Process {
            pid: child_pid,
            ppid: parent_pid,
            pgid: parent.pgid,
            sid: parent.sid,
            state: ProcessState::Running,
            exit_code: None,
            fd_table: parent.fd_table.clone(),  // Rc<RefCell<...>> for COW
            env: parent.env.clone(),
            cwd: parent.cwd.clone(),
            umask: parent.umask,
            argv: parent.argv.clone(),
            worker: None,  // No worker until exec()
            children: vec![],
            start_time: Instant::now(),
        };
        
        // Update parent's children list
        self.processes.get_mut(&parent_pid)
            .unwrap()
            .children
            .push(child_pid);
        
        self.processes.insert(child_pid, child);
        
        Ok(ForkResult::Child(child_pid))
    }
}
```

#### Exec Implementation

```rust
impl ProcessTable {
    pub async fn exec(
        &mut self,
        pid: u32,
        path: &Path,
        argv: &[String],
        vfs: &dyn FileSystem,
    ) -> Result<()> {
        let process = self.processes.get_mut(&pid)
            .ok_or(Error::NoSuchProcess)?;
        
        // Check if executable exists and is WASM
        let stat = vfs.stat(path)?;
        if !stat.mode.is_executable() {
            return Err(Error::PermissionDenied);
        }
        
        // Load WASM binary
        let wasm_bytes = vfs.read_all(path)?;
        
        // Close O_CLOEXEC file descriptors
        process.fd_table.close_cloexec();
        
        // Update argv
        process.argv = argv.to_vec();
        
        // Terminate existing worker if any
        if let Some(worker) = process.worker.take() {
            worker.terminate();
        }
        
        // Spawn new Web Worker with WASM module
        let worker = spawn_wasm_worker(
            &wasm_bytes,
            pid,
            &process.env,
            &process.argv,
            &process.fd_table,
        ).await?;
        
        process.worker = Some(worker);
        process.state = ProcessState::Running;
        
        Ok(())
    }
}
```

---

### 3. Pipe Implementation

Inter-process communication via SharedArrayBuffer.

```rust
// crates/colony-process/src/pipe.rs

pub struct Pipe {
    buffer: SharedArrayBuffer,
    
    // Atomics for lock-free coordination
    read_pos: AtomicU32,
    write_pos: AtomicU32,
    
    // State flags
    read_closed: AtomicBool,
    write_closed: AtomicBool,
    
    capacity: u32,
}

impl Pipe {
    pub fn new(capacity: u32) -> Self {
        // SharedArrayBuffer must be power of 2 for ring buffer
        let capacity = capacity.next_power_of_two();
        
        // Allocate SharedArrayBuffer (requires COOP/COEP headers)
        let buffer = SharedArrayBuffer::new(capacity + 16);  // +16 for atomics
        
        Self {
            buffer,
            read_pos: AtomicU32::new(0),
            write_pos: AtomicU32::new(0),
            read_closed: AtomicBool::new(false),
            write_closed: AtomicBool::new(false),
            capacity,
        }
    }
    
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        if self.read_closed.load(Ordering::Acquire) {
            return Err(Error::BrokenPipe);  // SIGPIPE equivalent
        }
        
        let mut written = 0;
        
        for &byte in data {
            loop {
                let write_pos = self.write_pos.load(Ordering::Acquire);
                let read_pos = self.read_pos.load(Ordering::Acquire);
                
                // Check if buffer is full
                if write_pos.wrapping_sub(read_pos) >= self.capacity {
                    // Buffer full - would block
                    if written > 0 {
                        return Ok(written);  // Return what we wrote
                    }
                    // Wait for reader (Atomics.wait in JS)
                    self.wait_for_reader()?;
                    continue;
                }
                
                // Write byte to ring buffer
                let index = (write_pos % self.capacity) as usize;
                self.buffer.set(index, byte);
                
                // Advance write position
                self.write_pos.store(write_pos.wrapping_add(1), Ordering::Release);
                written += 1;
                
                // Notify waiting readers
                self.notify_reader();
                break;
            }
        }
        
        Ok(written)
    }
    
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut read = 0;
        
        for slot in buf.iter_mut() {
            loop {
                let read_pos = self.read_pos.load(Ordering::Acquire);
                let write_pos = self.write_pos.load(Ordering::Acquire);
                
                // Check if buffer is empty
                if read_pos == write_pos {
                    if self.write_closed.load(Ordering::Acquire) {
                        return Ok(read);  // EOF
                    }
                    if read > 0 {
                        return Ok(read);  // Return what we have
                    }
                    // Wait for writer
                    self.wait_for_writer()?;
                    continue;
                }
                
                // Read byte from ring buffer
                let index = (read_pos % self.capacity) as usize;
                *slot = self.buffer.get(index);
                
                // Advance read position
                self.read_pos.store(read_pos.wrapping_add(1), Ordering::Release);
                read += 1;
                
                // Notify waiting writers
                self.notify_writer();
                break;
            }
        }
        
        Ok(read)
    }
    
    pub fn close_read(&self) {
        self.read_closed.store(true, Ordering::Release);
        self.notify_writer();  // Wake blocked writers
    }
    
    pub fn close_write(&self) {
        self.write_closed.store(true, Ordering::Release);
        self.notify_reader();  // Wake blocked readers (EOF)
    }
}
```

---

### 4. Syscall Emulation Layer (`colony-syscall`)

Central router for all syscall-like operations.

```rust
// crates/colony-syscall/src/lib.rs

pub struct SyscallHandler {
    vfs: Arc<dyn FileSystem>,
    process_table: Arc<Mutex<ProcessTable>>,
}

impl SyscallHandler {
    // Process management
    pub fn fork(&self, pid: u32) -> Result<u32>;
    pub fn exec(&self, pid: u32, path: &Path, argv: &[String]) -> Result<()>;
    pub fn exit(&self, pid: u32, code: i32) -> Result<()>;
    pub fn wait(&self, pid: u32, options: WaitOptions) -> Result<WaitResult>;
    pub fn getpid(&self, pid: u32) -> u32;
    pub fn getppid(&self, pid: u32) -> u32;
    
    // File operations
    pub fn open(&self, pid: u32, path: &Path, flags: OpenFlags) -> Result<Fd>;
    pub fn close(&self, pid: u32, fd: Fd) -> Result<()>;
    pub fn read(&self, pid: u32, fd: Fd, buf: &mut [u8]) -> Result<usize>;
    pub fn write(&self, pid: u32, fd: Fd, buf: &[u8]) -> Result<usize>;
    pub fn lseek(&self, pid: u32, fd: Fd, offset: i64, whence: Whence) -> Result<u64>;
    pub fn fstat(&self, pid: u32, fd: Fd) -> Result<Stat>;
    pub fn dup(&self, pid: u32, old_fd: Fd) -> Result<Fd>;
    pub fn dup2(&self, pid: u32, old_fd: Fd, new_fd: Fd) -> Result<Fd>;
    pub fn pipe(&self, pid: u32) -> Result<(Fd, Fd)>;
    
    // Directory operations
    pub fn chdir(&self, pid: u32, path: &Path) -> Result<()>;
    pub fn getcwd(&self, pid: u32) -> Result<PathBuf>;
    pub fn mkdir(&self, pid: u32, path: &Path, mode: Mode) -> Result<()>;
    pub fn rmdir(&self, pid: u32, path: &Path) -> Result<()>;
    
    // Environment
    pub fn getenv(&self, pid: u32, name: &str) -> Option<String>;
    pub fn setenv(&self, pid: u32, name: &str, value: &str) -> Result<()>;
    pub fn unsetenv(&self, pid: u32, name: &str) -> Result<()>;
}
```

---

### 5. WASM Utilities

Each utility is a standalone WASM module implementing a simple interface.

#### Utility Interface

```rust
// Shared interface all utilities implement

#[wasm_bindgen]
pub struct WasmProcess {
    stdin: Fd,
    stdout: Fd,
    stderr: Fd,
    args: Vec<String>,
    env: HashMap<String, String>,
    syscall: SyscallProxy,  // Talks to main thread
}

#[wasm_bindgen]
impl WasmProcess {
    pub fn main(&mut self) -> i32;  // Return exit code
}
```

#### Example: cat.wasm

```rust
// crates/colony-utils/src/bin/cat.rs

use colony_wasm_util::prelude::*;

#[wasm_bindgen]
pub fn main(process: &mut WasmProcess) -> i32 {
    let args: Vec<_> = process.args().skip(1).collect();
    
    if args.is_empty() {
        // Read from stdin
        if let Err(e) = copy_fd(process.stdin(), process.stdout()) {
            eprintln!("cat: {}", e);
            return 1;
        }
    } else {
        for path in args {
            match process.open(&path, O_RDONLY) {
                Ok(fd) => {
                    if let Err(e) = copy_fd(fd, process.stdout()) {
                        eprintln!("cat: {}: {}", path, e);
                    }
                    process.close(fd);
                }
                Err(e) => {
                    eprintln!("cat: {}: {}", path, e);
                    return 1;
                }
            }
        }
    }
    
    0
}

fn copy_fd(src: Fd, dst: Fd) -> Result<()> {
    let mut buf = [0u8; 4096];
    loop {
        let n = process.read(src, &mut buf)?;
        if n == 0 { break; }
        process.write_all(dst, &buf[..n])?;
    }
    Ok(())
}
```

#### Example: grep.wasm

```rust
// crates/colony-utils/src/bin/grep.rs

use colony_wasm_util::prelude::*;
use regex::Regex;

#[wasm_bindgen]
pub fn main(process: &mut WasmProcess) -> i32 {
    let mut args = process.args().skip(1);
    
    let pattern = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("grep: missing pattern");
            return 2;
        }
    };
    
    let regex = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("grep: invalid pattern: {}", e);
            return 2;
        }
    };
    
    let files: Vec<_> = args.collect();
    let mut found = false;
    let show_filename = files.len() > 1;
    
    let inputs: Vec<(Option<String>, Fd)> = if files.is_empty() {
        vec![(None, process.stdin())]
    } else {
        files.iter().filter_map(|path| {
            match process.open(path, O_RDONLY) {
                Ok(fd) => Some((Some(path.clone()), fd)),
                Err(e) => {
                    eprintln!("grep: {}: {}", path, e);
                    None
                }
            }
        }).collect()
    };
    
    for (filename, fd) in inputs {
        let reader = BufReader::new(FdReader::new(fd));
        for line in reader.lines() {
            if let Ok(line) = line {
                if regex.is_match(&line) {
                    found = true;
                    if let Some(ref name) = filename {
                        if show_filename {
                            print!("{}:", name);
                        }
                    }
                    println!("{}", line);
                }
            }
        }
        if filename.is_some() {
            process.close(fd);
        }
    }
    
    if found { 0 } else { 1 }
}
```

---

### 6. Web UI Integration

#### Terminal Emulator (xterm.js)

```typescript
// web/src/terminal.ts

import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import { WebLinksAddon } from 'xterm-addon-web-links';

export class ColonyTerminal {
    private terminal: Terminal;
    private shell: ColonyShell;  // WASM bindings
    
    constructor(container: HTMLElement) {
        this.terminal = new Terminal({
            cursorBlink: true,
            fontSize: 14,
            fontFamily: 'JetBrains Mono, monospace',
            theme: {
                background: '#1e1e2e',
                foreground: '#cdd6f4',
                // ... catppuccin mocha theme
            }
        });
        
        this.terminal.loadAddon(new FitAddon());
        this.terminal.loadAddon(new WebLinksAddon());
        this.terminal.open(container);
        
        // Connect to Colony WASM
        this.shell = new ColonyShell();
        
        // Wire I/O
        this.terminal.onData(data => this.shell.write_stdin(data));
        this.shell.on_stdout(data => this.terminal.write(data));
        this.shell.on_stderr(data => this.terminal.write(data));
    }
    
    async init() {
        await this.shell.init();
        this.shell.start();
    }
}
```

#### Tutorial Mode UI

```typescript
// web/src/tutorial.ts

export class TutorialMode {
    private scriptPane: ScriptPane;
    private magicPane: MagicPane;
    private envPane: EnvironmentPane;
    private outputPane: OutputPane;
    private shell: ColonyShell;
    
    constructor(container: HTMLElement) {
        // Create 4-pane layout
        this.scriptPane = new ScriptPane();
        this.magicPane = new MagicPane();
        this.envPane = new EnvironmentPane();
        this.outputPane = new OutputPane();
        
        // Subscribe to trace events from WASM
        this.shell.on_trace_event(event => {
            switch (event.type) {
                case 'line_start':
                    this.scriptPane.highlight_line(event.line);
                    break;
                case 'ast':
                    this.magicPane.show_ast(event.ast);
                    break;
                case 'expansion':
                    this.magicPane.show_expansion(event.from, event.to);
                    break;
                case 'environment':
                    this.envPane.update(event.variables, event.changed);
                    break;
                case 'output':
                    this.outputPane.append(event.text);
                    break;
                case 'concept':
                    this.magicPane.show_concept(event.concept, event.explanation);
                    break;
            }
        });
    }
    
    async loadScript(source: string) {
        this.scriptPane.set_source(source);
        await this.shell.tutorial_load(source);
    }
    
    step() {
        this.shell.tutorial_step();
    }
    
    play() {
        this.shell.tutorial_play();
    }
    
    pause() {
        this.shell.tutorial_pause();
    }
}
```

---

## Security Considerations

### Browser Security Model

1. **Same-Origin Policy** - All filesystem access is sandboxed to origin
2. **No Network Access** - Cannot make HTTP requests from WASM utilities
3. **No Native Code** - Only WASM modules, no arbitrary execution
4. **Quota Limits** - OPFS/IndexedDB subject to browser storage quotas

### Required HTTP Headers

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

These headers are required for SharedArrayBuffer (pipes). Without them, fall back to single-process mode with MessageChannel pipes (slower).

### Content Security Policy

```
Content-Security-Policy: 
    default-src 'self';
    script-src 'self' 'wasm-unsafe-eval';
    worker-src 'self' blob:;
    style-src 'self' 'unsafe-inline';
```

---

## Performance Considerations

### Benchmarks (Estimated)

| Operation | Native | WASM (estimated) | Notes |
|-----------|--------|------------------|-------|
| Parse 1KB script | <1ms | 2-5ms | pest in WASM |
| fork() | <1ms | 10-50ms | Worker spawn |
| pipe write 1KB | <0.1ms | 1-5ms | SharedArrayBuffer |
| File read 1MB | 1ms | 10-50ms | OPFS |
| grep 10K lines | 50ms | 200-500ms | regex in WASM |

### Optimization Strategies

1. **Worker Pool** - Pre-spawn Workers, reuse for exec()
2. **WASM Caching** - Cache compiled modules in IndexedDB
3. **Batch Syscalls** - Reduce main thread round-trips
4. **Streaming Parse** - Parse scripts incrementally
5. **Lazy Loading** - Load utilities on first use

---

## Implementation Phases

### Phase 1: Parser Playground (4 weeks)

**Goal:** Parse-only mode, no execution

- [ ] Compile colony-parser to WASM
- [ ] Compile colony-expand to WASM (without command substitution)
- [ ] Basic web UI with Monaco editor
- [ ] AST visualization
- [ ] Syntax error display
- [ ] Variable expansion preview

**Deliverable:** "Colony Parser Playground" - paste script, see AST

### Phase 2: Virtual Filesystem (4 weeks)

**Goal:** Full VFS with persistence

- [ ] MemFS implementation
- [ ] OPFS backend
- [ ] IndexedDB fallback
- [ ] Initial filesystem layout
- [ ] File tree UI component
- [ ] Basic editor integration

**Deliverable:** Can create/edit/delete files in browser

### Phase 3: Single-Process Execution (4 weeks)

**Goal:** Run shell without fork/exec

- [ ] colony-syscall layer (single-process mode)
- [ ] Builtins work with VFS
- [ ] Terminal emulator integration
- [ ] Basic REPL functionality
- [ ] No external commands yet

**Deliverable:** Interactive shell (builtins only)

### Phase 4: Core Utilities (4 weeks)

**Goal:** Common utilities as WASM

- [ ] cat, echo, ls, grep, head, tail, wc
- [ ] sort, uniq, tr, cut
- [ ] mkdir, rm, cp, mv, touch
- [ ] Worker-based utility execution
- [ ] Single-threaded "exec" (no true fork)

**Deliverable:** Can run `ls | grep foo | wc -l`

### Phase 5: Multi-Process (4 weeks)

**Goal:** True fork/exec with pipes

- [ ] Process table implementation
- [ ] SharedArrayBuffer pipes
- [ ] Worker per process
- [ ] Proper job control
- [ ] Background processes (&)

**Deliverable:** Full pipeline execution

### Phase 6: Tutorial Mode (3 weeks)

**Goal:** Port Tutorial Mode to web

- [ ] Web-based 4-pane layout
- [ ] Trace event streaming
- [ ] Concept explanations
- [ ] Step/play/pause controls
- [ ] Built-in tutorials

**Deliverable:** "Learn Shell" interactive tutorials

### Phase 7: Polish & Launch (3 weeks)

**Goal:** Production-ready

- [ ] PWA setup (offline support)
- [ ] Performance optimization
- [ ] Error handling
- [ ] Analytics integration
- [ ] Documentation
- [ ] Landing page

**Deliverable:** Public launch at play.colony.sh

---

## Monetization Strategy

### Free Tier
- Basic shell playground
- 5 built-in tutorials
- 10MB filesystem
- Community support

### Pro Tier ($9/month)
- All tutorials (20+)
- 100MB filesystem
- Cloud sync (filesystem backup)
- Custom themes
- No ads

### Team Tier ($29/month)
- Everything in Pro
- Shared filesystems
- Custom tutorials
- Priority support
- Embed on your site

### Enterprise
- Self-hosted option
- SSO integration
- Custom branding
- SLA

---

## Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| SharedArrayBuffer restrictions | Medium | High | Fallback to MessageChannel |
| WASM performance issues | Medium | Medium | Optimize hot paths, cache |
| Browser compatibility | Low | High | Target Chrome/Firefox/Safari |
| Storage quota limits | Low | Medium | Compress, warn user |
| Complex shell scripts fail | High | Low | Document limitations |

---

## Success Metrics

1. **Adoption**
   - 10K monthly active users within 6 months
   - 1K tutorial completions per month

2. **Engagement**
   - Average session > 10 minutes
   - 30% return rate

3. **Conversion**
   - 2% free → paid conversion
   - <5% monthly churn

4. **Technical**
   - <3s initial load time
   - >95% command compatibility (vs native)
   - <1% crash rate

---

## Open Questions

1. **Networking?** - Should we add fetch() as a builtin? (Security implications)
2. **File Upload/Download?** - Drag-and-drop files into VFS?
3. **Collaboration?** - Real-time shared sessions via WebRTC?
4. **Mobile?** - Touch-friendly UI? On-screen keyboard?
5. **Embed API?** - Let others embed Colony in their docs/courses?

---

## Appendix: Prior Art Deep Dive

### WebContainers (StackBlitz)

- Full Node.js in browser
- Custom filesystem layer
- Process emulation
- Closed source, but proves feasibility
- Key insight: They made npm work in browser

### Browsix

- Academic project (SOSP 2016)
- Fork via postMessage + structured clone
- Pipes via SharedArrayBuffer
- Open source, but unmaintained
- Key insight: Web Workers can approximate processes

### copy/v86

- Full x86 emulator in JS/WASM
- Runs actual Linux kernel
- Way overkill for our needs
- Key insight: You can emulate anything (slowly)

### pyte/xterm.js

- Terminal emulation is solved
- xterm.js is battle-tested
- Just need to wire it up

---

## Appendix: File Format for WASM Utilities

Each utility WASM module follows a standard format:

```
┌────────────────────────────────────────┐
│  WASM Module                           │
├────────────────────────────────────────┤
│  Imports:                              │
│    - syscall_open(path, flags) -> fd   │
│    - syscall_read(fd, buf, len) -> n   │
│    - syscall_write(fd, buf, len) -> n  │
│    - syscall_close(fd) -> result       │
│    - syscall_exit(code)                │
│    - ... (full syscall table)          │
├────────────────────────────────────────┤
│  Exports:                              │
│    - memory: WebAssembly.Memory        │
│    - _start() -> i32 (entry point)     │
├────────────────────────────────────────┤
│  Custom Sections:                      │
│    - colony:metadata (JSON)            │
│      {                                 │
│        "name": "grep",                 │
│        "version": "1.0.0",             │
│        "description": "...",           │
│        "usage": "grep [options] pat"   │
│      }                                 │
└────────────────────────────────────────┘
```

This allows the shell to introspect utilities for help text, completion, etc.

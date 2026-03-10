# Colony Shell Auto-Update

## Goal

Check GitHub releases on startup (once per day), notify if a newer version exists, and provide `colony update` to download and replace the binary.

## Components

### 1. Version Check (startup, background)

On interactive shell startup, spawn a tokio background task:

- Hit the GitHub releases API: `GET https://api.github.com/repos/colony-sh/colony-shell/releases/latest`
- Compare the release tag (e.g. `v0.5.0`) against the compiled-in version (`env!("CARGO_PKG_VERSION")`)
- **Cooldown:** Store last check timestamp in `~/.colony/update-check`. Skip if checked within 24 hours.
- If a newer version is found, print one line: `colony v0.5.0 available — run 'colony update' to install`
- Failures are completely silent — no network means no notification.

### 2. `colony update` Builtin

- Determine current platform target triple (e.g. `aarch64-apple-darwin`)
- Fetch latest release metadata from GitHub API
- Find the matching asset: `colony-{version}-{target}.tar.gz`
- Download to a temp file, extract the tarball
- Replace all binaries in the same directory as `std::env::current_exe()` (colony + panel binaries)
- Print result: `Updated colony v0.4.1 → v0.5.0`

### 3. Release Workflow

Expand `.github/workflows/publish.yml` to build on a 4-target matrix:

| Target | Runner |
|--------|--------|
| `aarch64-apple-darwin` | `macos-latest` |
| `x86_64-apple-darwin` | `macos-13` |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` (cross-compile) |

Each job:
1. Build all binaries in release mode
2. Package into `colony-{version}-{target}.tar.gz` containing: `colony`, `filetree`, `filedetails`, `preview`, `gitstatus`, `gitstage`, `jobs-panel`, `history-panel`, `env-panel`, `logs-panel`, `session-panel`
3. Upload tarball as a release asset

## New Crate: `colony-update`

Keeps update logic isolated from the shell. Used by both the startup check and the `colony update` builtin.

### Public API

```rust
/// Check if a newer version is available. Returns None if up-to-date or check skipped.
pub async fn check_for_update(force: bool) -> Option<UpdateInfo>;

/// Download and install the latest release, replacing binaries in place.
pub async fn perform_update() -> Result<UpdateResult, UpdateError>;

/// Information about an available update.
pub struct UpdateInfo {
    pub current: semver::Version,
    pub latest: semver::Version,
    pub release_url: String,
}

/// Result of a successful update.
pub struct UpdateResult {
    pub from: semver::Version,
    pub to: semver::Version,
    pub binaries_replaced: Vec<String>,
}
```

### Dependencies

- `reqwest` — HTTP client for GitHub API + asset download
- `semver` — version comparison
- `flate2` — gzip decompression
- `tar` — tarball extraction
- `serde`, `serde_json` — GitHub API response parsing
- `dirs` — `~/.colony` path resolution

### Platform Detection

Compile-time target triple via:
```rust
const TARGET: &str = env!("TARGET");
```

Set by `build.rs`:
```rust
fn main() {
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET").unwrap());
}
```

## File Layout

```
crates/colony-update/
├── Cargo.toml
├── build.rs
└── src/
    └── lib.rs
```

Integration points:
- `crates/colony-exec/src/builtins.rs` — register `update` builtin
- `crates/colony-shell/src/repl_tui.rs` — startup check (background task, result shown in scrollback)
- `crates/colony-shell/src/repl.rs` — startup check (non-TUI mode)
- `crates/colony-shell/Cargo.toml` — add `colony-update` dependency
- `.github/workflows/publish.yml` — multi-platform build matrix

## Update Check File Format

`~/.colony/update-check`:
```json
{"last_check": 1710000000, "latest_version": "0.5.0"}
```

Simple JSON. The cached `latest_version` is used to show the notification without re-fetching if the cooldown hasn't expired but the user hasn't updated yet.

## Self-Update Mechanics

Binary replacement strategy:
1. Download tarball to a temp directory
2. Extract all binaries
3. For each binary, rename the current one to `{name}.old`, copy the new one in place, delete `{name}.old`
4. On failure, rename `{name}.old` back to restore the original

This avoids issues with replacing a running binary on some platforms.

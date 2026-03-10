# Auto-Update Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add startup update checking and a `colony update` command that downloads pre-built binaries from GitHub releases.

**Architecture:** New `colony-update` crate handles all update logic (check, download, install). The TUI spawns a background check on startup and shows a one-liner if an update is available. The `update` builtin emits a tuple; the TUI intercepts it and runs the async update with progress output. The CI workflow is expanded to build on 4 targets and upload tarballs.

**Tech Stack:** reqwest (HTTP), semver (version comparison), flate2 + tar (archive extraction), serde_json (GitHub API)

---

## File Structure

| File | Responsibility |
|------|---------------|
| `crates/colony-update/Cargo.toml` | Crate manifest with reqwest, semver, flate2, tar |
| `crates/colony-update/build.rs` | Injects `TARGET` env var at compile time |
| `crates/colony-update/src/lib.rs` | Public API: `check_for_update`, `perform_update`, types |
| `crates/colony-update/src/github.rs` | GitHub releases API client |
| `crates/colony-update/src/install.rs` | Download tarball, extract, replace binaries |
| `crates/colony-update/src/cache.rs` | Read/write `~/.colony/update-check` cooldown file |
| `crates/colony-exec/src/builtins.rs` | Add `"update"` match arm (tuple-based) |
| `crates/colony-shell/src/repl_tui.rs` | Startup background check + handle update tuple |
| `crates/colony-shell/src/repl.rs` | Startup background check (non-TUI) |
| `crates/colony-shell/Cargo.toml` | Add colony-update dependency |
| `crates/colony-exec/Cargo.toml` | No changes needed (tuple-only) |
| `Cargo.toml` | Add colony-update to workspace members + dependencies |
| `.github/workflows/publish.yml` | Multi-platform build matrix with tarball uploads |

---

## Chunk 1: colony-update crate

### Task 1: Scaffold the crate

**Files:**
- Create: `crates/colony-update/Cargo.toml`
- Create: `crates/colony-update/build.rs`
- Create: `crates/colony-update/src/lib.rs`
- Create: `crates/colony-update/src/github.rs`
- Create: `crates/colony-update/src/install.rs`
- Create: `crates/colony-update/src/cache.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create `crates/colony-update/Cargo.toml`**

```toml
[package]
name = "colony-update"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
description = "Auto-update support for colony shell"

[dependencies]
reqwest = { version = "0.12", features = ["json", "stream"] }
semver = "1"
flate2 = "1"
tar = "0.4"
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
dirs = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
```

- [ ] **Step 2: Create `crates/colony-update/build.rs`**

```rust
fn main() {
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
```

- [ ] **Step 3: Create `crates/colony-update/src/lib.rs`**

```rust
//! Auto-update support for Colony Shell.
//!
//! Checks GitHub releases for newer versions and provides
//! download + install functionality.

mod cache;
mod github;
mod install;

use semver::Version;

pub use github::ReleaseInfo;
pub use install::UpdateResult;

/// Repo owner/name for GitHub API calls.
const GITHUB_REPO: &str = "colony-sh/colony-shell";

/// Current version compiled into the binary.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Target triple this binary was compiled for.
pub const TARGET: &str = env!("TARGET");

/// Error type for update operations.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("no release found")]
    NoRelease,

    #[error("no binary for platform {0}")]
    NoPlatformBinary(String),

    #[error("version parse error: {0}")]
    VersionParse(#[from] semver::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current: Version,
    pub latest: Version,
    pub release_url: String,
}

/// Check if a newer version is available.
///
/// Respects a 24-hour cooldown unless `force` is true.
/// Returns `None` if up-to-date, on cooldown, or on any error.
pub async fn check_for_update(force: bool) -> Option<UpdateInfo> {
    let current = Version::parse(CURRENT_VERSION).ok()?;

    // Check cooldown
    if !force {
        if let Some(cached) = cache::read_cache() {
            if !cached.is_expired() {
                // Still in cooldown — use cached latest to decide
                let latest = Version::parse(&cached.latest_version).ok()?;
                if latest > current {
                    return Some(UpdateInfo {
                        current: current.clone(),
                        latest,
                        release_url: cached.release_url,
                    });
                }
                return None;
            }
        }
    }

    // Fetch latest release from GitHub
    let release = github::fetch_latest_release(GITHUB_REPO).await.ok()?;
    let latest = Version::parse(release.tag.trim_start_matches('v')).ok()?;

    // Update cache
    cache::write_cache(&latest, &release.html_url);

    if latest > current {
        Some(UpdateInfo {
            current,
            latest,
            release_url: release.html_url,
        })
    } else {
        None
    }
}

/// Download and install the latest release.
pub async fn perform_update() -> Result<UpdateResult, UpdateError> {
    let current = Version::parse(CURRENT_VERSION)?;
    let release = github::fetch_latest_release(GITHUB_REPO).await?;
    let latest = Version::parse(release.tag.trim_start_matches('v'))?;

    if latest <= current {
        return Ok(UpdateResult {
            from: current.clone(),
            to: current,
            binaries_replaced: Vec::new(),
        });
    }

    let asset_name = format!("colony-{}-{}.tar.gz", latest, TARGET);
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| UpdateError::NoPlatformBinary(TARGET.to_string()))?;

    let binaries = install::download_and_install(&asset.download_url).await?;

    // Clear the cache so the next startup check sees we're up-to-date
    cache::write_cache(&latest, &release.html_url);

    Ok(UpdateResult {
        from: current,
        to: latest,
        binaries_replaced: binaries,
    })
}
```

- [ ] **Step 4: Create `crates/colony-update/src/github.rs`**

```rust
//! GitHub Releases API client.

use serde::Deserialize;

use crate::UpdateError;

/// A GitHub release.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub html_url: String,
    pub assets: Vec<ReleaseAsset>,
}

/// A single asset attached to a release.
#[derive(Debug, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Fetch the latest release from a GitHub repository.
pub async fn fetch_latest_release(repo: &str) -> Result<ReleaseInfo, UpdateError> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent("colony-shell-updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp: ApiRelease = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()
        .map_err(|_| UpdateError::NoRelease)?
        .json()
        .await?;

    Ok(ReleaseInfo {
        tag: resp.tag_name,
        html_url: resp.html_url,
        assets: resp
            .assets
            .into_iter()
            .map(|a| ReleaseAsset {
                name: a.name,
                download_url: a.browser_download_url,
            })
            .collect(),
    })
}
```

- [ ] **Step 5: Create `crates/colony-update/src/install.rs`**

```rust
//! Download and install release binaries.

use std::path::PathBuf;

use semver::Version;

use crate::UpdateError;

/// Result of a successful update.
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub from: Version,
    pub to: Version,
    pub binaries_replaced: Vec<String>,
}

/// Download a tarball from `url`, extract it, and replace binaries
/// in the same directory as the current executable.
pub async fn download_and_install(url: &str) -> Result<Vec<String>, UpdateError> {
    let install_dir = std::env::current_exe()?
        .canonicalize()?
        .parent()
        .ok_or_else(|| UpdateError::Other("cannot determine install directory".into()))?
        .to_path_buf();

    tracing::info!("downloading update from {url}");

    let client = reqwest::Client::builder()
        .user_agent("colony-shell-updater")
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| UpdateError::Other(format!("download failed: {e}")))?
        .bytes()
        .await?;

    tracing::info!("downloaded {} bytes, extracting", bytes.len());

    // Extract tarball
    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);

    let mut replaced = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();

        // Get just the filename (tarballs may have a directory prefix)
        let Some(file_name) = path.file_name() else {
            continue;
        };
        let file_name_str = file_name.to_string_lossy().to_string();

        // Skip non-files (directories, symlinks)
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let target = install_dir.join(&file_name_str);
        let backup = install_dir.join(format!("{file_name_str}.old"));

        // Rename current binary to .old (ignore error if it doesn't exist)
        let had_backup = std::fs::rename(&target, &backup).is_ok();

        // Extract new binary
        if let Err(e) = extract_entry(&mut entry, &target) {
            // Restore backup on failure
            if had_backup {
                let _ = std::fs::rename(&backup, &target);
            }
            return Err(e);
        }

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
        }

        // Remove backup
        if had_backup {
            let _ = std::fs::remove_file(&backup);
        }

        replaced.push(file_name_str);
    }

    Ok(replaced)
}

fn extract_entry(
    entry: &mut tar::Entry<'_, flate2::read::GzDecoder<&[u8]>>,
    target: &PathBuf,
) -> Result<(), UpdateError> {
    use std::io::Read;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    std::fs::write(target, &buf)?;
    Ok(())
}
```

- [ ] **Step 6: Create `crates/colony-update/src/cache.rs`**

```rust
//! Cooldown cache for update checks.
//!
//! Stores the last check timestamp and latest known version
//! in `~/.colony/update-check`.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// How often to re-check (24 hours).
const COOLDOWN_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub last_check: u64,
    pub latest_version: String,
    #[serde(default)]
    pub release_url: String,
}

impl CacheEntry {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.last_check) >= COOLDOWN_SECS
    }
}

fn cache_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".colony").join("update-check"))
}

pub fn read_cache() -> Option<CacheEntry> {
    let path = cache_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write_cache(version: &semver::Version, release_url: &str) {
    let Some(path) = cache_path() else { return };

    // Ensure directory exists
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = CacheEntry {
        last_check: now,
        latest_version: version.to_string(),
        release_url: release_url.to_string(),
    };

    let _ = std::fs::write(path, serde_json::to_string(&entry).unwrap_or_default());
}
```

- [ ] **Step 7: Register in workspace**

Add to `Cargo.toml` (workspace root):

In `[workspace] members` array, add `"crates/colony-update"`.

In `[workspace.dependencies]` section, add:
```toml
colony-update = { path = "crates/colony-update" }
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p colony-update`
Expected: compiles with no errors.

- [ ] **Step 9: Commit**

```bash
git add crates/colony-update/ Cargo.toml
git commit -m "feat(update): add colony-update crate with GitHub release checking and binary install"
```

---

### Task 2: Add unit tests for cache module

**Files:**
- Modify: `crates/colony-update/src/cache.rs`

- [ ] **Step 1: Add tests to cache.rs**

Append to `crates/colony-update/src/cache.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_expired() {
        let old = CacheEntry {
            last_check: 0,
            latest_version: "1.0.0".to_string(),
            release_url: String::new(),
        };
        assert!(old.is_expired());
    }

    #[test]
    fn test_cache_entry_not_expired() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let fresh = CacheEntry {
            last_check: now,
            latest_version: "1.0.0".to_string(),
            release_url: String::new(),
        };
        assert!(!fresh.is_expired());
    }

    #[test]
    fn test_cache_roundtrip() {
        let entry = CacheEntry {
            last_check: 1000,
            latest_version: "2.0.0".to_string(),
            release_url: "https://example.com".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: CacheEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.latest_version, "2.0.0");
        assert_eq!(parsed.last_check, 1000);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p colony-update`
Expected: 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/colony-update/src/cache.rs
git commit -m "test(update): add cache module tests"
```

---

## Chunk 2: Shell integration

### Task 3: Register `update` builtin in colony-exec

**Files:**
- Modify: `crates/colony-exec/src/builtins.rs`

The `update` builtin emits a tuple that the TUI/REPL picks up and handles asynchronously.

- [ ] **Step 1: Add "update" to `is_builtin_base`**

In `crates/colony-exec/src/builtins.rs`, find the `is_builtin_base` function. Add `"update"` to the list of builtin names (after `"sidebar"`):

```rust
            | "sidebar"
            | "update"
    )
```

- [ ] **Step 2: Add match arm in `execute_builtin`**

In `crates/colony-exec/src/builtins.rs`, find the `execute_builtin` function's match block. Add before the `_ => unreachable!` arm:

```rust
        "update" => {
            tuple_space.out(colony_tuple::Tuple::new(vec![
                colony_tuple::Value::Str("update_command".to_string()),
                colony_tuple::Value::Str("run".to_string()),
            ]));
            Ok(BuiltinResult::Success(0))
        }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p colony-exec`
Expected: compiles (warnings OK).

- [ ] **Step 4: Commit**

```bash
git add crates/colony-exec/src/builtins.rs
git commit -m "feat(update): register update builtin in colony-exec"
```

---

### Task 4: Wire update check into TUI startup

**Files:**
- Modify: `crates/colony-shell/Cargo.toml`
- Modify: `crates/colony-shell/src/repl_tui.rs`

- [ ] **Step 1: Add colony-update dependency to colony-shell**

In `crates/colony-shell/Cargo.toml`, add to `[dependencies]`:

```toml
colony-update = { path = "../colony-update" }
```

- [ ] **Step 2: Add background update check after init scripts**

In `crates/colony-shell/src/repl_tui.rs`, find the block after `process_sidebar_commands` (around line 551). Add a background update check that sends results through a channel:

```rust
    // Spawn background update check
    let (update_tx, mut update_rx) = tokio::sync::mpsc::channel::<String>(1);
    tokio::spawn(async move {
        if let Some(info) = colony_update::check_for_update(false).await {
            let msg = format!(
                "colony v{} available \u{2014} run 'colony update' to install",
                info.latest
            );
            let _ = update_tx.send(msg).await;
        }
    });
```

- [ ] **Step 3: Check for update notification in the REPL loop**

In `read_input_line`, at the top of the loop (after `poll_panels` and before `event::poll`), add a non-blocking check on the channel. To do this, pass `update_rx` through the call chain:

Add `update_rx: &mut tokio::sync::mpsc::Receiver<String>` parameter to `run_repl_loop` and `read_input_line`.

In `read_input_line`, before the `event::poll` call:

```rust
        // Check for background update notification
        if let Ok(msg) = update_rx.try_recv() {
            scrollback.add_output(&msg);
            redraw(
                terminal, prompt_manager, ctx, editor,
                panel_host, scrollback, status_message, *show_help,
            )?;
        }
```

- [ ] **Step 4: Handle the `update` command tuple**

In `run_repl_loop`, after the `process_sidebar_commands` call (around line 694), add:

```rust
        // Process update command tuple
        process_update_command(&executor, scrollback).await;
```

Add the handler function:

```rust
/// Handle the `update` builtin by checking for and applying updates.
async fn process_update_command(executor: &Executor, scrollback: &mut Scrollback) {
    let tuple_space = executor.tuple_space();
    let pattern = colony_tuple::Tuple::pattern(&[
        colony_tuple::Value::Str("update_command".to_string()),
        colony_tuple::Value::Wildcard,
    ]);

    let Some(_tuple) = tuple_space.inp(pattern) else {
        return;
    };

    scrollback.add_output("Checking for updates...");

    match colony_update::perform_update().await {
        Ok(result) => {
            if result.binaries_replaced.is_empty() {
                scrollback.add_output(&format!("colony v{} is up to date.", result.to));
            } else {
                scrollback.add_output(&format!(
                    "Updated colony v{} \u{2192} v{} ({} binaries replaced). Restart your shell to use the new version.",
                    result.from,
                    result.to,
                    result.binaries_replaced.len()
                ));
            }
        }
        Err(e) => {
            scrollback.add_error(&format!("Update failed: {e}"));
        }
    }
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p colony-shell`
Expected: compiles with no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/colony-shell/Cargo.toml crates/colony-shell/src/repl_tui.rs
git commit -m "feat(update): wire startup check and update command into TUI REPL"
```

---

### Task 5: Wire update check into non-TUI REPL

**Files:**
- Modify: `crates/colony-shell/src/repl.rs`

- [ ] **Step 1: Add background update check before the main loop**

In `crates/colony-shell/src/repl.rs`, in `run_repl`, before the main `loop {`, add:

```rust
    // Spawn background update check
    let (update_tx, mut update_rx) = tokio::sync::mpsc::channel::<String>(1);
    tokio::spawn(async move {
        if let Some(info) = colony_update::check_for_update(false).await {
            let msg = format!(
                "colony v{} available \u{2014} run 'colony update' to install",
                info.latest
            );
            let _ = update_tx.send(msg).await;
        }
    });
```

- [ ] **Step 2: Check for notification inside the loop**

At the top of the main loop body, before the prompt is rendered, add:

```rust
        // Show update notification if available
        if let Ok(msg) = update_rx.try_recv() {
            println!("{msg}");
        }
```

- [ ] **Step 3: Handle update command in non-TUI**

After command execution in the loop, add a similar tuple check:

```rust
        // Process update command
        {
            let tuple_space = executor.tuple_space();
            let pattern = colony_tuple::Tuple::pattern(&[
                colony_tuple::Value::Str("update_command".to_string()),
                colony_tuple::Value::Wildcard,
            ]);
            if tuple_space.inp(pattern).is_some() {
                println!("Checking for updates...");
                match colony_update::perform_update().await {
                    Ok(result) => {
                        if result.binaries_replaced.is_empty() {
                            println!("colony v{} is up to date.", result.to);
                        } else {
                            println!(
                                "Updated colony v{} \u{2192} v{} ({} binaries replaced). Restart your shell to use the new version.",
                                result.from, result.to, result.binaries_replaced.len()
                            );
                        }
                    }
                    Err(e) => eprintln!("Update failed: {e}"),
                }
            }
        }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p colony-shell`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/colony-shell/src/repl.rs
git commit -m "feat(update): wire startup check and update command into non-TUI REPL"
```

---

## Chunk 3: CI workflow

### Task 6: Expand publish workflow for multi-platform releases

**Files:**
- Modify: `.github/workflows/publish.yml`

- [ ] **Step 1: Replace publish.yml with multi-platform build**

Replace `.github/workflows/publish.yml` with:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.runner }}
    strategy:
      matrix:
        include:
          - target: aarch64-apple-darwin
            runner: macos-latest
          - target: x86_64-apple-darwin
            runner: macos-13
          - target: x86_64-unknown-linux-gnu
            runner: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            runner: ubuntu-latest
            cross: true

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross-compilation tools
        if: matrix.cross
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu
          echo '[target.aarch64-unknown-linux-gnu]' >> ~/.cargo/config.toml
          echo 'linker = "aarch64-linux-gnu-gcc"' >> ~/.cargo/config.toml

      - name: Build release binaries
        run: cargo build --release --target ${{ matrix.target }} -p colony-shell

      - name: Determine version from tag
        id: version
        run: echo "version=${GITHUB_REF#refs/tags/v}" >> "$GITHUB_OUTPUT"

      - name: Package tarball
        run: |
          ARCHIVE="colony-${{ steps.version.outputs.version }}-${{ matrix.target }}.tar.gz"
          BIN_DIR="target/${{ matrix.target }}/release"

          # Collect all binaries
          BINS=""
          for bin in colony filetree filedetails preview gitstatus gitstage jobs-panel history-panel env-panel logs-panel session-panel; do
            if [ -f "$BIN_DIR/$bin" ]; then
              BINS="$BINS $BIN_DIR/$bin"
            fi
          done

          tar czf "$ARCHIVE" -C "$BIN_DIR" $(for b in $BINS; do basename "$b"; done)
          echo "ARCHIVE=$ARCHIVE" >> "$GITHUB_ENV"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target }}
          path: ${{ env.ARCHIVE }}

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write

    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: artifacts/**/*.tar.gz
          draft: false
          prerelease: false

  publish:
    name: Publish to crates.io
    needs: release
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Verify version matches tag
        run: |
          TAG_VERSION=${GITHUB_REF#refs/tags/v}
          CARGO_VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "colony-shell") | .version')
          if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "Tag version ($TAG_VERSION) doesn't match Cargo version ($CARGO_VERSION)"
            exit 1
          fi

      - name: Publish crates in dependency order
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          for crate in colony-protocol colony-memory colony-tuple colony-parser colony-expand colony-channels colony-config colony-tree-sitter colony-exec colony-mcp colony-focus colony-update colony-agent colony-shell; do
            echo "Publishing $crate..."
            cargo publish -p "$crate" --allow-dirty || echo "Already published or failed"
            sleep 10
          done
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/publish.yml
git commit -m "ci: multi-platform release builds with tarball packaging"
```

---

### Task 7: Integration test (manual)

- [ ] **Step 1: Run the full build**

Run: `cargo build -p colony-shell`
Expected: everything compiles.

- [ ] **Step 2: Test the update check manually**

Run colony and verify:
- No errors on startup (update check runs silently in background)
- Typing `update` shows "Checking for updates..." then either "up to date" or an error (since no releases exist yet)

- [ ] **Step 3: Final commit if any fixups needed**

```bash
git add -A
git commit -m "fix(update): integration fixups"
```

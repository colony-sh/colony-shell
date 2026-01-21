# macOS 26 Update Plan for Colony Projects

**Date:** 2026-01-18
**Author:** Claude (AI Assistant)
**Status:** Pending Approval

## Executive Summary

Both colony-shell and colony-terminal need updates to support macOS 26 (Tahoe). Research into Alacritty, Warp, Zed, and Ghostty GitHub issues revealed several critical compatibility issues that affect terminal emulators on the new OS version.

## Issues Identified

### 1. NSAutoFillHeuristicController Performance Degradation (Critical)

- **Source:** Ghostty #8616, Alacritty #8696, Zed #33182
- **Symptom:** Progressive scrolling/rendering performance degradation over time on macOS 26
- **Root Cause:** macOS 26 introduced `NSAutoFillHeuristicController` that hooks into all `NSTextInputClient` implementations, causing cumulative performance issues
- **Fix:** Set `NSAutoFillHeuristicControllerEnabled` default to `false`
- **Affects:** colony-terminal (GPUI-based, uses NSTextInputClient)

### 2. Fullscreen Wake-from-Sleep Crash (High)

- **Source:** Alacritty #8727, #8722
- **Symptom:** Application crashes when running fullscreen on external monitor after waking from sleep
- **Root Cause:** `winit` library issue with monitor handle storage
- **Fix:** Bump `winit` to 0.30.12+ which includes macOS 26 fixes
- **Affects:** colony-terminal (if GPUI uses winit internally)

### 3. GPUI/Metal MTLCompilerService Errors (Medium)

- **Source:** Zed #46486
- **Symptom:** `gpui 0.2.2` from crates.io fails with MTLCompilerService error after macOS update
- **Fix:** Update to latest GPUI from Zed repository
- **Affects:** colony-terminal (uses GPUI via git dependency)

### 4. OpenGL/CGLFlushDrawable Issues (Medium)

- **Source:** Alacritty crash logs in #8727
- **Symptom:** Crashes in `NSSoftwareSurface frontBuffer` during OpenGL flush operations
- **Affects:** colony-terminal if using OpenGL rendering path

## colony-terminal Update Plan

### Task List

| Task | Priority | Description | Estimated Effort |
|------|----------|-------------|------------------|
| T1 | Critical | Disable NSAutoFillHeuristicController | 1 hour |
| T2 | High | Update GPUI git revision to latest Zed | 2-4 hours |
| T3 | Medium | Test fullscreen + external monitor + sleep/wake cycle | 1 hour |
| T4 | Medium | Verify Metal shader compilation on macOS 26 | 1 hour |
| T5 | Low | Review and update portable-pty dependency | 1 hour |

### T1: Disable AutoFill Heuristics

**Option A - Via UserDefaults in Rust:**
```rust
// In macOS app initialization
unsafe {
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setBool_forKey(false, ns_string!("NSAutoFillHeuristicControllerEnabled"));
}
```

**Option B - Via Info.plist:**
```xml
<key>NSAutoFillHeuristicControllerEnabled</key>
<false/>
```

**Recommended:** Option A for programmatic control, or Option B for simplicity.

### T2: Update GPUI Dependency

Current in `Cargo.toml`:
```toml
gpui = { git = "https://github.com/zed-industries/zed", rev = "7ce845210d3af82a57a7518e0abe8c167d60cc6a" }
```

Action: Update to a more recent commit from Zed's main branch that includes macOS 26 fixes. Check Zed's changelog for relevant PRs.

### T3-T5: Testing and Minor Updates

- Test fullscreen behavior on external monitors
- Verify Metal compilation works correctly
- Update portable-pty if needed for PTY compatibility

## colony-shell Update Plan

### Task List

| Task | Priority | Description | Estimated Effort |
|------|----------|-------------|------------------|
| T1 | Medium | Audit dependencies for macOS 26 compatibility | 1-2 hours |
| T2 | Medium | Test nix crate syscalls on macOS 26 | 1 hour |
| T3 | Low | Update tree-sitter if needed | 30 min |
| T4 | Low | Test PTY creation/handling on macOS 26 | 1 hour |

### Notes

colony-shell is primarily a CLI/shell application and should be less affected since it doesn't use:
- GUI frameworks (no GPUI/AppKit direct usage)
- NSTextInputClient
- OpenGL/Metal rendering

Main concerns are:
- Unix syscalls via `nix` crate (PTY, signals, process management)
- Terminal escape sequence handling
- Signal handling compatibility

## Testing Matrix

| Test Case | colony-shell | colony-terminal |
|-----------|--------------|-----------------|
| Basic launch | Required | Required |
| Scrolling performance (extended use) | N/A | Critical |
| External monitor + fullscreen | N/A | High |
| Wake from sleep | Required | High |
| PTY spawning | Required | Required |
| OSC 2337 protocol | Required | Required |
| Tab completion | Required | N/A |
| Theme switching | N/A | Required |
| Job control (fg/bg/jobs) | Required | N/A |
| Signal handling (SIGINT, SIGTSTP) | Required | Required |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| GPUI API changes when updating | Medium | High | Review Zed changelog, test thoroughly |
| New macOS 26 issues not yet discovered | Medium | Medium | Monitor upstream issues |
| Breaking changes in dependencies | Low | Medium | Pin versions, test thoroughly |
| nix crate syscall changes | Low | High | Test all PTY and signal operations |

## Implementation Order

1. **colony-terminal first** - Has the most critical macOS 26 issues
   - Apply NSAutoFill fix (T1) - minimal change, high impact
   - Update GPUI dependency (T2) - test extensively
   - Run full test suite

2. **colony-shell second** - Lower risk
   - Audit and test on macOS 26
   - Update dependencies as needed
   - Verify PTY and signal handling

## References

- Ghostty NSAutoFill fix: https://github.com/ghostty-org/ghostty/discussions/8616
- Alacritty macOS 26 performance: https://github.com/alacritty/alacritty/issues/8696
- Alacritty fullscreen crash: https://github.com/alacritty/alacritty/issues/8727
- Alacritty winit bump: https://github.com/alacritty/alacritty/issues/8722
- Zed GPUI macOS issue: https://github.com/zed-industries/zed/issues/46486

## Approval

- [ ] Plan reviewed
- [ ] Priority order confirmed
- [ ] Ready to begin implementation

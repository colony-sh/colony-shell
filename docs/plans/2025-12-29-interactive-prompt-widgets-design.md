# Interactive Prompt Widgets Design

## Overview

Enable clickable prompt components that trigger interactive widgets in colony-terminal. The first implementation is a **git branch picker** that appears when clicking the git_branch component.

## Architecture

```
┌─────────────────┐     OSC 2337      ┌──────────────────┐
│  colony-shell   │ ───────────────▶  │  colony-terminal │
│                 │                   │                  │
│  PromptManager  │                   │  WidgetRenderer  │
│  renders prompt │                   │  handles clicks  │
│  with metadata  │                   │  shows popups    │
│                 │  ◀─────────────── │                  │
│  handles action │   OSC 2337        │  sends actions   │
└─────────────────┘   response        └──────────────────┘
```

## Protocol Messages

### 1. Prompt Message (Shell → Terminal)

Already implemented. The `Prompt` message includes component data with semantic information:

```json
{
  "type": "Prompt",
  "left": [
    {
      "id": "pwd",
      "spans": [{"text": "~/project", "fg": "cyan"}],
      "data": {"full": "/home/user/project", "display": "~/project"}
    },
    {
      "id": "git_branch",
      "spans": [{"text": " main", "fg": "green"}],
      "data": {"branch": "main", "detached": false},
      "interactive": {
        "click": "branch_picker",
        "hover": "Click to switch branches"
      }
    }
  ],
  "right": []
}
```

**New field: `interactive`**
- `click`: Widget type to show on click
- `hover`: Tooltip text on hover

### 2. Widget Request (Terminal → Shell)

When user clicks an interactive component, terminal requests widget data:

```json
{
  "type": "WidgetRequest",
  "widget": "branch_picker",
  "component_id": "git_branch",
  "context": {
    "current_branch": "main"
  }
}
```

### 3. Widget Data (Shell → Terminal)

Shell responds with data to populate the widget:

```json
{
  "type": "WidgetData",
  "widget": "branch_picker",
  "data": {
    "current": "main",
    "local": [
      {"name": "main", "current": true, "ahead": 0, "behind": 0},
      {"name": "feature-x", "current": false, "ahead": 2, "behind": 1},
      {"name": "bugfix-123", "current": false, "ahead": 0, "behind": 0}
    ],
    "remote": [
      {"name": "origin/main", "tracking": "main"},
      {"name": "origin/develop", "tracking": null},
      {"name": "origin/feature-y", "tracking": null}
    ],
    "allow_create": true
  }
}
```

### 4. Widget Action (Terminal → Shell)

User selects an option from the widget:

```json
{
  "type": "WidgetAction",
  "widget": "branch_picker",
  "action": "switch",
  "payload": {
    "branch": "feature-x"
  }
}
```

Or for creating a new branch:

```json
{
  "type": "WidgetAction",
  "widget": "branch_picker",
  "action": "create",
  "payload": {
    "name": "feature-new",
    "from": "main"
  }
}
```

### 5. Widget Result (Shell → Terminal)

Shell confirms the action result:

```json
{
  "type": "WidgetResult",
  "widget": "branch_picker",
  "success": true,
  "message": "Switched to branch 'feature-x'"
}
```

Or on error:

```json
{
  "type": "WidgetResult",
  "widget": "branch_picker",
  "success": false,
  "error": "Cannot switch branches with uncommitted changes"
}
```

---

## Terminal Implementation Guide

### Rendering Interactive Components

1. **Parse Prompt message** and identify components with `interactive` field
2. **Render with visual cue** (underline, different cursor on hover)
3. **Show tooltip** from `interactive.hover` on mouse hover
4. **On click**, send `WidgetRequest` to shell via OSC 2337

### Branch Picker Widget UI

```
┌─ Switch Branch ─────────────────────────┐
│ 🔍 Filter...                            │
├─────────────────────────────────────────┤
│ LOCAL                                   │
│   ● main                    ✓ current   │
│   ○ feature-x               ↑2 ↓1       │
│   ○ bugfix-123                          │
├─────────────────────────────────────────┤
│ REMOTE                                  │
│   ○ origin/develop          checkout    │
│   ○ origin/feature-y        checkout    │
├─────────────────────────────────────────┤
│ ＋ Create new branch...                 │
└─────────────────────────────────────────┘
```

**Keyboard navigation:**
- `↑/↓` or `j/k`: Navigate list
- `Enter`: Select/switch to branch
- `/`: Focus filter input
- `Esc`: Close widget
- `n`: Create new branch

**Visual indicators:**
- `●` Current branch
- `○` Other branches
- `↑N ↓M` Ahead/behind remote
- `✓ current` Label for active branch

### Widget Positioning

Position the widget dropdown:
1. **Anchor point**: Bottom-left of the clicked component
2. **Direction**: Prefer downward, flip upward if near bottom of terminal
3. **Width**: Min 250px, max 400px, or fit content
4. **Max height**: 60% of terminal height, then scroll

### State Management

```typescript
interface BranchPickerState {
  isOpen: boolean;
  filter: string;
  selectedIndex: number;
  branches: Branch[];
  loading: boolean;
  error: string | null;
}
```

### Interaction Flow

```
User clicks git_branch component
         │
         ▼
Terminal sends WidgetRequest
         │
         ▼
Shell gathers branch data (git branch -a, git status)
         │
         ▼
Shell sends WidgetData
         │
         ▼
Terminal renders BranchPicker popup
         │
         ▼
User navigates and selects branch
         │
         ▼
Terminal sends WidgetAction
         │
         ▼
Shell executes git checkout
         │
         ▼
Shell sends WidgetResult
         │
         ▼
Terminal closes popup, shows toast if error
         │
         ▼
Shell re-renders prompt (branch changed)
```

---

## Shell Implementation

### Enhanced PromptComponentData

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptComponentData {
    pub id: String,
    pub spans: Vec<PromptSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive: Option<InteractiveConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveConfig {
    pub click: String,  // Widget type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<String>,  // Tooltip
}
```

### New Protocol Messages

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProtocolMessage {
    // ... existing variants ...
    
    WidgetRequest {
        widget: String,
        component_id: String,
        context: serde_json::Value,
    },
    
    WidgetData {
        widget: String,
        data: serde_json::Value,
    },
    
    WidgetAction {
        widget: String,
        action: String,
        payload: serde_json::Value,
    },
    
    WidgetResult {
        widget: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}
```

### Branch Picker Handler

```rust
pub async fn handle_branch_picker_request(
    executor: &Executor,
) -> Result<BranchPickerData> {
    // Get local branches
    let local_output = Command::new("git")
        .args(["branch", "-vv"])
        .output()?;
    
    // Get remote branches
    let remote_output = Command::new("git")
        .args(["branch", "-r"])
        .output()?;
    
    // Get current branch
    let current = Command::new("git")
        .args(["branch", "--show-current"])
        .output()?;
    
    // Parse and return
    Ok(BranchPickerData {
        current: parse_current(&current.stdout),
        local: parse_local_branches(&local_output.stdout),
        remote: parse_remote_branches(&remote_output.stdout),
        allow_create: true,
    })
}

pub async fn handle_branch_switch(
    executor: &mut Executor,
    branch: &str,
) -> Result<WidgetResult> {
    let output = Command::new("git")
        .args(["checkout", branch])
        .output()?;
    
    if output.status.success() {
        Ok(WidgetResult {
            success: true,
            message: Some(format!("Switched to branch '{}'", branch)),
            error: None,
        })
    } else {
        Ok(WidgetResult {
            success: false,
            message: None,
            error: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        })
    }
}
```

---

## Future Widget Types

The same pattern can be used for other interactive prompt components:

| Component | Widget | Interaction |
|-----------|--------|-------------|
| `git_branch` | `branch_picker` | Switch/create branches |
| `git_status` | `status_detail` | Show changed files, stage/unstage |
| `pwd` | `directory_picker` | Quick cd to recent/bookmarked dirs |
| `jobs` | `job_manager` | fg/bg/kill jobs |
| `exit_status` | `error_detail` | Show full error output |
| `model` | `model_picker` | Switch AI model |

---

## Implementation Phases

### Phase 1: Protocol (colony-protocol)
- Add `InteractiveConfig` to `PromptComponentData`
- Add `WidgetRequest`, `WidgetData`, `WidgetAction`, `WidgetResult` messages
- Add helper functions for emitting/parsing

### Phase 2: Shell Support (colony-ui-core, colony-shell)
- Update `git_branch` component to include `interactive` config
- Add `BranchPickerHandler` for gathering branch data
- Add action handlers for switch/create
- Wire into REPL event loop

### Phase 3: Terminal Widget (colony-terminal)
- Parse and render interactive prompt components
- Implement `BranchPicker` widget component
- Handle keyboard navigation
- Send/receive OSC 2337 messages

---

## Testing

### Shell Side
```rust
#[test]
fn test_branch_picker_data_serialization() {
    let data = BranchPickerData {
        current: "main".to_string(),
        local: vec![
            LocalBranch { name: "main".to_string(), current: true, ahead: 0, behind: 0 },
        ],
        remote: vec![],
        allow_create: true,
    };
    
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("\"current\":\"main\""));
}

#[test]
fn test_widget_action_parsing() {
    let json = r#"{"type":"WidgetAction","widget":"branch_picker","action":"switch","payload":{"branch":"feature-x"}}"#;
    let msg: ProtocolMessage = serde_json::from_str(json).unwrap();
    // assert matches
}
```

### Terminal Side
- Unit test widget state management
- Integration test OSC 2337 round-trip
- E2E test with mock shell responses

//! Panel host: manages all panel processes and coordinates their lifecycle.
//!
//! [`PanelHost`] is the central coordinator for the panel system. It spawns
//! child processes, manages their I/O through [`PanelSlot`]s, resolves layout
//! via [`LayoutEngine`], handles focus cycling, and broadcasts tuple events.

use std::collections::HashMap;

use crate::layout::{LayoutEngine, PanelConstraints, PanelId, PanelPosition, ResolvedLayout};
use crate::protocol::{FrameLine, InputEvent, PanelMessage, ProtocolError};
use crate::slot::PanelSlot;

/// Identifies the focused panel or main REPL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Main,
    Panel(PanelId),
}

/// The panel host manages all panel processes.
pub struct PanelHost {
    /// Active panel slots indexed by ID.
    slots: HashMap<PanelId, PanelSlot>,
    /// Layout engine for position resolution.
    layout: LayoutEngine,
    /// Next panel ID to assign.
    next_id: usize,
    /// Current focus.
    pub focus: Focus,
}

impl PanelHost {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            layout: LayoutEngine::new(),
            next_id: 0,
            focus: Focus::Main,
        }
    }

    /// Spawn a panel process and add it to the layout.
    pub async fn add_panel(
        &mut self,
        program: &str,
        args: &[&str],
        position: PanelPosition,
        weight: u16,
        min_size: u16,
        max_size: u16,
        outer: bool,
    ) -> Result<PanelId, ProtocolError> {
        let id = PanelId(self.next_id);
        self.next_id += 1;

        let slot = PanelSlot::spawn(program, args, position).await?;

        self.layout.add_panel(
            id,
            position,
            PanelConstraints {
                weight,
                min_size,
                max_size,
                outer,
                content_size: min_size, // Start at min, updated on first frame
            },
        );

        self.slots.insert(id, slot);
        Ok(id)
    }

    /// Remove a panel, killing its process.
    pub async fn remove_panel(&mut self, id: PanelId) {
        if let Some(mut slot) = self.slots.remove(&id) {
            let _ = slot.send_event(&InputEvent::Shutdown).await;
            slot.kill().await;
        }
        self.layout.remove_panel(id);
        if self.focus == Focus::Panel(id) {
            self.focus = Focus::Main;
        }
    }

    /// Remove all panels.
    pub async fn reset(&mut self) {
        let ids: Vec<PanelId> = self.slots.keys().copied().collect();
        for id in ids {
            self.remove_panel(id).await;
        }
    }

    /// Remove all panels at a given position.
    pub async fn reset_position(&mut self, position: PanelPosition) {
        let ids: Vec<PanelId> = self
            .slots
            .iter()
            .filter(|(_, slot)| slot.position() == position)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.remove_panel(id).await;
        }
    }

    /// Poll all panels for new messages (non-blocking).
    ///
    /// Returns list of panels that produced new frames or emitted tuples.
    /// Dead panels (exited or errored) are automatically cleaned up.
    pub async fn poll_panels(&mut self) -> Vec<(PanelId, PanelMessage)> {
        let mut messages = Vec::new();
        let mut dead = Vec::new();

        for (id, slot) in &mut self.slots {
            // Try to read a message with a very short timeout.
            // We read *before* checking exit so buffered output from a
            // fast-exiting process is still consumed.
            match tokio::time::timeout(
                std::time::Duration::from_millis(5),
                slot.read_message(),
            )
            .await
            {
                Ok(Ok(Some(msg))) => {
                    if let PanelMessage::Frame(ref lines) = msg {
                        slot.set_current_frame(lines.clone());
                    }
                    if let PanelMessage::Config(ref config) = msg {
                        slot.set_config(config.clone());
                    }
                    messages.push((*id, msg));
                }
                Ok(Ok(None)) => {
                    // EOF — process closed stdout
                    dead.push(*id);
                }
                Ok(Err(_)) => {
                    dead.push(*id);
                }
                Err(_) => {
                    // Timeout — no message ready. Check if process exited.
                    if slot.try_wait().is_some() {
                        dead.push(*id);
                    }
                }
            }
        }

        // Clean up dead panels
        for id in dead {
            self.slots.remove(&id);
            self.layout.remove_panel(id);
            if self.focus == Focus::Panel(id) {
                self.focus = Focus::Main;
            }
        }

        messages
    }

    /// Send an input event to the focused panel.
    pub async fn send_to_focused(&mut self, event: &InputEvent) -> Result<(), ProtocolError> {
        let Focus::Panel(id) = self.focus else {
            return Ok(());
        };
        let Some(slot) = self.slots.get_mut(&id) else {
            return Ok(());
        };
        slot.send_event(event).await
    }

    /// Send a tuple event to all panels whose subscriptions match the key.
    pub async fn broadcast_tuple(&mut self, key: &str, value: &serde_json::Value) {
        for slot in self.slots.values_mut() {
            if slot.config().subscriptions.iter().any(|s| s == key || s == "*") {
                let _ = slot
                    .send_event(&InputEvent::Tuple {
                        data: serde_json::json!({ "key": key, "value": value }),
                    })
                    .await;
            }
        }
    }

    /// Resolve the current layout.
    pub fn resolve_layout(&self, area: ratatui::layout::Rect) -> ResolvedLayout {
        self.layout.resolve(area)
    }

    /// Get the current frame for a panel.
    pub fn current_frame(&self, id: PanelId) -> &[FrameLine] {
        self.slots
            .get(&id)
            .map(|s| s.current_frame())
            .unwrap_or(&[])
    }

    /// Get all panel IDs.
    pub fn panel_ids(&self) -> Vec<PanelId> {
        self.slots.keys().copied().collect()
    }

    /// Check if there are any panels.
    pub fn has_panels(&self) -> bool {
        !self.slots.is_empty()
    }

    /// Cycle focus to the next panel (or back to main).
    pub fn cycle_focus(&mut self) {
        let mut ids = self.panel_ids();
        ids.sort_by_key(|id| id.0);
        if ids.is_empty() {
            self.focus = Focus::Main;
            return;
        }
        self.focus = match self.focus {
            Focus::Main => Focus::Panel(ids[0]),
            Focus::Panel(current) => {
                let pos = ids.iter().position(|id| *id == current);
                match pos {
                    Some(i) if i + 1 < ids.len() => Focus::Panel(ids[i + 1]),
                    _ => Focus::Main,
                }
            }
        };
    }

    /// Send resize events to all panels with their current dimensions.
    pub async fn notify_resize(&mut self, layout: &ResolvedLayout) {
        for (id, rect) in &layout.panels {
            if let Some(slot) = self.slots.get_mut(id) {
                let _ = slot
                    .send_event(&InputEvent::Resize {
                        width: rect.width,
                        height: rect.height,
                    })
                    .await;
            }
        }
    }

    /// Shutdown all panels gracefully.
    pub async fn shutdown(&mut self) {
        for slot in self.slots.values_mut() {
            let _ = slot.send_event(&InputEvent::Shutdown).await;
        }
        // Give panels a moment to exit, then kill
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        for slot in self.slots.values_mut() {
            slot.kill().await;
        }
        self.slots.clear();
        self.layout.clear();
    }
}

impl Default for PanelHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_cycle_empty() {
        let mut host = PanelHost::new();
        assert_eq!(host.focus, Focus::Main);
        host.cycle_focus();
        assert_eq!(host.focus, Focus::Main); // no panels
    }

    #[tokio::test]
    async fn test_add_and_poll() {
        let script = r#"echo "---FRAME---"; echo '{"text":"hello"}'; echo "---FRAME---""#;
        let mut host = PanelHost::new();
        let id = host
            .add_panel("sh", &["-c", script], PanelPosition::Left, 1, 15, 40, false)
            .await
            .unwrap();

        // Poll with retries — the child process may need a moment
        let mut messages = Vec::new();
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            messages = host.poll_panels().await;
            if !messages.is_empty() {
                break;
            }
        }
        assert!(!messages.is_empty());
        let (msg_id, msg) = &messages[0];
        assert_eq!(*msg_id, id);
        assert!(matches!(msg, PanelMessage::Frame(_)));

        host.shutdown().await;
    }

    #[tokio::test]
    async fn test_remove_panel() {
        let script = "sleep 60";
        let mut host = PanelHost::new();
        let id = host
            .add_panel("sh", &["-c", script], PanelPosition::Right, 1, 15, 40, false)
            .await
            .unwrap();

        assert!(host.has_panels());
        host.remove_panel(id).await;
        assert!(!host.has_panels());
    }

    #[tokio::test]
    async fn test_reset() {
        let mut host = PanelHost::new();
        host.add_panel("true", &[], PanelPosition::Left, 1, 15, 40, false)
            .await
            .unwrap();
        host.add_panel("true", &[], PanelPosition::Right, 1, 15, 40, false)
            .await
            .unwrap();

        assert_eq!(host.panel_ids().len(), 2);
        host.reset().await;
        assert!(!host.has_panels());
    }

    #[tokio::test]
    async fn test_focus_cycle_with_panels() {
        let mut host = PanelHost::new();
        let _id1 = host
            .add_panel("true", &[], PanelPosition::Left, 1, 15, 40, false)
            .await
            .unwrap();
        let _id2 = host
            .add_panel("true", &[], PanelPosition::Right, 1, 15, 40, false)
            .await
            .unwrap();

        assert_eq!(host.focus, Focus::Main);

        host.cycle_focus();
        let Focus::Panel(first) = host.focus else {
            panic!("expected Panel focus");
        };

        host.cycle_focus();
        let Focus::Panel(second) = host.focus else {
            panic!("expected Panel focus");
        };
        assert_ne!(first, second);

        host.cycle_focus();
        assert_eq!(host.focus, Focus::Main);

        host.shutdown().await;
    }
}

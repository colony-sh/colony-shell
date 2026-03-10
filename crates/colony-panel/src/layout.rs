//! Layout engine: resolves panel positions and constraints to Rect areas.

use std::collections::HashMap;

use ratatui::layout::Rect;

/// Unique identifier for a panel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelId(pub usize);

/// Which edge of the main area a panel is docked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPosition {
    Left,
    Right,
    Top,
    Bottom,
}

/// Size and behavior constraints for a panel.
#[derive(Debug, Clone)]
pub struct PanelConstraints {
    pub weight: u16,
    pub min_size: u16,
    pub max_size: u16,
    pub outer: bool,
    pub content_size: u16,
}

/// The result of resolving panel layout against a terminal area.
#[derive(Debug)]
pub struct ResolvedLayout {
    pub main: Rect,
    pub panels: HashMap<PanelId, Rect>,
}

/// Resolves registered panels into concrete [`Rect`] areas.
pub struct LayoutEngine {
    panels: Vec<(PanelId, PanelPosition, PanelConstraints)>,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
        }
    }

    pub fn add_panel(&mut self, id: PanelId, position: PanelPosition, constraints: PanelConstraints) {
        self.panels.push((id, position, constraints));
    }

    pub fn remove_panel(&mut self, id: PanelId) {
        self.panels.retain(|(pid, _, _)| *pid != id);
    }

    pub fn clear(&mut self) {
        self.panels.clear();
    }

    pub fn clear_position(&mut self, position: PanelPosition) {
        self.panels.retain(|(_, pos, _)| *pos != position);
    }

    pub fn resolve(&self, area: Rect) -> ResolvedLayout {
        // Stub - returns main as full area
        ResolvedLayout {
            main: area,
            panels: HashMap::new(),
        }
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

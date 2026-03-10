//! Layout engine: resolves panel positions and constraints to Rect areas.

use std::collections::HashMap;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

/// Unique identifier for a panel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelId(pub usize);

/// Which edge of the main area a panel is docked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// Resolves all registered panels into concrete [`Rect`] areas within `area`.
    ///
    /// Resolution order:
    /// 1. Outer top panels (full width, stacked vertically from top)
    /// 2. Outer bottom panels (full width, stacked vertically from bottom)
    /// 3. Left panels (stacked horizontally from left)
    /// 4. Right panels (stacked horizontally from right)
    /// 5. Inner top panels (within remaining main column, stacked vertically)
    /// 6. Inner bottom panels (within remaining main column, stacked vertically from bottom)
    /// 7. Main gets everything left over
    pub fn resolve(&self, area: Rect) -> ResolvedLayout {
        let mut panels = HashMap::new();

        // Track the remaining region as we allocate panels.
        let mut x = area.x;
        let mut y = area.y;
        let mut w = area.width;
        let mut h = area.height;

        // Helper: compute clamped panel size, ensuring main retains at least
        // `reserve` in the relevant dimension.
        let clamp_size = |c: &PanelConstraints, available: u16, reserve: u16| -> u16 {
            let ideal = c.content_size.clamp(c.min_size, c.max_size);
            let max_allowed = available.saturating_sub(reserve);
            ideal.min(max_allowed)
        };

        // --- 1. Outer top panels (full width, stacked from top) ---
        let outer_top: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Top && c.outer)
            .collect();
        for (id, _, constraints) in &outer_top {
            let size = clamp_size(constraints, h, MAIN_MIN_HEIGHT);
            if size == 0 {
                continue;
            }
            panels.insert(*id, Rect::new(x, y, w, size));
            y += size;
            h = h.saturating_sub(size);
        }

        // --- 2. Outer bottom panels (full width, stacked from bottom) ---
        let outer_bottom: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Bottom && c.outer)
            .collect();
        for (id, _, constraints) in &outer_bottom {
            let size = clamp_size(constraints, h, MAIN_MIN_HEIGHT);
            if size == 0 {
                continue;
            }
            let panel_y = y + h - size;
            panels.insert(*id, Rect::new(x, panel_y, w, size));
            h = h.saturating_sub(size);
        }

        // --- 3. Left panels (stacked horizontally from left) ---
        let left: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, _)| *pos == PanelPosition::Left)
            .collect();
        let mut total_left: u16 = 0;
        for (id, _, constraints) in &left {
            let size = clamp_size(constraints, w - total_left, MAIN_MIN_WIDTH);
            if size == 0 {
                continue;
            }
            panels.insert(*id, Rect::new(x + total_left, y, size, h));
            total_left += size;
        }
        x += total_left;
        w = w.saturating_sub(total_left);

        // --- 4. Right panels (stacked horizontally from right) ---
        let right: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, _)| *pos == PanelPosition::Right)
            .collect();
        let mut total_right: u16 = 0;
        for (id, _, constraints) in &right {
            let size = clamp_size(constraints, w - total_right, MAIN_MIN_WIDTH);
            if size == 0 {
                continue;
            }
            total_right += size;
            let panel_x = x + w - total_right;
            panels.insert(*id, Rect::new(panel_x, y, size, h));
        }
        w = w.saturating_sub(total_right);

        // --- 5. Inner top panels (within remaining main column) ---
        let inner_top: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Top && !c.outer)
            .collect();
        for (id, _, constraints) in &inner_top {
            let size = clamp_size(constraints, h, MAIN_MIN_HEIGHT);
            if size == 0 {
                continue;
            }
            panels.insert(*id, Rect::new(x, y, w, size));
            y += size;
            h = h.saturating_sub(size);
        }

        // --- 6. Inner bottom panels (within remaining main column) ---
        let inner_bottom: Vec<_> = self
            .panels
            .iter()
            .filter(|(_, pos, c)| *pos == PanelPosition::Bottom && !c.outer)
            .collect();
        for (id, _, constraints) in &inner_bottom {
            let size = clamp_size(constraints, h, MAIN_MIN_HEIGHT);
            if size == 0 {
                continue;
            }
            let panel_y = y + h - size;
            panels.insert(*id, Rect::new(x, panel_y, w, size));
            h = h.saturating_sub(size);
        }

        // --- 7. Main gets the remaining area ---
        let main = Rect::new(x, y, w, h);

        ResolvedLayout { main, panels }
    }
}

const MAIN_MIN_WIDTH: u16 = 10;
const MAIN_MIN_HEIGHT: u16 = 5;

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_panels() {
        let engine = LayoutEngine::new();
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        assert_eq!(result.main, Rect::new(0, 0, 80, 24));
        assert!(result.panels.is_empty());
    }

    #[test]
    fn test_single_left_panel() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 40, outer: false, content_size: 25,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        assert_eq!(result.panels[&PanelId(0)].width, 25);
        assert_eq!(result.panels[&PanelId(0)].height, 24);
        assert!(result.main.width >= 10);
    }

    #[test]
    fn test_two_left_panels_horizontal() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 2, min_size: 15, max_size: 40, outer: false, content_size: 25,
        });
        engine.add_panel(PanelId(1), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 30, outer: false, content_size: 20,
        });
        let result = engine.resolve(Rect::new(0, 0, 100, 24));
        assert!(result.panels.contains_key(&PanelId(0)));
        assert!(result.panels.contains_key(&PanelId(1)));
        // Both at same y, different x
        assert_eq!(result.panels[&PanelId(0)].y, 0);
        assert_eq!(result.panels[&PanelId(1)].y, 0);
        assert!(result.panels[&PanelId(0)].x < result.panels[&PanelId(1)].x);
    }

    #[test]
    fn test_outer_top_full_width() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 25, outer: false, content_size: 20,
        });
        engine.add_panel(PanelId(1), PanelPosition::Top, PanelConstraints {
            weight: 1, min_size: 3, max_size: 10, outer: true, content_size: 5,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        assert_eq!(result.panels[&PanelId(1)].width, 80);
        assert_eq!(result.panels[&PanelId(1)].y, 0);
    }

    #[test]
    fn test_inner_top_between_panels() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 20, outer: false, content_size: 20,
        });
        engine.add_panel(PanelId(1), PanelPosition::Top, PanelConstraints {
            weight: 1, min_size: 3, max_size: 10, outer: false, content_size: 5,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        // Inner top should not span the left panel
        assert!(result.panels[&PanelId(1)].width < 80);
        assert!(result.panels[&PanelId(1)].x >= result.panels[&PanelId(0)].width);
    }

    #[test]
    fn test_main_enforced_minimum() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 30, max_size: 40, outer: false, content_size: 35,
        });
        engine.add_panel(PanelId(1), PanelPosition::Right, PanelConstraints {
            weight: 1, min_size: 30, max_size: 40, outer: false, content_size: 35,
        });
        let result = engine.resolve(Rect::new(0, 0, 60, 24));
        assert!(result.main.width >= 10);
    }

    #[test]
    fn test_left_and_right() {
        let mut engine = LayoutEngine::new();
        engine.add_panel(PanelId(0), PanelPosition::Left, PanelConstraints {
            weight: 1, min_size: 15, max_size: 25, outer: false, content_size: 20,
        });
        engine.add_panel(PanelId(1), PanelPosition::Right, PanelConstraints {
            weight: 1, min_size: 15, max_size: 25, outer: false, content_size: 20,
        });
        let result = engine.resolve(Rect::new(0, 0, 80, 24));
        // Left panel at x=0, right panel at far right
        assert_eq!(result.panels[&PanelId(0)].x, 0);
        let right_panel = result.panels[&PanelId(1)];
        assert_eq!(right_panel.x + right_panel.width, 80);
    }
}

//! Sidebar manager - handles sidebar state and panel rendering

use crate::component::{SidebarComponent, SidebarOutput, SidebarPosition};
use crate::context::ComponentContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for a sidebar
#[derive(Debug, Clone)]
pub struct SidebarConfig {
    pub position: SidebarPosition,
    pub width: u16,
    pub visible: bool,
    pub panels: Vec<String>,
    pub collapsed_panels: Vec<String>,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            position: SidebarPosition::Left,
            width: 30,
            visible: false,
            panels: vec![],
            collapsed_panels: vec![],
        }
    }
}

/// Data for a single sidebar panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarPanelData {
    pub id: String,
    pub title: String,
    pub collapsed: bool,
    pub output: SidebarOutput,
}

/// Rendered data for a complete sidebar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarData {
    pub position: SidebarPosition,
    pub width: u16,
    pub visible: bool,
    pub panels: Vec<SidebarPanelData>,
}

/// Manages sidebar state and rendering
pub struct SidebarManager {
    /// Registered sidebar components
    components: HashMap<String, Arc<dyn SidebarComponent>>,
    /// Left sidebar configuration
    left: SidebarConfig,
    /// Right sidebar configuration
    right: SidebarConfig,
    /// Panel collapse state
    collapsed: HashMap<String, bool>,
    /// Cache for rendered output
    cache: HashMap<String, SidebarOutput>,
}

impl SidebarManager {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            left: SidebarConfig {
                position: SidebarPosition::Left,
                width: 25,
                visible: false,
                panels: vec!["file_tree".to_string()],
                collapsed_panels: vec![],
            },
            right: SidebarConfig {
                position: SidebarPosition::Right,
                width: 30,
                visible: false,
                panels: vec![
                    "session".to_string(),
                    "mcp".to_string(),
                    "git_files".to_string(),
                    "context".to_string(),
                ],
                collapsed_panels: vec!["session".to_string()],
            },
            collapsed: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Register a sidebar component
    pub fn register(&mut self, component: Arc<dyn SidebarComponent>) {
        let id = component.id().to_string();
        self.components.insert(id, component);
    }

    /// Configure the left sidebar
    pub fn configure_left(&mut self, width: u16, panels: Vec<String>) {
        self.left.width = width;
        self.left.panels = panels;
    }

    /// Configure the right sidebar
    pub fn configure_right(&mut self, width: u16, panels: Vec<String>) {
        self.right.width = width;
        self.right.panels = panels;
    }

    /// Toggle visibility of a sidebar
    pub fn toggle(&mut self, position: SidebarPosition) {
        match position {
            SidebarPosition::Left => self.left.visible = !self.left.visible,
            SidebarPosition::Right => self.right.visible = !self.right.visible,
        }
    }

    /// Set visibility of a sidebar
    pub fn set_visible(&mut self, position: SidebarPosition, visible: bool) {
        match position {
            SidebarPosition::Left => self.left.visible = visible,
            SidebarPosition::Right => self.right.visible = visible,
        }
    }

    /// Check if a sidebar is visible
    pub fn is_visible(&self, position: SidebarPosition) -> bool {
        match position {
            SidebarPosition::Left => self.left.visible,
            SidebarPosition::Right => self.right.visible,
        }
    }

    /// Get sidebar width
    pub fn width(&self, position: SidebarPosition) -> u16 {
        match position {
            SidebarPosition::Left => self.left.width,
            SidebarPosition::Right => self.right.width,
        }
    }

    /// Toggle collapse state of a panel
    pub fn toggle_panel(&mut self, panel_id: &str) {
        let collapsed = self.collapsed.entry(panel_id.to_string()).or_insert(false);
        *collapsed = !*collapsed;
        // Invalidate cache
        self.cache.remove(panel_id);
    }

    /// Check if a panel is collapsed
    pub fn is_collapsed(&self, panel_id: &str) -> bool {
        *self.collapsed.get(panel_id).unwrap_or(&false)
    }

    /// Notify of a tuple space event for cache invalidation
    pub fn on_event(&mut self, event_type: &str) {
        // Invalidate cache for components that subscribe to this event
        for (id, component) in &self.components {
            for pattern in component.subscriptions() {
                if let Some(colony_tuple::Value::Str(subscribed)) = pattern.first() {
                    if subscribed == event_type {
                        self.cache.remove(id);
                        break;
                    }
                }
            }
        }
    }

    /// Invalidate all cache
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Render a sidebar
    pub fn render(&mut self, position: SidebarPosition, ctx: &ComponentContext) -> SidebarData {
        let (panel_ids, config_position, config_width, config_visible) = match position {
            SidebarPosition::Left => (
                self.left.panels.clone(),
                self.left.position,
                self.left.width,
                self.left.visible,
            ),
            SidebarPosition::Right => (
                self.right.panels.clone(),
                self.right.position,
                self.right.width,
                self.right.visible,
            ),
        };

        let mut panels = Vec::new();

        for id in panel_ids {
            let component = match self.components.get(&id) {
                Some(c) => c,
                None => continue,
            };

            let collapsed = self.is_collapsed(&id);
            let title = component.title().to_string();

            // Use cached output if available and not collapsed
            let output = if collapsed {
                SidebarOutput::hidden()
            } else if let Some(cached) = self.cache.get(&id) {
                cached.clone()
            } else {
                let output = component.render_block(ctx);
                self.cache.insert(id.clone(), output.clone());
                output
            };

            panels.push(SidebarPanelData {
                id,
                title,
                collapsed,
                output,
            });
        }

        SidebarData {
            position: config_position,
            width: config_width,
            visible: config_visible,
            panels,
        }
    }

    /// Render both sidebars
    pub fn render_all(&mut self, ctx: &ComponentContext) -> (SidebarData, SidebarData) {
        let left = self.render(SidebarPosition::Left, ctx);
        let right = self.render(SidebarPosition::Right, ctx);
        (left, right)
    }

    /// Handle an action on a panel
    pub fn handle_action(&self, panel_id: &str, action: &str, ctx: &ComponentContext) {
        if let Some(component) = self.components.get(panel_id) {
            component.handle_action(action, ctx);
        }
    }
}

impl Default for SidebarManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_visibility() {
        let mut manager = SidebarManager::new();
        assert!(!manager.is_visible(SidebarPosition::Left));

        manager.toggle(SidebarPosition::Left);
        assert!(manager.is_visible(SidebarPosition::Left));

        manager.toggle(SidebarPosition::Left);
        assert!(!manager.is_visible(SidebarPosition::Left));
    }

    #[test]
    fn test_panel_collapse() {
        let mut manager = SidebarManager::new();
        assert!(!manager.is_collapsed("file_tree"));

        manager.toggle_panel("file_tree");
        assert!(manager.is_collapsed("file_tree"));
    }
}

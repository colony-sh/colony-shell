//! Layout presets and pane definitions
//!
//! Named layout presets that define pane arrangements. Presets can be
//! defined in `.colonyrc` or `~/.colony/layouts.toml`.

use serde::{Deserialize, Serialize};

/// Direction of a pane split
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// A single pane definition within a layout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneDefinition {
    pub direction: SplitDirection,
    pub ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// A named layout preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPreset {
    pub name: String,
    pub panes: Vec<PaneDefinition>,
}

/// Manages layout presets
pub struct PaneLayout {
    presets: Vec<LayoutPreset>,
}

impl PaneLayout {
    /// Create with built-in default presets
    pub fn new() -> Self {
        Self {
            presets: vec![
                LayoutPreset {
                    name: "default".to_string(),
                    panes: vec![PaneDefinition {
                        direction: SplitDirection::Horizontal,
                        ratio: 1.0,
                        command: None,
                    }],
                },
                LayoutPreset {
                    name: "dev".to_string(),
                    panes: vec![
                        PaneDefinition {
                            direction: SplitDirection::Vertical,
                            ratio: 0.7,
                            command: None,
                        },
                        PaneDefinition {
                            direction: SplitDirection::Horizontal,
                            ratio: 0.5,
                            command: None,
                        },
                    ],
                },
                LayoutPreset {
                    name: "debug".to_string(),
                    panes: vec![
                        PaneDefinition {
                            direction: SplitDirection::Vertical,
                            ratio: 0.5,
                            command: None,
                        },
                        PaneDefinition {
                            direction: SplitDirection::Vertical,
                            ratio: 0.5,
                            command: None,
                        },
                    ],
                },
            ],
        }
    }

    /// Load additional presets from a TOML file
    pub fn load_from_toml(&mut self, toml_str: &str) -> std::result::Result<(), toml::de::Error> {
        let table: LayoutToml = toml::from_str(toml_str)?;
        for preset in table.layouts {
            // Replace existing presets with same name
            self.presets.retain(|p| p.name != preset.name);
            self.presets.push(preset);
        }
        Ok(())
    }

    /// Get a preset by name
    pub fn get(&self, name: &str) -> Option<&LayoutPreset> {
        self.presets.iter().find(|p| p.name == name)
    }

    /// List all available preset names
    pub fn preset_names(&self) -> Vec<&str> {
        self.presets.iter().map(|p| p.name.as_str()).collect()
    }
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// TOML deserialization wrapper
#[derive(Deserialize)]
struct LayoutToml {
    #[serde(default)]
    layouts: Vec<LayoutPreset>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_presets() {
        let layout = PaneLayout::new();
        assert!(layout.get("default").is_some());
        assert!(layout.get("dev").is_some());
        assert!(layout.get("debug").is_some());
        assert!(layout.get("nonexistent").is_none());
    }

    #[test]
    fn test_preset_names() {
        let layout = PaneLayout::new();
        let names = layout.preset_names();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"debug"));
    }

    #[test]
    fn test_load_from_toml() {
        let mut layout = PaneLayout::new();
        let toml_str = r#"
[[layouts]]
name = "custom"
panes = [
    { direction = "vertical", ratio = 0.6 },
    { direction = "horizontal", ratio = 0.4 }
]
"#;
        layout.load_from_toml(toml_str).unwrap();
        let custom = layout.get("custom").unwrap();
        assert_eq!(custom.panes.len(), 2);
        assert_eq!(custom.panes[0].direction, SplitDirection::Vertical);
    }

    #[test]
    fn test_toml_override_existing() {
        let mut layout = PaneLayout::new();
        let toml_str = r#"
[[layouts]]
name = "dev"
panes = [
    { direction = "horizontal", ratio = 0.8 }
]
"#;
        layout.load_from_toml(toml_str).unwrap();
        let dev = layout.get("dev").unwrap();
        assert_eq!(dev.panes.len(), 1);
        assert_eq!(dev.panes[0].ratio, 0.8);
    }

    #[test]
    fn test_pane_definition_serialization() {
        let pane = PaneDefinition {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            command: Some("cargo watch".to_string()),
        };
        let json = serde_json::to_string(&pane).unwrap();
        assert!(json.contains("\"direction\":\"vertical\""));
        assert!(json.contains("\"ratio\":0.5"));
        assert!(json.contains("\"command\":\"cargo watch\""));
    }

    #[test]
    fn test_pane_definition_no_command() {
        let pane = PaneDefinition {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            command: None,
        };
        let json = serde_json::to_string(&pane).unwrap();
        assert!(!json.contains("\"command\""));
    }
}

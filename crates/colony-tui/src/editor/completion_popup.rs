//! Completion popup widget state.
//!
//! Manages the state of a floating completion popup that appears above the
//! input editor. This module handles selection state, scrolling, and filtering.
//! Actual rendering is done by the TUI layer consuming this state.

use super::completion::CompletionItem;

/// Maximum number of visible items in the popup.
const MAX_VISIBLE_ITEMS: usize = 10;

/// State for the completion popup widget.
#[derive(Debug, Clone)]
pub struct CompletionPopup {
    /// All completion items (filtered, scored, sorted).
    items: Vec<CompletionItem>,
    /// Index of the currently selected item (None = no selection).
    selected: Option<usize>,
    /// Scroll offset (index of the first visible item).
    scroll_offset: usize,
    /// Whether the popup is visible.
    visible: bool,
}

impl Default for CompletionPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionPopup {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            scroll_offset: 0,
            visible: false,
        }
    }

    /// Show the popup with the given completion items.
    ///
    /// Resets selection to the first item.
    pub fn show(&mut self, items: Vec<CompletionItem>) {
        if items.is_empty() {
            self.hide();
            return;
        }
        self.items = items;
        self.selected = Some(0);
        self.scroll_offset = 0;
        self.visible = true;
    }

    /// Hide the popup and clear items.
    pub fn hide(&mut self) {
        self.items.clear();
        self.selected = None;
        self.scroll_offset = 0;
        self.visible = false;
    }

    /// Is the popup currently visible?
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get all items.
    pub fn items(&self) -> &[CompletionItem] {
        &self.items
    }

    /// Get the currently selected item, if any.
    pub fn selected_item(&self) -> Option<&CompletionItem> {
        self.selected.and_then(|idx| self.items.get(idx))
    }

    /// Get the selected index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Get the scroll offset.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Number of items visible in the popup window.
    pub fn visible_count(&self) -> usize {
        self.items.len().min(MAX_VISIBLE_ITEMS)
    }

    /// Get the slice of items currently visible in the popup.
    pub fn visible_items(&self) -> &[CompletionItem] {
        let end = (self.scroll_offset + MAX_VISIBLE_ITEMS).min(self.items.len());
        &self.items[self.scroll_offset..end]
    }

    /// Move selection to the next item (Tab / Down).
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let new_idx = match self.selected {
            Some(idx) if idx + 1 < self.items.len() => idx + 1,
            Some(_) | None => 0, // Wrap around
        };

        self.selected = Some(new_idx);
        self.ensure_selected_visible();
    }

    /// Move selection to the previous item (Shift+Tab / Up).
    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let new_idx = match self.selected {
            Some(idx) => {
                if idx > 0 {
                    idx - 1
                } else {
                    self.items.len() - 1 // Wrap around
                }
            }
            None => self.items.len() - 1,
        };

        self.selected = Some(new_idx);
        self.ensure_selected_visible();
    }

    /// Accept the currently selected item, returning its text.
    pub fn accept(&mut self) -> Option<String> {
        let text = self.selected_item().map(|item| item.text.clone());
        self.hide();
        text
    }

    /// Dismiss the popup without accepting.
    pub fn dismiss(&mut self) {
        self.hide();
    }

    /// Update items (e.g., when the prefix changes during typing).
    pub fn update_items(&mut self, items: Vec<CompletionItem>) {
        if items.is_empty() {
            self.hide();
            return;
        }

        let prev_selected_text = self.selected_item().map(|i| i.text.clone());
        self.items = items;

        // Try to maintain selection on the same text
        self.selected = if let Some(ref text) = prev_selected_text {
            self.items.iter().position(|i| i.text == *text).or(Some(0))
        } else {
            Some(0)
        };

        self.ensure_selected_visible();
        self.visible = true;
    }

    /// Ensure the selected item is within the visible scroll window.
    fn ensure_selected_visible(&mut self) {
        let Some(idx) = self.selected else { return };

        if idx < self.scroll_offset {
            self.scroll_offset = idx;
        } else if idx >= self.scroll_offset + MAX_VISIBLE_ITEMS {
            self.scroll_offset = idx + 1 - MAX_VISIBLE_ITEMS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::completion::CompletionKind;

    fn make_items(names: &[&str]) -> Vec<CompletionItem> {
        names
            .iter()
            .enumerate()
            .map(|(i, name)| CompletionItem {
                text: name.to_string(),
                label: name.to_string(),
                description: None,
                kind: CompletionKind::Command,
                score: 100 - i as i64,
                match_indices: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn test_popup_initially_hidden() {
        let popup = CompletionPopup::new();
        assert!(!popup.is_visible());
        assert!(popup.selected_item().is_none());
    }

    #[test]
    fn test_show_and_hide() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["ls", "less", "lsof"]));
        assert!(popup.is_visible());
        assert_eq!(popup.items().len(), 3);
        assert_eq!(popup.selected_index(), Some(0));

        popup.hide();
        assert!(!popup.is_visible());
        assert!(popup.items().is_empty());
    }

    #[test]
    fn test_show_empty_hides() {
        let mut popup = CompletionPopup::new();
        popup.show(Vec::new());
        assert!(!popup.is_visible());
    }

    #[test]
    fn test_select_next() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["a", "b", "c"]));

        assert_eq!(popup.selected_item().unwrap().text, "a");

        popup.select_next();
        assert_eq!(popup.selected_item().unwrap().text, "b");

        popup.select_next();
        assert_eq!(popup.selected_item().unwrap().text, "c");

        // Wrap around
        popup.select_next();
        assert_eq!(popup.selected_item().unwrap().text, "a");
    }

    #[test]
    fn test_select_prev() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["a", "b", "c"]));

        // Wrap to end
        popup.select_prev();
        assert_eq!(popup.selected_item().unwrap().text, "c");

        popup.select_prev();
        assert_eq!(popup.selected_item().unwrap().text, "b");
    }

    #[test]
    fn test_accept() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["ls", "less"]));
        popup.select_next(); // select "less"

        let text = popup.accept();
        assert_eq!(text, Some("less".to_string()));
        assert!(!popup.is_visible());
    }

    #[test]
    fn test_dismiss() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["a", "b"]));
        popup.dismiss();
        assert!(!popup.is_visible());
    }

    #[test]
    fn test_visible_items_with_scrolling() {
        let names: Vec<String> = (0..15).map(|i| format!("item_{}", i)).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let items = make_items(&name_refs);

        let mut popup = CompletionPopup::new();
        popup.show(items);

        assert_eq!(popup.visible_count(), 10);
        assert_eq!(popup.visible_items().len(), 10);
        assert_eq!(popup.visible_items()[0].text, "item_0");

        // Navigate to item 11 (index 10)
        for _ in 0..10 {
            popup.select_next();
        }

        assert_eq!(popup.selected_item().unwrap().text, "item_10");
        // Scroll should have adjusted
        assert!(popup.scroll_offset() > 0);
        let visible = popup.visible_items();
        assert!(visible.iter().any(|i| i.text == "item_10"));
    }

    #[test]
    fn test_update_items_maintains_selection() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["aaa", "bbb", "ccc"]));
        popup.select_next(); // select "bbb"

        popup.update_items(make_items(&["bbb", "ccc", "ddd"]));
        assert_eq!(popup.selected_item().unwrap().text, "bbb");
    }

    #[test]
    fn test_update_items_resets_when_not_found() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["aaa", "bbb"]));
        popup.select_next(); // select "bbb"

        popup.update_items(make_items(&["xxx", "yyy"]));
        assert_eq!(popup.selected_item().unwrap().text, "xxx");
    }

    #[test]
    fn test_update_empty_hides() {
        let mut popup = CompletionPopup::new();
        popup.show(make_items(&["a"]));
        popup.update_items(Vec::new());
        assert!(!popup.is_visible());
    }
}

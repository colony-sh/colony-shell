//! File node representation

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Unique identifier for a node (hash of path)
pub type NodeId = u64;

/// Type of file system entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Unknown or special file
    Other,
}

impl FileType {
    /// Get an icon for this file type
    pub fn icon(&self) -> &'static str {
        match self {
            FileType::File => "",
            FileType::Directory => "",
            FileType::Symlink => "",
            FileType::Other => "",
        }
    }
}

/// A node in the file tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Unique identifier (hash of path)
    pub id: NodeId,
    /// File/directory name (not full path)
    pub name: String,
    /// Full path from root
    pub path: PathBuf,
    /// Type of entry
    pub file_type: FileType,
    /// Size in bytes (for files)
    pub size: Option<u64>,
    /// Last modification time
    pub modified: Option<SystemTime>,
    /// Whether this node is expanded (for directories)
    pub expanded: bool,
    /// Children node IDs (for directories)
    pub children: Vec<NodeId>,
    /// Parent node ID (None for root)
    pub parent: Option<NodeId>,
    /// Depth in tree (0 for root)
    pub depth: usize,
    /// Whether this is a hidden file (starts with .)
    pub hidden: bool,
    /// File extension (for icon selection)
    pub extension: Option<String>,
}

impl FileNode {
    /// Create a new file node from a path
    pub fn new(path: PathBuf, parent: Option<NodeId>, depth: usize) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else if metadata.is_file() {
            FileType::File
        } else {
            FileType::Other
        };

        let size = if file_type == FileType::File {
            Some(metadata.len())
        } else {
            None
        };

        let modified = metadata.modified().ok();
        let hidden = name.starts_with('.');
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let id = Self::hash_path(&path);

        Ok(Self {
            id,
            name,
            path,
            file_type,
            size,
            modified,
            expanded: false,
            children: Vec::new(),
            parent,
            depth,
            hidden,
            extension,
        })
    }

    /// Create a root node for a directory
    pub fn root(path: PathBuf) -> std::io::Result<Self> {
        let mut node = Self::new(path, None, 0)?;
        node.expanded = true; // Root is always expanded
        Ok(node)
    }

    /// Hash a path to get a unique ID
    pub fn hash_path(path: &std::path::Path) -> NodeId {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    /// Check if this is a directory
    pub fn is_dir(&self) -> bool {
        self.file_type == FileType::Directory
    }

    /// Check if this is a file
    pub fn is_file(&self) -> bool {
        self.file_type == FileType::File
    }

    /// Toggle expanded state (for directories)
    pub fn toggle_expanded(&mut self) {
        if self.is_dir() {
            self.expanded = !self.expanded;
        }
    }

    /// Get the file icon based on type and extension
    pub fn icon(&self) -> &'static str {
        match self.file_type {
            FileType::Directory => {
                if self.expanded {
                    "\u{f07c}"
                } else {
                    "\u{f07b}"
                }
            }
            FileType::Symlink => "",
            FileType::File => self.file_icon(),
            FileType::Other => "",
        }
    }

    /// Get icon for file based on extension
    fn file_icon(&self) -> &'static str {
        file_icon_for_ext(self.extension.as_deref().unwrap_or(""))
    }

    /// Format file size for display
    pub fn format_size(&self) -> String {
        match self.size {
            Some(bytes) if bytes < 1024 => format!("{}B", bytes),
            Some(bytes) if bytes < 1024 * 1024 => format!("{:.1}K", bytes as f64 / 1024.0),
            Some(bytes) if bytes < 1024 * 1024 * 1024 => {
                format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
            }
            Some(bytes) => format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0)),
            None => String::new(),
        }
    }
}

/// Map a file extension to a Nerd Font icon codepoint.
///
/// This is the single canonical source of file-extension-to-icon mappings
/// used across colony-filetree, colony-filetree-bin, and colony-ui-core.
pub fn file_icon_for_ext(ext: &str) -> &'static str {
    match ext {
        // Rust
        "rs" => "\u{e7a8}",
        // JavaScript/TypeScript
        "js" => "\u{e74e}",
        "ts" => "\u{e628}",
        "jsx" | "tsx" => "\u{e7ba}",
        // Web
        "html" | "htm" => "\u{e736}",
        "css" | "scss" | "sass" => "\u{e749}",
        // Python
        "py" => "\u{e73c}",
        // Go
        "go" => "\u{e626}",
        // C/C++
        "c" | "h" => "\u{e61e}",
        "cpp" | "hpp" | "cc" | "cxx" => "\u{e61d}",
        // Java/Kotlin
        "java" => "\u{e738}",
        "kt" | "kts" => "\u{e634}",
        // Shell
        "sh" | "bash" | "zsh" => "\u{f489}",
        // Config
        "json" => "\u{e60b}",
        "yaml" | "yml" => "\u{f481}",
        "toml" => "\u{f0669}",
        "xml" => "\u{f05c0}",
        // Docs
        "md" | "markdown" => "\u{e73e}",
        "txt" => "\u{f0219}",
        "pdf" => "\u{f1c1}",
        // Git
        "gitignore" => "\u{e702}",
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "\u{f1c5}",
        // Archives
        "zip" | "tar" | "gz" | "rar" | "7z" => "\u{f1c6}",
        // Lock files
        "lock" => "\u{f023}",
        // Default
        _ => "\u{f15b}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_node_creation() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();

        let node = FileNode::new(file_path.clone(), None, 0).unwrap();
        assert_eq!(node.name, "test.rs");
        assert_eq!(node.file_type, FileType::File);
        assert_eq!(node.extension, Some("rs".to_string()));
        assert!(!node.hidden);
    }

    #[test]
    fn test_directory_node() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let node = FileNode::new(dir_path, None, 0).unwrap();
        assert!(node.is_dir());
        assert!(!node.expanded);
    }

    #[test]
    fn test_hidden_file() {
        let temp = TempDir::new().unwrap();
        let hidden_path = temp.path().join(".hidden");
        fs::write(&hidden_path, "secret").unwrap();

        let node = FileNode::new(hidden_path, None, 0).unwrap();
        assert!(node.hidden);
    }

    #[test]
    fn test_format_size() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let node = FileNode::new(file_path, None, 0).unwrap();
        assert_eq!(node.format_size(), "11B");
    }

    #[test]
    fn test_path_hashing() {
        let path1 = PathBuf::from("/a/b/c");
        let path2 = PathBuf::from("/a/b/c");
        let path3 = PathBuf::from("/a/b/d");

        assert_eq!(FileNode::hash_path(&path1), FileNode::hash_path(&path2));
        assert_ne!(FileNode::hash_path(&path1), FileNode::hash_path(&path3));
    }
}

//! filedetails: a panel binary that displays file metadata.
//!
//! Subscribes to `filetree_select` tuples and shows detailed information
//! about the currently selected file (name, path, size, permissions,
//! modification time, type, and line count for text files).

use colony_panel::protocol::{
    FrameLine, InputEvent, PanelConfig, SENTINEL_CONFIG, SENTINEL_FRAME,
};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::SystemTime;

fn main() {
    let stdout = io::stdout();
    let stdin = io::stdin();

    // Emit config: subscribe to filetree_select
    {
        let mut out = stdout.lock();
        writeln!(out, "{SENTINEL_CONFIG}").ok();
        let config = PanelConfig {
            subscriptions: vec!["filetree_select".to_string()],
        };
        writeln!(out, "{}", serde_json::to_string(&config).expect("config must serialize")).ok();
        out.flush().ok();
    }

    // Initial empty frame
    emit_frame(&stdout, &[]);

    // Event loop
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(event) = serde_json::from_str::<InputEvent>(&line) else {
            continue;
        };

        match event {
            InputEvent::Tuple { ref data } => {
                let Some(key) = data.get("key").and_then(|k| k.as_str()) else {
                    continue;
                };
                if key != "filetree_select" {
                    continue;
                }
                let Some(path_str) = data.get("value").and_then(|v| v.as_str()) else {
                    continue;
                };
                let lines = build_details(Path::new(path_str));
                emit_frame(&stdout, &lines);
            }
            InputEvent::Shutdown => break,
            InputEvent::Key { .. } | InputEvent::Resize { .. } | InputEvent::Focus { .. } => {}
        }
    }
}

fn emit_frame(stdout: &io::Stdout, lines: &[FrameLine]) {
    let mut out = stdout.lock();
    writeln!(out, "{SENTINEL_FRAME}").ok();
    for line in lines {
        writeln!(
            out,
            "{}",
            serde_json::to_string(line).expect("FrameLine must serialize")
        )
        .ok();
    }
    writeln!(out, "{SENTINEL_FRAME}").ok();
    out.flush().ok();
}

fn build_details(path: &Path) -> Vec<FrameLine> {
    let mut lines = Vec::new();

    // File name
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    lines.push(detail_line("bold,white", &name));

    // Full path
    lines.push(label_value("Path", &path.display().to_string()));

    // Stat the file; if it fails, show error and return
    let Ok(metadata) = fs::metadata(path) else {
        lines.push(detail_line("red", "Unable to read file metadata"));
        return lines;
    };

    // File type
    let file_type = if metadata.is_dir() {
        "Directory".to_string()
    } else if metadata.is_symlink() {
        "Symlink".to_string()
    } else {
        extension_type(path)
    };
    lines.push(label_value("Type", &file_type));

    // Size
    lines.push(label_value("Size", &human_readable_size(metadata.len())));

    // Permissions
    let mode = metadata.permissions().mode();
    lines.push(label_value("Permissions", &format_permissions(mode)));

    // Last modified
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| format_time(t).ok())
        .unwrap_or_else(|| "unknown".to_string());
    lines.push(label_value("Modified", &modified));

    // Line count for regular text files
    if metadata.is_file() && is_likely_text(path) {
        let count = count_lines(path);
        lines.push(label_value("Lines", &count.to_string()));
    }

    lines
}

fn detail_line(style: &str, text: &str) -> FrameLine {
    FrameLine {
        text: text.to_string(),
        icon: None,
        style: Some(style.to_string()),
        indent: None,
        action: None,
        expandable: None,
        expanded: None,
    }
}

fn label_value(label: &str, value: &str) -> FrameLine {
    FrameLine {
        text: format!("{label}: {value}"),
        icon: None,
        style: Some("dim".to_string()),
        indent: None,
        action: None,
        expandable: None,
        expanded: None,
    }
}

fn human_readable_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut size = bytes as f64;
    for &unit in UNITS {
        if size < 1024.0 {
            return if size.fract() < 0.05 {
                format!("{:.0} {unit}", size)
            } else {
                format!("{:.1} {unit}", size)
            };
        }
        size /= 1024.0;
    }
    format!("{:.1} PB", size)
}

fn format_permissions(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    let flags = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for &(bit, ch) in &flags {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    format!("{s} ({mode:o})")
}

fn format_time(time: SystemTime) -> Result<String, std::time::SystemTimeError> {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH)?;
    let secs = duration.as_secs();

    // Manual UTC breakdown (no chrono dependency)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Days since 1970-01-01
    let (year, month, day) = days_to_date(days);
    Ok(format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02} UTC"))
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm from Howard Hinnant
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn extension_type(path: &Path) -> String {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return "File".to_string();
    };
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "Rust source",
        "toml" => "TOML config",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "md" => "Markdown",
        "txt" => "Text",
        "sh" | "bash" | "zsh" => "Shell script",
        "py" => "Python",
        "js" => "JavaScript",
        "ts" => "TypeScript",
        "go" => "Go source",
        "c" => "C source",
        "cpp" | "cc" | "cxx" => "C++ source",
        "h" | "hpp" => "C/C++ header",
        "lock" => "Lock file",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "Image",
        "pdf" => "PDF document",
        "zip" | "tar" | "gz" | "bz2" | "xz" => "Archive",
        other => return format!(".{other} file"),
    }
    .to_string()
}

fn is_likely_text(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "rs" | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "md"
            | "txt"
            | "sh"
            | "bash"
            | "zsh"
            | "py"
            | "js"
            | "ts"
            | "go"
            | "c"
            | "cpp"
            | "cc"
            | "cxx"
            | "h"
            | "hpp"
            | "css"
            | "html"
            | "xml"
            | "lock"
            | "cfg"
            | "ini"
            | "env"
            | "log"
    )
}

fn count_lines(path: &Path) -> usize {
    let Ok(file) = fs::File::open(path) else {
        return 0;
    };
    BufReader::new(file).lines().count()
}

//! Build list data: scanning saved builds from the user's build directory.

use std::path::{Path, PathBuf};

/// A saved build entry (either a build file or a folder).
#[derive(Debug, Clone)]
pub enum BuildEntry {
    Build(BuildInfo),
    Folder(FolderInfo),
}

/// Metadata for a saved build file.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub file_name: String,
    pub build_name: String,
    pub full_path: PathBuf,
    pub sub_path: String,
    pub level: Option<u32>,
    pub class_name: Option<String>,
    pub ascend_class_name: Option<String>,
    pub modified: f64,
}

/// A subfolder in the build directory.
#[derive(Debug, Clone)]
pub struct FolderInfo {
    pub folder_name: String,
    pub full_path: PathBuf,
    pub sub_path: String,
    pub modified: f64,
}

/// Scan a build directory for .xml build files and subfolders.
pub fn scan_builds(build_path: &str, sub_path: &str) -> Vec<BuildEntry> {
    let dir = Path::new(build_path).join(sub_path);
    let mut entries = Vec::new();

    if !dir.is_dir() {
        log::warn!("Build directory not found: {}", dir.display());
        return entries;
    }

    let read_dir = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) => {
            log::warn!("Failed to read build directory: {e}");
            return entries;
        }
    };

    for entry in read_dir.filter_map(|e| e.ok()) {
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        let name = entry.file_name().to_string_lossy().to_string();
        let full_path = entry.path();
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        if file_type.is_dir() {
            entries.push(BuildEntry::Folder(FolderInfo {
                folder_name: name,
                full_path,
                sub_path: sub_path.to_string(),
                modified,
            }));
        } else if file_type.is_file() && name.ends_with(".xml") {
            let build_name = name.strip_suffix(".xml").unwrap_or(&name).to_string();
            let (level, class_name, ascend_class_name) = parse_build_header(&full_path);

            entries.push(BuildEntry::Build(BuildInfo {
                file_name: name,
                build_name,
                full_path,
                sub_path: sub_path.to_string(),
                level,
                class_name,
                ascend_class_name,
                modified,
            }));
        }
    }

    // Sort: folders first, then by name
    entries.sort_by(|a, b| {
        let a_is_folder = matches!(a, BuildEntry::Folder(_));
        let b_is_folder = matches!(b, BuildEntry::Folder(_));
        match (a_is_folder, b_is_folder) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => entry_name(a)
                .to_lowercase()
                .cmp(&entry_name(b).to_lowercase()),
        }
    });

    entries
}

fn entry_name(entry: &BuildEntry) -> &str {
    match entry {
        BuildEntry::Build(b) => &b.build_name,
        BuildEntry::Folder(f) => &f.folder_name,
    }
}

/// Parse the <Build> tag from the first few hundred bytes of a build XML
/// to extract level, className, and ascendClassName.
fn parse_build_header(path: &Path) -> (Option<u32>, Option<String>, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return (None, None, None),
    };

    // Only look at the first 500 chars for the <Build ...> tag
    let header = &text[..text.len().min(500)];
    let build_tag = match header.find("<Build ") {
        Some(start) => {
            let end = header[start..].find('>').map(|e| start + e + 1);
            match end {
                Some(end) => &header[start..end],
                None => return (None, None, None),
            }
        }
        None => return (None, None, None),
    };

    let level = extract_attr(build_tag, "level").and_then(|v| v.parse().ok());
    let class_name = extract_attr(build_tag, "className").map(|s| s.to_string());
    let ascend_class_name = extract_attr(build_tag, "ascendClassName").map(|s| s.to_string());

    (level, class_name, ascend_class_name)
}

fn extract_attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

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

/// Build a BuildInfo for an arbitrary .xml path (used by the recent list,
/// whose entries can live anywhere under the build directory).
pub fn build_info_from_path(path: &Path) -> Option<BuildInfo> {
    if !path.is_file() {
        return None;
    }
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let build_name = file_name
        .strip_suffix(".xml")
        .unwrap_or(&file_name)
        .to_string();
    let (level, class_name, ascend_class_name) = parse_build_header(path);
    let modified = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Some(BuildInfo {
        file_name,
        build_name,
        full_path: path.to_path_buf(),
        sub_path: String::new(),
        level,
        class_name,
        ascend_class_name,
        modified,
    })
}

/// Key stats parsed from a build XML for the hover preview tooltip.
#[derive(Debug, Clone, Default)]
pub struct BuildPreview {
    pub level: Option<u32>,
    pub class_name: Option<String>,
    pub ascend_class_name: Option<String>,
    /// (label, value) pairs of the headline stats stored in the XML.
    pub stats: Vec<(String, f64)>,
}

/// Parse the preview data (class, level, headline stats) from a build XML.
/// Build files store the last calc results as `<PlayerStat stat="..." .../>`
/// elements, so no Lua round-trip is needed.
pub fn build_preview(path: &Path) -> BuildPreview {
    let (level, class_name, ascend_class_name) = parse_build_header(path);
    let mut preview = BuildPreview {
        level,
        class_name,
        ascend_class_name,
        stats: Vec::new(),
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return preview;
    };

    let wanted = [
        ("CombinedDPS", "Combined DPS"),
        ("TotalDPS", "Hit DPS"),
        ("Life", "Life"),
        ("EnergyShield", "Energy Shield"),
        ("TotalEHP", "Effective Hit Pool"),
    ];
    for (stat, label) in wanted {
        let needle = format!("stat=\"{stat}\"");
        if let Some(pos) = text.find(&needle) {
            // The value attribute sits on the same element, either side of stat=
            let elem_start = text[..pos].rfind('<').unwrap_or(0);
            let elem_end = text[pos..].find('>').map(|e| pos + e).unwrap_or(pos);
            let elem = &text[elem_start..elem_end];
            if let Some(value) = extract_attr(elem, "value").and_then(|v| v.parse::<f64>().ok()) {
                preview.stats.push((label.to_string(), value));
            }
        }
    }
    preview
}

/// Path of the recent-builds list file (in the app's own data directory).
fn recent_builds_file() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pob-egui")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("recent_builds.txt"))
}

/// Load the recent-builds list, most recent first. Entries whose files no
/// longer exist are dropped.
pub fn load_recent_builds() -> Vec<PathBuf> {
    let Some(file) = recent_builds_file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    text.lines()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}

/// Record a build as most recently opened (deduplicated, capped at 10).
pub fn add_recent_build(path: &Path) {
    let Some(file) = recent_builds_file() else {
        return;
    };
    let mut list = load_recent_builds();
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(10);
    let text: String = list.iter().map(|p| format!("{}\n", p.display())).collect();
    if let Err(e) = std::fs::write(&file, text) {
        log::warn!("Failed to write recent builds: {e}");
    }
}

/// Validate a user-supplied build or folder name: non-empty, no path
/// separators, no leading/trailing whitespace surprises.
pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Name cannot contain / or \\".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("Invalid name".to_string());
    }
    Ok(())
}

/// Delete a build file.
pub fn delete_build(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| format!("Failed to delete build: {e}"))
}

/// Delete a folder and everything in it.
pub fn delete_folder(path: &Path) -> Result<(), String> {
    std::fs::remove_dir_all(path).map_err(|e| format!("Failed to delete folder: {e}"))
}

/// Rename a build file (new_name is the build name without extension) or a
/// folder. Fails if the target already exists.
pub fn rename_entry(path: &Path, new_name: &str, is_folder: bool) -> Result<PathBuf, String> {
    validate_name(new_name)?;
    let new_name = new_name.trim();
    let parent = path
        .parent()
        .ok_or_else(|| "Cannot determine parent directory".to_string())?;
    let target = if is_folder {
        parent.join(new_name)
    } else {
        parent.join(format!("{new_name}.xml"))
    };
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(format!("\"{new_name}\" already exists"));
    }
    std::fs::rename(path, &target).map_err(|e| format!("Failed to rename: {e}"))?;
    Ok(target)
}

/// Create a new subfolder in the given directory.
pub fn create_folder(build_path: &str, sub_path: &str, name: &str) -> Result<(), String> {
    validate_name(name)?;
    let target = Path::new(build_path).join(sub_path).join(name.trim());
    if target.exists() {
        return Err(format!("\"{}\" already exists", name.trim()));
    }
    std::fs::create_dir(&target).map_err(|e| format!("Failed to create folder: {e}"))
}

/// Move a build file into another directory. Fails if a file with the same
/// name already exists there.
pub fn move_build(path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file_name = path
        .file_name()
        .ok_or_else(|| "Invalid build path".to_string())?;
    let target = dest_dir.join(file_name);
    if target.exists() {
        return Err(format!(
            "\"{}\" already exists in the target folder",
            file_name.to_string_lossy()
        ));
    }
    std::fs::rename(path, &target).map_err(|e| format!("Failed to move build: {e}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::write(path, "<PathOfBuilding></PathOfBuilding>").unwrap();
    }

    #[test]
    fn validate_name_rejects_bad_names() {
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("a\\b").is_err());
        assert!(validate_name(".").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("My Build").is_ok());
    }

    #[test]
    fn delete_build_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.xml");
        touch(&file);
        delete_build(&file).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn delete_folder_removes_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("folder");
        std::fs::create_dir(&sub).unwrap();
        touch(&sub.join("a.xml"));
        delete_folder(&sub).unwrap();
        assert!(!sub.exists());
    }

    #[test]
    fn rename_build_appends_xml_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("old.xml");
        touch(&file);
        let new_path = rename_entry(&file, "new", false).unwrap();
        assert_eq!(new_path, dir.path().join("new.xml"));
        assert!(new_path.exists());
        assert!(!file.exists());
    }

    #[test]
    fn rename_folder_keeps_plain_name() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("old");
        std::fs::create_dir(&sub).unwrap();
        let new_path = rename_entry(&sub, "new", true).unwrap();
        assert_eq!(new_path, dir.path().join("new"));
        assert!(new_path.is_dir());
    }

    #[test]
    fn rename_refuses_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.xml");
        let b = dir.path().join("b.xml");
        touch(&a);
        touch(&b);
        assert!(rename_entry(&a, "b", false).is_err());
        assert!(a.exists());
    }

    #[test]
    fn rename_to_same_name_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.xml");
        touch(&a);
        let result = rename_entry(&a, "a", false).unwrap();
        assert_eq!(result, a);
        assert!(a.exists());
    }

    #[test]
    fn create_folder_and_reject_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_str().unwrap();
        create_folder(base, "", "sub").unwrap();
        assert!(dir.path().join("sub").is_dir());
        assert!(create_folder(base, "", "sub").is_err());
    }

    #[test]
    fn move_build_between_folders() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let file = dir.path().join("a.xml");
        touch(&file);
        move_build(&file, &sub).unwrap();
        assert!(sub.join("a.xml").exists());
        assert!(!file.exists());
    }

    #[test]
    fn move_build_refuses_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let file = dir.path().join("a.xml");
        touch(&file);
        touch(&sub.join("a.xml"));
        assert!(move_build(&file, &sub).is_err());
        assert!(file.exists());
    }

    #[test]
    fn build_preview_parses_header_and_stats() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hero.xml");
        std::fs::write(
            &file,
            "<PathOfBuilding><Build level=\"92\" className=\"Witch\" \
             ascendClassName=\"Necromancer\">\
             <PlayerStat stat=\"Life\" value=\"5432\"/>\
             <PlayerStat stat=\"CombinedDPS\" value=\"1234567.89\"/>\
             </Build></PathOfBuilding>",
        )
        .unwrap();

        let preview = build_preview(&file);
        assert_eq!(preview.level, Some(92));
        assert_eq!(preview.ascend_class_name.as_deref(), Some("Necromancer"));
        assert!(
            preview
                .stats
                .iter()
                .any(|(l, v)| l == "Life" && (*v - 5432.0).abs() < 0.01)
        );
        assert!(
            preview
                .stats
                .iter()
                .any(|(l, v)| l == "Combined DPS" && (*v - 1234567.89).abs() < 0.01)
        );
    }

    #[test]
    fn build_info_from_path_reads_header() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("My Build.xml");
        std::fs::write(
            &file,
            "<PathOfBuilding><Build level=\"12\" className=\"Duelist\"></Build></PathOfBuilding>",
        )
        .unwrap();
        let info = build_info_from_path(&file).expect("info");
        assert_eq!(info.build_name, "My Build");
        assert_eq!(info.level, Some(12));
        assert!(build_info_from_path(&dir.path().join("missing.xml")).is_none());
    }

    #[test]
    fn scan_finds_builds_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("folder")).unwrap();
        std::fs::write(
            dir.path().join("hero.xml"),
            "<PathOfBuilding><Build level=\"92\" className=\"Witch\" \
             ascendClassName=\"Necromancer\"></Build></PathOfBuilding>",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a build").unwrap();

        let entries = scan_builds(dir.path().to_str().unwrap(), "");
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            BuildEntry::Folder(f) => assert_eq!(f.folder_name, "folder"),
            BuildEntry::Build(_) => panic!("expected folder first"),
        }
        match &entries[1] {
            BuildEntry::Build(b) => {
                assert_eq!(b.build_name, "hero");
                assert_eq!(b.level, Some(92));
                assert_eq!(b.class_name.as_deref(), Some("Witch"));
                assert_eq!(b.ascend_class_name.as_deref(), Some("Necromancer"));
            }
            BuildEntry::Folder(_) => panic!("expected build second"),
        }
    }
}

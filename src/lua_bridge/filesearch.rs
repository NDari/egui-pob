//! NewFileSearch implementation using glob for pattern matching.
//!
//! Returns a Lua userdata handle with methods:
//! - GetFileName() → string (filename only, not full path)
//! - GetFileModifiedTime() → number (seconds since epoch)
//! - NextFile() → boolean (advance to next match, false if no more)

use mlua::prelude::*;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

struct FileSearchHandle {
    entries: Vec<FileEntry>,
    index: usize,
}

struct FileEntry {
    name: String,
    modified: f64,
}

impl FileSearchHandle {
    fn current(&self) -> Option<&FileEntry> {
        self.entries.get(self.index)
    }
}

impl LuaUserData for FileSearchHandle {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("GetFileName", |_, this, ()| {
            Ok(this.current().map(|e| e.name.clone()).unwrap_or_default())
        });

        methods.add_method("GetFileModifiedTime", |_, this, ()| {
            Ok(this.current().map(|e| e.modified).unwrap_or(0.0))
        });

        methods.add_method_mut("NextFile", |_, this, ()| {
            this.index += 1;
            Ok(this.index < this.entries.len())
        });
    }
}

/// Register the NewFileSearch global function.
pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();
    g.set(
        "NewFileSearch",
        lua.create_function(|lua, (pattern, folders_only): (String, Option<bool>)| {
            let folders_only = folders_only.unwrap_or(false);
            let entries = search_files(&pattern, folders_only);
            if entries.is_empty() {
                return Ok(LuaValue::Nil);
            }
            let handle = FileSearchHandle { entries, index: 0 };
            lua.create_userdata(handle).map(LuaValue::UserData)
        })?,
    )?;
    Ok(())
}

fn search_files(pattern: &str, folders_only: bool) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    if folders_only {
        // For folder search: pattern is "path/*" — list subdirectories
        let dir = if let Some(stripped) = pattern.strip_suffix("/*") {
            PathBuf::from(stripped)
        } else if let Some(stripped) = pattern.strip_suffix("*") {
            PathBuf::from(stripped)
        } else {
            PathBuf::from(pattern)
        };

        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.filter_map(|e| e.ok()) {
                if let Ok(ft) = entry.file_type()
                    && ft.is_dir()
                {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    entries.push(FileEntry { name, modified });
                }
            }
        }
    } else {
        // For file search: use glob pattern matching
        match glob::glob(pattern) {
            Ok(paths) => {
                for path in paths.filter_map(|p| p.ok()) {
                    if path.is_file() {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let modified = path
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs_f64())
                            .unwrap_or(0.0);
                        entries.push(FileEntry { name, modified });
                    }
                }
            }
            Err(e) => {
                log::warn!("NewFileSearch: invalid pattern '{pattern}': {e}");
            }
        }
    }

    entries
}

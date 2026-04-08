//! Config option data: reading and writing build configuration from Lua.

use mlua::prelude::*;

/// A config option definition read from upstream's ConfigOptions.lua.
#[derive(Debug, Clone)]
pub enum ConfigOption {
    /// Section header (visual separator).
    Section { label: String },
    Check {
        var: String,
        label: String,
        value: bool,
    },
    Count {
        var: String,
        label: String,
        value: String,
    },
    List {
        var: String,
        label: String,
        options: Vec<ListEntry>,
        selected_index: usize,
    },
    Text {
        var: String,
        label: String,
        value: String,
    },
}

#[derive(Debug, Clone)]
pub struct ListEntry {
    pub label: String,
    pub val: LuaValueKind,
}

/// Simplified representation of Lua values for list option entries.
#[derive(Debug, Clone, PartialEq)]
pub enum LuaValueKind {
    String(String),
    Number(f64),
    Integer(i64),
    Bool(bool),
    Nil,
}

impl ConfigOption {
    pub fn var(&self) -> Option<&str> {
        match self {
            ConfigOption::Section { .. } => None,
            ConfigOption::Check { var, .. }
            | ConfigOption::Count { var, .. }
            | ConfigOption::List { var, .. }
            | ConfigOption::Text { var, .. } => Some(var),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ConfigOption::Section { label }
            | ConfigOption::Check { label, .. }
            | ConfigOption::Count { label, .. }
            | ConfigOption::List { label, .. }
            | ConfigOption::Text { label, .. } => label,
        }
    }
}

/// Extract config option definitions and current values from the Lua VM.
pub fn extract_config_options(lua: &Lua) -> Result<Vec<ConfigOption>, mlua::Error> {
    let build: LuaTable = lua
        .load("return mainObject_ref.main.modes['BUILD']")
        .eval()?;
    let config_tab: LuaTable = build.get("configTab")?;
    let input: LuaTable = config_tab.get("input")?;

    // ConfigOptions.lua is loaded as a local in ConfigTab.lua.
    // We load it directly to get the option definitions.
    let option_list: LuaTable = lua
        .load("return LoadModule('Modules/ConfigOptions')")
        .eval()?;

    let mut options = Vec::new();

    for pair in option_list.pairs::<i64, LuaTable>() {
        let (_, entry) = pair?;

        // Check if this is a section header (has section, no var)
        if let Ok(section) = entry.get::<String>("section") {
            options.push(ConfigOption::Section { label: section });
            continue;
        }

        // Skip entries without a var (spacers)
        let var: String = match entry.get("var") {
            Ok(v) => v,
            Err(_) => continue,
        };

        let label: String = entry.get("label").unwrap_or_default();
        let opt_type: String = entry.get("type").unwrap_or_default();

        // Strip PoB color codes from label for display
        let label = strip_color_codes(&label);

        match opt_type.as_str() {
            "check" => {
                let value: bool = input.get(var.as_str()).unwrap_or(false);
                options.push(ConfigOption::Check { var, label, value });
            }
            "count" | "countAllowZero" | "integer" | "float" => {
                let value: String = match input.get::<LuaValue>(var.as_str()) {
                    Ok(LuaValue::Number(n)) => format!("{n}"),
                    Ok(LuaValue::Integer(n)) => format!("{n}"),
                    Ok(LuaValue::String(s)) => {
                        s.to_str().map(|s| s.to_string()).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                options.push(ConfigOption::Count { var, label, value });
            }
            "list" => {
                let list_entries = parse_list_options(&entry)?;
                let current_val = input.get::<LuaValue>(var.as_str()).ok();
                let selected_index = find_selected_index(&list_entries, &current_val);
                options.push(ConfigOption::List {
                    var,
                    label,
                    options: list_entries,
                    selected_index,
                });
            }
            "text" => {
                let value: String = input.get(var.as_str()).unwrap_or_default();
                options.push(ConfigOption::Text { var, label, value });
            }
            _ => {
                // Unknown type — skip
                log::debug!("Skipping unknown config type '{opt_type}' for var '{var}'");
            }
        }
    }

    Ok(options)
}

/// Write a config value back to Lua and trigger recalculation.
pub fn set_config_value(lua: &Lua, var: &str, value: LuaValue) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        configTab.input["{var}"] = ...
        build.buildFlag = true
        configTab:BuildModList()
    "#
    ))
    .call::<()>(value)?;

    // Run a frame to trigger recalculation
    lua.load("_runCallback('OnFrame')").exec()?;
    Ok(())
}

fn parse_list_options(entry: &LuaTable) -> Result<Vec<ListEntry>, mlua::Error> {
    let list: LuaTable = entry.get("list")?;
    let mut entries = Vec::new();

    for pair in list.pairs::<i64, LuaTable>() {
        let (_, item) = pair?;
        let label: String = item.get("label").unwrap_or_default();
        let val = match item.get::<LuaValue>("val")? {
            LuaValue::String(s) => {
                LuaValueKind::String(s.to_str().map(|s| s.to_string()).unwrap_or_default())
            }
            LuaValue::Number(n) => LuaValueKind::Number(n),
            LuaValue::Integer(n) => LuaValueKind::Integer(n),
            LuaValue::Boolean(b) => LuaValueKind::Bool(b),
            _ => LuaValueKind::Nil,
        };
        entries.push(ListEntry { label, val });
    }

    Ok(entries)
}

fn find_selected_index(entries: &[ListEntry], current_val: &Option<LuaValue>) -> usize {
    let Some(val) = current_val else {
        return 0;
    };

    let target = match val {
        LuaValue::String(s) => {
            LuaValueKind::String(s.to_str().map(|s| s.to_string()).unwrap_or_default())
        }
        LuaValue::Number(n) => LuaValueKind::Number(*n),
        LuaValue::Integer(n) => LuaValueKind::Integer(*n),
        LuaValue::Boolean(b) => LuaValueKind::Bool(*b),
        _ => return 0,
    };

    entries.iter().position(|e| e.val == target).unwrap_or(0)
}

/// Strip PoB color escape codes (^0-^9 and ^xRRGGBB) from text.
fn strip_color_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_digit() {
                i += 2;
                continue;
            } else if next == b'x'
                && i + 8 <= bytes.len()
                && bytes[i + 2..i + 8].iter().all(|b| b.is_ascii_hexdigit())
            {
                i += 8;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

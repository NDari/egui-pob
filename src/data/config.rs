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
        tooltip: Option<String>,
        vis: Visibility,
    },
    Count {
        var: String,
        label: String,
        value: String,
        tooltip: Option<String>,
        vis: Visibility,
    },
    List {
        var: String,
        label: String,
        options: Vec<ListEntry>,
        selected_index: usize,
        tooltip: Option<String>,
        vis: Visibility,
    },
    Text {
        var: String,
        label: String,
        value: String,
        tooltip: Option<String>,
        vis: Visibility,
    },
}

/// Per-option visibility state, mirroring upstream `ConfigTab`'s `control.shown`.
///
/// `relevant` and `show_all_excluded` come straight from upstream's shared
/// `Modules/ConfigVisibility`; the rest reproduce the wrapper that ConfigTab
/// layers on top of it (ConfigTab.lua `if not varData.hideIfInvalid then ...`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Visibility {
    /// `ConfigVisibility.isRelevantForBuild`: every `ifX` predicate passes.
    pub relevant: bool,
    /// `ConfigVisibility.isShowAllExcluded`: stays hidden even under "show all".
    pub show_all_excluded: bool,
    /// The current value differs from the option's default.
    pub modified: bool,
    /// Upstream `hideIfInvalid`: never list this when irrelevant, modified or not.
    pub hide_if_invalid: bool,
}

impl Visibility {
    /// Upstream's inner `control.shown`: the predicates pass, or the "show all"
    /// toggle overrides them for an option that is not on the exclusion list.
    pub fn eligible(&self, show_all: bool) -> bool {
        self.relevant || (show_all && !self.show_all_excluded)
    }

    /// Upstream's outer `control.shown`: an ineligible option stays listed when
    /// the user has moved it off its default, unless it is `hideIfInvalid`.
    pub fn shown(&self, show_all: bool) -> bool {
        self.eligible(show_all) || (!self.hide_if_invalid && self.modified)
    }

    /// Listed only because it is modified. Upstream colours these red and adds
    /// an "invalid" line to the tooltip.
    pub fn invalid(&self, show_all: bool) -> bool {
        !self.eligible(show_all) && !self.hide_if_invalid && self.modified
    }
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

    pub fn vis(&self) -> Visibility {
        match self {
            // Sections are laid out by their children; upstream hides a section
            // when none of its controls are shown (ConfigTab.lua UpdateControls).
            ConfigOption::Section { .. } => Visibility {
                relevant: true,
                ..Default::default()
            },
            ConfigOption::Check { vis, .. }
            | ConfigOption::Count { vis, .. }
            | ConfigOption::List { vis, .. }
            | ConfigOption::Text { vis, .. } => *vis,
        }
    }

    pub fn tooltip(&self) -> Option<&str> {
        match self {
            ConfigOption::Section { .. } => None,
            ConfigOption::Check { tooltip, .. }
            | ConfigOption::Count { tooltip, .. }
            | ConfigOption::List { tooltip, .. }
            | ConfigOption::Text { tooltip, .. } => tooltip.as_deref(),
        }
    }
}

/// Extract config option definitions and current values from the Lua VM.
///
/// Returns tooltip text and per-option visibility computed from the current build state
/// (nodes allocated, conditions used, etc.).
pub fn extract_config_options(lua: &Lua) -> Result<Vec<ConfigOption>, mlua::Error> {
    let build: LuaTable = lua
        .load("return mainObject_ref.main.modes['BUILD']")
        .eval()?;
    let config_tab: LuaTable = build.get("configTab")?;
    let input: LuaTable = config_tab.get("input")?;

    let option_list: LuaTable = lua
        .load("return LoadModule('Modules/ConfigOptions')")
        .eval()?;

    // Visibility comes from upstream's shared ConfigVisibility module, the same
    // one the Compare tab uses, so the two views cannot drift apart. The extra
    // `modified` / `hideIfInvalid` state reproduces the wrapper ConfigTab layers
    // on top of it (ConfigTab.lua, `if not varData.hideIfInvalid then ...`).
    let visibility_fn: LuaFunction = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local configTab = build.configTab
            local configVisibility = LoadModule("Modules/ConfigVisibility")

            return function(varData)
                local relevant = configVisibility.isRelevantForBuild(varData, build) and true or false
                local excluded = configVisibility.isShowAllExcluded(varData) and true or false

                local cur = configTab.input[varData.var]
                local modified = cur ~= nil and cur ~= configTab:GetDefaultState(varData.var, type(cur))

                -- tooltip: only the static string form (ignore tooltipFunc)
                local tooltip = type(varData.tooltip) == "string" and varData.tooltip or nil
                return relevant, excluded, modified, varData.hideIfInvalid and true or false, tooltip
            end
            "#,
        )
        .eval()?;

    let mut options = Vec::new();

    for pair in option_list.pairs::<i64, LuaTable>() {
        let (_, entry) = pair?;

        // Section header
        if let Ok(section) = entry.get::<String>("section") {
            options.push(ConfigOption::Section {
                label: strip_color_codes(&section),
            });
            continue;
        }

        let var: String = match entry.get("var") {
            Ok(v) => v,
            Err(_) => continue,
        };

        let label: String = entry.get("label").unwrap_or_default();
        let opt_type: String = entry.get("type").unwrap_or_default();
        let label = strip_color_codes(&label);

        let (relevant, show_all_excluded, modified, hide_if_invalid, tooltip_raw) = visibility_fn
            .call::<(bool, bool, bool, bool, Option<String>)>(entry.clone())
            .unwrap_or((true, false, false, false, None));
        let vis = Visibility {
            relevant,
            show_all_excluded,
            modified,
            hide_if_invalid,
        };
        let tooltip = tooltip_raw.map(|s| strip_color_codes(&s));

        match opt_type.as_str() {
            "check" => {
                let value: bool = input.get(var.as_str()).unwrap_or(false);
                options.push(ConfigOption::Check {
                    var,
                    label,
                    value,
                    tooltip,
                    vis,
                });
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
                options.push(ConfigOption::Count {
                    var,
                    label,
                    value,
                    tooltip,
                    vis,
                });
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
                    tooltip,
                    vis,
                });
            }
            "text" => {
                let value: String = input.get(var.as_str()).unwrap_or_default();
                options.push(ConfigOption::Text {
                    var,
                    label,
                    value,
                    tooltip,
                    vis,
                });
            }
            _ => {
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
        configTab:AddUndoState()
    "#
    ))
    .call::<()>(value)?;

    // Run a frame to trigger recalculation
    lua.load("_runCallback('OnFrame')").exec()?;
    Ok(())
}

/// Undo the last config change (Ctrl+Z), via upstream's UndoHandler on
/// ConfigTab. RestoreUndoState rebuilds the mod list itself.
pub fn undo(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.configTab:Undo()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
}

/// Redo the last undone config change (Ctrl+Y).
pub fn redo(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.configTab:Redo()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
}

/// Reset every config option in the active config set to its default value,
/// mirroring upstream's `ConfigTab:NewConfigSet` default initialization.
pub fn reset_config_to_defaults(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        local varList = LoadModule("Modules/ConfigOptions")
        wipeTable(configTab.input)
        wipeTable(configTab.placeholder)
        for _, varData in ipairs(varList) do
            if varData.var then
                configTab.input[varData.var] = varData.defaultState
                configTab.placeholder[varData.var] = varData.defaultPlaceholderState
                if varData.defaultIndex then
                    configTab.input[varData.var] = varData.list[varData.defaultIndex].val
                end
            end
        end
        configTab:UpdateControls()
        configTab:BuildModList()
        configTab:AddUndoState()
        build.buildFlag = true
    "#,
    )
    .exec()?;

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
pub fn strip_color_codes(text: &str) -> String {
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

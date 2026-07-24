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
        visible: bool,
    },
    Count {
        var: String,
        label: String,
        value: String,
        tooltip: Option<String>,
        visible: bool,
    },
    List {
        var: String,
        label: String,
        options: Vec<ListEntry>,
        selected_index: usize,
        tooltip: Option<String>,
        visible: bool,
    },
    Text {
        var: String,
        label: String,
        value: String,
        tooltip: Option<String>,
        visible: bool,
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

    pub fn is_visible(&self) -> bool {
        match self {
            ConfigOption::Section { .. } => true,
            ConfigOption::Check { visible, .. }
            | ConfigOption::Count { visible, .. }
            | ConfigOption::List { visible, .. }
            | ConfigOption::Text { visible, .. } => *visible,
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

    // Helper: evaluate visibility and tooltip for one entry in Lua.
    let visibility_fn: LuaFunction = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local env = build.calcsTab and build.calcsTab.mainEnv
            local spec = build.spec
            local input = build.configTab.input

            local function anyMatch(v, check)
                if type(v) == "table" then
                    for _, entry in ipairs(v) do
                        if check(entry) then return true end
                    end
                    return false
                end
                return check(v) and true or false
            end

            return function(varData)
                local shown = true
                local function fail(check)
                    return not anyMatch(check, function() return true end) or false
                end

                if varData.ifNode then
                    shown = shown and anyMatch(varData.ifNode, function(nodeId)
                        if spec.allocNodes[nodeId] then return true end
                        local node = spec.nodes[nodeId]
                        if node and node.type == "Keystone" and env
                           and env.keystonesAdded and env.keystonesAdded[node.dn] then
                            return true
                        end
                        return false
                    end)
                end
                if shown and varData.ifOption then
                    shown = shown and anyMatch(varData.ifOption, function(opt)
                        return input[opt] and true or false
                    end)
                end
                if shown and varData.ifCond then
                    shown = shown and anyMatch(varData.ifCond, function(cond)
                        return env and env.conditionsUsed and env.conditionsUsed[cond] and true or false
                    end)
                end
                if shown and varData.ifMinionCond then
                    shown = shown and anyMatch(varData.ifMinionCond, function(cond)
                        return env and env.minionConditionsUsed and env.minionConditionsUsed[cond] and true or false
                    end)
                end
                if shown and varData.ifEnemyCond then
                    shown = shown and anyMatch(varData.ifEnemyCond, function(cond)
                        return env and env.enemyConditionsUsed and env.enemyConditionsUsed[cond] and true or false
                    end)
                end
                if shown and varData.ifCondTrue then
                    shown = shown and anyMatch(varData.ifCondTrue, function(cond)
                        return env and env.player and env.player.modDB
                               and env.player.modDB.conditions[cond] and true or false
                    end)
                end
                if shown and varData.ifMult then
                    shown = shown and anyMatch(varData.ifMult, function(m)
                        return env and env.multipliersUsed and env.multipliersUsed[m] and true or false
                    end)
                end
                if shown and varData.ifEnemyMult then
                    shown = shown and anyMatch(varData.ifEnemyMult, function(m)
                        return env and env.enemyMultipliersUsed and env.enemyMultipliersUsed[m] and true or false
                    end)
                end
                if shown and varData.ifStat then
                    shown = shown and anyMatch(varData.ifStat, function(s)
                        return env and ((env.perStatsUsed and env.perStatsUsed[s])
                               or (env.enemyMultipliersUsed and env.enemyMultipliersUsed[s])) and true or false
                    end)
                end
                if shown and varData.ifSkill then
                    shown = shown and anyMatch(varData.ifSkill, function(name)
                        if not env or not env.player or not env.player.activeSkillList then return false end
                        for _, sk in ipairs(env.player.activeSkillList) do
                            if sk.activeEffect and sk.activeEffect.grantedEffect
                               and sk.activeEffect.grantedEffect.name == name then
                                return true
                            end
                        end
                        return false
                    end)
                end
                if shown and varData.ifSkillFlag then
                    shown = shown and anyMatch(varData.ifSkillFlag, function(flag)
                        if not env or not env.player or not env.player.mainSkill then return false end
                        local sk = env.player.mainSkill
                        return sk.skillFlags and sk.skillFlags[flag] or false
                    end)
                end
                if shown and varData.ifSkillData then
                    shown = shown and anyMatch(varData.ifSkillData, function(key)
                        if not env or not env.player or not env.player.mainSkill then return false end
                        local sk = env.player.mainSkill
                        return sk.skillData and sk.skillData[key] and true or false
                    end)
                end

                -- tooltip: only the static string form (ignore tooltipFunc)
                local tooltip = type(varData.tooltip) == "string" and varData.tooltip or nil
                return shown, tooltip
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

        let (visible, tooltip_raw) = visibility_fn
            .call::<(bool, Option<String>)>(entry.clone())
            .unwrap_or((true, None));
        let tooltip = tooltip_raw.map(|s| strip_color_codes(&s));

        match opt_type.as_str() {
            "check" => {
                let value: bool = input.get(var.as_str()).unwrap_or(false);
                options.push(ConfigOption::Check {
                    var,
                    label,
                    value,
                    tooltip,
                    visible,
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
                    visible,
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
                    visible,
                });
            }
            "text" => {
                let value: String = input.get(var.as_str()).unwrap_or_default();
                options.push(ConfigOption::Text {
                    var,
                    label,
                    value,
                    tooltip,
                    visible,
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

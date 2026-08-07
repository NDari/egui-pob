//! Custom modifier groups: the free-text modifier blocks a build carries in
//! its config, each sourced into the calc engine as `Custom:<title>`.
//!
//! The data model, mod building, undo state and XML persistence are all
//! upstream's (`ConfigTab`'s `customModsList`, `BuildModList`, `CreateUndoState`,
//! `Load`/`Save`) and run headless, so this module only reads the list and
//! edits it through the same commit sequence upstream's controls use.

use mlua::prelude::*;

/// One custom modifier group.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomModGroup {
    pub title: String,
    /// Disabled groups keep their text but contribute no mods.
    pub enabled: bool,
    pub text: String,
}

/// Whether a single line of a group's text parses into modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    /// Blank or whitespace only; contributes nothing and is not an error.
    Blank,
    /// `modLib.parseMod` returned mods and no leftover text.
    Parsed,
    /// Upstream's parser does not recognise this line, so it is ignored.
    Unsupported,
}

/// Lua that runs after every edit, matching what upstream's custom-mod
/// controls do in their callbacks: record undo, rebuild the mod list, and mark
/// the build dirty. `UpdateCustomModsControls` is deliberately not called - it
/// only rebuilds upstream's own controls (and appends to `configTab.controls`
/// every time), so we reproduce just its data guarantee where it matters.
const COMMIT: &str = r#"
    build.configTab:AddUndoState()
    build.configTab:BuildModList()
    build.buildFlag = true
"#;

fn run(lua: &Lua, body: &str, arg: LuaValue) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local set = build.configTab.configSets[build.configTab.activeConfigSetId]
        set.customModsList = set.customModsList or {{}}
        local list = set.customModsList
        local arg = ...
        {body}
        {COMMIT}
    "#
    ))
    .call::<()>(arg)?;

    // Run a frame so the recalculation lands before we read stats back.
    lua.load("_runCallback('OnFrame')").exec()
}

/// Read the active config set's custom modifier groups, in order.
pub fn list_groups(lua: &Lua) -> Result<Vec<CustomModGroup>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local set = build.configTab.configSets[build.configTab.activeConfigSetId]
            set.customModsList = set.customModsList or {}
            return set.customModsList
        "#,
        )
        .eval()?;

    let mut groups = Vec::new();
    for pair in list.pairs::<i64, LuaTable>() {
        let (_, block) = pair?;
        groups.push(CustomModGroup {
            title: block.get("title").unwrap_or_else(|_| "Default".to_string()),
            // Upstream treats only an explicit `false` as disabled.
            enabled: block.get::<Option<bool>>("enabled")?.unwrap_or(true),
            text: block.get("text").unwrap_or_default(),
        });
    }
    Ok(groups)
}

/// Append a new group, named the way upstream's "Add Mod Group" button names
/// them.
pub fn add_group(lua: &Lua) -> Result<(), mlua::Error> {
    run(
        lua,
        r#"table.insert(list, { title = "Group " .. (#list + 1), enabled = true, text = "" })"#,
        LuaValue::Nil,
    )
}

/// Delete the group at `index` (0-based). Deleting the last one leaves an
/// empty "Default" group behind, as upstream does, so a build always has a
/// group to type into.
pub fn delete_group(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    run(
        lua,
        r#"
        table.remove(list, arg)
        if #list == 0 then
            table.insert(list, { title = "Default", enabled = true, text = "" })
        end
        "#,
        LuaValue::Integer(index as i64 + 1),
    )
}

/// Rename the group at `index` (0-based). The title is the mod source, so this
/// re-sources every mod in the group.
pub fn set_title(lua: &Lua, index: usize, title: &str) -> Result<(), mlua::Error> {
    let arg = lua.create_table()?;
    arg.set(1, index as i64 + 1)?;
    arg.set(2, title)?;
    run(
        lua,
        r#"if list[arg[1]] then list[arg[1]].title = arg[2] end"#,
        LuaValue::Table(arg),
    )
}

/// Enable or disable the group at `index` (0-based).
pub fn set_enabled(lua: &Lua, index: usize, enabled: bool) -> Result<(), mlua::Error> {
    let arg = lua.create_table()?;
    arg.set(1, index as i64 + 1)?;
    arg.set(2, enabled)?;
    run(
        lua,
        r#"if list[arg[1]] then list[arg[1]].enabled = arg[2] end"#,
        LuaValue::Table(arg),
    )
}

/// Replace the modifier text of the group at `index` (0-based).
pub fn set_text(lua: &Lua, index: usize, text: &str) -> Result<(), mlua::Error> {
    let arg = lua.create_table()?;
    arg.set(1, index as i64 + 1)?;
    arg.set(2, text)?;
    run(
        lua,
        r#"if list[arg[1]] then list[arg[1]].text = arg[2] end"#,
        LuaValue::Table(arg),
    )
}

/// Classify each line of `text` by running it through upstream's modifier
/// parser, the same test upstream's editor uses to colour lines: a line counts
/// only when `modLib.parseMod` returns mods and no unparsed remainder.
///
/// Lines are split here rather than in Lua so the result indexes exactly like
/// the editor's own lines.
pub fn line_status(lua: &Lua, text: &str) -> Result<Vec<LineStatus>, mlua::Error> {
    let lines = lua.create_table()?;
    for (i, line) in text.split('\n').enumerate() {
        lines.set(i as i64 + 1, line)?;
    }

    let parsed: LuaTable = lua
        .load(
            r#"
            local lines = ...
            local out = {}
            for i = 1, #lines do
                local stripped = StripEscapes(lines[i]):match("^%s*(.-)%s*$")
                if stripped == "" then
                    out[i] = 0
                else
                    local mods, extra = modLib.parseMod(stripped)
                    out[i] = (mods and not extra) and 1 or 2
                end
            end
            return out
        "#,
        )
        .call::<LuaTable>(lines)?;

    let mut out = Vec::new();
    for i in 1..=text.split('\n').count() {
        out.push(match parsed.get::<i64>(i as i64).unwrap_or(0) {
            1 => LineStatus::Parsed,
            2 => LineStatus::Unsupported,
            _ => LineStatus::Blank,
        });
    }
    Ok(out)
}

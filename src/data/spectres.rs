//! Spectre library data: available spectres and the build's spectre list
//! (upstream's OpenSpectreLibrary popup, Build.lua).

use mlua::prelude::*;

use super::items::TooltipLine;

/// An available spectre. Display names are not unique; `id` (the metadata
/// path) is the identity.
#[derive(Debug, Clone)]
pub struct SpectreEntry {
    pub id: String,
    pub name: String,
    /// Skill names, for the library's skill-search modes.
    pub skills: Vec<String>,
}

/// All spectres from data.spectres, sorted by name then id like upstream's
/// source list.
pub fn list_available(lua: &Lua) -> Result<Vec<SpectreEntry>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local out = {}
        for id in pairs(build.data.spectres) do
            local minion = build.data.minions[id]
            if minion then
                local skills = {}
                for _, skillId in ipairs(minion.skillList or {}) do
                    local skill = build.data.skills[skillId]
                    if skill then
                        table.insert(skills, skill.name)
                    end
                end
                table.insert(out, { id = id, name = minion.name, skills = skills })
            end
        end
        table.sort(out, function(a, b)
            if a.name == b.name then
                return a.id < b.id
            else
                return a.name < b.name
            end
        end)
        return out
    "#,
        )
        .eval()?;
    let mut entries = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let skills: LuaTable = entry.get("skills")?;
        entries.push(SpectreEntry {
            id: entry.get("id").unwrap_or_default(),
            name: entry.get("name").unwrap_or_default(),
            skills: skills.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(entries)
}

/// The build's current spectre list as (id, name) pairs, in order.
pub fn list_in_build(lua: &Lua) -> Result<Vec<(String, String)>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local out = {}
        for _, id in ipairs(build.spectreList) do
            local minion = build.data.minions[id]
            table.insert(out, { id = id, name = minion and minion.name or id })
        end
        return out
    "#,
        )
        .eval()?;
    let mut entries = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        entries.push((
            entry.get("id").unwrap_or_default(),
            entry.get("name").unwrap_or_default(),
        ));
    }
    Ok(entries)
}

/// Commit a new spectre list (the library popup's Save), mirroring upstream:
/// build.spectreList is replaced and mod/build flags set so the next calc
/// pass picks it up. Persistence comes free through upstream Save.
pub fn set_spectre_list(lua: &Lua, ids: &[String]) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local ids = ...
        local build = mainObject_ref.main.modes['BUILD']
        build.spectreList = {}
        for _, id in ipairs(ids) do
            table.insert(build.spectreList, id)
        end
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call(ids.to_vec())
}

/// The spectre hover tooltip, via upstream MinionListControl:AddValueTooltip
/// (life/defence multipliers, resists, skills) captured headless.
pub fn spectre_tooltip(lua: &Lua, id: &str) -> Result<Vec<TooltipLine>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local id = ...
        local build = mainObject_ref.main.modes['BUILD']
        if not build.data.minions[id] then
            return { lines = {} }
        end
        local ctrl = new("MinionListControl", nil, {0, 0, 100, 100}, build.data, {})
        local tt = new("Tooltip")
        local ok, err = pcall(function()
            ctrl:AddValueTooltip(tt, 1, id)
        end)
        local lines = {}
        for _, line in ipairs(tt.lines) do
            table.insert(lines, {
                text = line.text or "",
                size = line.size or 16,
                sep = line.text == nil,
            })
        end
        return { lines = lines, err = not ok and tostring(err) or nil }
    "#,
        )
        .call(id)?;

    if let Ok(err) = result.get::<String>("err") {
        log::warn!("Spectre tooltip failed for {id}: {err}");
    }

    let lines_table: LuaTable = result.get("lines")?;
    let mut lines = Vec::new();
    for pair in lines_table.sequence_values::<LuaTable>() {
        let line = pair?;
        lines.push(TooltipLine {
            text: line.get("text").unwrap_or_default(),
            size: line.get("size").unwrap_or(16.0),
            is_separator: line.get("sep").unwrap_or(false),
        });
    }
    Ok(lines)
}

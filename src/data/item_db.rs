//! Unique and rare-template item databases (upstream's uniqueDB / rareDB).
//!
//! Both DBs are filled by a coroutine that upstream resumes once per frame
//! (`main.onFrameFuncs["LoadItems"]`), so callers must pump frames via
//! [`pump_loading`] until [`is_loading`] reports false before extracting.

use mlua::prelude::*;

use super::items::TooltipLine;

/// One entry from the unique or rare-template database.
#[derive(Debug, Clone)]
pub struct DbItem {
    /// Display name ("Title, Base Name").
    pub name: String,
    /// Base item type ("Body Armour", "One Handed Sword", ...).
    pub item_type: String,
    /// Raw item text (used for tooltips and for adding to the build).
    pub raw: String,
    /// Lowercased name, for search.
    pub search_name: String,
    /// Lowercased implicit + explicit mod lines, for search.
    pub search_mods: String,
}

/// True while the item databases are still being loaded.
pub fn is_loading(lua: &Lua) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local main = mainObject_ref.main
        return (main.uniqueDB.loading or main.rareDB.loading) and true or false
    "#,
    )
    .eval()
}

/// Resume the DB-loading coroutine up to `frames` times. Returns true while
/// still loading.
pub fn pump_loading(lua: &Lua, frames: u32) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local frames = ...
        local main = mainObject_ref.main
        for i = 1, frames do
            local f = main.onFrameFuncs and main.onFrameFuncs["LoadItems"]
            if not f then break end
            f()
        end
        return (main.uniqueDB.loading or main.rareDB.loading) and true or false
    "#,
    )
    .call(frames)
}

/// Extract all items from the unique (or rare-template) database, sorted by
/// name. Call only once loading has finished.
pub fn extract_db(lua: &Lua, unique: bool) -> Result<Vec<DbItem>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local unique = ...
        local main = mainObject_ref.main
        local db = unique and main.uniqueDB or main.rareDB
        local out = {}
        for name, item in pairs(db.list) do
            local mods = {}
            for _, m in ipairs(item.implicitModLines or {}) do
                table.insert(mods, m.line)
            end
            for _, m in ipairs(item.explicitModLines or {}) do
                table.insert(mods, m.line)
            end
            table.insert(out, {
                name = name,
                type = item.type or "?",
                raw = item:BuildRaw(),
                mods = table.concat(mods, "\n"),
            })
        end
        table.sort(out, function(a, b) return a.name:lower() < b.name:lower() end)
        return out
    "#,
        )
        .call(unique)?;

    let mut items = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let name: String = entry.get("name").unwrap_or_default();
        let mods: String = entry.get("mods").unwrap_or_default();
        items.push(DbItem {
            search_name: name.to_lowercase(),
            search_mods: mods.to_lowercase(),
            name,
            item_type: entry.get("type").unwrap_or_default(),
            raw: entry.get("raw").unwrap_or_default(),
        });
    }
    Ok(items)
}

/// Build the full upstream tooltip for raw item text (for DB items that are
/// not part of the build).
pub fn tooltip_from_raw(lua: &Lua, raw: &str) -> Result<Vec<TooltipLine>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local raw = ...
            local build = mainObject_ref.main.modes['BUILD']
            local item = new("Item", raw)
            if not item or not item.base then
                return { lines = {} }
            end
            local tt = new("Tooltip")
            local ok, err = pcall(function()
                build.itemsTab:AddItemTooltip(tt, item)
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
        .call(raw)?;

    if let Ok(err) = result.get::<String>("err") {
        log::warn!("AddItemTooltip failed for db item: {err}");
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

//! Config sets: multiple independent configuration value sets per build
//! (upstream's configSets / configSetOrderList / activeConfigSetId).

use mlua::prelude::*;

/// One config set, in display order.
#[derive(Debug, Clone)]
pub struct ConfigSetInfo {
    pub id: i64,
    pub title: String,
}

/// List config sets in order plus the active set id.
pub fn list_config_sets(lua: &Lua) -> Result<(Vec<ConfigSetInfo>, i64), mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local configTab = mainObject_ref.main.modes['BUILD'].configTab
        local out = { sets = {}, active = configTab.activeConfigSetId or 1 }
        for _, id in ipairs(configTab.configSetOrderList) do
            local set = configTab.configSets[id]
            table.insert(out.sets, { id = id, title = set and set.title or "" })
        end
        return out
    "#,
        )
        .eval()?;

    let mut sets = Vec::new();
    let list: LuaTable = result.get("sets")?;
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        sets.push(ConfigSetInfo {
            id: entry.get("id").unwrap_or(0),
            title: entry.get("title").unwrap_or_default(),
        });
    }
    Ok((sets, result.get("active").unwrap_or(1)))
}

/// Switch the active config set (swaps input/placeholder and rebuilds mods).
pub fn set_active_config_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.configTab:SetActiveConfigSet({id})
        build.configTab:AddUndoState()
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Create a new config set (default values) and make it active.
pub fn new_config_set(lua: &Lua, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        local set = configTab:NewConfigSet(nil, title)
        set.title = title
        table.insert(configTab.configSetOrderList, set.id)
        configTab:SetActiveConfigSet(set.id)
        configTab:AddUndoState()
        _runCallback('OnFrame')
    "#,
    )
    .call(title)
}

/// Copy a config set (full deep copy). Does not switch to the copy.
pub fn copy_config_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        local configSet = configTab.configSets[id]
        if not configSet then
            return
        end
        local newSet = copyTable(configSet)
        newSet.id = 1
        while configTab.configSets[newSet.id] do
            newSet.id = newSet.id + 1
        end
        newSet.title = title
        configTab.configSets[newSet.id] = newSet
        table.insert(configTab.configSetOrderList, newSet.id)
        configTab:AddUndoState()
        build:SyncLoadouts()
        _runCallback('OnFrame')
    "#,
    )
    .call((id, title))
}

/// Rename a config set.
pub fn rename_config_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        if configTab.configSets[id] then
            configTab.configSets[id].title = title
            configTab.modFlag = true
            configTab:AddUndoState()
            build:SyncLoadouts()
        end
    "#,
    )
    .call((id, title))
}

/// Delete a config set (the last one cannot be deleted). If it was active,
/// the previous set in the order becomes active.
pub fn delete_config_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local configTab = build.configTab
        local order = configTab.configSetOrderList
        if #order <= 1 then
            return
        end
        local index
        for i, sid in ipairs(order) do
            if sid == {id} then
                index = i
                break
            end
        end
        if not index then
            return
        end
        table.remove(order, index)
        configTab.configSets[{id}] = nil
        if configTab.activeConfigSetId == {id} then
            configTab:SetActiveConfigSet(order[math.max(1, index - 1)])
        end
        configTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

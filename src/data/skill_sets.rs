//! Skill sets: multiple independent socket-group lists per build
//! (upstream's skillSets / skillSetOrderList / activeSkillSetId).

use mlua::prelude::*;

/// One skill set, in display order.
#[derive(Debug, Clone)]
pub struct SkillSetInfo {
    pub id: i64,
    pub title: String,
}

/// List skill sets in order plus the active set id.
pub fn list_skill_sets(lua: &Lua) -> Result<(Vec<SkillSetInfo>, i64), mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local skillsTab = mainObject_ref.main.modes['BUILD'].skillsTab
        local out = { sets = {}, active = skillsTab.activeSkillSetId or 1 }
        for _, id in ipairs(skillsTab.skillSetOrderList) do
            local set = skillsTab.skillSets[id]
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
        sets.push(SkillSetInfo {
            id: entry.get("id").unwrap_or(0),
            title: entry.get("title").unwrap_or_default(),
        });
    }
    Ok((sets, result.get("active").unwrap_or(1)))
}

/// Switch the active skill set (upstream SetActiveSkillSet swaps the socket
/// group list and flags a rebuild).
pub fn set_active_skill_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.skillsTab:SetActiveSkillSet({id})
        build.skillsTab:AddUndoState()
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Create a new empty skill set with the given title and make it active.
pub fn new_skill_set(lua: &Lua, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local set = skillsTab:NewSkillSet()
        set.title = title
        table.insert(skillsTab.skillSetOrderList, set.id)
        skillsTab:SetActiveSkillSet(set.id)
        skillsTab:AddUndoState()
        _runCallback('OnFrame')
    "#,
    )
    .call(title)
}

/// Deep-copy a skill set (groups and gems), mirroring upstream's Copy button.
pub fn copy_skill_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local skillSet = skillsTab.skillSets[id]
        if not skillSet then
            return
        end
        local newSkillSet = copyTable(skillSet, true)
        newSkillSet.socketGroupList = { }
        for _, socketGroup in pairs(skillSet.socketGroupList) do
            local newGroup = copyTable(socketGroup, true)
            newGroup.gemList = { }
            for gemIndex, gem in pairs(socketGroup.gemList) do
                newGroup.gemList[gemIndex] = copyTable(gem, true)
            end
            table.insert(newSkillSet.socketGroupList, newGroup)
        end
        newSkillSet.id = 1
        while skillsTab.skillSets[newSkillSet.id] do
            newSkillSet.id = newSkillSet.id + 1
        end
        newSkillSet.title = title
        skillsTab.skillSets[newSkillSet.id] = newSkillSet
        table.insert(skillsTab.skillSetOrderList, newSkillSet.id)
        skillsTab:AddUndoState()
        build:SyncLoadouts()
        _runCallback('OnFrame')
    "#,
    )
    .call((id, title))
}

/// Rename a skill set.
pub fn rename_skill_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        if skillsTab.skillSets[id] then
            skillsTab.skillSets[id].title = title
            skillsTab.modFlag = true
            skillsTab:AddUndoState()
            build:SyncLoadouts()
        end
    "#,
    )
    .call((id, title))
}

/// Delete a skill set (the last one cannot be deleted). If it was active,
/// the previous set in the order becomes active, like upstream.
pub fn delete_skill_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local order = skillsTab.skillSetOrderList
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
        skillsTab.skillSets[{id}] = nil
        if skillsTab.activeSkillSetId == {id} then
            skillsTab:SetActiveSkillSet(order[math.max(1, index - 1)])
        end
        skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

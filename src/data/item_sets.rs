//! Item sets: multiple independent equipment selections per build
//! (upstream's itemSets / itemSetOrderList / activeItemSetId), plus the
//! weapon-swap flag of the active set.

use mlua::prelude::*;

/// One item set, in display order.
#[derive(Debug, Clone)]
pub struct ItemSetInfo {
    pub id: i64,
    pub title: String,
}

/// List item sets in order plus the active set id.
pub fn list_item_sets(lua: &Lua) -> Result<(Vec<ItemSetInfo>, i64), mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
        local out = { sets = {}, active = itemsTab.activeItemSetId or 1 }
        for _, id in ipairs(itemsTab.itemSetOrderList) do
            local set = itemsTab.itemSets[id]
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
        sets.push(ItemSetInfo {
            id: entry.get("id").unwrap_or(0),
            title: entry.get("title").unwrap_or_default(),
        });
    }
    Ok((sets, result.get("active").unwrap_or(1)))
}

/// Switch the active item set. Upstream's SetActiveItemSet stores the
/// outgoing set's slot selections and equips the incoming set's.
pub fn set_active_item_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.itemsTab:SetActiveItemSet({id})
        build.itemsTab:AddUndoState()
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Create a new empty item set with the given title and make it active.
pub fn new_item_set(lua: &Lua, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local set = itemsTab:NewItemSet()
        set.title = title
        table.insert(itemsTab.itemSetOrderList, set.id)
        itemsTab:SetActiveItemSet(set.id)
        itemsTab:AddUndoState()
        _runCallback('OnFrame')
    "#,
    )
    .call(title)
}

/// Copy an item set (full deep copy, like upstream's Copy button). Does not
/// switch to the copy.
pub fn copy_item_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local itemSet = itemsTab.itemSets[id]
        if not itemSet then
            return
        end
        local newSet = copyTable(itemSet)
        newSet.id = 1
        while itemsTab.itemSets[newSet.id] do
            newSet.id = newSet.id + 1
        end
        newSet.title = title
        itemsTab.itemSets[newSet.id] = newSet
        table.insert(itemsTab.itemSetOrderList, newSet.id)
        itemsTab:AddUndoState()
        build:SyncLoadouts()
        _runCallback('OnFrame')
    "#,
    )
    .call((id, title))
}

/// Rename an item set.
pub fn rename_item_set(lua: &Lua, id: i64, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local id, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        if itemsTab.itemSets[id] then
            itemsTab.itemSets[id].title = title
            itemsTab.modFlag = true
            itemsTab:AddUndoState()
            build:SyncLoadouts()
        end
    "#,
    )
    .call((id, title))
}

/// Delete an item set (the last one cannot be deleted). If it was active,
/// the previous set in the order becomes active.
pub fn delete_item_set(lua: &Lua, id: i64) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local order = itemsTab.itemSetOrderList
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
        itemsTab.itemSets[{id}] = nil
        if itemsTab.activeItemSetId == {id} then
            itemsTab:SetActiveItemSet(order[math.max(1, index - 1)])
        end
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Whether the active item set uses the second weapon set (weapon swap).
pub fn use_second_weapon_set(lua: &Lua) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
        return itemsTab.activeItemSet
            and itemsTab.activeItemSet.useSecondWeaponSet == true or false
    "#,
    )
    .eval()
}

/// Toggle the active item set's weapon swap.
pub fn set_use_second_weapon_set(lua: &Lua, enabled: bool) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        if itemsTab.activeItemSet then
            itemsTab.activeItemSet.useSecondWeaponSet = {enabled}
            itemsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

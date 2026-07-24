//! Loadouts: named combinations of tree spec + item set + skill set + config
//! set, matched by title (exactly, or via upstream's `{linkId}` brace groups).
//!
//! All logic is upstream's: `SyncLoadouts` computes the list into the
//! `buildLoadouts` dropdown control, and the control's callback activates all
//! four sets. We drive both rather than reimplementing the matching rules.

use mlua::prelude::*;

/// Special entries upstream mixes into the dropdown list.
const SPECIAL_ENTRIES: &[&str] = &[
    "No Loadouts",
    "^7^7Loadouts:",
    "^7^7-----",
    "^7^7New Loadout",
    "^7^7Sync",
    "^7^7Help >>",
];

/// List available loadouts and the currently matched one (None when the
/// active sets don't line up with any loadout).
pub fn list_loadouts(lua: &Lua) -> Result<(Vec<String>, Option<String>), mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        build:SyncLoadouts()
        local control = build.controls.buildLoadouts
        local out = { list = {} }
        for _, v in ipairs(control.list) do
            table.insert(out.list, v)
        end
        out.selected = control.list[control.selIndex or 1]
        return out
    "#,
        )
        .eval()?;

    let list_table: LuaTable = result.get("list")?;
    let mut list = Vec::new();
    for entry in list_table.sequence_values::<String>() {
        let entry = entry?;
        if !SPECIAL_ENTRIES.contains(&entry.as_str()) {
            list.push(entry);
        }
    }
    let selected: Option<String> = result
        .get::<String>("selected")
        .ok()
        .filter(|s| !SPECIAL_ENTRIES.contains(&s.as_str()));
    Ok((list, selected))
}

/// Activate a loadout by name: switches the tree spec, item set, skill set,
/// and config set via the upstream dropdown callback. Returns false when the
/// name no longer matches a loadout.
pub fn activate_loadout(lua: &Lua, name: &str) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local value = ...
        local build = mainObject_ref.main.modes['BUILD']
        build:SyncLoadouts()
        local control = build.controls.buildLoadouts
        for i, v in ipairs(control.list) do
            if v == value then
                control.selFunc(i, value)
                build.buildFlag = true
                _runCallback('OnFrame')
                return true
            end
        end
        return false
    "#,
    )
    .call(name)
}

/// Create a new loadout: a fresh tree spec, item set, skill set, and config
/// set all sharing the given title (upstream's New Loadout popup).
pub fn new_loadout(lua: &Lua, name: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local loadout = ...
        local build = mainObject_ref.main.modes['BUILD']

        local newSpec = new("PassiveSpec", build, latestTreeVersion)
        newSpec.title = loadout
        table.insert(build.treeTab.specList, newSpec)

        local itemSet = build.itemsTab:NewItemSet(#build.itemsTab.itemSets + 1)
        table.insert(build.itemsTab.itemSetOrderList, itemSet.id)
        itemSet.title = loadout

        local skillSet = build.skillsTab:NewSkillSet(#build.skillsTab.skillSets + 1)
        table.insert(build.skillsTab.skillSetOrderList, skillSet.id)
        skillSet.title = loadout

        local configSet = build.configTab:NewConfigSet(#build.configTab.configSets + 1)
        table.insert(build.configTab.configSetOrderList, configSet.id)
        configSet.title = loadout

        build:SyncLoadouts()
        build.modFlag = true
    "#,
    )
    .call(name)
}

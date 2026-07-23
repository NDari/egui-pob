//! Skills data: socket groups and gems extracted from Lua's build.skillsTab.

use mlua::prelude::*;

/// A socket group containing linked gems.
#[derive(Debug, Clone)]
pub struct SocketGroup {
    pub index: usize,
    pub label: String,
    pub slot: Option<String>,
    pub enabled: bool,
    pub is_main: bool,
    /// True if this group is created by an equipped item (cannot be deleted).
    pub from_item: bool,
    pub gems: Vec<GemInfo>,
}

/// A gem within a socket group.
#[derive(Debug, Clone)]
pub struct GemInfo {
    pub name: String,
    pub level: i64,
    pub quality: i64,
    pub enabled: bool,
    pub is_support: bool,
}

/// Extract all socket groups and the main skill index from the loaded build.
pub fn extract_skills(lua: &Lua) -> Result<Vec<SocketGroup>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local skillsTab = build.skillsTab
            local mainGroup = build.mainSocketGroup or 1
            local result = {}
            for i, group in ipairs(skillsTab.socketGroupList) do
                local entry = {
                    index = i,
                    label = group.label or "",
                    slot = group.slot,
                    enabled = group.enabled ~= false,
                    isMain = (i == mainGroup),
                    fromItem = group.source ~= nil,
                    gems = {}
                }
                if group.gemList then
                    for _, gem in ipairs(group.gemList) do
                        local gemEntry = {
                            name = gem.nameSpec or "",
                            level = gem.level or 1,
                            quality = gem.quality or 0,
                            enabled = gem.enabled ~= false,
                            isSupport = false
                        }
                        if gem.gemData and gem.gemData.tags then
                            gemEntry.isSupport = gem.gemData.tags.support == true
                        end
                        table.insert(entry.gems, gemEntry)
                    end
                end
                table.insert(result, entry)
            end
            return result
        "#,
        )
        .eval()?;

    let mut groups = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        let gems_table: LuaTable = entry.get("gems")?;
        let mut gems = Vec::new();
        for gem_pair in gems_table.sequence_values::<LuaTable>() {
            let gem = gem_pair?;
            gems.push(GemInfo {
                name: gem.get("name").unwrap_or_default(),
                level: gem.get("level").unwrap_or(1),
                quality: gem.get("quality").unwrap_or(0),
                enabled: gem.get("enabled").unwrap_or(true),
                is_support: gem.get("isSupport").unwrap_or(false),
            });
        }
        groups.push(SocketGroup {
            index: entry.get("index")?,
            label: entry.get("label").unwrap_or_default(),
            slot: entry.get("slot").ok(),
            enabled: entry.get("enabled").unwrap_or(true),
            is_main: entry.get("isMain").unwrap_or(false),
            from_item: entry.get("fromItem").unwrap_or(false),
            gems,
        });
    }

    Ok(groups)
}

/// Set the main socket group index in Lua and trigger recalc.
pub fn set_main_socket_group(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.mainSocketGroup = {index}
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Create a new empty socket group.
pub fn new_socket_group(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        table.insert(build.skillsTab.socketGroupList, { label = "", enabled = true, gemList = { } })
        build.skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
}

/// Delete a socket group by index. Item-granted groups are skipped.
/// The main socket group index is adjusted, matching upstream's OnSelDelete.
pub fn delete_socket_group(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local list = skillsTab.socketGroupList
        local group = list[{index}]
        if group and not group.source then
            table.remove(list, {index})
            if skillsTab.displayGroup == group then
                skillsTab.displayGroup = nil
            end
            if build.mainSocketGroup > {index} then
                build.mainSocketGroup = build.mainSocketGroup - 1
            end
            if build.mainSocketGroup > #list then
                build.mainSocketGroup = math.max(#list, 1)
            end
            local calcsInput = build.calcsTab and build.calcsTab.input
            if calcsInput and calcsInput.skill_number and calcsInput.skill_number > {index} then
                calcsInput.skill_number = calcsInput.skill_number - 1
            end
            skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

/// Enable or disable a socket group.
pub fn set_group_enabled(lua: &Lua, index: usize, enabled: bool) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local group = build.skillsTab.socketGroupList[{index}]
        if group then
            group.enabled = {enabled}
            build.skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

/// Set a socket group's custom label.
pub fn set_group_label(lua: &Lua, index: usize, label: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, label = ...
        local build = mainObject_ref.main.modes['BUILD']
        local group = build.skillsTab.socketGroupList[index]
        if group then
            group.label = label
            build.skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((index, label))
}

/// Which gem property to change.
#[derive(Debug, Clone, Copy)]
pub enum GemProperty {
    Level(i64),
    Quality(i64),
    Enabled(bool),
}

/// Set a property on a gem and reprocess the group.
pub fn set_gem_property(
    lua: &Lua,
    group_index: usize,
    gem_index: usize,
    property: GemProperty,
) -> Result<(), mlua::Error> {
    let (key, value) = match property {
        GemProperty::Level(v) => ("level", LuaValue::Integer(v)),
        GemProperty::Quality(v) => ("quality", LuaValue::Integer(v)),
        GemProperty::Enabled(v) => ("enabled", LuaValue::Boolean(v)),
    };
    lua.load(
        r#"
        local groupIndex, gemIndex, key, value = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local group = skillsTab.socketGroupList[groupIndex]
        if group and group.gemList[gemIndex] then
            group.gemList[gemIndex][key] = value
            skillsTab:ProcessSocketGroup(group)
            skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((group_index, gem_index, key, value))
}

/// Add a gem to a socket group by name. The name is fuzzily resolved by Lua's
/// FindSkillGem (e.g. "CtF" matches "Cold to Fire"). Returns Some(error) if
/// the name is unrecognised or ambiguous (the gem is not added).
pub fn add_gem(
    lua: &Lua,
    group_index: usize,
    name_spec: &str,
) -> Result<Option<String>, mlua::Error> {
    lua.load(
        r#"
        local groupIndex, nameSpec = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local group = skillsTab.socketGroupList[groupIndex]
        if not group then
            return "No such socket group"
        end
        local gem = {
            nameSpec = nameSpec, level = 20, quality = 0, enabled = true,
            enableGlobal1 = true, enableGlobal2 = true, count = 1, new = true,
        }
        table.insert(group.gemList, gem)
        skillsTab:ProcessSocketGroup(group)
        if gem.errMsg then
            table.remove(group.gemList)
            return gem.errMsg
        end
        skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
        return nil
    "#,
    )
    .call((group_index, name_spec))
}

/// Remove a gem from a socket group.
pub fn remove_gem(lua: &Lua, group_index: usize, gem_index: usize) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local group = skillsTab.socketGroupList[{group_index}]
        if group and group.gemList[{gem_index}] then
            table.remove(group.gemList, {gem_index})
            skillsTab:ProcessSocketGroup(group)
            skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

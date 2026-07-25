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
    /// Include this group's skills in the Full DPS total.
    pub include_in_full_dps: bool,
    /// Count multiplier (upstream shows it for item-granted groups only).
    pub group_count: i64,
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
    /// Number of copies of this gem's skill (totems, triggered copies, ...).
    pub count: i64,
    /// True when the count applies (gem grants a non-support effect).
    pub has_count: bool,
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
                    includeInFullDPS = group.includeInFullDPS == true,
                    groupCount = group.groupCount or 1,
                    gems = {}
                }
                if group.gemList then
                    for _, gem in ipairs(group.gemList) do
                        local gemEntry = {
                            name = gem.nameSpec or "",
                            level = gem.level or 1,
                            quality = gem.quality or 0,
                            enabled = gem.enabled ~= false,
                            isSupport = false,
                            count = gem.count or 1,
                        }
                        if gem.gemData and gem.gemData.tags then
                            gemEntry.isSupport = gem.gemData.tags.support == true
                        end
                        -- Count applies when the gem grants a usable
                        -- non-support effect (upstream slot.count.shown)
                        local grantedList = gem.gemData and gem.gemData.grantedEffectList
                            or { gem.grantedEffect }
                        local hasCount = false
                        for gi, effect in ipairs(grantedList) do
                            if effect and not effect.support and not effect.unsupported
                               and (not effect.hasGlobalEffect or gem["enableGlobal" .. gi]) then
                                hasCount = true
                                break
                            end
                        end
                        gemEntry.hasCount = hasCount
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
                count: gem.get("count").unwrap_or(1),
                has_count: gem.get("hasCount").unwrap_or(false),
            });
        }
        groups.push(SocketGroup {
            index: entry.get("index")?,
            label: entry.get("label").unwrap_or_default(),
            slot: entry.get("slot").ok(),
            enabled: entry.get("enabled").unwrap_or(true),
            is_main: entry.get("isMain").unwrap_or(false),
            from_item: entry.get("fromItem").unwrap_or(false),
            include_in_full_dps: entry.get("includeInFullDPS").unwrap_or(false),
            group_count: entry.get("groupCount").unwrap_or(1),
            gems,
        });
    }

    Ok(groups)
}

/// Gem list/default options (upstream SkillsTab fields; persisted with the
/// build by upstream's save code).
#[derive(Debug, Clone)]
pub struct GemOptions {
    pub sort_by_dps: bool,
    pub sort_field: String,
    pub default_level: String,
    pub default_quality: i64,
    pub show_support_types: String,
    pub show_legacy_gems: bool,
}

/// Sort-by-DPS stat choices: (label, stat key).
pub const GEM_SORT_FIELDS: [(&str, &str); 9] = [
    ("Full DPS", "FullDPS"),
    ("Combined DPS", "CombinedDPS"),
    ("Hit DPS", "TotalDPS"),
    ("Average Hit", "AverageDamage"),
    ("DoT DPS", "TotalDot"),
    ("Bleed DPS", "BleedDPS"),
    ("Ignite DPS", "IgniteDPS"),
    ("Poison DPS", "TotalPoisonDPS"),
    ("Effective Hit Pool", "TotalEHP"),
];

/// Default gem level choices: (label, key).
pub const GEM_DEFAULT_LEVELS: [(&str, &str); 5] = [
    ("Normal Maximum", "normalMaximum"),
    ("Corrupted Maximum", "corruptedMaximum"),
    ("Awakened Maximum", "awakenedMaximum"),
    ("Match Character Level", "characterLevel"),
    ("Level 1", "levelOne"),
];

/// Support gem type filter choices: (label, key).
pub const GEM_SUPPORT_TYPES: [(&str, &str); 3] = [
    ("All", "ALL"),
    ("Non-Exceptional", "NORMAL"),
    ("Exceptional", "EXCEPTIONAL"),
];

/// Read the gem options from the skills tab.
pub fn gem_options(lua: &Lua) -> Result<GemOptions, mlua::Error> {
    let t: LuaTable = lua
        .load(
            r#"
        local skillsTab = mainObject_ref.main.modes['BUILD'].skillsTab
        return {
            sortByDPS = skillsTab.sortGemsByDPS ~= false,
            sortField = skillsTab.sortGemsByDPSField or "CombinedDPS",
            defaultLevel = skillsTab.defaultGemLevel or "normalMaximum",
            defaultQuality = skillsTab.defaultGemQuality or 0,
            showSupportTypes = skillsTab.showSupportGemTypes or "ALL",
            showLegacy = skillsTab.showLegacyGems == true,
        }
    "#,
        )
        .eval()?;
    Ok(GemOptions {
        sort_by_dps: t.get("sortByDPS").unwrap_or(true),
        sort_field: t
            .get("sortField")
            .unwrap_or_else(|_| "CombinedDPS".to_string()),
        default_level: t
            .get("defaultLevel")
            .unwrap_or_else(|_| "normalMaximum".to_string()),
        default_quality: t.get("defaultQuality").unwrap_or(0),
        show_support_types: t
            .get("showSupportTypes")
            .unwrap_or_else(|_| "ALL".to_string()),
        show_legacy_gems: t.get("showLegacy").unwrap_or(false),
    })
}

/// Write the gem options to the skills tab.
pub fn set_gem_options(lua: &Lua, options: &GemOptions) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local sortByDPS, sortField, defaultLevel, defaultQuality, showSupportTypes, showLegacy = ...
        local skillsTab = mainObject_ref.main.modes['BUILD'].skillsTab
        skillsTab.sortGemsByDPS = sortByDPS
        skillsTab.sortGemsByDPSField = sortField
        skillsTab.defaultGemLevel = defaultLevel
        skillsTab.defaultGemQuality = defaultQuality
        skillsTab.showSupportGemTypes = showSupportTypes
        skillsTab.showLegacyGems = showLegacy
        skillsTab.modFlag = true
    "#,
    )
    .call((
        options.sort_by_dps,
        options.sort_field.as_str(),
        options.default_level.as_str(),
        options.default_quality,
        options.show_support_types.as_str(),
        options.show_legacy_gems,
    ))
}

/// Socket group slot choices, matching upstream's groupSlotDropList.
pub const GROUP_SLOT_LIST: [(&str, Option<&str>); 14] = [
    ("None", None),
    ("Weapon 1", Some("Weapon 1")),
    ("Weapon 2", Some("Weapon 2")),
    ("Weapon 1 (Swap)", Some("Weapon 1 Swap")),
    ("Weapon 2 (Swap)", Some("Weapon 2 Swap")),
    ("Helmet", Some("Helmet")),
    ("Body Armour", Some("Body Armour")),
    ("Gloves", Some("Gloves")),
    ("Boots", Some("Boots")),
    ("Amulet", Some("Amulet")),
    ("Ring 1", Some("Ring 1")),
    ("Ring 2", Some("Ring 2")),
    ("Ring 3", Some("Ring 3")),
    ("Belt", Some("Belt")),
];

/// Assign the item slot a socket group is socketed in (None to clear).
pub fn set_group_slot(lua: &Lua, index: usize, slot_name: Option<&str>) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, slotName = ...
        local build = mainObject_ref.main.modes['BUILD']
        local group = build.skillsTab.socketGroupList[index]
        if group and not group.source then
            group.slot = slotName
            build.skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((index, slot_name))
}

/// Include or exclude a socket group from the Full DPS total.
pub fn set_group_full_dps(lua: &Lua, index: usize, include: bool) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, include = ...
        local build = mainObject_ref.main.modes['BUILD']
        local group = build.skillsTab.socketGroupList[index]
        if group then
            group.includeInFullDPS = include
            build.skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((index, include))
}

/// Set the count multiplier on an item-granted socket group.
pub fn set_group_count(lua: &Lua, index: usize, count: i64) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, count = ...
        local build = mainObject_ref.main.modes['BUILD']
        local group = build.skillsTab.socketGroupList[index]
        if group then
            group.groupCount = math.max(count, 1)
            build.skillsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((index, count))
}

/// Delete every socket group that is not item-granted.
pub fn delete_all_socket_groups(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local list = skillsTab.socketGroupList
        for i = #list, 1, -1 do
            if not list[i].source then
                table.remove(list, i)
            end
        end
        build.mainSocketGroup = 1
        skillsTab.displayGroup = nil
        skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
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

/// Serialize a socket group to upstream's clipboard text format by calling
/// upstream's own CopySocketGroup with the `Copy` global shimmed to capture
/// the text (tier 1: the format must never drift from upstream's).
/// Returns None if the group does not exist.
pub fn copy_socket_group_text(lua: &Lua, index: usize) -> Result<Option<String>, mlua::Error> {
    lua.load(
        r#"
        local index = ...
        local skillsTab = mainObject_ref.main.modes['BUILD'].skillsTab
        local group = skillsTab.socketGroupList[index]
        if not group then
            return nil
        end
        local captured
        local origCopy = Copy
        Copy = function(text) captured = text end
        local ok, err = pcall(function()
            skillsTab:CopySocketGroup(group)
        end)
        Copy = origCopy
        if not ok then
            error(err)
        end
        return captured
    "#,
    )
    .call(index)
}

/// Append clipboard text as a new socket group by calling upstream's own
/// PasteSocketGroup with the `Paste` global shimmed to return `text` (tier
/// 1: the format must never drift from upstream's). Upstream adds the group
/// and its undo state itself. Returns false when the text contains no valid
/// gem lines (nothing is added).
pub fn paste_socket_group_text(lua: &Lua, text: &str) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local text = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local countBefore = #skillsTab.socketGroupList
        local origPaste = Paste
        Paste = function() return text end
        local ok, err = pcall(function()
            skillsTab:PasteSocketGroup()
        end)
        Paste = origPaste
        if not ok then
            error(err)
        end
        if #skillsTab.socketGroupList == countBefore then
            return false
        end
        _runCallback('OnFrame')
        return true
    "#,
    )
    .call(text)
}

/// Move a socket group to a new position in the list (1-based indices,
/// insert-at semantics like upstream's draggable list). The main socket
/// group and the calcs tab's skill number follow the move, porting
/// SkillListControl:OnOrderChange.
pub fn move_socket_group(lua: &Lua, from: usize, to: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local from, to = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local list = skillsTab.socketGroupList
        if from == to or not list[from] or not list[to] then
            return
        end
        local group = table.remove(list, from)
        table.insert(list, to, group)
        local function adjust(idx)
            if idx == from then
                return to
            elseif idx > from and idx <= to then
                return idx - 1
            elseif idx < from and idx >= to then
                return idx + 1
            end
            return idx
        end
        build.mainSocketGroup = adjust(build.mainSocketGroup)
        local calcsInput = build.calcsTab and build.calcsTab.input
        if calcsInput and calcsInput.skill_number then
            calcsInput.skill_number = adjust(calcsInput.skill_number)
        end
        skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((from, to))
}

/// Move a gem to a new position within its socket group (1-based indices,
/// insert-at semantics).
pub fn move_gem(lua: &Lua, group_index: usize, from: usize, to: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local groupIndex, from, to = ...
        local build = mainObject_ref.main.modes['BUILD']
        local skillsTab = build.skillsTab
        local group = skillsTab.socketGroupList[groupIndex]
        if not group or from == to or not group.gemList[from] or not group.gemList[to] then
            return
        end
        local gem = table.remove(group.gemList, from)
        table.insert(group.gemList, to, gem)
        skillsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((group_index, from, to))
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
#[derive(Debug, Clone)]
pub enum GemProperty {
    Level(i64),
    Quality(i64),
    Enabled(bool),
    /// Number of copies of the skill.
    Count(i64),
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
        GemProperty::Count(v) => ("count", LuaValue::Integer(v.max(1))),
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
        -- Apply the default gem level/quality options (upstream ProcessGemLevel)
        if gem.gemData then
            gem.level = skillsTab:ProcessGemLevel(gem.gemData)
            gem.quality = skillsTab.defaultGemQuality or 0
            skillsTab:ProcessSocketGroup(group)
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

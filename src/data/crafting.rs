//! Item crafting: create Normal/Magic/Rare items from a base, select prefix/
//! suffix affixes with tiers and roll ranges, and add custom modifiers.
//!
//! Affix eligibility (spawn weights, group exclusion, tag interactions) is
//! upstream's: the list building mirrors ItemsTab:UpdateAffixControl, and
//! applying an affix goes through Item:Craft().

use mlua::prelude::*;

/// One selectable affix tier.
#[derive(Debug, Clone)]
pub struct AffixOption {
    pub mod_id: String,
    pub label: String,
}

/// One prefix/suffix slot of a crafted item.
#[derive(Debug, Clone)]
pub struct AffixSlot {
    pub is_prefix: bool,
    /// 1-based index within the prefix or suffix list.
    pub index: usize,
    /// Selected modId ("None" when empty).
    pub selected: String,
    /// Roll position within the tier (0-1).
    pub range: f64,
    /// True when the selected mod has a roll range.
    pub has_range: bool,
    pub options: Vec<AffixOption>,
}

/// Craftable state of an item (present only for crafted Magic/Rare items).
#[derive(Debug, Clone, Default)]
pub struct CraftInfo {
    pub slots: Vec<AffixSlot>,
}

/// Item type categories for the craft popup (upstream itemBaseTypeList).
pub fn base_type_list(lua: &Lua) -> Result<Vec<String>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local out = {}
        for _, t in ipairs(mainObject_ref.main.modes['BUILD'].data.itemBaseTypeList) do
            table.insert(out, t)
        end
        return out
    "#,
        )
        .eval()?;
    Ok(list.sequence_values::<String>().flatten().collect())
}

/// Base item names for a type category.
pub fn base_list(lua: &Lua, type_name: &str) -> Result<Vec<String>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local typeName = ...
        local out = {}
        for _, entry in ipairs(mainObject_ref.main.modes['BUILD'].data.itemBaseLists[typeName] or {}) do
            table.insert(out, entry.label or entry.name)
        end
        return out
    "#,
        )
        .call(type_name)?;
    Ok(list.sequence_values::<String>().flatten().collect())
}

/// Create a new item from a base (upstream's CraftItem makeItem) and add it
/// to the build unequipped. Returns the new item's id.
pub fn craft_item(
    lua: &Lua,
    rarity: &str,
    type_name: &str,
    base_index: usize,
    title: &str,
) -> Result<Option<i64>, mlua::Error> {
    lua.load(
        r#"
        local rarity, typeName, baseIdx, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local base = build.data.itemBaseLists[typeName] and build.data.itemBaseLists[typeName][baseIdx]
        if not base then
            return nil
        end
        local item = new("Item")
        item.name = base.name
        item.base = base.base
        item.baseName = base.name
        item.buffModLines = { }
        item.enchantModLines = { }
        item.classRequirementModLines = { }
        item.scourgeModLines = { }
        item.implicitModLines = { }
        item.explicitModLines = { }
        item.crucibleModLines = { }
        if base.base.type == "Amulet" or base.base.type == "Belt" or base.base.type == "Jewel"
           or base.base.type == "Quiver" or base.base.type == "Ring" or base.base.type == "Graft" then
            item.quality = nil
        else
            item.quality = 0
        end
        -- Flasks/charms/tinctures cap at Magic, like upstream
        if (base.base.flask or (base.base.type == "Jewel" and base.base.subType == "Charm")
            or base.base.type == "Tincture") and rarity == "RARE" then
            rarity = "MAGIC"
        end
        if rarity == "MAGIC" or rarity == "RARE" then
            item.crafted = true
        end
        item.rarity = rarity
        if rarity == "RARE" then
            item.title = title:match("%S") and title or "New Item"
        end
        if base.base.implicit then
            local implicitIndex = 1
            for line in base.base.implicit:gmatch("[^\n]+") do
                local modList, extra = modLib.parseMod(line)
                table.insert(item.implicitModLines, {
                    line = line, extra = extra, modList = modList or { },
                    modTags = base.base.implicitModTypes and base.base.implicitModTypes[implicitIndex] or { },
                })
                implicitIndex = implicitIndex + 1
            end
        end
        item:NormaliseQuality()
        item:BuildAndParseRaw()
        build.itemsTab:AddItem(item, true)
        build.itemsTab:PopulateSlots()
        build.itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
        return item.id
    "#,
    )
    .call((rarity, type_name, base_index, title))
}

/// Read the affix slots of a crafted item (None for non-crafted items).
pub fn craft_info(lua: &Lua, item_id: i64) -> Result<Option<CraftInfo>, mlua::Error> {
    let result: Option<LuaTable> = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.crafted or not item.affixes or not item.affixLimit
           or item.affixLimit == 0 then
            return nil
        end
        local out = { slots = {} }
        local function buildSlot(type, outputTable, outputIndex)
            -- Tags and group exclusions from the other selected affixes
            -- (mirrors ItemsTab:UpdateAffixControl)
            local extraTags, excludeGroups = {}, {}
            for _, tbl in ipairs({"prefixes", "suffixes"}) do
                for index = 1, (item[tbl].limit or (item.affixLimit / 2)) do
                    if index ~= outputIndex or tbl ~= outputTable then
                        local mod = item.affixes[item[tbl][index] and item[tbl][index].modId]
                        if mod then
                            if mod.group then
                                excludeGroups[mod.group] = true
                            end
                            if mod.tags then
                                for _, tag in ipairs(mod.tags) do
                                    extraTags[tag] = true
                                end
                            end
                        end
                    end
                end
            end
            if item.clusterJewel and item.clusterJewelSkill then
                local skill = item.clusterJewel.skills[item.clusterJewelSkill]
                if skill then
                    extraTags[skill.tag] = true
                end
            end
            local cur = item[outputTable][outputIndex]
            local selAffix = cur and cur.modId or "None"
            local affixList = {}
            for modId, mod in pairs(item.affixes) do
                if mod.type == type and not excludeGroups[mod.group]
                   and not item:CheckIfModIsDelve(mod) then
                    if item:GetModSpawnWeight(mod, extraTags) > 0 or modId == selAffix then
                        table.insert(affixList, modId)
                    end
                end
            end
            table.sort(affixList, function(a, b)
                local modA = item.affixes[a]
                local modB = item.affixes[b]
                for i = 1, math.max(#modA, #modB) do
                    if not modA[i] then
                        return true
                    elseif not modB[i] then
                        return false
                    elseif modA.statOrder[i] ~= modB.statOrder[i] then
                        return modA.statOrder[i] < modB.statOrder[i]
                    end
                end
                if modA.level ~= modB.level then
                    return modA.level < modB.level
                end
                return a < b
            end)
            local slot = {
                isPrefix = type == "Prefix",
                index = outputIndex,
                selected = selAffix,
                range = cur and cur.range or 0.5,
                options = {},
            }
            for _, modId in ipairs(affixList) do
                local mod = item.affixes[modId]
                local modString = table.concat(mod, "/")
                table.insert(slot.options, {
                    modId = modId,
                    label = (mod.affix or "?") .. "  " .. modString
                        .. "  [ilvl " .. (mod.level or 1) .. "]",
                })
            end
            local curMod = item.affixes[selAffix]
            slot.hasRange = (curMod and table.concat(curMod, "/"):match("%(%-?[%d%.]+%-%-?[%d%.]+%)") ~= nil) or false
            table.insert(out.slots, slot)
        end
        for i = 1, (item.prefixes.limit or (item.affixLimit / 2)) do
            buildSlot("Prefix", "prefixes", i)
        end
        for i = 1, (item.suffixes.limit or (item.affixLimit / 2)) do
            buildSlot("Suffix", "suffixes", i)
        end
        return out
    "#,
        )
        .call(item_id)?;

    let Some(result) = result else {
        return Ok(None);
    };
    let mut info = CraftInfo::default();
    let slots: LuaTable = result.get("slots")?;
    for slot in slots.sequence_values::<LuaTable>() {
        let slot = slot?;
        let options_table: LuaTable = slot.get("options")?;
        let mut options = Vec::new();
        for opt in options_table.sequence_values::<LuaTable>() {
            let opt = opt?;
            options.push(AffixOption {
                mod_id: opt.get("modId").unwrap_or_default(),
                label: opt.get("label").unwrap_or_default(),
            });
        }
        info.slots.push(AffixSlot {
            is_prefix: slot.get("isPrefix").unwrap_or(false),
            index: slot.get("index").unwrap_or(1),
            selected: slot.get("selected").unwrap_or_else(|_| "None".to_string()),
            range: slot.get("range").unwrap_or(0.5),
            has_range: slot.get("hasRange").unwrap_or(false),
            options,
        });
    }
    Ok(Some(info))
}

/// Select an affix (modId "None" clears the slot) with a roll range, then
/// re-craft the item.
pub fn set_affix(
    lua: &Lua,
    item_id: i64,
    is_prefix: bool,
    index: usize,
    mod_id: &str,
    range: f64,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, isPrefix, index, modId, range = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.crafted then
            return
        end
        item[isPrefix and "prefixes" or "suffixes"][index] = { modId = modId, range = range }
        item:Craft()
        build.itemsTab:PopulateSlots()
        build.itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, is_prefix, index, mod_id, range))
}

/// Cluster jewel craft options (crafted cluster jewels only).
#[derive(Debug, Clone)]
pub struct ClusterCraftInfo {
    /// (skillId, display name), sorted by name.
    pub skills: Vec<(String, String)>,
    pub selected_skill: String,
    pub node_count: i64,
    pub min_nodes: i64,
    pub max_nodes: i64,
}

/// Read the cluster jewel craft state of a crafted cluster jewel.
pub fn cluster_craft_info(
    lua: &Lua,
    item_id: i64,
) -> Result<Option<ClusterCraftInfo>, mlua::Error> {
    let result: Option<LuaTable> = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.crafted or not item.clusterJewel then
            return nil
        end
        local unavailable = {
            ["affliction_strength"] = true,
            ["affliction_dexterity"] = true,
            ["affliction_intelligence"] = true,
        }
        local out = { skills = {} }
        for skillId, skill in pairs(item.clusterJewel.skills) do
            if not unavailable[skillId] then
                table.insert(out.skills, { id = skillId, name = skill.name })
            end
        end
        table.sort(out.skills, function(a, b) return a.name < b.name end)
        local sel = item.clusterJewelSkill
        if not sel or not item.clusterJewel.skills[sel] then
            sel = out.skills[1] and out.skills[1].id or ""
        end
        out.selected = sel
        out.minNodes = item.clusterJewel.minNodes
        out.maxNodes = item.clusterJewel.maxNodes
        -- New cluster jewels default to minimum passives (upstream v2.64)
        out.nodeCount = math.min(
            math.max(item.clusterJewelNodeCount or item.clusterJewel.minNodes,
                item.clusterJewel.minNodes),
            item.clusterJewel.maxNodes)
        return out
    "#,
        )
        .call(item_id)?;

    let Some(result) = result else {
        return Ok(None);
    };
    let skills_table: LuaTable = result.get("skills")?;
    let mut skills = Vec::new();
    for skill in skills_table.sequence_values::<LuaTable>() {
        let skill = skill?;
        skills.push((
            skill.get("id").unwrap_or_default(),
            skill.get("name").unwrap_or_default(),
        ));
    }
    Ok(Some(ClusterCraftInfo {
        skills,
        selected_skill: result.get("selected").unwrap_or_default(),
        node_count: result.get("nodeCount").unwrap_or(0),
        min_nodes: result.get("minNodes").unwrap_or(0),
        max_nodes: result.get("maxNodes").unwrap_or(0),
    }))
}

/// Set a crafted cluster jewel's skill and node count, rebuilding the
/// enchant lines like upstream's CraftClusterJewel.
pub fn set_cluster_jewel(
    lua: &Lua,
    item_id: i64,
    skill_id: &str,
    node_count: i64,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, skillId, nodeCount = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.crafted or not item.clusterJewel
           or not item.clusterJewel.skills[skillId] then
            return
        end
        item.clusterJewelSkill = skillId
        item.clusterJewelNodeCount = math.min(
            math.max(nodeCount, item.clusterJewel.minNodes), item.clusterJewel.maxNodes)
        wipeTable(item.enchantModLines)
        table.insert(item.enchantModLines, {
            line = "Adds " .. item.clusterJewelNodeCount .. " Passive Skills", crafted = true })
        if item.clusterJewel.size == "Large" then
            table.insert(item.enchantModLines,
                { line = "2 Added Passive Skills are Jewel Sockets", crafted = true })
        elseif item.clusterJewel.size == "Medium" then
            table.insert(item.enchantModLines,
                { line = "1 Added Passive Skill is a Jewel Socket", crafted = true })
        end
        local skill = item.clusterJewel.skills[skillId]
        table.insert(item.enchantModLines,
            { line = table.concat(skill.enchant, "\n"), crafted = true })
        item:Craft()
        build.itemsTab:PopulateSlots()
        build.itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, skill_id, node_count))
}

/// Socket layout of an item.
#[derive(Debug, Clone, Default)]
pub struct ItemSockets {
    /// (color "R"/"G"/"B"/"W"/"A", link group) per socket.
    pub sockets: Vec<(String, i64)>,
    /// How many sockets the base can have (excludes abyssal).
    pub selectable_count: i64,
    pub abyssal_count: i64,
}

/// Read an item's sockets (None when the base has no selectable sockets).
pub fn item_sockets(lua: &Lua, item_id: i64) -> Result<Option<ItemSockets>, mlua::Error> {
    let result: Option<LuaTable> = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.selectableSocketCount or item.selectableSocketCount == 0 then
            return nil
        end
        local out = {
            selectable = item.selectableSocketCount,
            abyssal = item.abyssalSocketCount or 0,
            sockets = {},
        }
        for _, socket in ipairs(item.sockets or {}) do
            table.insert(out.sockets, { color = socket.color, group = socket.group })
        end
        return out
    "#,
        )
        .call(item_id)?;

    let Some(result) = result else {
        return Ok(None);
    };
    let sockets_table: LuaTable = result.get("sockets")?;
    let mut sockets = Vec::new();
    for socket in sockets_table.sequence_values::<LuaTable>() {
        let socket = socket?;
        sockets.push((
            socket.get("color").unwrap_or_default(),
            socket.get("group").unwrap_or(0),
        ));
    }
    Ok(Some(ItemSockets {
        sockets,
        selectable_count: result.get("selectable").unwrap_or(0),
        abyssal_count: result.get("abyssal").unwrap_or(0),
    }))
}

/// Set a socket's color (1-based index; "R"/"G"/"B"/"W").
pub fn set_socket_color(
    lua: &Lua,
    item_id: i64,
    index: usize,
    color: &str,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, index, color = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item or not item.sockets or not item.sockets[index] then
            return
        end
        item.sockets[index].color = color
        item:BuildAndParseRaw()
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, index, color))
}

/// Link or unlink sockets `index` and `index + 1` (upstream's group shift).
pub fn set_socket_link(
    lua: &Lua,
    item_id: i64,
    index: usize,
    linked: bool,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, index, linked = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item or not item.sockets or not item.sockets[index]
           or not item.sockets[index + 1] then
            return
        end
        if linked and item.sockets[index].group ~= item.sockets[index + 1].group then
            for s = index + 1, #item.sockets do
                item.sockets[s].group = item.sockets[s].group - 1
            end
        elseif not linked and item.sockets[index].group == item.sockets[index + 1].group then
            for s = index + 1, #item.sockets do
                item.sockets[s].group = item.sockets[s].group + 1
            end
        end
        item:BuildAndParseRaw()
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, index, linked))
}

/// Add a socket (up to the base's selectable count), like upstream's "+".
pub fn add_socket(lua: &Lua, item_id: i64) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item or not item.sockets
           or #item.sockets >= item.selectableSocketCount + (item.abyssalSocketCount or 0) then
            return
        end
        local insertIndex = #item.sockets - (item.abyssalSocketCount or 0) + 1
        local prevGroup = item.sockets[insertIndex - 1] and item.sockets[insertIndex - 1].group or -1
        table.insert(item.sockets, insertIndex, {
            color = item.defaultSocketColor,
            group = prevGroup + 1,
        })
        for s = insertIndex + 1, #item.sockets do
            item.sockets[s].group = item.sockets[s].group + 1
        end
        item:BuildAndParseRaw()
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call(item_id)
}

/// Catalyst names, in upstream's index order (index 1 = Abrasive).
/// Catalyst names, 1-based indices matching upstream Item.lua's
/// `catalystList` exactly (the index is stored on the item and drives the
/// scaling in `getCatalystScalar`, so order must never drift; registered in
/// ports.toml). Sinistral/Dextral were inserted mid-list in v2.66.
pub const CATALYSTS: [&str; 12] = [
    "Abrasive (Attack)",
    "Accelerating (Speed)",
    "Dextral (Suffix)",
    "Fertile (Life & Mana)",
    "Imbued (Caster)",
    "Intrinsic (Attribute)",
    "Noxious (Physical & Chaos Damage)",
    "Prismatic (Resistance)",
    "Sinistral (Prefix)",
    "Tempering (Defense)",
    "Turbulent (Elemental)",
    "Unstable (Critical)",
];

/// Current catalyst state (None when catalysts don't apply to the item:
/// upstream shows them for crafted/tagged amulets, rings, and belts).
pub fn catalyst_info(lua: &Lua, item_id: i64) -> Result<Option<(usize, i64)>, mlua::Error> {
    let result: Option<LuaTable> = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not (item.crafted or item.hasModTags)
           or not (item.base.type == "Amulet" or item.base.type == "Ring"
                   or item.base.type == "Belt") then
            return nil
        end
        return { catalyst = item.catalyst or 0, quality = item.catalystQuality or 20 }
    "#,
        )
        .call(item_id)?;
    Ok(result.map(|t| {
        (
            t.get::<usize>("catalyst").unwrap_or(0),
            t.get::<i64>("quality").unwrap_or(20),
        )
    }))
}

/// Set the catalyst (0 = none, else 1-based into CATALYSTS) and quality.
/// Crafted items re-craft so affix values pick up the catalyst scalar.
pub fn set_catalyst(
    lua: &Lua,
    item_id: i64,
    catalyst: usize,
    quality: i64,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, catalyst, quality = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        item.catalyst = catalyst
        item.catalystQuality = catalyst > 0 and quality or nil
        if item.crafted then
            item:Craft()
        else
            item:BuildAndParseRaw()
        end
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, catalyst, quality))
}

/// Lua snippet defining `buildCorruptList(item)`: corrupted implicit mods
/// with spawn weight, sorted like upstream's CorruptDisplayItem.
const CORRUPT_LIST_LUA: &str = r#"
    local function buildCorruptList(item)
        local list = {}
        for modId, mod in pairs(item.affixes) do
            if mod.type == "Corrupted" and item:GetModSpawnWeight(mod) > 0 then
                table.insert(list, mod)
            end
        end
        table.sort(list, function(a, b)
            local an = a[1]:lower():gsub("%(.-%)", "$"):gsub("[%+%-%%]", ""):gsub("%d+", "$")
            local bn = b[1]:lower():gsub("%(.-%)", "$"):gsub("[%+%-%%]", ""):gsub("%d+", "$")
            if an ~= bn then
                return an < bn
            else
                return a.level < b.level
            end
        end)
        return list
    end
"#;

/// A corrupted-implicit choice.
#[derive(Debug, Clone)]
pub struct CorruptOption {
    /// 1-based index into the deterministic list (used to apply).
    pub index: usize,
    pub label: String,
    /// Mod group (the two implicits must differ in group).
    pub group: String,
}

/// Corrupted implicit mods available for an item.
pub fn corrupt_options(lua: &Lua, item_id: i64) -> Result<Vec<CorruptOption>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId = ...
        {CORRUPT_LIST_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {{}}
        if not item or not item.affixes then
            return out
        end
        for i, mod in ipairs(buildCorruptList(item)) do
            table.insert(out, {{
                index = i,
                label = table.concat(mod, "/"),
                group = mod.group or "",
            }})
        end
        return out
    "#
        ))
        .call(item_id)?;

    let mut options = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        options.push(CorruptOption {
            index: entry.get("index").unwrap_or(0),
            label: entry.get("label").unwrap_or_default(),
            group: entry.get("group").unwrap_or_default(),
        });
    }
    Ok(options)
}

/// Corrupt an item, optionally with one or two corrupted implicits (indices
/// from [`corrupt_options`]). Mirrors upstream's corruptItem: chosen
/// implicits replace the item's implicit lines.
pub fn corrupt_item(
    lua: &Lua,
    item_id: i64,
    first: Option<usize>,
    second: Option<usize>,
) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local itemId, first, second = ...
        {CORRUPT_LIST_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        newItem.corrupted = true
        local list = item.affixes and buildCorruptList(item) or {{}}
        local newImplicit = {{}}
        for _, sel in ipairs({{ first, second }}) do
            local mod = sel and sel > 0 and list[sel]
            if mod then
                for _, modLine in ipairs(mod) do
                    if mod.modTags[1] then
                        table.insert(newImplicit,
                            {{ line = "{{tags:" .. table.concat(mod.modTags, ",") .. "}}" .. modLine }})
                    else
                        table.insert(newImplicit, {{ line = modLine }})
                    end
                end
            end
        end
        if #newImplicit > 0 then
            wipeTable(newItem.implicitModLines)
            for i, implicit in ipairs(newImplicit) do
                table.insert(newItem.implicitModLines, i, implicit)
            end
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .call((item_id, first, second))
}

/// One scalable explicit line eligible for a Vaal-orb roll range (upstream's
/// "Roll Ranges" corrupt mode; uniques and relics only).
#[derive(Debug, Clone)]
pub struct RollRangeMod {
    /// 1-based index into the item's explicitModLines.
    pub index: usize,
    /// Current corruptedRange multiplier (1.0 = unmodified).
    pub current: f64,
    /// The line rendered at `current`.
    pub label: String,
}

/// Explicit mods that can take a corruptedRange, mirroring upstream
/// CorruptDisplayItem's slider gate: scalable lines on the selected variant
/// (or variant-less), for UNIQUE/RELIC items.
pub fn corrupt_roll_ranges(lua: &Lua, item_id: i64) -> Result<Vec<RollRangeMod>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId = ...
        local main = mainObject_ref.main
        local item = main.modes['BUILD'].itemsTab.items[itemId]
        local out = {}
        if not item or (item.rarity ~= "UNIQUE" and item.rarity ~= "RELIC") then
            return out
        end
        for i, mod in ipairs(item.explicitModLines) do
            local variantIds = {}
            for id, _ in pairs(mod.variantList or {}) do
                table.insert(variantIds, id)
            end
            local selectedVariant
            for _, variantId in ipairs(variantIds) do
                if item.variant == variantId or item.variantAlt == variantId
                   or item.variantAlt2 == variantId or item.variantAlt3 == variantId
                   or item.variantAlt4 == variantId or item.variantAlt5 == variantId then
                    selectedVariant = true
                end
            end
            local modRange = mod.range or main.defaultItemAffixQuality
            if itemLib.isModLineScalable(mod.line, modRange, mod.valueScalar)
               and (#variantIds == 0 or selectedVariant) then
                local current = mod.corruptedRange or 1
                table.insert(out, {
                    index = i,
                    current = current,
                    label = itemLib.applyRange(mod.line, modRange, mod.valueScalar or 1, current),
                })
            end
        end
        return out
    "#,
        )
        .call(item_id)?;
    let mut mods = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        mods.push(RollRangeMod {
            index: entry.get("index").unwrap_or(0),
            current: entry.get("current").unwrap_or(1.0),
            label: entry.get("label").unwrap_or_default(),
        });
    }
    Ok(mods)
}

/// Render an explicit line at a given corruptedRange multiplier (the live
/// preview next to the roll-range slider), via upstream itemLib.applyRange.
pub fn preview_roll_range(
    lua: &Lua,
    item_id: i64,
    index: usize,
    value: f64,
) -> Result<String, mlua::Error> {
    lua.load(
        r#"
        local itemId, index, value = ...
        local main = mainObject_ref.main
        local item = main.modes['BUILD'].itemsTab.items[itemId]
        local mod = item and item.explicitModLines[index]
        if not mod then
            return ""
        end
        return itemLib.applyRange(mod.line, mod.range or main.defaultItemAffixQuality,
            mod.valueScalar or 1, value)
    "#,
    )
    .call((item_id, index, value))
}

/// Corrupt an item in "Roll Ranges" mode (Volatile Vaal Orb): implicits are
/// left alone; each explicit gets its corruptedRange set (nil when 1.0),
/// mirroring upstream corruptItem's non-implicit branch. `indices` and
/// `values` are parallel.
pub fn corrupt_item_rolls(
    lua: &Lua,
    item_id: i64,
    indices: &[usize],
    values: &[f64],
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, indices, values = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        newItem.corrupted = true
        for k, i in ipairs(indices) do
            local modLine = newItem.explicitModLines[i]
            local corruptedRange = values[k]
            if modLine and corruptedRange then
                modLine.corruptedRange = corruptedRange ~= 1 and corruptedRange or nil
            end
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, indices.to_vec(), values.to_vec()))
}

/// One row of an item's modifier-range list (upstream rangeLineList):
/// scalable lines get a roll slider, Foulborn-mappable lines get a mutate
/// toggle.
#[derive(Debug, Clone)]
pub struct ModRangeLine {
    /// 1-based index into the item's rangeLineList.
    pub position: usize,
    /// The line rendered at its current roll.
    pub label: String,
    /// Roll position within the mod's range (0..1).
    pub range: f64,
    /// True when the line is scalable (upstream showSlider).
    pub has_slider: bool,
    /// True when the line can be swapped with its Foulborn counterpart
    /// (upstream modId + newModId, derived from data.foulbornMap).
    pub can_mutate: bool,
    /// True when the line currently is the mutated (Foulborn) version.
    pub mutated: bool,
}

/// The item's modifier-range rows (upstream's Modifier Range section).
pub fn mod_range_lines(lua: &Lua, item_id: i64) -> Result<Vec<ModRangeLine>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId = ...
        local main = mainObject_ref.main
        local item = main.modes['BUILD'].itemsTab.items[itemId]
        local out = {}
        if not item or not item.rangeLineList then
            return out
        end
        for i, line in ipairs(item.rangeLineList) do
            table.insert(out, {
                position = i,
                label = itemLib.applyRange(line.line,
                    line.range or main.defaultItemAffixQuality,
                    line.valueScalar or 1, line.corruptedRange or 1),
                range = line.range or main.defaultItemAffixQuality or 0.5,
                hasSlider = line.showSlider and true or false,
                canMutate = (line.modId and line.newModId) and true or false,
                mutated = line.mutated and true or false,
            })
        end
        return out
    "#,
        )
        .call(item_id)?;
    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        rows.push(ModRangeLine {
            position: entry.get("position").unwrap_or(0),
            label: entry.get("label").unwrap_or_default(),
            range: entry.get("range").unwrap_or(0.5),
            has_slider: entry.get("hasSlider").unwrap_or(false),
            can_mutate: entry.get("canMutate").unwrap_or(false),
            mutated: entry.get("mutated").unwrap_or(false),
        });
    }
    Ok(rows)
}

/// Set a range line's roll position (upstream's Modifier Range slider):
/// clones the item, updates the line's range, and re-parses.
pub fn set_mod_range(
    lua: &Lua,
    item_id: i64,
    position: usize,
    range: f64,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, position, range = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        local line = newItem.rangeLineList and newItem.rangeLineList[position]
        if not line then
            return
        end
        line.range = range
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, position, range))
}

/// Toggle a line's Foulborn mutation via upstream Item:MutateMod (swaps the
/// line with its data.foulbornMap counterpart in either direction).
pub fn mutate_mod(lua: &Lua, item_id: i64, position: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, position = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        local line = newItem.rangeLineList and newItem.rangeLineList[position]
        if not line or not line.modId or not line.newModId then
            return
        end
        newItem:MutateMod(line.modId, line.newModId, not line.mutated)
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, position))
}

/// Lua snippet defining `buildImplicitLists(build, item, sourceId)`, a port
/// of upstream's AddImplicitToDisplayItem buildMods: returns (modGroups,
/// modList) with the same deterministic ordering.
const IMPLICIT_LISTS_LUA: &str = r##"
    local function buildImplicitLists(item, sourceId)
        local modList, modGroups, groupIndexes = {}, {}, {}
        if sourceId == "EXARCH" or sourceId == "EATER" then
            for i, mod in pairs(item.affixes) do
                if item:GetModSpawnWeight(mod) > 0 and sourceId:lower() == mod.type:lower() then
                    local modLabel = table.concat(mod, "/")
                    local group = mod.group:gsub("PinnaclePresence", ""):gsub("UniquePresence", "")
                    if not groupIndexes[group] then
                        table.insert(modList, {})
                        table.insert(modGroups, {
                            label = modLabel, mod = mod,
                            modListIndex = #modList, defaultOrder = i,
                        })
                        groupIndexes[group] = #modGroups
                    end
                    table.insert(modList[groupIndexes[group]], {
                        label = modLabel, mod = mod, type = sourceId:lower(),
                        defaultOrder = i,
                    })
                end
            end
            table.sort(modGroups, function(a, b)
                local modA, modB = a.mod, b.mod
                for i = 1, math.max(#modA, #modB) do
                    if not modA[i] then
                        return true
                    elseif not modB[i] then
                        return false
                    elseif modA.statOrder[i] ~= modB.statOrder[i] then
                        return modA.statOrder[i] < modB.statOrder[i]
                    end
                end
                return modA.level > modB.level
            end)
            for i, _ in pairs(modList) do
                table.sort(modList[i], function(a, b)
                    local modA, modB = a.mod, b.mod
                    if modA.group ~= modB.group then
                        if modA.group:match("PinnaclePresence") then
                            return false
                        elseif modB.group:match("PinnaclePresence") then
                            return true
                        elseif modA.group:match("UniquePresence") then
                            return false
                        else
                            return true
                        end
                    end
                    for j = 1, math.max(#modA, #modB) do
                        if not modA[j] then
                            return true
                        elseif not modB[j] then
                            return false
                        elseif modA.statOrder[j] ~= modB.statOrder[j] then
                            return modA.statOrder[j] < modB.statOrder[j]
                        else
                            local modAVal = tonumber(a.defaultOrder:match("%d+$"))
                            local modBVal = tonumber(b.defaultOrder:match("%d+$"))
                            return modAVal < modBVal
                        end
                    end
                    return modA.level > modB.level
                end)
            end
            for i, _ in pairs(modGroups) do
                modGroups[i].label = modList[modGroups[i].modListIndex][1].label
                    :gsub("%([%d%.]+%-[%d%.]+%)", "#"):gsub("[%d%.]+", "#")
            end
        elseif sourceId == "DelveImplicit" then
            for i, mod in pairs(item.affixes) do
                if item:GetModSpawnWeight(mod) > 0 and sourceId:lower() == mod.type:lower() then
                    local modLabel = table.concat(mod, "/")
                    if not groupIndexes[mod.group] then
                        table.insert(modList, {})
                        table.insert(modGroups, {
                            label = modLabel, mod = mod,
                            modListIndex = #modList, defaultOrder = i,
                        })
                        groupIndexes[mod.group] = #modGroups
                    end
                    table.insert(modList[groupIndexes[mod.group]], {
                        label = modLabel, mod = mod, type = "custom",
                        defaultOrder = i,
                    })
                end
            end
            for i, _ in pairs(modList) do
                table.sort(modList[i], function(a, b)
                    return a.defaultOrder < b.defaultOrder
                end)
            end
        end
        return modGroups, modList
    end
"##;

/// A group of implicit mods (tiers within share a mod group).
#[derive(Debug, Clone)]
pub struct ImplicitGroup {
    pub label: String,
    /// Tier labels, in the same order used when applying.
    pub tiers: Vec<String>,
}

/// Implicit sources available for an item, per upstream's rules.
pub fn implicit_sources(lua: &Lua, item_id: i64) -> Result<Vec<(String, String)>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {}
        if not item then
            return out
        end
        if (item.rarity ~= "UNIQUE" and item.rarity ~= "RELIC")
           and (item.type == "Helmet" or item.type == "Body Armour"
                or item.type == "Gloves" or item.type == "Boots") then
            if item.cleansing then
                table.insert(out, { label = "Searing Exarch", id = "EXARCH" })
            end
            if item.tangle then
                table.insert(out, { label = "Eater of Worlds", id = "EATER" })
            end
        end
        if item.type ~= "Flask" and item.type ~= "Jewel" and item.type ~= "Graft" then
            table.insert(out, { label = "Delve", id = "DelveImplicit" })
        end
        table.insert(out, { label = "Custom", id = "CUSTOM" })
        return out
    "#,
        )
        .call(item_id)?;

    let mut sources = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        sources.push((
            entry.get("label").unwrap_or_default(),
            entry.get("id").unwrap_or_default(),
        ));
    }
    Ok(sources)
}

/// Implicit mod groups (with tiers) for a source.
pub fn implicit_mods(
    lua: &Lua,
    item_id: i64,
    source: &str,
) -> Result<Vec<ImplicitGroup>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId, sourceId = ...
        {IMPLICIT_LISTS_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {{}}
        if not item or not item.affixes then
            return out
        end
        local modGroups, modList = buildImplicitLists(item, sourceId)
        for _, group in ipairs(modGroups) do
            local entry = {{ label = group.label, tiers = {{}} }}
            for _, tier in ipairs(modList[group.modListIndex]) do
                table.insert(entry.tiers, tier.label)
            end
            table.insert(out, entry)
        end
        return out
    "#
        ))
        .call((item_id, source))?;

    let mut groups = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let tiers: LuaTable = entry.get("tiers")?;
        groups.push(ImplicitGroup {
            label: entry.get("label").unwrap_or_default(),
            tiers: tiers.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(groups)
}

/// Add an implicit from a source (1-based group and tier indices from
/// [`implicit_mods`]). Eldritch implicits replace an existing implicit of
/// the same source, like upstream.
pub fn add_implicit(
    lua: &Lua,
    item_id: i64,
    source: &str,
    group_index: usize,
    tier_index: usize,
) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local itemId, sourceId, groupIdx, tierIdx = ...
        {IMPLICIT_LISTS_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item or not item.affixes then
            return
        end
        local modGroups, modList = buildImplicitLists(item, sourceId)
        local group = modGroups[groupIdx]
        local listMod = group and modList[group.modListIndex][tierIdx]
        if not listMod then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        if sourceId == "EXARCH" or sourceId == "EATER" then
            local index
            for i, implicitMod in ipairs(newItem.implicitModLines) do
                if implicitMod[listMod.type] then
                    index = i
                    break
                end
            end
            if index then
                for i, line in ipairs(listMod.mod) do
                    newItem.implicitModLines[index + i - 1] =
                        {{ line = line, modTags = listMod.mod.modTags, [listMod.type] = true }}
                end
            else
                for _, line in ipairs(listMod.mod) do
                    table.insert(newItem.implicitModLines,
                        {{ line = line, modTags = listMod.mod.modTags, [listMod.type] = true }})
                end
            end
        else
            for _, line in ipairs(listMod.mod) do
                table.insert(newItem.implicitModLines,
                    {{ line = line, modTags = listMod.mod.modTags, [listMod.type] = true }})
            end
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .call((item_id, source, group_index, tier_index))
}

/// Sort values for the add-implicit popup: one value per implicit group (in
/// [`implicit_mods`] order), computed by applying the group's top tier to a
/// clone of the item and reading the selected power stat with the clone
/// equipped (upstream's popup sorting; one throwaway calc pass per group).
/// `stat_index` is 1-based into `data.powerStatList`.
pub fn implicit_sort_values(
    lua: &Lua,
    item_id: i64,
    source: &str,
    stat_index: usize,
) -> Result<Vec<f64>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId, sourceId, statIndex = ...
        {IMPLICIT_LISTS_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        local stat = data.powerStatList[statIndex]
        local out = {{}}
        if not item or not item.affixes or not stat or not stat.stat then
            return out
        end
        local modGroups, modList = buildImplicitLists(item, sourceId)
        local slotName = itemsTab:GetComparisonSlotNameForItem(item)
        local calcFunc = build.calcsTab:GetMiscCalculator(build)
        local useFullDPS = stat.stat == "FullDPS"
        for _, group in ipairs(modGroups) do
            local listMod = modList[group.modListIndex][1]
            local value = 0
            if listMod then
                local newItem = new("Item", item:BuildRaw())
                newItem.id = item.id
                for _, line in ipairs(listMod.mod) do
                    table.insert(newItem.implicitModLines,
                        {{ line = line, modTags = listMod.mod.modTags, [listMod.type] = true }})
                end
                newItem:BuildAndParseRaw()
                local output = calcFunc(
                    {{ repSlotName = slotName, repItem = newItem }}, useFullDPS)
                value = data.powerStatList.GetFromOutput(output, stat) or 0
            end
            table.insert(out, value)
        end
        return out
    "#
        ))
        .call((item_id, source, stat_index))?;
    let mut values = Vec::new();
    for v in list.sequence_values::<f64>() {
        values.push(v?);
    }
    Ok(values)
}

/// Add a custom implicit line.
pub fn add_custom_implicit(lua: &Lua, item_id: i64, line: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, line = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        table.insert(newItem.implicitModLines, { line = line, custom = true })
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, line))
}

/// An enchantment source (labyrinth difficulty, Heist, orbs, ...).
#[derive(Debug, Clone)]
pub struct EnchantSource {
    pub name: String,
    pub label: String,
}

/// Enchantment catalog shape for an item.
#[derive(Debug, Clone, Default)]
pub struct EnchantOptions {
    /// True when the catalog is keyed by skill (helmet enchants).
    pub has_skills: bool,
    /// All skills with enchants, sorted (empty when not skill-keyed).
    pub skills: Vec<String>,
    /// Skills the build actually uses (preferred preselection).
    pub used_skills: Vec<String>,
}

/// Read the enchantment catalog shape for an item (None when the base cannot
/// be enchanted).
pub fn enchant_options(lua: &Lua, item_id: i64) -> Result<Option<EnchantOptions>, mlua::Error> {
    let result: Option<LuaTable> = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item or not item.enchantments then
            return nil
        end
        local enchantments = item.enchantments
        local haveSkills = true
        for _, source in ipairs(build.data.enchantmentSource) do
            if enchantments[source.name] then
                haveSkills = false
                break
            end
        end
        local out = { hasSkills = haveSkills, skills = {}, usedSkills = {} }
        if haveSkills then
            for skillName in pairs(enchantments) do
                table.insert(out.skills, skillName)
            end
            table.sort(out.skills)
            local seen = {}
            for _, socketGroup in ipairs(build.skillsTab.socketGroupList) do
                for _, gemInstance in ipairs(socketGroup.gemList) do
                    if gemInstance.gemData then
                        for _, grantedEffect in ipairs(gemInstance.gemData.grantedEffectList) do
                            if not grantedEffect.support and enchantments[grantedEffect.name]
                               and not seen[grantedEffect.name] then
                                seen[grantedEffect.name] = true
                                table.insert(out.usedSkills, grantedEffect.name)
                            end
                        end
                    end
                end
            end
            table.sort(out.usedSkills)
        end
        return out
    "#,
        )
        .call(item_id)?;

    let Some(result) = result else {
        return Ok(None);
    };
    let get_vec = |key: &str| -> Vec<String> {
        result
            .get::<LuaTable>(key)
            .map(|t| t.sequence_values::<String>().flatten().collect())
            .unwrap_or_default()
    };
    Ok(Some(EnchantOptions {
        has_skills: result.get("hasSkills").unwrap_or(false),
        skills: get_vec("skills"),
        used_skills: get_vec("usedSkills"),
    }))
}

/// Sources and their enchant lines for an item (and skill, when skill-keyed).
pub fn enchant_catalog(
    lua: &Lua,
    item_id: i64,
    skill: Option<&str>,
) -> Result<Vec<(EnchantSource, Vec<String>)>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId, skill = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {}
        if not item or not item.enchantments then
            return out
        end
        local list = skill and item.enchantments[skill] or item.enchantments
        if not list then
            return out
        end
        for _, source in ipairs(build.data.enchantmentSource) do
            if list[source.name] then
                local entry = { name = source.name, label = source.label, lines = {} }
                for _, line in ipairs(list[source.name]) do
                    table.insert(entry.lines, line)
                end
                table.insert(out, entry)
            end
        end
        return out
    "#,
        )
        .call((item_id, skill))?;

    let mut catalog = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let lines: LuaTable = entry.get("lines")?;
        catalog.push((
            EnchantSource {
                name: entry.get("name").unwrap_or_default(),
                label: entry.get("label").unwrap_or_default(),
            },
            lines.sequence_values::<String>().flatten().collect(),
        ));
    }
    Ok(catalog)
}

/// Sort values for the enchant popup: one value per enchant line, nested in
/// the same (source, lines) shape as [`enchant_catalog`]. Each value applies
/// the line to a clone of the item (same two-line "a/b" and slot handling as
/// [`apply_enchant`]) and reads the selected power stat with the clone
/// equipped (upstream's popup sorting; one throwaway calc pass per line).
/// `stat_index` is 1-based into `data.powerStatList`.
pub fn enchant_sort_values(
    lua: &Lua,
    item_id: i64,
    skill: Option<&str>,
    slot: usize,
    stat_index: usize,
) -> Result<Vec<Vec<f64>>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId, skill, slot, statIndex = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local baseItem = itemsTab.items[itemId]
        local stat = data.powerStatList[statIndex]
        local out = {}
        if not baseItem or not baseItem.enchantments or not stat or not stat.stat then
            return out
        end
        local list = skill and baseItem.enchantments[skill] or baseItem.enchantments
        if not list then
            return out
        end
        local slotName = baseItem:GetPrimarySlot()
        local calcFunc = build.calcsTab:GetMiscCalculator(build)
        local useFullDPS = stat.stat == "FullDPS"
        for _, source in ipairs(build.data.enchantmentSource) do
            if list[source.name] then
                local vals = {}
                for _, line in ipairs(list[source.name]) do
                    local item = new("Item", baseItem:BuildRaw())
                    item.id = baseItem.id
                    local first, second = line:match("([^/]+)/([^/]+)")
                    if first then
                        item.enchantModLines = {
                            { crafted = true, line = first },
                            { crafted = true, line = second },
                        }
                    else
                        if not item.canHaveTwoEnchants and #item.enchantModLines > 1 then
                            item.enchantModLines = { item.enchantModLines[1] }
                        end
                        if #item.enchantModLines >= slot then
                            table.remove(item.enchantModLines, slot)
                        end
                        table.insert(item.enchantModLines, slot, { crafted = true, line = line })
                    end
                    item:BuildAndParseRaw()
                    local output = calcFunc({ repSlotName = slotName, repItem = item }, useFullDPS)
                    table.insert(vals, data.powerStatList.GetFromOutput(output, stat) or 0)
                end
                table.insert(out, vals)
            end
        end
        return out
    "#,
        )
        .call((item_id, skill, slot, stat_index))?;
    let mut values = Vec::new();
    for group in list.sequence_values::<LuaTable>() {
        let group = group?;
        let mut vals = Vec::new();
        for v in group.sequence_values::<f64>() {
            vals.push(v?);
        }
        values.push(vals);
    }
    Ok(values)
}

/// Apply an enchantment (by source + 1-based index), mirroring upstream's
/// enchantItem: handles the two-line "a/b" enchants and the single-enchant
/// limit. Pass `remove` via [`remove_enchant`].
pub fn apply_enchant(
    lua: &Lua,
    item_id: i64,
    skill: Option<&str>,
    source: &str,
    index: usize,
    slot: usize,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, skill, source, index, slot = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item or not item.enchantments then
            return
        end
        local list = skill and item.enchantments[skill] or item.enchantments
        local line = list and list[source] and list[source][index]
        if not line then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        local first, second = line:match("([^/]+)/([^/]+)")
        if first then
            newItem.enchantModLines = {
                { crafted = true, line = first },
                { crafted = true, line = second },
            }
        else
            if not newItem.canHaveTwoEnchants and #newItem.enchantModLines > 1 then
                newItem.enchantModLines = { newItem.enchantModLines[1] }
            end
            if #newItem.enchantModLines >= slot then
                table.remove(newItem.enchantModLines, slot)
            end
            table.insert(newItem.enchantModLines, slot, { crafted = true, line = line })
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, skill, source, index, slot))
}

/// Remove the enchantment in the given slot.
pub fn remove_enchant(lua: &Lua, item_id: i64, slot: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, slot = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        if #newItem.enchantModLines >= slot then
            table.remove(newItem.enchantModLines, slot)
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, slot))
}

/// A notable that can be anointed onto an amulet.
#[derive(Debug, Clone)]
pub struct AnointNotable {
    pub name: String,
    pub stats: Vec<String>,
    /// Oil names of the recipe (e.g. ["GoldenOil", "GoldenOil", "BlackOil"]).
    pub oils: Vec<String>,
}

/// All anointable notables (upstream's filter: any notable with an oil
/// recipe), sorted by name.
pub fn anoint_notables(lua: &Lua) -> Result<Vec<AnointNotable>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local spec = mainObject_ref.main.modes['BUILD'].spec
        local out = {}
        for _, node in pairs(spec.tree.nodes) do
            if node.type == "Notable" and node.recipe and #node.recipe >= 1 and node.dn then
                local entry = { name = node.dn, stats = {}, oils = {} }
                for _, line in ipairs(node.sd or {}) do
                    table.insert(entry.stats, line)
                end
                for _, oil in ipairs(node.recipe) do
                    table.insert(entry.oils, oil)
                end
                table.insert(out, entry)
            end
        end
        table.sort(out, function(a, b) return a.name < b.name end)
        return out
    "#,
        )
        .eval()?;

    let mut notables = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let stats: LuaTable = entry.get("stats")?;
        let oils: LuaTable = entry.get("oils")?;
        notables.push(AnointNotable {
            name: entry.get("name").unwrap_or_default(),
            stats: stats.sequence_values::<String>().flatten().collect(),
            oils: oils.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(notables)
}

/// Current anoints on an item ("Allocates X" enchant lines).
pub fn get_anoints(lua: &Lua, item_id: i64) -> Result<Vec<String>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {}
        if item then
            for _, mod in ipairs(item.enchantModLines or {}) do
                local name = mod.line and mod.line:match("^Allocates (.+)$")
                if name then
                    table.insert(out, name)
                end
            end
        end
        return out
    "#,
        )
        .call(item_id)?;
    Ok(list.sequence_values::<String>().flatten().collect())
}

/// Number of usable anoint slots on an item right now: 1 by default, more
/// with upstream's canHaveTwo/Three/FourEnchants flags (e.g. Stranglegasp),
/// gated so a new slot only opens once the previous ones are filled.
pub fn anoint_slot_count(lua: &Lua, item_id: i64) -> Result<usize, mlua::Error> {
    lua.load(
        r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item then
            return 1
        end
        local max = 1
        if item.canHaveFourEnchants then
            max = 4
        elseif item.canHaveThreeEnchants then
            max = 3
        elseif item.canHaveTwoEnchants then
            max = 2
        end
        return math.min(max, #item.enchantModLines + 1)
    "#,
    )
    .call(item_id)
}

/// Preview what anointing a notable would change, mirroring upstream's
/// AppendAnointTooltip: returns color-coded stat difference lines.
pub fn anoint_preview(
    lua: &Lua,
    item_id: i64,
    node_name: &str,
    slot: usize,
) -> Result<Vec<String>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local itemId, nodeName, slot = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        local out = {}
        if not item then
            return out
        end
        -- Find the notable on the tree for the allocated check
        local node
        for _, n in pairs(build.spec.nodes) do
            if n.dn == nodeName and n.type == "Notable" then
                node = n
                break
            end
        end
        if node and build.spec.allocNodes[node.id] then
            table.insert(out, "^7Anointing " .. nodeName
                .. " changes nothing because this node is already allocated on the tree.")
            return out
        end
        for _, mod in ipairs(item.enchantModLines or {}) do
            local cur = mod.line and mod.line:match("^Allocates (.+)$")
            if cur == nodeName then
                table.insert(out, "^7Anointing " .. nodeName
                    .. " changes nothing because this node is already anointed.")
                return out
            end
        end
        -- Build the anointed copy (not committed to the build)
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        if #newItem.enchantModLines >= slot then
            table.remove(newItem.enchantModLines, slot)
        end
        table.insert(newItem.enchantModLines, slot,
            { crafted = true, line = "Allocates " .. nodeName })
        newItem:BuildAndParseRaw()

        local calcFunc = build.calcsTab:GetMiscCalculator()
        local outputBase = calcFunc({ repSlotName = "Amulet", repItem = item })
        local outputNew = calcFunc({ repSlotName = "Amulet", repItem = newItem })
        local tt = new("Tooltip")
        local header = "^7Anointing " .. nodeName .. " will give you: "
        local ok, numChanges = pcall(function()
            return build:AddStatComparesToTooltip(tt, outputBase, outputNew, header)
        end)
        if not ok then
            table.insert(out, "^1Comparison failed: " .. tostring(numChanges))
            return out
        end
        for _, line in ipairs(tt.lines) do
            if line.text then
                table.insert(out, line.text)
            end
        end
        if numChanges == 0 then
            table.insert(out, "^7Anointing " .. nodeName .. " changes nothing.")
        end
        return out
    "#,
        )
        .call((item_id, node_name, slot))?;
    Ok(list.sequence_values::<String>().flatten().collect())
}

/// Power stats selectable for sorting crafting-popup mod lists (upstream's
/// buildModSortList: powerStatList entries not flagged ignoreForNodes).
/// `index` is 1-based into the unfiltered upstream list.
pub fn mod_sort_stats(lua: &Lua) -> Result<Vec<super::node_power::PowerStat>, mlua::Error> {
    super::node_power::list_power_stats(lua)
}

/// Sort values for the corrupt popup: for each corrupted-implicit option (in
/// the same deterministic order as [`corrupt_options`]), apply its mod lines
/// to a clone of the item and compute the selected power stat with the clone
/// equipped (upstream's popup sorting; one throwaway calc pass per option).
/// `stat_index` is 1-based into `data.powerStatList`.
pub fn corrupt_sort_values(
    lua: &Lua,
    item_id: i64,
    stat_index: usize,
) -> Result<Vec<f64>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId, statIndex = ...
        {CORRUPT_LIST_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local baseItem = itemsTab.items[itemId]
        local stat = data.powerStatList[statIndex]
        local out = {{}}
        if not baseItem or not stat or not stat.stat then
            return out
        end
        local slotName = itemsTab:GetComparisonSlotNameForItem(baseItem)
        local calcFunc = build.calcsTab:GetMiscCalculator(build)
        local useFullDPS = stat.stat == "FullDPS"
        for _, mod in ipairs(buildCorruptList(baseItem)) do
            local item = new("Item", baseItem:BuildRaw())
            item.id = baseItem.id
            for _, line in ipairs(mod) do
                table.insert(item.implicitModLines, {{ line = line }})
            end
            item:BuildAndParseRaw()
            local output = calcFunc({{ repSlotName = slotName, repItem = item }}, useFullDPS)
            table.insert(out, data.powerStatList.GetFromOutput(output, stat) or 0)
        end
        return out
    "#,
        ))
        .call((item_id, stat_index))?;
    let mut values = Vec::new();
    for v in list.sequence_values::<f64>() {
        values.push(v?);
    }
    Ok(values)
}

/// Anoint an item with a notable (None removes the first anoint), mirroring
/// upstream's anointItem: the enchant line in the slot is replaced.
pub fn anoint_item(
    lua: &Lua,
    item_id: i64,
    node_name: Option<&str>,
    slot: usize,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, nodeName, slot = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        if #newItem.enchantModLines >= slot then
            table.remove(newItem.enchantModLines, slot)
        end
        if nodeName then
            table.insert(newItem.enchantModLines, slot,
                { crafted = true, line = "Allocates " .. nodeName })
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, node_name, slot))
}

/// Append a custom modifier line to an item (crafted = bench-style mod,
/// otherwise a plain custom mod).
pub fn add_custom_mod(
    lua: &Lua,
    item_id: i64,
    line: &str,
    crafted: bool,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId, line, crafted = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item then
            return
        end
        local modList, extra = modLib.parseMod(line)
        table.insert(item.explicitModLines, {
            line = line, extra = extra, modList = modList or { },
            crafted = crafted, custom = not crafted,
        })
        item:BuildAndParseRaw()
        build.itemsTab:PopulateSlots()
        build.itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((item_id, line, crafted))
}

/// Shared list-building for the add-modifier popup, ported from upstream's
/// ItemsTab:AddCustomModifierToDisplayItem (source eligibility + the
/// per-source buildMods bodies + the checkLineForAllocates helper; registered
/// in ports.toml). Every function below builds the list through these so the
/// order is identical across listing, applying, and sorting.
const MOD_CATALOG_LUA: &str = r#"
    local function checkLineForAllocates(line, nodes)
        if nodes and string.match(line, "Allocates") then
            local nodeId = tonumber(string.match(line, "%d+"))
            if nodes[nodeId] then
                return "Allocates "..nodes[nodeId].name
            end
        end
        return line
    end
    local function buildModSourceList(item)
        local sourceList = {}
        if item.type ~= "Tincture" and item.type ~= "Graft" then
            if item.type ~= "Jewel" then
                table.insert(sourceList, { label = "Crafting Bench", sourceId = "MASTER" })
            end
            if item.type ~= "Jewel" and item.type ~= "Flask" and item.type ~= "Graft" then
                table.insert(sourceList, { label = "Essence", sourceId = "ESSENCE" })
                table.insert(sourceList, { label = "Veiled", sourceId = "VEILED" })
                table.insert(sourceList, { label = "Beastcraft", sourceId = "BEASTCRAFT" })
            end
            if item.type == "Helmet" or item.type == "Body Armour"
               or item.type == "Gloves" or item.type == "Boots" then
                table.insert(sourceList, { label = "Necropolis", sourceId = "NECROPOLIS" })
            end
            if not item.clusterJewel and item.type ~= "Flask" and item.type ~= "Graft" then
                table.insert(sourceList, { label = "Delve", sourceId = "DELVE" })
            end
            if not item.crafted then
                table.insert(sourceList, { label = "Prefix", sourceId = "PREFIX" })
                table.insert(sourceList, { label = "Suffix", sourceId = "SUFFIX" })
            end
        end
        table.insert(sourceList, { label = "Custom", sourceId = "CUSTOM" })
        return sourceList
    end
    local function buildModCatalog(build, item, sourceId)
        local modList = {}
        if sourceId == "MASTER" then
            local excludeGroups = {}
            for _, modLine in ipairs({ item.prefixes, item.suffixes }) do
                for i = 1, (modLine.limit or (item.affixLimit / 2)) do
                    if modLine[i] and modLine[i].modId ~= "None" then
                        excludeGroups[item.affixes[modLine[i].modId].group] = true
                    end
                end
            end
            for i, craft in ipairs(build.data.masterMods) do
                if craft.types[item.type] and not excludeGroups[craft.group] then
                    table.insert(modList, {
                        label = table.concat(craft, "/") .. " (" .. craft.type .. ")",
                        mod = craft,
                        type = "crafted",
                        affixType = craft.type,
                        defaultOrder = i,
                    })
                end
            end
            table.sort(modList, function(a, b)
                if a.affixType ~= b.affixType then
                    return a.affixType == "Prefix" and b.affixType == "Suffix"
                else
                    return a.defaultOrder < b.defaultOrder
                end
            end)
        elseif sourceId == "ESSENCE" then
            for _, essence in pairs(build.data.essences) do
                local modId = essence.mods[item.type]
                local mod = item.affixes[modId]
                table.insert(modList, {
                    label = essence.name .. "   [" .. table.concat(mod, "/") .. "]"
                        .. " (" .. mod.type .. ")",
                    mod = mod,
                    type = "custom",
                    essence = essence,
                })
            end
            table.sort(modList, function(a, b)
                if a.essence.type ~= b.essence.type then
                    return a.essence.type > b.essence.type
                else
                    return a.essence.tier > b.essence.tier
                end
            end)
        elseif sourceId == "PREFIX" or sourceId == "SUFFIX" then
            for _, mod in pairs(item.affixes) do
                if sourceId:lower() == mod.type:lower() and item:GetModSpawnWeight(mod) > 0 then
                    table.insert(modList, {
                        label = mod.affix .. "   [" .. table.concat(mod, "/") .. "]",
                        mod = mod,
                        type = "custom",
                    })
                end
            end
            table.sort(modList, function(a, b)
                local modA = a.mod
                local modB = b.mod
                for i = 1, math.max(#modA, #modB) do
                    if not modA[i] then
                        return true
                    elseif not modB[i] then
                        return false
                    elseif modA.statOrder[i] ~= modB.statOrder[i] then
                        return modA.statOrder[i] < modB.statOrder[i]
                    end
                end
                return modA.level > modB.level
            end)
        elseif sourceId == "VEILED" then
            for i, mod in pairs(build.data.veiledMods) do
                if item:GetModSpawnWeight(mod) > 0 then
                    table.insert(modList, {
                        label = table.concat(mod, "/") .. " (" .. mod.type .. ")",
                        mod = mod,
                        affixType = mod.type,
                        type = "custom",
                        defaultOrder = i,
                    })
                end
            end
            table.sort(modList, function(a, b)
                if a.affixType ~= b.affixType then
                    return a.affixType == "Prefix" and b.affixType == "Suffix"
                else
                    return a.defaultOrder < b.defaultOrder
                end
            end)
        elseif sourceId == "DELVE" then
            for i, mod in pairs(item.affixes) do
                if item:CheckIfModIsDelve(mod) and item:GetModSpawnWeight(mod) > 0 then
                    table.insert(modList, {
                        label = table.concat(mod, "/") .. " (" .. mod.type .. ")",
                        mod = mod,
                        affixType = mod.type,
                        type = "custom",
                        defaultOrder = i,
                    })
                end
            end
            table.sort(modList, function(a, b)
                if a.affixType ~= b.affixType then
                    return a.affixType == "Prefix" and b.affixType == "Suffix"
                else
                    return a.defaultOrder < b.defaultOrder
                end
            end)
        elseif sourceId == "NECROPOLIS" then
            for i, mod in pairs(build.data.necropolisMods) do
                if item:GetNecropolisModSpawnWeight(mod) > 0 then
                    table.insert(modList, {
                        label = table.concat(mod, "/") .. " (" .. mod.type .. ")",
                        mod = mod,
                        affixType = mod.type,
                        type = "custom",
                        defaultOrder = i,
                    })
                end
            end
            table.sort(modList, function(a, b)
                if a.affixType ~= b.affixType then
                    return a.affixType == "Prefix" and b.affixType == "Suffix"
                else
                    return a.defaultOrder < b.defaultOrder
                end
            end)
        elseif sourceId == "BEASTCRAFT" then
            for i, mod in pairs(build.data.beastCraft) do
                table.insert(modList, {
                    label = table.concat(mod, "/") .. " (" .. mod.type .. ")",
                    mod = mod,
                    affixType = mod.type,
                    type = "custom",
                    defaultOrder = i,
                })
            end
            table.sort(modList, function(a, b)
                if a.affixType ~= b.affixType then
                    return a.affixType == "Prefix" and b.affixType == "Suffix"
                else
                    return a.defaultOrder < b.defaultOrder
                end
            end)
        end
        return modList
    end
"#;

/// One entry of the add-modifier catalog.
#[derive(Debug, Clone)]
pub struct ModCatalogEntry {
    /// Display label (upstream's, with color codes stripped).
    pub label: String,
    /// The mod text lines this entry would add.
    pub lines: Vec<String>,
}

/// Mod sources available for an item in the add-modifier popup (label, id),
/// per upstream's eligibility rules. "Custom" (free text) is always last.
pub fn mod_catalog_sources(lua: &Lua, item_id: i64) -> Result<Vec<(String, String)>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId = ...
        {MOD_CATALOG_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item then
            return {{}}
        end
        return buildModSourceList(item)
    "#
        ))
        .call(item_id)?;
    let mut sources = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        sources.push((
            entry.get("label").unwrap_or_default(),
            entry.get("sourceId").unwrap_or_default(),
        ));
    }
    Ok(sources)
}

/// The mod list for one source of the add-modifier popup, in upstream's
/// display order. Entries are applied by 1-based index via
/// [`apply_catalog_mod`].
pub fn mod_catalog(
    lua: &Lua,
    item_id: i64,
    source_id: &str,
) -> Result<Vec<ModCatalogEntry>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId, sourceId = ...
        {MOD_CATALOG_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        local out = {{}}
        if not item then
            return out
        end
        for _, listMod in ipairs(buildModCatalog(build, item, sourceId)) do
            local entry = {{
                label = (listMod.label:gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", "")),
                lines = {{}},
            }}
            for _, line in ipairs(listMod.mod) do
                table.insert(entry.lines, line)
            end
            table.insert(out, entry)
        end
        return out
    "#
        ))
        .call((item_id, source_id))?;
    let mut entries = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let lines: LuaTable = entry.get("lines")?;
        entries.push(ModCatalogEntry {
            label: entry.get("label").unwrap_or_default(),
            lines: lines.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(entries)
}

/// Apply a catalog mod (1-based index into [`mod_catalog`]'s order) to an
/// item, mirroring upstream's addModifier: all of the mod's lines are added
/// as explicits, flagged crafted (bench) or custom per the source.
pub fn apply_catalog_mod(
    lua: &Lua,
    item_id: i64,
    source_id: &str,
    index: usize,
) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local itemId, sourceId, index = ...
        {MOD_CATALOG_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if not item then
            return
        end
        local listMod = buildModCatalog(build, item, sourceId)[index]
        if not listMod then
            return
        end
        local newItem = new("Item", item:BuildRaw())
        newItem.id = item.id
        for _, line in ipairs(listMod.mod) do
            table.insert(newItem.explicitModLines,
                {{ line = line, modTags = listMod.mod.modTags, [listMod.type] = true }})
        end
        newItem:BuildAndParseRaw()
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .call((item_id, source_id, index))
}

/// Sort values for the add-modifier popup: one value per catalog entry (in
/// [`mod_catalog`] order), computed by applying the entry's lines to a clone
/// of the item and reading the selected power stat with the clone equipped
/// (upstream's popup sorting incl. the checkLineForAllocates substitution;
/// one throwaway calc pass per entry). `stat_index` is 1-based into
/// `data.powerStatList`.
pub fn mod_catalog_sort_values(
    lua: &Lua,
    item_id: i64,
    source_id: &str,
    stat_index: usize,
) -> Result<Vec<f64>, mlua::Error> {
    let list: LuaTable = lua
        .load(format!(
            r#"
        local itemId, sourceId, statIndex = ...
        {MOD_CATALOG_LUA}
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        local stat = data.powerStatList[statIndex]
        local out = {{}}
        if not item or not stat or not stat.stat then
            return out
        end
        local slotName = item:GetPrimarySlot()
        local calcFunc = build.calcsTab:GetMiscCalculator(build)
        local useFullDPS = stat.stat == "FullDPS"
        for _, listMod in ipairs(buildModCatalog(build, item, sourceId)) do
            local newItem = new("Item", item:BuildRaw())
            newItem.id = item.id
            for _, line in ipairs(listMod.mod) do
                table.insert(newItem.explicitModLines, {{
                    line = checkLineForAllocates(line, build.spec.nodes),
                    modTags = listMod.mod.modTags,
                    [listMod.type] = true,
                }})
            end
            newItem:BuildAndParseRaw()
            local output = calcFunc({{ repSlotName = slotName, repItem = newItem }}, useFullDPS)
            table.insert(out, data.powerStatList.GetFromOutput(output, stat) or 0)
        end
        return out
    "#
        ))
        .call((item_id, source_id, stat_index))?;
    let mut values = Vec::new();
    for v in list.sequence_values::<f64>() {
        values.push(v?);
    }
    Ok(values)
}

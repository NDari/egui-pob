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
        out.nodeCount = math.min(
            math.max(item.clusterJewelNodeCount or item.clusterJewel.maxNodes,
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

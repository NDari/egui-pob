//! Item data: equipped items, the item list, and item mutations, all backed
//! by Lua's build.itemsTab. Item text parsing uses upstream's Item:ParseRaw
//! (via the Item constructor) so mod parsing stays in sync with upstream.

use mlua::prelude::*;

/// An equipped item in a slot.
#[derive(Debug, Clone)]
pub struct EquippedItem {
    pub slot_name: String,
    pub label: String,
    pub sel_item_id: i64,
    pub item: Option<ItemInfo>,
    /// Items from the build's item list that can be equipped in this slot.
    pub valid_items: Vec<SlotChoice>,
}

/// An item that can be selected in a slot's dropdown.
#[derive(Debug, Clone)]
pub struct SlotChoice {
    pub id: i64,
    pub name: String,
    pub rarity: String,
}

/// An entry in the build's full item list.
#[derive(Debug, Clone)]
pub struct ItemListEntry {
    pub id: i64,
    pub name: String,
    pub rarity: String,
    /// Slot label if the item is currently equipped in the active item set.
    pub equipped_slot: Option<String>,
}

/// One line of an item tooltip, produced by upstream's AddItemTooltip.
#[derive(Debug, Clone)]
pub struct TooltipLine {
    /// Text with PoB colour codes; empty for separator lines.
    pub text: String,
    /// Upstream font size (typically 14-24).
    pub size: f32,
    pub is_separator: bool,
}

/// Item details.
#[derive(Debug, Clone)]
pub struct ItemInfo {
    pub id: i64,
    pub name: String,
    pub base_name: String,
    pub rarity: String,
    pub item_type: String,
    pub quality: i64,
    pub level_req: Option<i64>,
    pub implicit_mods: Vec<String>,
    pub explicit_mods: Vec<String>,
}

/// Colour for an item name based on rarity (matches upstream colorCodes).
pub fn rarity_color(rarity: &str) -> egui::Color32 {
    match rarity {
        "NORMAL" => egui::Color32::from_rgb(200, 200, 200),
        "MAGIC" => egui::Color32::from_rgb(136, 136, 255),
        "RARE" => egui::Color32::from_rgb(255, 255, 119),
        "UNIQUE" => egui::Color32::from_rgb(175, 96, 37),
        "RELIC" => egui::Color32::from_rgb(82, 217, 127),
        _ => egui::Color32::from_rgb(200, 200, 200),
    }
}

impl ItemInfo {
    /// Color for the item name based on rarity.
    pub fn rarity_color(&self) -> egui::Color32 {
        rarity_color(&self.rarity)
    }
}

/// Extract all equipped items from the loaded build.
pub fn extract_equipped_items(lua: &Lua) -> Result<Vec<EquippedItem>, mlua::Error> {
    let items: Vec<EquippedItem> = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local itemsTab = build.itemsTab
            local spec = build.spec
            local result = {}
            for _, slot in ipairs(itemsTab.orderedSlots) do
                -- Skip jewel sockets that aren't allocated on the tree
                if slot.nodeId and not spec.allocNodes[slot.nodeId] then
                    goto continue
                end
                -- Skip weapon swap slots unless they have an item equipped or
                -- the active item set uses the second weapon set
                if slot.slotName:find("Swap") then
                    local swapActive = itemsTab.activeItemSet
                        and itemsTab.activeItemSet.useSecondWeaponSet == true
                    local swapItemId = 0
                    if itemsTab.activeItemSet and itemsTab.activeItemSet[slot.slotName] then
                        swapItemId = itemsTab.activeItemSet[slot.slotName].selItemId or 0
                    elseif slot.selItemId then
                        swapItemId = slot.selItemId
                    end
                    if swapItemId <= 0 and not swapActive then
                        goto continue
                    end
                end
                -- Skip abyssal sockets whose parent item doesn't have enough abyssal sockets
                if slot.parentSlot and slot.slotName:find("Abyssal Socket") then
                    local parentSlotName = slot.parentSlot.slotName
                    local parentItemId = 0
                    if itemsTab.activeItemSet and itemsTab.activeItemSet[parentSlotName] then
                        parentItemId = itemsTab.activeItemSet[parentSlotName].selItemId or 0
                    elseif slot.parentSlot.selItemId then
                        parentItemId = slot.parentSlot.selItemId
                    end
                    local abyssalCount = 0
                    if parentItemId > 0 and itemsTab.items[parentItemId] then
                        abyssalCount = itemsTab.items[parentItemId].abyssalSocketCount or 0
                    end
                    -- Extract the socket number from the slot name (e.g. "Helmet Abyssal Socket 2" -> 2)
                    local socketNum = tonumber(slot.slotName:match("Abyssal Socket (%d+)")) or 0
                    if socketNum > abyssalCount then
                        goto continue
                    end
                end
                -- Skip Ring 3 unless AdditionalRingSlot flag is set
                if slot.slotName == "Ring 3" then
                    local calcsTab = build.calcsTab
                    if not calcsTab or not calcsTab.mainEnv or
                       not calcsTab.mainEnv.modDB:Flag(nil, "AdditionalRingSlot") then
                        goto continue
                    end
                end
                local slotName = slot.slotName
                local entry = { slotName = slotName, label = slot.label or slotName }
                local selItemId = 0
                if not slot.nodeId and itemsTab.activeItemSet and itemsTab.activeItemSet[slotName] then
                    selItemId = itemsTab.activeItemSet[slotName].selItemId or 0
                elseif slot.selItemId then
                    selItemId = slot.selItemId
                end
                entry.selItemId = selItemId
                -- Items that could be equipped in this slot
                entry.validItems = {}
                for _, id in ipairs(itemsTab.itemOrderList) do
                    local it = itemsTab.items[id]
                    if it and itemsTab:IsItemValidForSlot(it, slotName) then
                        table.insert(entry.validItems, {
                            id = id,
                            name = it.name or "",
                            rarity = it.rarity or "NORMAL",
                        })
                    end
                end
                if selItemId > 0 and itemsTab.items[selItemId] then
                    local item = itemsTab.items[selItemId]
                    entry.id = selItemId
                    entry.name = item.title or item.name or ""
                    entry.baseName = item.baseName or item.base and item.base.name or ""
                    entry.rarity = item.rarity or "NORMAL"
                    entry.itemType = item.type or ""
                    entry.quality = item.quality or 0
                    if item.requirements then
                        entry.levelReq = item.requirements.level
                    end
                    -- Collect mod lines from the raw text
                    entry.implicitMods = {}
                    entry.explicitMods = {}
                    if item.implicitModLines then
                        for _, modLine in ipairs(item.implicitModLines) do
                            if modLine.line then
                                table.insert(entry.implicitMods, modLine.line)
                            end
                        end
                    end
                    if item.explicitModLines then
                        for _, modLine in ipairs(item.explicitModLines) do
                            if modLine.line then
                                table.insert(entry.explicitMods, modLine.line)
                            end
                        end
                    end
                end
                table.insert(result, entry)
                ::continue::
            end
            return result
        "#,
        )
        .eval::<LuaTable>()
        .and_then(|table| {
            let mut items = Vec::new();
            for pair in table.sequence_values::<LuaTable>() {
                let entry = pair?;
                let slot_name: String = entry.get("slotName")?;
                let label: String = entry.get("label").unwrap_or_else(|_| slot_name.clone());
                let item = if entry.contains_key("id")? {
                    Some(ItemInfo {
                        id: entry.get("id")?,
                        name: entry.get("name").unwrap_or_default(),
                        base_name: entry.get("baseName").unwrap_or_default(),
                        rarity: entry.get("rarity").unwrap_or_default(),
                        item_type: entry.get("itemType").unwrap_or_default(),
                        quality: entry.get("quality").unwrap_or(0),
                        level_req: entry.get("levelReq").ok(),
                        implicit_mods: lua_string_list(&entry, "implicitMods"),
                        explicit_mods: lua_string_list(&entry, "explicitMods"),
                    })
                } else {
                    None
                };
                let mut valid_items = Vec::new();
                if let Ok(choices) = entry.get::<LuaTable>("validItems") {
                    for choice in choices.sequence_values::<LuaTable>() {
                        let choice = choice?;
                        valid_items.push(SlotChoice {
                            id: choice.get("id")?,
                            name: choice.get("name").unwrap_or_default(),
                            rarity: choice.get("rarity").unwrap_or_default(),
                        });
                    }
                }
                items.push(EquippedItem {
                    slot_name,
                    label,
                    sel_item_id: entry.get("selItemId").unwrap_or(0),
                    item,
                    valid_items,
                });
            }
            Ok(items)
        })?;

    Ok(items)
}

/// Extract the build's full item list in display order.
pub fn extract_item_list(lua: &Lua) -> Result<Vec<ItemListEntry>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local itemsTab = build.itemsTab
            local equipped = {}
            for slotName, slot in pairs(itemsTab.slots) do
                local selItemId = 0
                if not slot.nodeId and itemsTab.activeItemSet and itemsTab.activeItemSet[slotName] then
                    selItemId = itemsTab.activeItemSet[slotName].selItemId or 0
                elseif slot.selItemId then
                    selItemId = slot.selItemId
                end
                if selItemId > 0 and not equipped[selItemId] then
                    equipped[selItemId] = slot.label or slotName
                end
            end
            local result = {}
            for _, id in ipairs(itemsTab.itemOrderList) do
                local item = itemsTab.items[id]
                if item then
                    table.insert(result, {
                        id = id,
                        name = item.name or "",
                        rarity = item.rarity or "NORMAL",
                        slot = equipped[id],
                    })
                end
            end
            return result
        "#,
        )
        .eval()?;

    let mut entries = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        entries.push(ItemListEntry {
            id: entry.get("id")?,
            name: entry.get("name").unwrap_or_default(),
            rarity: entry.get("rarity").unwrap_or_default(),
            equipped_slot: entry.get("slot").ok(),
        });
    }
    Ok(entries)
}

/// Equip an item in a slot (item_id 0 unequips). Uses upstream's
/// ItemSlot:SetSelItemId so jewel sockets and item sets stay consistent.
pub fn equip_item(lua: &Lua, slot_name: &str, item_id: i64) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local slotName, itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local slot = itemsTab.slots[slotName]
        if slot and (itemId == 0 or itemsTab.items[itemId]) then
            slot:SetSelItemId(itemId)
            itemsTab:PopulateSlots()
            itemsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call((slot_name, item_id))
}

/// Delete an item from the build. Upstream's DeleteItem also unequips it
/// from all item sets and jewel sockets.
pub fn delete_item(lua: &Lua, item_id: i64) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local item = itemsTab.items[itemId]
        if item then
            itemsTab:DeleteItem(item)
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#,
    )
    .call(item_id)
}

/// Parse raw item text (as copied from the game or trade site) with
/// upstream's Item:ParseRaw and add it to the build, auto-equipping into a
/// free valid slot. Returns Some(error) if the text is not a valid item.
pub fn add_item_from_raw(lua: &Lua, raw: &str) -> Result<Option<String>, mlua::Error> {
    lua.load(
        r#"
        local raw = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        local newItem = new("Item", raw)
        if not newItem or not newItem.base then
            return "Unrecognised item text"
        end
        newItem:NormaliseQuality()
        newItem:BuildModList()
        itemsTab:AddItem(newItem, false)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
        return nil
    "#,
    )
    .call(raw)
}

/// Get an item's raw text (upstream's Item:BuildRaw), as shown in the
/// edit-item popup. Includes the "Rarity:" line.
pub fn get_item_raw(lua: &Lua, item_id: i64) -> Result<String, mlua::Error> {
    lua.load(
        r#"
        local itemId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local item = build.itemsTab.items[itemId]
        if not item then
            return ""
        end
        return item:BuildRaw()
    "#,
    )
    .call(item_id)
}

/// Check whether raw item text parses to a valid item.
pub fn validate_item_raw(lua: &Lua, raw: &str) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local raw = ...
        local item = new("Item", raw)
        return item ~= nil and item.base ~= nil
    "#,
    )
    .call(raw)
}

/// Replace an existing item's text, re-parsing with upstream's Item:ParseRaw.
/// The item keeps its id, so it stays equipped wherever it was. Mirrors
/// upstream's EditDisplayItemText save path (no quality normalisation).
/// Returns Some(error) if the new text is not a valid item.
pub fn replace_item_from_raw(
    lua: &Lua,
    item_id: i64,
    raw: &str,
) -> Result<Option<String>, mlua::Error> {
    lua.load(
        r#"
        local itemId, raw = ...
        local build = mainObject_ref.main.modes['BUILD']
        local itemsTab = build.itemsTab
        if not itemsTab.items[itemId] then
            return "No such item"
        end
        local newItem = new("Item", raw)
        if not newItem or not newItem.base then
            return "Unrecognised item text"
        end
        newItem.id = itemId
        itemsTab:AddItem(newItem, true)
        itemsTab:PopulateSlots()
        itemsTab:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
        return nil
    "#,
    )
    .call((item_id, raw))
}

/// Editable properties parsed from raw item text: variants, quality, and
/// influence. Drives the dropdowns in the item edit dialog.
#[derive(Debug, Clone, Default)]
pub struct ItemEditInfo {
    /// Variant names (empty when the item has no variants).
    pub variants: Vec<String>,
    /// Selected variant, 1-based (0 = unset).
    pub variant: usize,
    /// Selected variant per alt-variant slot, 1-based. One entry per
    /// `hasAltVariant`..`hasAltVariant5` flag present on the item.
    pub alt_variants: Vec<usize>,
    /// Item quality (None when the base has no quality field).
    pub quality: Option<i64>,
    pub can_be_influenced: bool,
    /// Influence display names ("Shaper", "Elder", ...).
    pub influence_names: Vec<String>,
    /// Selected influences, 1-based into `influence_names` (0 = none).
    pub influence1: usize,
    pub influence2: usize,
}

/// A single edit applied to raw item text via the edit dialog.
#[derive(Debug, Clone)]
pub enum ItemEditOp {
    /// Select a variant (1-based).
    Variant(usize),
    /// Select an alt variant: (alt slot 1-5, selection 1-based).
    AltVariant(u8, usize),
    /// Set item quality.
    Quality(i64),
    /// Set influences (0 = none, else 1-based into `influence_names`).
    Influence(usize, usize),
}

/// Parse raw item text into its editable properties.
pub fn item_edit_info(lua: &Lua, raw: &str) -> Result<ItemEditInfo, mlua::Error> {
    let t: LuaTable = lua
        .load(
            r#"
        local raw = ...
        local out = { variants = {}, altVariants = {}, influenceNames = {} }
        local item = new("Item", raw)
        if not item or not item.base then
            return out
        end
        if item.variantList then
            for _, name in ipairs(item.variantList) do
                table.insert(out.variants, name)
            end
            out.variant = item.variant or #item.variantList
            for i = 1, 5 do
                local hasKey = i == 1 and "hasAltVariant" or ("hasAltVariant" .. i)
                local selKey = i == 1 and "variantAlt" or ("variantAlt" .. i)
                if item[hasKey] then
                    table.insert(out.altVariants, item[selKey] or #item.variantList)
                end
            end
        end
        out.quality = item.quality
        out.canBeInfluenced = item.canBeInfluenced or false
        local found = {}
        for i, info in ipairs(itemLib.influenceInfo.all) do
            table.insert(out.influenceNames, info.display)
            if item[info.key] then
                table.insert(found, i)
            end
        end
        out.influence1 = found[1] or 0
        out.influence2 = found[2] or 0
        return out
    "#,
        )
        .call(raw)?;

    let get_vec = |key: &str| -> Vec<String> {
        t.get::<LuaTable>(key)
            .map(|tbl| {
                tbl.sequence_values::<String>()
                    .filter_map(|r| r.ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    let alt_variants: Vec<usize> = t
        .get::<LuaTable>("altVariants")
        .map(|tbl| {
            tbl.sequence_values::<usize>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();

    Ok(ItemEditInfo {
        variants: get_vec("variants"),
        variant: t.get("variant").unwrap_or(0),
        alt_variants,
        quality: t.get("quality").ok(),
        can_be_influenced: t.get("canBeInfluenced").unwrap_or(false),
        influence_names: get_vec("influenceNames"),
        influence1: t.get("influence1").unwrap_or(0),
        influence2: t.get("influence2").unwrap_or(0),
    })
}

/// Apply an edit to raw item text and return the rebuilt text (upstream's
/// `BuildAndParseRaw`). Returns None when the text does not parse.
pub fn apply_item_edit(
    lua: &Lua,
    raw: &str,
    op: &ItemEditOp,
) -> Result<Option<String>, mlua::Error> {
    let (kind, a, b): (&str, i64, i64) = match op {
        ItemEditOp::Variant(v) => ("variant", *v as i64, 0),
        ItemEditOp::AltVariant(slot, v) => ("altvariant", *slot as i64, *v as i64),
        ItemEditOp::Quality(q) => ("quality", *q, 0),
        ItemEditOp::Influence(i1, i2) => ("influence", *i1 as i64, *i2 as i64),
    };
    lua.load(
        r#"
        local raw, kind, a, b = ...
        local item = new("Item", raw)
        if not item or not item.base then
            return nil
        end
        if kind == "variant" then
            item.variant = a
        elseif kind == "altvariant" then
            local selKey = a == 1 and "variantAlt" or ("variantAlt" .. a)
            item[selKey] = b
        elseif kind == "quality" then
            item.quality = a
        elseif kind == "influence" then
            item:ResetInfluence()
            local influenceInfo = itemLib.influenceInfo.all
            for _, idx in ipairs({ a, b }) do
                if idx > 0 and influenceInfo[idx] then
                    item[influenceInfo[idx].key] = true
                end
            end
        end
        item:BuildAndParseRaw()
        return item:BuildRaw()
    "#,
    )
    .call((raw, kind, a, b))
}

/// Sort the item list with upstream's SortItemList (by slot order, equipped
/// first, then by name).
pub fn sort_item_list(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.itemsTab:SortItemList()
    "#,
    )
    .exec()
}

/// Build the full upstream tooltip for an item using ItemsTab:AddItemTooltip.
/// Pass a slot name to include the "Equipping this item will..." stat diff
/// for that slot; pass None for the plain item tooltip.
pub fn item_tooltip_lines(
    lua: &Lua,
    item_id: i64,
    slot_name: Option<&str>,
) -> Result<Vec<TooltipLine>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local itemId, slotName = ...
            local build = mainObject_ref.main.modes['BUILD']
            local itemsTab = build.itemsTab
            local item = itemsTab.items[itemId]
            if not item then
                return { lines = {} }
            end
            local slot = slotName and itemsTab.slots[slotName] or nil
            local tt = new("Tooltip")
            local ok, err = pcall(function()
                itemsTab:AddItemTooltip(tt, item, slot)
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
        .call((item_id, slot_name))?;

    if let Ok(err) = result.get::<String>("err") {
        log::warn!("AddItemTooltip failed for item {item_id}: {err}");
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

fn lua_string_list(table: &LuaTable, key: &str) -> Vec<String> {
    table
        .get::<LuaTable>(key)
        .map(|t| {
            t.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default()
}

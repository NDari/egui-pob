//! Item data: equipped items extracted from Lua's build.itemsTab.

use mlua::prelude::*;

/// An equipped item in a slot.
#[derive(Debug, Clone)]
pub struct EquippedItem {
    pub slot_name: String,
    pub item: Option<ItemInfo>,
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

impl ItemInfo {
    /// Color for the item name based on rarity.
    pub fn rarity_color(&self) -> egui::Color32 {
        match self.rarity.as_str() {
            "NORMAL" => egui::Color32::from_rgb(200, 200, 200),
            "MAGIC" => egui::Color32::from_rgb(136, 136, 255),
            "RARE" => egui::Color32::from_rgb(255, 255, 119),
            "UNIQUE" => egui::Color32::from_rgb(175, 96, 37),
            "RELIC" => egui::Color32::from_rgb(82, 217, 127),
            _ => egui::Color32::from_rgb(200, 200, 200),
        }
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
                -- Skip weapon swap slots unless they have an item equipped
                if slot.slotName:find("Swap") then
                    local swapItemId = 0
                    if itemsTab.activeItemSet and itemsTab.activeItemSet[slot.slotName] then
                        swapItemId = itemsTab.activeItemSet[slot.slotName].selItemId or 0
                    elseif slot.selItemId then
                        swapItemId = slot.selItemId
                    end
                    if swapItemId <= 0 then
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
                local entry = { slotName = slotName }
                local selItemId = 0
                if itemsTab.activeItemSet and itemsTab.activeItemSet[slotName] then
                    selItemId = itemsTab.activeItemSet[slotName].selItemId or 0
                elseif slot.selItemId then
                    selItemId = slot.selItemId
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
                items.push(EquippedItem { slot_name, item });
            }
            Ok(items)
        })?;

    Ok(items)
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

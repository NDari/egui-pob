//! Items tab: equipment slots with equip dropdowns, the build's item list,
//! full upstream tooltips, paste-from-clipboard, and item deletion.

use std::collections::HashMap;

use pob_egui::data::crafting::{
    self, AnointNotable, ClusterCraftInfo, CraftInfo, EnchantOptions, EnchantSource,
};
use pob_egui::data::item_db::{self, DbItem};
use pob_egui::data::item_sets::{self, ItemSetInfo};
use pob_egui::data::items::{self, EquippedItem, ItemListEntry, TooltipLine};
use pob_egui::lua_bridge::LuaBridge;

use super::theme::{self, Theme};

/// Drag-and-drop payload: an item dragged from the item list or from an
/// equipped slot.
struct ItemDragPayload {
    item_id: i64,
    /// Set when the drag started from an equipped slot.
    source_slot: Option<String>,
}

/// Pending name prompt in the item set manager.
enum SetAction {
    New,
    Copy(i64),
    Rename(i64),
}

struct SetPrompt {
    action: SetAction,
    text: String,
}

/// Item set manager UI state (survives the panel rebuilds that follow every
/// item change).
#[derive(Default)]
pub struct ItemSetsUi {
    manage_open: bool,
    prompt: Option<SetPrompt>,
    confirm_delete: Option<i64>,
}

/// New-item craft dialog state.
struct NewCraftDialog {
    rarity_idx: usize,
    type_idx: usize,
    base_idx: usize,
    title: String,
    types: Vec<String>,
    bases: Vec<String>,
}

/// Crafting UI state (survives panel rebuilds).
#[derive(Default)]
pub struct CraftUi {
    new_dialog: Option<NewCraftDialog>,
    /// Item whose affixes are being edited.
    edit_item: Option<i64>,
    custom_line: String,
    custom_crafted: bool,
    /// Item being anointed.
    anoint_item: Option<i64>,
    anoint_search: String,
    anoint_selected: Option<String>,
    /// Target anoint slot (1-based; multi-slot on Stranglegasp-likes).
    anoint_slot: usize,
    /// Item being enchanted.
    enchant_item: Option<i64>,
    enchant_skill: Option<String>,
    enchant_all_skills: bool,
    /// (source name, 1-based line index) selected in the enchant dialog.
    enchant_selection: Option<(String, usize)>,
    /// Item being corrupted; selections are 0 = None else 1-based option idx.
    corrupt_item: Option<i64>,
    corrupt_sel: (usize, usize),
    /// Sort mode for corrupt options (0 = default order, else 1-based into
    /// the popup sort stat list).
    corrupt_sort: usize,
    /// Item whose sockets/catalyst are being edited.
    socket_item: Option<i64>,
    /// Item receiving an implicit.
    implicit_item: Option<i64>,
    implicit_source_idx: usize,
    implicit_group_idx: usize,
    implicit_tier_idx: usize,
    implicit_custom: String,
    /// Sort mode for implicit groups (0 = default order, else 1-based into
    /// the popup sort stat list).
    implicit_sort: usize,
}

const CRAFT_RARITIES: [(&str, &str); 3] =
    [("Normal", "NORMAL"), ("Magic", "MAGIC"), ("Rare", "RARE")];

/// Enchant catalog cache: (item id, skill filter, sources with their lines).
type EnchantCatalogCache = (i64, Option<String>, Vec<(EnchantSource, Vec<String>)>);

/// Socket dialog cache: (item id, sockets, catalyst state).
type SocketCache = (i64, Option<crafting::ItemSockets>, Option<(usize, i64)>);

/// State for the items panel.
pub struct ItemsPanel {
    equipped: Vec<EquippedItem>,
    item_list: Vec<ItemListEntry>,
    error: Option<String>,
    /// Error from the last paste attempt (cleared on success by panel rebuild).
    paste_error: Option<String>,
    /// Item id pending delete confirmation.
    confirm_delete: Option<i64>,
    /// Open edit/create item dialog.
    edit_dialog: Option<EditItemDialog>,
    /// Cached tooltip lines keyed by (item id, slot context).
    tooltip_cache: HashMap<(i64, Option<String>), Vec<TooltipLine>>,
    /// Unique / rare-template database browser (survives panel rebuilds).
    pub item_db: ItemDbBrowser,
    /// Item sets in order + the active set id.
    sets: Vec<ItemSetInfo>,
    active_set: i64,
    /// Active set's weapon-swap flag.
    use_swap: bool,
    /// Item set manager state (survives panel rebuilds).
    pub sets_ui: ItemSetsUi,
    /// Crafting UI state (survives panel rebuilds).
    pub craft_ui: CraftUi,
    /// Cached affix data for the craft edit dialog (dropped on rebuild).
    craft_info_cache: Option<(i64, CraftInfo, Option<ClusterCraftInfo>)>,
    /// Cached anointable-notable list (loaded on first use).
    anoint_notables_cache: Option<Vec<AnointNotable>>,
    /// Cached anoint preview lines keyed by (item, slot, notable).
    anoint_preview_cache: Option<(i64, usize, String, Vec<String>)>,
    /// Cached enchant options for the enchant dialog.
    enchant_opts_cache: Option<(i64, EnchantOptions)>,
    /// Cached enchant catalog keyed by (item, skill).
    enchant_catalog_cache: Option<EnchantCatalogCache>,
    /// Cached corrupted-implicit options.
    corrupt_opts_cache: Option<(i64, Vec<crafting::CorruptOption>)>,
    /// Power stats for crafting-popup sorting (upstream buildModSortList).
    sort_stats: Vec<pob_egui::data::node_power::PowerStat>,
    /// Cached corrupt sort values keyed by (item, powerStatList index).
    corrupt_sort_cache: Option<(i64, usize, Vec<f64>)>,
    /// Cached implicit sort values keyed by (item, source, stat index).
    implicit_sort_cache: Option<(i64, String, usize, Vec<f64>)>,
    /// Cached socket + catalyst state for the socket dialog.
    socket_cache: Option<SocketCache>,
    /// Cached implicit sources.
    implicit_sources_cache: Option<(i64, Vec<(String, String)>)>,
    /// Cached implicit mod groups keyed by (item, source).
    implicit_mods_cache: Option<(i64, String, Vec<crafting::ImplicitGroup>)>,
}

/// Raw-text item editor state. `item_id` is None when creating a new item.
struct EditItemDialog {
    item_id: Option<i64>,
    text: String,
    error: Option<String>,
    /// Result of validating `validated_text` (avoids re-parsing every frame).
    valid: bool,
    validated_text: String,
    /// Editable properties (variants, quality, influence) of the current text.
    info: Option<items::ItemEditInfo>,
}

impl ItemsPanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        let mut error = None;
        let equipped = items::extract_equipped_items(lua).unwrap_or_else(|e| {
            log::error!("Failed to load equipped items: {e}");
            error = Some(format!("Failed to load items: {e}"));
            Vec::new()
        });
        let item_list = items::extract_item_list(lua).unwrap_or_else(|e| {
            log::error!("Failed to load item list: {e}");
            error = Some(format!("Failed to load item list: {e}"));
            Vec::new()
        });
        if error.is_none() {
            log::info!(
                "Loaded {} equipment slots, {} items",
                equipped.len(),
                item_list.len()
            );
        }
        let (sets, active_set) = item_sets::list_item_sets(lua).unwrap_or_else(|e| {
            log::error!("Failed to list item sets: {e}");
            (Vec::new(), 1)
        });
        let use_swap = item_sets::use_second_weapon_set(lua).unwrap_or(false);
        Self {
            equipped,
            item_list,
            error,
            paste_error: None,
            confirm_delete: None,
            edit_dialog: None,
            tooltip_cache: HashMap::new(),
            item_db: ItemDbBrowser::default(),
            sets,
            active_set,
            use_swap,
            sets_ui: ItemSetsUi::default(),
            craft_ui: CraftUi::default(),
            craft_info_cache: None,
            anoint_notables_cache: None,
            anoint_preview_cache: None,
            enchant_opts_cache: None,
            enchant_catalog_cache: None,
            corrupt_opts_cache: None,
            sort_stats: pob_egui::data::node_power::list_power_stats(lua).unwrap_or_default(),
            corrupt_sort_cache: None,
            implicit_sort_cache: None,
            socket_cache: None,
            implicit_sources_cache: None,
            implicit_mods_cache: None,
        }
    }

    /// Returns true if the build changed (items equipped/added/deleted).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if let Some(ref err) = self.error {
            ui.colored_label(Theme::ERROR, err);
            return false;
        }

        let mut changed = false;

        // Undo/redo (Ctrl+Z / Ctrl+Y), only when no widget (e.g. a search or
        // edit field) has keyboard focus
        if ui.ctx().memory(|m| m.focused().is_none()) {
            let undo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            let redo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y));
            if undo_pressed || redo_pressed {
                let result = if undo_pressed {
                    items::undo(bridge.lua())
                } else {
                    items::redo(bridge.lua())
                };
                match result {
                    // The parent rebuilds the panel (and the tree, since
                    // undo can change socketed jewels)
                    Ok(()) => changed = true,
                    Err(e) => log::error!("Items undo/redo failed: {e}"),
                }
            }
        }

        changed |= self.show_edit_dialog(ui, bridge);

        // Item set selector row
        if !self.sets.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Item set:");
                let active_label = self
                    .sets
                    .iter()
                    .find(|s| s.id == self.active_set)
                    .map(item_set_label)
                    .unwrap_or_else(|| "Default".to_string());
                egui::ComboBox::from_id_salt("item_set_select")
                    .selected_text(active_label)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for set in &self.sets {
                            if ui
                                .selectable_label(set.id == self.active_set, item_set_label(set))
                                .clicked()
                                && set.id != self.active_set
                            {
                                match item_sets::set_active_item_set(bridge.lua(), set.id) {
                                    Ok(()) => changed = true,
                                    Err(e) => log::error!("Failed to switch item set: {e}"),
                                }
                            }
                        }
                    });
                if ui.button("Manage...").clicked() {
                    self.sets_ui.manage_open = true;
                }
                ui.separator();
                let mut use_swap = self.use_swap;
                if ui
                    .checkbox(&mut use_swap, "Weapon swap")
                    .on_hover_text(
                        "Use the second weapon set (Weapon 1/2 Swap slots) for this item set",
                    )
                    .changed()
                {
                    match item_sets::set_use_second_weapon_set(bridge.lua(), use_swap) {
                        Ok(()) => changed = true,
                        Err(e) => log::error!("Failed to toggle weapon swap: {e}"),
                    }
                }
            });
        }

        changed |= self.show_set_manager(ui, bridge);
        changed |= self.show_craft_dialogs(ui, bridge);

        // Top bar: paste from clipboard, create custom item
        ui.horizontal(|ui| {
            if ui
                .button("Paste item")
                .on_hover_text("Add an item copied from the game or a trade site (Ctrl+V)")
                .clicked()
            {
                changed |= self.paste_item_from_clipboard(bridge);
            }
            if ui
                .button("New item")
                .on_hover_text("Create a custom item from raw text")
                .clicked()
            {
                self.edit_dialog = Some(EditItemDialog {
                    item_id: None,
                    text: "Rarity: RARE\nNew Item\n".to_string(),
                    error: None,
                    valid: false,
                    validated_text: String::new(),
                    info: None,
                });
            }
            if ui
                .button("Item DB")
                .on_hover_text("Browse the unique and rare-template item databases")
                .clicked()
            {
                self.item_db.open = !self.item_db.open;
            }
            if ui
                .button("Craft item")
                .on_hover_text("Create a Normal, Magic, or Rare item from a base type")
                .clicked()
            {
                let types = crafting::base_type_list(bridge.lua()).unwrap_or_default();
                let bases = types
                    .first()
                    .map(|t| crafting::base_list(bridge.lua(), t).unwrap_or_default())
                    .unwrap_or_default();
                self.craft_ui.new_dialog = Some(NewCraftDialog {
                    rarity_idx: 2,
                    type_idx: 0,
                    base_idx: 0,
                    title: String::new(),
                    types,
                    bases,
                });
            }
            if let Some(ref err) = self.paste_error {
                ui.colored_label(Theme::ERROR, err);
            }
        });

        changed |= self.item_db.show(ui, bridge);

        // Ctrl+V anywhere in the tab (egui delivers clipboard text as Paste events)
        let pasted: Option<String> = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        if let Some(text) = pasted
            && !ui.ctx().wants_keyboard_input()
        {
            changed |= self.add_item_raw(bridge, &text);
        }

        ui.separator();

        let panel_width = ui.available_width();
        ui.columns(2, |cols| {
            cols[0].push_id("equipment_slots", |ui| {
                ui.strong("Equipment");
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("slots_scroll")
                    .show(ui, |ui| {
                        changed |= self.show_slots(ui, bridge, panel_width * 0.5);
                    });
            });
            cols[1].push_id("item_list", |ui| {
                ui.horizontal(|ui| {
                    ui.strong("All Items");
                    if !self.item_list.is_empty()
                        && ui
                            .small_button("Sort")
                            .on_hover_text("Sort by slot, equipped first")
                            .clicked()
                    {
                        match items::sort_item_list(bridge.lua()) {
                            Ok(()) => changed = true,
                            Err(e) => log::error!("Failed to sort items: {e}"),
                        }
                    }
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("items_scroll")
                    .show(ui, |ui| {
                        changed |= self.show_item_list(ui, bridge);
                    });
            });
        });

        changed
    }

    /// Craft dialogs: the new-item popup and the affix editor.
    /// Returns true if the build changed.
    fn show_craft_dialogs(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        // New item dialog
        if let Some(dialog) = &mut self.craft_ui.new_dialog {
            let mut close = false;
            egui::Window::new("Craft Item")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Rarity:");
                        egui::ComboBox::from_id_salt("craft_rarity")
                            .selected_text(CRAFT_RARITIES[dialog.rarity_idx].0)
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for (i, (label, _)) in CRAFT_RARITIES.iter().enumerate() {
                                    ui.selectable_value(&mut dialog.rarity_idx, i, *label);
                                }
                            });
                        if dialog.rarity_idx == 2 {
                            ui.label("Name:");
                            ui.add(
                                egui::TextEdit::singleline(&mut dialog.title)
                                    .hint_text("New Item")
                                    .desired_width(140.0),
                            );
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Type:");
                        let current_type = dialog
                            .types
                            .get(dialog.type_idx)
                            .map(String::as_str)
                            .unwrap_or("?");
                        let mut new_type = None;
                        egui::ComboBox::from_id_salt("craft_type")
                            .selected_text(current_type)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for (i, t) in dialog.types.iter().enumerate() {
                                    if ui.selectable_label(i == dialog.type_idx, t).clicked()
                                        && i != dialog.type_idx
                                    {
                                        new_type = Some(i);
                                    }
                                }
                            });
                        if let Some(i) = new_type {
                            dialog.type_idx = i;
                            dialog.base_idx = 0;
                            dialog.bases = crafting::base_list(bridge.lua(), &dialog.types[i])
                                .unwrap_or_default();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Base:");
                        let current_base = dialog
                            .bases
                            .get(dialog.base_idx)
                            .map(String::as_str)
                            .unwrap_or("?");
                        egui::ComboBox::from_id_salt("craft_base")
                            .selected_text(current_base)
                            .width(240.0)
                            .show_ui(ui, |ui| {
                                for (i, b) in dialog.bases.iter().enumerate() {
                                    ui.selectable_value(&mut dialog.base_idx, i, b);
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!dialog.bases.is_empty(), egui::Button::new("Create"))
                            .clicked()
                        {
                            let rarity = CRAFT_RARITIES[dialog.rarity_idx].1;
                            let type_name = dialog.types[dialog.type_idx].clone();
                            match crafting::craft_item(
                                bridge.lua(),
                                rarity,
                                &type_name,
                                dialog.base_idx + 1,
                                dialog.title.trim(),
                            ) {
                                Ok(Some(id)) => {
                                    changed = true;
                                    // Open the affix editor for Magic/Rare
                                    if rarity != "NORMAL" {
                                        self.craft_ui.edit_item = Some(id);
                                    }
                                    close = true;
                                }
                                Ok(None) => log::error!("Craft failed: base not found"),
                                Err(e) => log::error!("Craft failed: {e}"),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.craft_ui.new_dialog = None;
            }
        }

        // Affix editor
        if let Some(item_id) = self.craft_ui.edit_item {
            // Fetch (or refetch after rebuild) the affix data
            if self
                .craft_info_cache
                .as_ref()
                .is_none_or(|(id, _, _)| *id != item_id)
            {
                match crafting::craft_info(bridge.lua(), item_id) {
                    Ok(Some(info)) => {
                        let cluster = crafting::cluster_craft_info(bridge.lua(), item_id)
                            .ok()
                            .flatten();
                        self.craft_info_cache = Some((item_id, info, cluster));
                    }
                    Ok(None) => {
                        // Item deleted or no longer crafted
                        self.craft_ui.edit_item = None;
                        self.craft_info_cache = None;
                    }
                    Err(e) => {
                        log::error!("Failed to load craft info: {e}");
                        self.craft_ui.edit_item = None;
                    }
                }
            }
        }
        if let (Some(item_id), Some((_, info, cluster))) =
            (self.craft_ui.edit_item, &self.craft_info_cache)
        {
            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Crafted Item".to_string());
            let mut close = false;
            // (is_prefix, index, mod_id, range) applied after the UI pass
            let mut apply: Option<(bool, usize, String, f64)> = None;
            let mut add_custom: Option<(String, bool)> = None;
            let mut apply_cluster: Option<(String, i64)> = None;

            egui::Window::new(format!("Craft: {item_name}"))
                .id(egui::Id::new("craft_affix_editor"))
                .collapsible(false)
                .resizable(true)
                .default_width(560.0)
                .show(ui.ctx(), |ui| {
                    // Cluster jewel skill + node count
                    if let Some(cluster) = cluster {
                        ui.horizontal(|ui| {
                            ui.label("Skill:");
                            let selected_name = cluster
                                .skills
                                .iter()
                                .find(|(id, _)| *id == cluster.selected_skill)
                                .map(|(_, name)| name.as_str())
                                .unwrap_or("?");
                            egui::ComboBox::from_id_salt("cluster_skill")
                                .selected_text(selected_name)
                                .width(300.0)
                                .show_ui(ui, |ui| {
                                    for (id, name) in &cluster.skills {
                                        if ui
                                            .selectable_label(*id == cluster.selected_skill, name)
                                            .clicked()
                                            && *id != cluster.selected_skill
                                        {
                                            apply_cluster = Some((id.clone(), cluster.node_count));
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Added passives:");
                            let mut count = cluster.node_count;
                            let resp = ui.add(egui::Slider::new(
                                &mut count,
                                cluster.min_nodes..=cluster.max_nodes,
                            ));
                            if (resp.drag_stopped() || (resp.changed() && !resp.dragged()))
                                && count != cluster.node_count
                            {
                                apply_cluster = Some((cluster.selected_skill.clone(), count));
                            }
                        });
                        ui.separator();
                    }
                    for slot in &info.slots {
                        let slot_label = if slot.is_prefix {
                            format!("Prefix {}", slot.index)
                        } else {
                            format!("Suffix {}", slot.index)
                        };
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [60.0, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(&slot_label)
                                        .small()
                                        .color(Theme::TEXT_MUTED),
                                ),
                            );
                            let selected_label = if slot.selected == "None" {
                                "None".to_string()
                            } else {
                                slot.options
                                    .iter()
                                    .find(|o| o.mod_id == slot.selected)
                                    .map(|o| truncate_label(&o.label, 60))
                                    .unwrap_or_else(|| slot.selected.clone())
                            };
                            egui::ComboBox::from_id_salt(format!(
                                "craft_affix_{}_{}",
                                slot.is_prefix, slot.index
                            ))
                            .selected_text(selected_label)
                            .width(460.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(slot.selected == "None", "None")
                                    .clicked()
                                    && slot.selected != "None"
                                {
                                    apply =
                                        Some((slot.is_prefix, slot.index, "None".to_string(), 0.5));
                                }
                                for opt in &slot.options {
                                    if ui
                                        .selectable_label(opt.mod_id == slot.selected, &opt.label)
                                        .clicked()
                                        && opt.mod_id != slot.selected
                                    {
                                        apply = Some((
                                            slot.is_prefix,
                                            slot.index,
                                            opt.mod_id.clone(),
                                            slot.range,
                                        ));
                                    }
                                }
                            });
                        });
                        // Roll position within the tier
                        if slot.has_range && slot.selected != "None" {
                            ui.horizontal(|ui| {
                                ui.add_space(64.0);
                                let mut range = slot.range;
                                let resp = ui.add(
                                    egui::Slider::new(&mut range, 0.0..=1.0)
                                        .text("roll")
                                        .show_value(false),
                                );
                                if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                                    apply = Some((
                                        slot.is_prefix,
                                        slot.index,
                                        slot.selected.clone(),
                                        range,
                                    ));
                                }
                            });
                        }
                    }
                    ui.separator();

                    // Custom modifier line
                    ui.horizontal(|ui| {
                        ui.label("Custom mod:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.craft_ui.custom_line)
                                .hint_text("e.g. +1 to Level of Socketed Gems")
                                .desired_width(260.0),
                        );
                        ui.checkbox(&mut self.craft_ui.custom_crafted, "Bench craft")
                            .on_hover_text(
                                "Mark the line as a crafted (bench) mod instead of a custom mod",
                            );
                        if ui
                            .add_enabled(
                                !self.craft_ui.custom_line.trim().is_empty(),
                                egui::Button::new("Add"),
                            )
                            .clicked()
                        {
                            add_custom = Some((
                                self.craft_ui.custom_line.trim().to_string(),
                                self.craft_ui.custom_crafted,
                            ));
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some((skill_id, count)) = apply_cluster {
                match crafting::set_cluster_jewel(bridge.lua(), item_id, &skill_id, count) {
                    Ok(()) => {
                        changed = true;
                        self.craft_info_cache = None;
                    }
                    Err(e) => log::error!("Failed to set cluster jewel: {e}"),
                }
            }
            if let Some((is_prefix, index, mod_id, range)) = apply {
                match crafting::set_affix(bridge.lua(), item_id, is_prefix, index, &mod_id, range) {
                    Ok(()) => {
                        changed = true;
                        self.craft_info_cache = None;
                    }
                    Err(e) => log::error!("Failed to set affix: {e}"),
                }
            }
            if let Some((line, crafted)) = add_custom {
                match crafting::add_custom_mod(bridge.lua(), item_id, &line, crafted) {
                    Ok(()) => {
                        changed = true;
                        self.craft_ui.custom_line.clear();
                        self.craft_info_cache = None;
                    }
                    Err(e) => log::error!("Failed to add custom mod: {e}"),
                }
            }
            if close {
                self.craft_ui.edit_item = None;
                self.craft_info_cache = None;
            }
        }

        // Anoint dialog
        if let Some(item_id) = self.craft_ui.anoint_item {
            if self.anoint_notables_cache.is_none() {
                self.anoint_notables_cache =
                    Some(crafting::anoint_notables(bridge.lua()).unwrap_or_else(|e| {
                        log::error!("Failed to list anoint notables: {e}");
                        Vec::new()
                    }));
            }
            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Amulet".to_string());
            let current = crafting::get_anoints(bridge.lua(), item_id).unwrap_or_default();
            let slot_count = crafting::anoint_slot_count(bridge.lua(), item_id).unwrap_or(1);
            let slot = self.craft_ui.anoint_slot.clamp(1, slot_count.max(1));
            let notables = self.anoint_notables_cache.as_ref().unwrap();
            let mut close = false;
            // Some(None) = remove selected slot, Some(Some(name)) = apply
            let mut action: Option<Option<String>> = None;
            let mut remove_slot: Option<usize> = None;

            egui::Window::new(format!("Anoint: {item_name}"))
                .id(egui::Id::new("anoint_dialog"))
                .collapsible(false)
                .resizable(true)
                .default_size([420.0, 460.0])
                .show(ui.ctx(), |ui| {
                    if !current.is_empty() {
                        for (i, anoint) in current.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Anoint {}: {anoint}", i + 1));
                                if ui.small_button("Remove").clicked() {
                                    remove_slot = Some(i + 1);
                                }
                            });
                        }
                        ui.separator();
                    }
                    if slot_count > 1 {
                        ui.horizontal(|ui| {
                            ui.label("Anoint into slot:");
                            for s in 1..=slot_count {
                                if ui.selectable_label(slot == s, format!("{s}")).clicked() {
                                    self.craft_ui.anoint_slot = s;
                                }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.craft_ui.anoint_search)
                                .hint_text("notable name or stat")
                                .desired_width(220.0),
                        );
                    });

                    let filter = self.craft_ui.anoint_search.trim().to_lowercase();
                    let filtered: Vec<&AnointNotable> = notables
                        .iter()
                        .filter(|n| {
                            filter.is_empty()
                                || n.name.to_lowercase().contains(&filter)
                                || n.stats.iter().any(|s| s.to_lowercase().contains(&filter))
                        })
                        .collect();
                    ui.label(format!("{} notables", filtered.len()));
                    egui::ScrollArea::vertical()
                        .id_salt("anoint_scroll")
                        .max_height(240.0)
                        .show_rows(ui, 18.0, filtered.len(), |ui, range| {
                            for notable in &filtered[range] {
                                let is_sel = self.craft_ui.anoint_selected.as_deref()
                                    == Some(notable.name.as_str());
                                if ui.selectable_label(is_sel, &notable.name).clicked() {
                                    self.craft_ui.anoint_selected = Some(notable.name.clone());
                                }
                            }
                        });

                    // Selected notable details: stats + oil recipe
                    if let Some(selected) = self
                        .craft_ui
                        .anoint_selected
                        .as_deref()
                        .and_then(|name| notables.iter().find(|n| n.name == name))
                    {
                        ui.separator();
                        ui.strong(&selected.name);
                        for stat in &selected.stats {
                            ui.label(
                                egui::RichText::new(stat)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(136, 136, 255)),
                            );
                        }
                        let oils: Vec<&str> = selected
                            .oils
                            .iter()
                            .map(|o| o.strip_suffix("Oil").unwrap_or(o))
                            .collect();
                        ui.label(
                            egui::RichText::new(format!("Oils: {}", oils.join(" + ")))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(248, 230, 202)),
                        );

                        // Stat comparison preview (one throwaway calc pass,
                        // cached per item/slot/notable)
                        let key_matches =
                            self.anoint_preview_cache
                                .as_ref()
                                .is_some_and(|(id, s, name, _)| {
                                    *id == item_id && *s == slot && name == &selected.name
                                });
                        if !key_matches {
                            let lines = crafting::anoint_preview(
                                bridge.lua(),
                                item_id,
                                &selected.name,
                                slot,
                            )
                            .unwrap_or_else(|e| {
                                log::error!("Anoint preview failed: {e}");
                                Vec::new()
                            });
                            self.anoint_preview_cache =
                                Some((item_id, slot, selected.name.clone(), lines));
                        }
                        if let Some((_, _, _, lines)) = &self.anoint_preview_cache {
                            ui.separator();
                            for line in lines {
                                ui.label(theme::pob_layout_job(line, 11.0, egui::Color32::WHITE));
                            }
                        }
                    }

                    ui.horizontal(|ui| {
                        let can_apply = self.craft_ui.anoint_selected.is_some();
                        if ui
                            .add_enabled(can_apply, egui::Button::new("Anoint"))
                            .clicked()
                        {
                            action = Some(self.craft_ui.anoint_selected.clone());
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some(node_name) = action {
                match crafting::anoint_item(bridge.lua(), item_id, node_name.as_deref(), slot) {
                    Ok(()) => {
                        changed = true;
                        self.anoint_preview_cache = None;
                    }
                    Err(e) => log::error!("Failed to anoint: {e}"),
                }
            }
            if let Some(s) = remove_slot {
                match crafting::anoint_item(bridge.lua(), item_id, None, s) {
                    Ok(()) => {
                        changed = true;
                        self.anoint_preview_cache = None;
                    }
                    Err(e) => log::error!("Failed to remove anoint: {e}"),
                }
            }
            if close {
                self.craft_ui.anoint_item = None;
                self.anoint_preview_cache = None;
            }
        }

        // Enchant dialog
        if let Some(item_id) = self.craft_ui.enchant_item {
            // Load the catalog shape once per item
            if self
                .enchant_opts_cache
                .as_ref()
                .is_none_or(|(id, _)| *id != item_id)
            {
                match crafting::enchant_options(bridge.lua(), item_id) {
                    Ok(Some(opts)) => {
                        // Default to a skill the build uses
                        self.craft_ui.enchant_skill =
                            opts.used_skills.first().or(opts.skills.first()).cloned();
                        self.craft_ui.enchant_all_skills = opts.used_skills.is_empty();
                        self.enchant_opts_cache = Some((item_id, opts));
                    }
                    Ok(None) => self.craft_ui.enchant_item = None,
                    Err(e) => {
                        log::error!("Failed to load enchant options: {e}");
                        self.craft_ui.enchant_item = None;
                    }
                }
            }
        }
        if let (Some(item_id), Some((_, opts))) =
            (self.craft_ui.enchant_item, &self.enchant_opts_cache)
        {
            let skill_key = if opts.has_skills {
                self.craft_ui.enchant_skill.clone()
            } else {
                None
            };
            // (Re)load the source catalog when the item/skill changes
            if self
                .enchant_catalog_cache
                .as_ref()
                .is_none_or(|(id, skill, _)| *id != item_id || *skill != skill_key)
            {
                let catalog =
                    crafting::enchant_catalog(bridge.lua(), item_id, skill_key.as_deref())
                        .unwrap_or_else(|e| {
                            log::error!("Failed to load enchant catalog: {e}");
                            Vec::new()
                        });
                self.enchant_catalog_cache = Some((item_id, skill_key.clone(), catalog));
            }
            let catalog = &self.enchant_catalog_cache.as_ref().unwrap().2;

            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Item".to_string());
            let mut close = false;
            let mut apply = false;
            let mut remove = false;
            let mut new_skill: Option<String> = None;

            egui::Window::new(format!("Enchant: {item_name}"))
                .id(egui::Id::new("enchant_dialog"))
                .collapsible(false)
                .resizable(true)
                .default_size([560.0, 420.0])
                .show(ui.ctx(), |ui| {
                    if opts.has_skills {
                        ui.horizontal(|ui| {
                            ui.label("Skill:");
                            let skills: &[String] = if self.craft_ui.enchant_all_skills
                                || opts.used_skills.is_empty()
                            {
                                &opts.skills
                            } else {
                                &opts.used_skills
                            };
                            let current = self.craft_ui.enchant_skill.as_deref().unwrap_or("-");
                            egui::ComboBox::from_id_salt("enchant_skill")
                                .selected_text(current)
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for skill in skills {
                                        if ui
                                            .selectable_label(
                                                Some(skill.as_str())
                                                    == self.craft_ui.enchant_skill.as_deref(),
                                                skill,
                                            )
                                            .clicked()
                                        {
                                            new_skill = Some(skill.clone());
                                        }
                                    }
                                });
                            let mut all = self.craft_ui.enchant_all_skills;
                            if ui
                                .add_enabled(
                                    !opts.used_skills.is_empty(),
                                    egui::Checkbox::new(&mut all, "All skills"),
                                )
                                .on_hover_text("Show all skills, not just those used by this build")
                                .changed()
                            {
                                self.craft_ui.enchant_all_skills = all;
                            }
                        });
                        ui.separator();
                    }

                    egui::ScrollArea::vertical()
                        .id_salt("enchant_scroll")
                        .max_height(280.0)
                        .show(ui, |ui| {
                            for (source, lines) in catalog {
                                ui.label(
                                    egui::RichText::new(&source.label)
                                        .strong()
                                        .color(Theme::TEXT_MUTED),
                                );
                                for (i, line) in lines.iter().enumerate() {
                                    let key = (source.name.clone(), i + 1);
                                    let is_sel =
                                        self.craft_ui.enchant_selection.as_ref() == Some(&key);
                                    if ui.selectable_label(is_sel, line).clicked() {
                                        self.craft_ui.enchant_selection = Some(key);
                                    }
                                }
                                ui.add_space(4.0);
                            }
                            if catalog.is_empty() {
                                ui.colored_label(Theme::TEXT_DIM, "No enchantments available");
                            }
                        });

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.craft_ui.enchant_selection.is_some(),
                                egui::Button::new("Enchant"),
                            )
                            .clicked()
                        {
                            apply = true;
                        }
                        if ui.button("Remove enchant").clicked() {
                            remove = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some(skill) = new_skill {
                self.craft_ui.enchant_skill = Some(skill);
                self.craft_ui.enchant_selection = None;
            }
            if apply && let Some((source, index)) = self.craft_ui.enchant_selection.clone() {
                match crafting::apply_enchant(
                    bridge.lua(),
                    item_id,
                    skill_key.as_deref(),
                    &source,
                    index,
                    1,
                ) {
                    Ok(()) => changed = true,
                    Err(e) => log::error!("Failed to enchant: {e}"),
                }
            }
            if remove {
                match crafting::remove_enchant(bridge.lua(), item_id, 1) {
                    Ok(()) => changed = true,
                    Err(e) => log::error!("Failed to remove enchant: {e}"),
                }
            }
            if close {
                self.craft_ui.enchant_item = None;
                self.enchant_opts_cache = None;
                self.enchant_catalog_cache = None;
            }
        }

        // Sockets & catalyst dialog
        if let Some(item_id) = self.craft_ui.socket_item {
            if self
                .socket_cache
                .as_ref()
                .is_none_or(|(id, _, _)| *id != item_id)
            {
                let sockets = crafting::item_sockets(bridge.lua(), item_id).unwrap_or_else(|e| {
                    log::error!("Failed to read sockets: {e}");
                    None
                });
                let catalyst = crafting::catalyst_info(bridge.lua(), item_id).unwrap_or_else(|e| {
                    log::error!("Failed to read catalyst: {e}");
                    None
                });
                self.socket_cache = Some((item_id, sockets, catalyst));
            }
            let (_, sockets, catalyst) = self.socket_cache.as_ref().unwrap();
            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Item".to_string());
            let mut close = false;
            // Deferred ops
            let mut set_color: Option<(usize, &'static str)> = None;
            let mut set_link: Option<(usize, bool)> = None;
            let mut add_socket = false;
            let mut set_catalyst: Option<(usize, i64)> = None;

            egui::Window::new(format!("Sockets & Catalyst: {item_name}"))
                .id(egui::Id::new("socket_dialog"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    if let Some(sockets) = sockets {
                        let normal_count = sockets.sockets.len() - sockets.abyssal_count as usize;
                        ui.horizontal(|ui| {
                            for (i, (color, group)) in sockets.sockets.iter().enumerate() {
                                if color == "A" {
                                    ui.label(
                                        egui::RichText::new("A")
                                            .color(egui::Color32::from_rgb(120, 220, 120)),
                                    );
                                    continue;
                                }
                                let color_rich = |c: &str| match c {
                                    "R" => egui::RichText::new("R")
                                        .color(egui::Color32::from_rgb(224, 80, 48)),
                                    "G" => egui::RichText::new("G")
                                        .color(egui::Color32::from_rgb(112, 255, 112)),
                                    "B" => egui::RichText::new("B")
                                        .color(egui::Color32::from_rgb(112, 112, 255)),
                                    _ => egui::RichText::new("W").color(egui::Color32::WHITE),
                                };
                                egui::ComboBox::from_id_salt(format!("socket_color_{i}"))
                                    .selected_text(color_rich(color))
                                    .width(44.0)
                                    .show_ui(ui, |ui| {
                                        for c in ["R", "G", "B", "W"] {
                                            if ui
                                                .selectable_label(color == c, color_rich(c))
                                                .clicked()
                                                && color != c
                                            {
                                                set_color = Some((i + 1, c));
                                            }
                                        }
                                    });
                                // Link checkbox between this and the next socket
                                if i + 1 < normal_count {
                                    let mut linked =
                                        sockets.sockets.get(i + 1).is_some_and(|(_, g)| g == group);
                                    if ui
                                        .checkbox(&mut linked, "")
                                        .on_hover_text("Link to the next socket")
                                        .changed()
                                    {
                                        set_link = Some((i + 1, linked));
                                    }
                                }
                            }
                            let cap = sockets.selectable_count + sockets.abyssal_count;
                            if (sockets.sockets.len() as i64) < cap
                                && ui.button("+").on_hover_text("Add a socket").clicked()
                            {
                                add_socket = true;
                            }
                        });
                    } else {
                        ui.colored_label(Theme::TEXT_DIM, "This item has no sockets.");
                    }

                    if let Some((catalyst, quality)) = catalyst {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Catalyst:");
                            let current = if *catalyst == 0 {
                                "None"
                            } else {
                                crafting::CATALYSTS
                                    .get(catalyst - 1)
                                    .copied()
                                    .unwrap_or("?")
                            };
                            egui::ComboBox::from_id_salt("catalyst_select")
                                .selected_text(current)
                                .width(230.0)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(*catalyst == 0, "None").clicked() {
                                        set_catalyst = Some((0, *quality));
                                    }
                                    for (i, name) in crafting::CATALYSTS.iter().enumerate() {
                                        if ui.selectable_label(*catalyst == i + 1, *name).clicked()
                                        {
                                            set_catalyst = Some((i + 1, *quality));
                                        }
                                    }
                                });
                            if *catalyst > 0 {
                                let mut q = *quality;
                                let resp = ui.add(
                                    egui::DragValue::new(&mut q)
                                        .range(1..=20)
                                        .prefix("Q ")
                                        .suffix("%"),
                                );
                                if (resp.drag_stopped() || (resp.changed() && !resp.dragged()))
                                    && q != *quality
                                {
                                    set_catalyst = Some((*catalyst, q));
                                }
                            }
                        });
                    }

                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });

            let mut applied = false;
            if let Some((index, color)) = set_color {
                match crafting::set_socket_color(bridge.lua(), item_id, index, color) {
                    Ok(()) => applied = true,
                    Err(e) => log::error!("Failed to set socket color: {e}"),
                }
            }
            if let Some((index, linked)) = set_link {
                match crafting::set_socket_link(bridge.lua(), item_id, index, linked) {
                    Ok(()) => applied = true,
                    Err(e) => log::error!("Failed to set link: {e}"),
                }
            }
            if add_socket {
                match crafting::add_socket(bridge.lua(), item_id) {
                    Ok(()) => applied = true,
                    Err(e) => log::error!("Failed to add socket: {e}"),
                }
            }
            if let Some((catalyst, quality)) = set_catalyst {
                match crafting::set_catalyst(bridge.lua(), item_id, catalyst, quality) {
                    Ok(()) => applied = true,
                    Err(e) => log::error!("Failed to set catalyst: {e}"),
                }
            }
            if applied {
                changed = true;
                self.socket_cache = None;
            }
            if close {
                self.craft_ui.socket_item = None;
                self.socket_cache = None;
            }
        }

        // Corrupt dialog
        if let Some(item_id) = self.craft_ui.corrupt_item {
            if self
                .corrupt_opts_cache
                .as_ref()
                .is_none_or(|(id, _)| *id != item_id)
            {
                let opts = crafting::corrupt_options(bridge.lua(), item_id).unwrap_or_else(|e| {
                    log::error!("Failed to load corrupt options: {e}");
                    Vec::new()
                });
                self.corrupt_opts_cache = Some((item_id, opts));
            }
            // Resolve the option ordering: sorted by a power stat when a
            // sort mode is selected (upstream's popup sorting; one calc
            // pass per option, cached per item + stat)
            if self.craft_ui.corrupt_sort > 0 {
                let stat_index = self
                    .sort_stats
                    .get(self.craft_ui.corrupt_sort - 1)
                    .map(|s| s.index)
                    .unwrap_or(0);
                let stale = self
                    .corrupt_sort_cache
                    .as_ref()
                    .is_none_or(|(id, si, _)| *id != item_id || *si != stat_index);
                if stale && stat_index > 0 {
                    match crafting::corrupt_sort_values(bridge.lua(), item_id, stat_index) {
                        Ok(values) => {
                            self.corrupt_sort_cache = Some((item_id, stat_index, values));
                        }
                        Err(e) => {
                            log::error!("Corrupt sort failed: {e}");
                            self.craft_ui.corrupt_sort = 0;
                        }
                    }
                }
            }
            let sort_values: Option<Vec<f64>> = (self.craft_ui.corrupt_sort > 0)
                .then(|| self.corrupt_sort_cache.as_ref().map(|(_, _, v)| v.clone()))
                .flatten();
            let options = &self.corrupt_opts_cache.as_ref().unwrap().1;
            let mut order: Vec<usize> = (0..options.len()).collect();
            if let Some(vals) = &sort_values
                && vals.len() == options.len()
            {
                order.sort_by(|&a, &b| {
                    vals[b]
                        .partial_cmp(&vals[a])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Item".to_string());
            let mut close = false;
            let mut do_corrupt = false;
            let mut new_sort: Option<usize> = None;

            egui::Window::new(format!("Corrupt: {item_name}"))
                .id(egui::Id::new("corrupt_dialog"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Sort by:");
                        let current = if self.craft_ui.corrupt_sort == 0 {
                            "Default".to_string()
                        } else {
                            self.sort_stats
                                .get(self.craft_ui.corrupt_sort - 1)
                                .map(|s| s.label.clone())
                                .unwrap_or_else(|| "?".to_string())
                        };
                        egui::ComboBox::from_id_salt("corrupt_sort")
                            .selected_text(current)
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(self.craft_ui.corrupt_sort == 0, "Default")
                                    .clicked()
                                {
                                    new_sort = Some(0);
                                }
                                for (i, stat) in self.sort_stats.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            self.craft_ui.corrupt_sort == i + 1,
                                            &stat.label,
                                        )
                                        .clicked()
                                    {
                                        new_sort = Some(i + 1);
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "Sort options by their impact on this stat \
                                 (one calc pass per option on first use)",
                            );
                    });
                    let combo = |ui: &mut egui::Ui,
                                 label: &str,
                                 sel: usize,
                                 exclude_group: Option<&str>|
                     -> Option<usize> {
                        let mut picked = None;
                        ui.horizontal(|ui| {
                            ui.label(label);
                            let current = if sel == 0 {
                                "None".to_string()
                            } else {
                                options
                                    .iter()
                                    .find(|o| o.index == sel)
                                    .map(|o| truncate_label(&o.label, 55))
                                    .unwrap_or_else(|| "?".to_string())
                            };
                            egui::ComboBox::from_id_salt(format!("corrupt_{label}"))
                                .selected_text(current)
                                .width(420.0)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(sel == 0, "None").clicked() {
                                        picked = Some(0);
                                    }
                                    for &pos in &order {
                                        let opt = &options[pos];
                                        if exclude_group == Some(opt.group.as_str()) {
                                            continue;
                                        }
                                        let text = match &sort_values {
                                            Some(vals) if vals.len() == options.len() => {
                                                format!("{}  [{:.1}]", opt.label, vals[pos])
                                            }
                                            _ => opt.label.clone(),
                                        };
                                        if ui.selectable_label(sel == opt.index, text).clicked() {
                                            picked = Some(opt.index);
                                        }
                                    }
                                });
                        });
                        picked
                    };
                    let group_of = |sel: usize| -> Option<&str> {
                        options
                            .iter()
                            .find(|o| o.index == sel && sel > 0)
                            .map(|o| o.group.as_str())
                    };
                    let (sel1, sel2) = self.craft_ui.corrupt_sel;
                    if let Some(p) = combo(ui, "Implicit 1:", sel1, group_of(sel2)) {
                        self.craft_ui.corrupt_sel.0 = p;
                    }
                    if let Some(p) = combo(ui, "Implicit 2:", sel2, group_of(sel1)) {
                        self.craft_ui.corrupt_sel.1 = p;
                    }
                    ui.label(
                        egui::RichText::new(
                            "Corrupting with no implicits selected only marks the item Corrupted.",
                        )
                        .small()
                        .color(Theme::TEXT_DIM),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Corrupt").clicked() {
                            do_corrupt = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some(s) = new_sort {
                self.craft_ui.corrupt_sort = s;
            }
            if do_corrupt {
                let (s1, s2) = self.craft_ui.corrupt_sel;
                match crafting::corrupt_item(
                    bridge.lua(),
                    item_id,
                    (s1 > 0).then_some(s1),
                    (s2 > 0).then_some(s2),
                ) {
                    Ok(()) => {
                        changed = true;
                        close = true;
                    }
                    Err(e) => log::error!("Failed to corrupt: {e}"),
                }
            }
            if close {
                self.craft_ui.corrupt_item = None;
                self.corrupt_opts_cache = None;
                self.corrupt_sort_cache = None;
                self.craft_ui.corrupt_sort = 0;
            }
        }

        // Add implicit dialog
        if let Some(item_id) = self.craft_ui.implicit_item {
            if self
                .implicit_sources_cache
                .as_ref()
                .is_none_or(|(id, _)| *id != item_id)
            {
                let sources =
                    crafting::implicit_sources(bridge.lua(), item_id).unwrap_or_else(|e| {
                        log::error!("Failed to load implicit sources: {e}");
                        Vec::new()
                    });
                self.implicit_sources_cache = Some((item_id, sources));
            }
            let sources = self.implicit_sources_cache.as_ref().unwrap().1.clone();
            let source_idx = self
                .craft_ui
                .implicit_source_idx
                .min(sources.len().saturating_sub(1));
            let source_id = sources
                .get(source_idx)
                .map(|(_, id)| id.clone())
                .unwrap_or_default();
            let is_custom = source_id == "CUSTOM";
            if !is_custom
                && self
                    .implicit_mods_cache
                    .as_ref()
                    .is_none_or(|(id, src, _)| *id != item_id || *src != source_id)
            {
                let groups = crafting::implicit_mods(bridge.lua(), item_id, &source_id)
                    .unwrap_or_else(|e| {
                        log::error!("Failed to load implicit mods: {e}");
                        Vec::new()
                    });
                self.implicit_mods_cache = Some((item_id, source_id.clone(), groups));
            }
            // Resolve the group ordering: sorted by a power stat when a
            // sort mode is selected (one calc pass per group, cached)
            if self.craft_ui.implicit_sort > 0 && !is_custom {
                let stat_index = self
                    .sort_stats
                    .get(self.craft_ui.implicit_sort - 1)
                    .map(|s| s.index)
                    .unwrap_or(0);
                let stale = self
                    .implicit_sort_cache
                    .as_ref()
                    .is_none_or(|(id, src, si, _)| {
                        *id != item_id || *src != source_id || *si != stat_index
                    });
                if stale && stat_index > 0 {
                    match crafting::implicit_sort_values(
                        bridge.lua(),
                        item_id,
                        &source_id,
                        stat_index,
                    ) {
                        Ok(values) => {
                            self.implicit_sort_cache =
                                Some((item_id, source_id.clone(), stat_index, values));
                        }
                        Err(e) => {
                            log::error!("Implicit sort failed: {e}");
                            self.craft_ui.implicit_sort = 0;
                        }
                    }
                }
            }
            let implicit_sort_values: Option<Vec<f64>> = (self.craft_ui.implicit_sort > 0
                && !is_custom)
                .then(|| {
                    self.implicit_sort_cache
                        .as_ref()
                        .map(|(_, _, _, v)| v.clone())
                })
                .flatten();
            let item_name = self
                .item_list
                .iter()
                .find(|e| e.id == item_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Item".to_string());
            let mut close = false;
            let mut do_add = false;
            let mut new_implicit_sort: Option<usize> = None;

            egui::Window::new(format!("Add Implicit: {item_name}"))
                .id(egui::Id::new("implicit_dialog"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Source:");
                        let current = sources
                            .get(source_idx)
                            .map(|(label, _)| label.as_str())
                            .unwrap_or("?");
                        egui::ComboBox::from_id_salt("implicit_source")
                            .selected_text(current)
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for (i, (label, _)) in sources.iter().enumerate() {
                                    if ui.selectable_label(i == source_idx, label).clicked()
                                        && i != source_idx
                                    {
                                        self.craft_ui.implicit_source_idx = i;
                                        self.craft_ui.implicit_group_idx = 0;
                                        self.craft_ui.implicit_tier_idx = 0;
                                    }
                                }
                            });
                    });
                    if is_custom {
                        ui.horizontal(|ui| {
                            ui.label("Modifier:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.craft_ui.implicit_custom)
                                    .hint_text("e.g. +25 to maximum Life")
                                    .desired_width(300.0),
                            );
                        });
                    } else if let Some((_, _, groups)) = &self.implicit_mods_cache {
                        ui.horizontal(|ui| {
                            ui.label("Sort by:");
                            let current = if self.craft_ui.implicit_sort == 0 {
                                "Default".to_string()
                            } else {
                                self.sort_stats
                                    .get(self.craft_ui.implicit_sort - 1)
                                    .map(|s| s.label.clone())
                                    .unwrap_or_else(|| "?".to_string())
                            };
                            egui::ComboBox::from_id_salt("implicit_sort")
                                .selected_text(current)
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            self.craft_ui.implicit_sort == 0,
                                            "Default",
                                        )
                                        .clicked()
                                    {
                                        new_implicit_sort = Some(0);
                                    }
                                    for (i, stat) in self.sort_stats.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                self.craft_ui.implicit_sort == i + 1,
                                                &stat.label,
                                            )
                                            .clicked()
                                        {
                                            new_implicit_sort = Some(i + 1);
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "Sort types by their top tier's impact on this stat \
                                     (one calc pass per type on first use)",
                                );
                        });
                        let mut group_order: Vec<usize> = (0..groups.len()).collect();
                        if let Some(vals) = &implicit_sort_values
                            && vals.len() == groups.len()
                        {
                            group_order.sort_by(|&a, &b| {
                                vals[b]
                                    .partial_cmp(&vals[a])
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            });
                        }
                        let group_idx = self
                            .craft_ui
                            .implicit_group_idx
                            .min(groups.len().saturating_sub(1));
                        ui.horizontal(|ui| {
                            ui.label("Type:");
                            let current = groups
                                .get(group_idx)
                                .map(|g| truncate_label(&g.label, 60))
                                .unwrap_or_else(|| "-".to_string());
                            egui::ComboBox::from_id_salt("implicit_group")
                                .selected_text(current)
                                .width(440.0)
                                .show_ui(ui, |ui| {
                                    for &pos in &group_order {
                                        let group = &groups[pos];
                                        let text = match &implicit_sort_values {
                                            Some(vals) if vals.len() == groups.len() => {
                                                format!("{}  [{:.1}]", group.label, vals[pos])
                                            }
                                            _ => group.label.clone(),
                                        };
                                        if ui.selectable_label(pos == group_idx, text).clicked()
                                            && pos != group_idx
                                        {
                                            self.craft_ui.implicit_group_idx = pos;
                                            self.craft_ui.implicit_tier_idx = 0;
                                        }
                                    }
                                });
                        });
                        if let Some(group) = groups.get(group_idx) {
                            let tier_idx = self
                                .craft_ui
                                .implicit_tier_idx
                                .min(group.tiers.len().saturating_sub(1));
                            ui.horizontal(|ui| {
                                ui.label("Tier:");
                                let current = group
                                    .tiers
                                    .get(tier_idx)
                                    .map(|t| truncate_label(t, 60))
                                    .unwrap_or_else(|| "-".to_string());
                                egui::ComboBox::from_id_salt("implicit_tier")
                                    .selected_text(current)
                                    .width(440.0)
                                    .show_ui(ui, |ui| {
                                        for (i, tier) in group.tiers.iter().enumerate() {
                                            if ui.selectable_label(i == tier_idx, tier).clicked() {
                                                self.craft_ui.implicit_tier_idx = i;
                                            }
                                        }
                                    });
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        let can_add = if is_custom {
                            !self.craft_ui.implicit_custom.trim().is_empty()
                        } else {
                            self.implicit_mods_cache
                                .as_ref()
                                .is_some_and(|(_, _, g)| !g.is_empty())
                        };
                        if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                            do_add = true;
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });

            if let Some(sort) = new_implicit_sort {
                self.craft_ui.implicit_sort = sort;
            }
            if do_add {
                let result = if is_custom {
                    crafting::add_custom_implicit(
                        bridge.lua(),
                        item_id,
                        self.craft_ui.implicit_custom.trim(),
                    )
                } else {
                    crafting::add_implicit(
                        bridge.lua(),
                        item_id,
                        &source_id,
                        self.craft_ui.implicit_group_idx + 1,
                        self.craft_ui.implicit_tier_idx + 1,
                    )
                };
                match result {
                    Ok(()) => {
                        changed = true;
                        self.craft_ui.implicit_custom.clear();
                    }
                    Err(e) => log::error!("Failed to add implicit: {e}"),
                }
            }
            if close {
                self.craft_ui.implicit_item = None;
                self.implicit_sources_cache = None;
                self.implicit_mods_cache = None;
                self.implicit_sort_cache = None;
                self.craft_ui.implicit_sort = 0;
            }
        }

        changed
    }

    /// Manage Item Sets dialog. Returns true if the sets changed.
    fn show_set_manager(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if !self.sets_ui.manage_open {
            return false;
        }
        let mut changed = false;
        let mut close = false;
        let mut activate: Option<i64> = None;
        let mut delete: Option<i64> = None;

        egui::Window::new("Manage Item Sets")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                for set in &self.sets {
                    ui.horizontal(|ui| {
                        let is_active = set.id == self.active_set;
                        let label = if is_active {
                            egui::RichText::new(item_set_label(set))
                                .color(super::theme::Theme::MAIN_SKILL)
                        } else {
                            egui::RichText::new(item_set_label(set))
                        };
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.sets.len() > 1 && ui.small_button("Delete").clicked() {
                                self.sets_ui.confirm_delete = Some(set.id);
                            }
                            if ui.small_button("Rename").clicked() {
                                self.sets_ui.prompt = Some(SetPrompt {
                                    action: SetAction::Rename(set.id),
                                    text: set.title.clone(),
                                });
                            }
                            if ui.small_button("Copy").clicked() {
                                self.sets_ui.prompt = Some(SetPrompt {
                                    action: SetAction::Copy(set.id),
                                    text: format!("{} (copy)", item_set_label(set)),
                                });
                            }
                            if !is_active && ui.small_button("Activate").clicked() {
                                activate = Some(set.id);
                            }
                        });
                    });
                }
                ui.separator();

                if let Some(prompt) = &mut self.sets_ui.prompt {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut prompt.text).desired_width(200.0));
                    });
                }

                ui.horizontal(|ui| {
                    if let Some(prompt) = &self.sets_ui.prompt {
                        let name = prompt.text.trim().to_string();
                        if ui
                            .add_enabled(!name.is_empty(), egui::Button::new("OK"))
                            .clicked()
                        {
                            let result = match prompt.action {
                                SetAction::New => item_sets::new_item_set(bridge.lua(), &name),
                                SetAction::Copy(id) => {
                                    item_sets::copy_item_set(bridge.lua(), id, &name)
                                }
                                SetAction::Rename(id) => {
                                    item_sets::rename_item_set(bridge.lua(), id, &name)
                                }
                            };
                            match result {
                                Ok(()) => changed = true,
                                Err(e) => log::error!("Item set action failed: {e}"),
                            }
                            self.sets_ui.prompt = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.sets_ui.prompt = None;
                        }
                    } else {
                        if ui.button("New Set").clicked() {
                            self.sets_ui.prompt = Some(SetPrompt {
                                action: SetAction::New,
                                text: String::new(),
                            });
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    }
                });

                if let Some(id) = self.sets_ui.confirm_delete {
                    let title = self
                        .sets
                        .iter()
                        .find(|s| s.id == id)
                        .map(item_set_label)
                        .unwrap_or_default();
                    ui.separator();
                    ui.colored_label(
                        Theme::ERROR,
                        format!("Delete '{title}'? Its equipment selection is lost."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            delete = Some(id);
                            self.sets_ui.confirm_delete = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.sets_ui.confirm_delete = None;
                        }
                    });
                }
            });

        if let Some(id) = activate {
            match item_sets::set_active_item_set(bridge.lua(), id) {
                Ok(()) => changed = true,
                Err(e) => log::error!("Failed to switch item set: {e}"),
            }
        }
        if let Some(id) = delete {
            match item_sets::delete_item_set(bridge.lua(), id) {
                Ok(()) => changed = true,
                Err(e) => log::error!("Failed to delete item set: {e}"),
            }
        }
        if close {
            self.sets_ui.manage_open = false;
        }
        changed
    }

    fn show_slots(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, label_span: f32) -> bool {
        // (slot_name, new_item_id) selected this frame
        let mut pending_equip: Option<(String, i64)> = None;
        // (target slot, dragged item, source slot if dragged from a slot)
        let mut pending_drop: Option<(String, i64, Option<String>)> = None;

        egui::Grid::new("slot_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for slot in &self.equipped {
                    ui.label(egui::RichText::new(&slot.label).color(Theme::TEXT_MUTED));

                    let selected_text = match &slot.item {
                        Some(item) => egui::RichText::new(&item.name).color(item.rarity_color()),
                        None => {
                            if slot.sel_item_id > 0 {
                                // Equipped but details not extracted (e.g. jewel)
                                let name = slot
                                    .valid_items
                                    .iter()
                                    .find(|c| c.id == slot.sel_item_id)
                                    .map(|c| c.name.clone())
                                    .unwrap_or_else(|| "?".into());
                                egui::RichText::new(name).color(Theme::TEXT_DEFAULT)
                            } else {
                                egui::RichText::new("None").color(Theme::TEXT_DIM)
                            }
                        }
                    };

                    ui.horizontal(|ui| {
                        let combo_resp = egui::ComboBox::from_id_salt(&slot.slot_name)
                            .selected_text(selected_text)
                            .width((label_span - 120.0).max(160.0))
                            .show_ui(ui, |ui| {
                                let none_resp = ui.selectable_label(slot.sel_item_id == 0, "None");
                                if none_resp.clicked() && slot.sel_item_id != 0 {
                                    pending_equip = Some((slot.slot_name.clone(), 0));
                                }
                                for choice in &slot.valid_items {
                                    let resp = ui.selectable_label(
                                        choice.id == slot.sel_item_id,
                                        egui::RichText::new(&choice.name)
                                            .color(items::rarity_color(&choice.rarity)),
                                    );
                                    let resp = hover_tooltip(
                                        resp,
                                        &mut self.tooltip_cache,
                                        bridge,
                                        choice.id,
                                        Some(&slot.slot_name),
                                    );
                                    if resp.clicked() && choice.id != slot.sel_item_id {
                                        pending_equip = Some((slot.slot_name.clone(), choice.id));
                                    }
                                }
                            });

                        // Drop target: accept a dragged item that is valid
                        // for this slot (highlight while hovering)
                        if let Some(payload) =
                            combo_resp.response.dnd_hover_payload::<ItemDragPayload>()
                        {
                            let accepts = payload.item_id > 0
                                && payload.source_slot.as_deref() != Some(slot.slot_name.as_str())
                                && slot.valid_items.iter().any(|c| c.id == payload.item_id);
                            if accepts {
                                ui.painter().rect_stroke(
                                    combo_resp.response.rect,
                                    3.0,
                                    egui::Stroke::new(2.0_f32, Theme::MAIN_SKILL),
                                    egui::StrokeKind::Outside,
                                );
                                if let Some(payload) =
                                    combo_resp.response.dnd_release_payload::<ItemDragPayload>()
                                {
                                    pending_drop = Some((
                                        slot.slot_name.clone(),
                                        payload.item_id,
                                        payload.source_slot.clone(),
                                    ));
                                }
                            }
                        }

                        // Tooltip for the currently equipped item on hover,
                        // and a drag handle to move it to another slot
                        if slot.sel_item_id > 0 {
                            hover_tooltip(
                                combo_resp.response,
                                &mut self.tooltip_cache,
                                bridge,
                                slot.sel_item_id,
                                Some(&slot.slot_name),
                            );
                            ui.dnd_drag_source(
                                egui::Id::new(("slot_drag", &slot.slot_name)),
                                ItemDragPayload {
                                    item_id: slot.sel_item_id,
                                    source_slot: Some(slot.slot_name.clone()),
                                },
                                |ui| {
                                    ui.label(egui::RichText::new("≡").color(Theme::TEXT_DIM))
                                        .on_hover_text("Drag to move to another slot");
                                },
                            );
                        }
                    });

                    ui.end_row();
                }
            });

        if let Some((slot_name, item_id)) = pending_equip {
            if let Err(e) = items::equip_item(bridge.lua(), &slot_name, item_id) {
                log::error!("Failed to equip item {item_id} in {slot_name}: {e}");
                return false;
            }
            return true;
        }
        if let Some((target, item_id, source)) = pending_drop {
            let result = match source {
                // Slot-to-slot drag: swap with the displaced item
                Some(source_slot) => {
                    items::move_item_between_slots(bridge.lua(), item_id, &source_slot, &target)
                        .inspect(|&moved| {
                            if !moved {
                                log::warn!("Item {item_id} is not valid for {target}");
                            }
                        })
                }
                // Drag from the item list: plain equip (upstream drop behavior)
                None => items::equip_item(bridge.lua(), &target, item_id).map(|_| true),
            };
            match result {
                Ok(changed) => return changed,
                Err(e) => {
                    log::error!("Failed to drop item {item_id} on {target}: {e}");
                    return false;
                }
            }
        }
        false
    }

    fn show_item_list(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if self.item_list.is_empty() {
            ui.colored_label(
                Theme::TEXT_DIM,
                "No items. Paste one from the game with Ctrl+V.",
            );
            return false;
        }

        let mut deleted: Option<i64> = None;
        let entries = self.item_list.clone();
        for entry in &entries {
            ui.horizontal(|ui| {
                if self.confirm_delete == Some(entry.id) {
                    ui.colored_label(Theme::ERROR, "Delete?");
                    if ui.small_button("Yes").clicked() {
                        deleted = Some(entry.id);
                        self.confirm_delete = None;
                    }
                    if ui.small_button("No").clicked() {
                        self.confirm_delete = None;
                    }
                } else {
                    if ui.small_button("🗑").on_hover_text("Delete item").clicked() {
                        self.confirm_delete = Some(entry.id);
                    }
                    if ui
                        .small_button("✏")
                        .on_hover_text("Edit item text")
                        .clicked()
                    {
                        match items::get_item_raw(bridge.lua(), entry.id) {
                            Ok(raw) if !raw.is_empty() => {
                                self.edit_dialog = Some(EditItemDialog {
                                    item_id: Some(entry.id),
                                    text: raw,
                                    error: None,
                                    valid: true,
                                    validated_text: String::new(),
                                    info: None,
                                });
                            }
                            Ok(_) => log::error!("Item {} has no raw text", entry.id),
                            Err(e) => log::error!("Failed to get item text: {e}"),
                        }
                    }
                }

                // The name label doubles as a drag source (drop on a slot to
                // equip, like upstream's list-to-slot dragging)
                let resp = ui
                    .dnd_drag_source(
                        egui::Id::new(("item_list_drag", entry.id)),
                        ItemDragPayload {
                            item_id: entry.id,
                            source_slot: None,
                        },
                        |ui| {
                            ui.label(
                                egui::RichText::new(&entry.name)
                                    .color(items::rarity_color(&entry.rarity)),
                            )
                        },
                    )
                    .inner
                    .on_hover_text("Right-click for crafting options; drag onto a slot to equip");
                resp.context_menu(|ui| {
                    if entry.crafted && ui.button("Edit affixes...").clicked() {
                        self.craft_ui.edit_item = Some(entry.id);
                        ui.close_menu();
                    }
                    if entry.item_type == "Amulet" && ui.button("Anoint...").clicked() {
                        self.craft_ui.anoint_item = Some(entry.id);
                        self.craft_ui.anoint_selected = None;
                        self.craft_ui.anoint_slot = 1;
                        ui.close_menu();
                    }
                    if entry.has_enchantments && ui.button("Enchant...").clicked() {
                        self.craft_ui.enchant_item = Some(entry.id);
                        self.craft_ui.enchant_skill = None;
                        self.craft_ui.enchant_selection = None;
                        ui.close_menu();
                    }
                    if ui.button("Sockets & catalyst...").clicked() {
                        self.craft_ui.socket_item = Some(entry.id);
                        ui.close_menu();
                    }
                    if ui.button("Corrupt...").clicked() {
                        self.craft_ui.corrupt_item = Some(entry.id);
                        self.craft_ui.corrupt_sel = (0, 0);
                        ui.close_menu();
                    }
                    if ui.button("Add implicit...").clicked() {
                        self.craft_ui.implicit_item = Some(entry.id);
                        self.craft_ui.implicit_source_idx = 0;
                        self.craft_ui.implicit_group_idx = 0;
                        self.craft_ui.implicit_tier_idx = 0;
                        self.craft_ui.implicit_custom.clear();
                        ui.close_menu();
                    }
                });
                hover_tooltip(resp, &mut self.tooltip_cache, bridge, entry.id, None);

                if let Some(ref slot) = entry.equipped_slot {
                    ui.colored_label(Theme::TEXT_DIM, format!("({slot})"));
                }
            });
        }

        if let Some(id) = deleted {
            if let Err(e) = items::delete_item(bridge.lua(), id) {
                log::error!("Failed to delete item {id}: {e}");
                return false;
            }
            return true;
        }
        false
    }

    /// Draw the edit/create item dialog if open. Returns true if the build
    /// changed (item saved).
    fn show_edit_dialog(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let Some(dialog) = &mut self.edit_dialog else {
            return false;
        };

        let mut changed = false;
        let mut close = false;
        let mut pending_op: Option<items::ItemEditOp> = None;
        let is_edit = dialog.item_id.is_some();
        let title = if is_edit {
            "Edit Item Text"
        } else {
            "Create Custom Item from Text"
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut dialog.text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(450.0)
                                .desired_rows(16),
                        );
                    });

                // Re-validate only when the text changes
                if dialog.text != dialog.validated_text {
                    dialog.valid =
                        items::validate_item_raw(bridge.lua(), &dialog.text).unwrap_or(false);
                    dialog.validated_text = dialog.text.clone();
                    dialog.info = if dialog.valid {
                        items::item_edit_info(bridge.lua(), &dialog.text)
                            .map_err(|e| log::error!("Failed to read item edit info: {e}"))
                            .ok()
                    } else {
                        None
                    };
                }
                if !dialog.valid {
                    ui.colored_label(
                        Theme::ERROR,
                        "Invalid item text. For Rare and Unique items the first two lines \
                         after \"Rarity:\" must be the title and base name.",
                    );
                }
                if let Some(ref err) = dialog.error {
                    ui.colored_label(Theme::ERROR, err);
                }

                // Variant / influence / quality dropdowns (like upstream's
                // edit popup); edits rebuild the raw text above.
                if let Some(ref info) = dialog.info {
                    pending_op = show_item_edit_controls(ui, info);
                }

                ui.horizontal(|ui| {
                    let label = if is_edit { "Save" } else { "Create" };
                    if ui
                        .add_enabled(dialog.valid, egui::Button::new(label))
                        .clicked()
                    {
                        let result = match dialog.item_id {
                            Some(id) => {
                                items::replace_item_from_raw(bridge.lua(), id, &dialog.text)
                            }
                            None => items::add_item_from_raw(bridge.lua(), &dialog.text),
                        };
                        match result {
                            Ok(None) => {
                                changed = true;
                                close = true;
                            }
                            Ok(Some(err)) => dialog.error = Some(err),
                            Err(e) => dialog.error = Some(format!("Failed: {e}")),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if let Some(op) = pending_op {
            match items::apply_item_edit(bridge.lua(), &dialog.text, &op) {
                Ok(Some(new_raw)) => dialog.text = new_raw,
                Ok(None) => log::error!("Item edit produced unparseable text"),
                Err(e) => log::error!("Failed to apply item edit: {e}"),
            }
        }

        if close {
            self.edit_dialog = None;
        }
        changed
    }

    fn paste_item_from_clipboard(&mut self, bridge: &LuaBridge) -> bool {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => self.add_item_raw(bridge, &text),
            Err(e) => {
                self.paste_error = Some(format!("Clipboard unavailable: {e}"));
                false
            }
        }
    }

    fn add_item_raw(&mut self, bridge: &LuaBridge, raw: &str) -> bool {
        if raw.trim().is_empty() {
            self.paste_error = Some("Clipboard is empty".into());
            return false;
        }
        match items::add_item_from_raw(bridge.lua(), raw) {
            Ok(None) => {
                self.paste_error = None;
                true
            }
            Ok(Some(err)) => {
                self.paste_error = Some(err);
                false
            }
            Err(e) => {
                log::error!("Failed to add item: {e}");
                self.paste_error = Some(format!("Failed to add item: {e}"));
                false
            }
        }
    }
}

/// How the item DB search box matches (like upstream's search-mode dropdown).
#[derive(Clone, Copy, PartialEq, Default)]
enum DbSearchMode {
    #[default]
    Anywhere,
    Names,
    Modifiers,
}

/// Browser window for the unique and rare-template item databases.
#[derive(Default)]
pub struct ItemDbBrowser {
    pub open: bool,
    loaded: bool,
    uniques: Vec<DbItem>,
    rares: Vec<DbItem>,
    /// False = uniques tab, true = rare templates tab.
    show_rares: bool,
    search: String,
    search_mode: DbSearchMode,
    /// Selected base type filter ("" = any type).
    type_filter: String,
    unique_types: Vec<String>,
    rare_types: Vec<String>,
    /// Tooltip lines cached by item name.
    tooltip_cache: HashMap<String, Vec<TooltipLine>>,
}

impl ItemDbBrowser {
    /// Draw the browser window if open. Returns true if an item was added.
    fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if !self.open {
            return false;
        }
        let mut changed = false;
        let mut open = self.open;

        egui::Window::new("Item Database")
            .open(&mut open)
            .default_size([420.0, 500.0])
            .resizable(true)
            .show(ui.ctx(), |ui| {
                // The DBs load via an upstream coroutine resumed once per
                // frame; pump it in bigger steps until done.
                if !self.loaded {
                    match item_db::pump_loading(bridge.lua(), 200) {
                        Ok(true) => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Loading item databases...");
                            });
                            ui.ctx().request_repaint();
                            return;
                        }
                        Ok(false) => {
                            self.uniques = item_db::extract_db(bridge.lua(), true)
                                .map_err(|e| log::error!("Failed to load unique DB: {e}"))
                                .unwrap_or_default();
                            self.rares = item_db::extract_db(bridge.lua(), false)
                                .map_err(|e| log::error!("Failed to load rare DB: {e}"))
                                .unwrap_or_default();
                            self.unique_types = distinct_types(&self.uniques);
                            self.rare_types = distinct_types(&self.rares);
                            self.loaded = true;
                        }
                        Err(e) => {
                            ui.colored_label(Theme::ERROR, format!("DB load failed: {e}"));
                            return;
                        }
                    }
                }

                // Tab selector + filters
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.show_rares, "Uniques").clicked() {
                        self.show_rares = false;
                        self.type_filter.clear();
                    }
                    if ui
                        .selectable_label(self.show_rares, "Rare Templates")
                        .clicked()
                    {
                        self.show_rares = true;
                        self.type_filter.clear();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search")
                            .desired_width(160.0),
                    );
                    egui::ComboBox::from_id_salt("item_db_search_mode")
                        .selected_text(match self.search_mode {
                            DbSearchMode::Anywhere => "Anywhere",
                            DbSearchMode::Names => "Names",
                            DbSearchMode::Modifiers => "Modifiers",
                        })
                        .width(100.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.search_mode,
                                DbSearchMode::Anywhere,
                                "Anywhere",
                            );
                            ui.selectable_value(
                                &mut self.search_mode,
                                DbSearchMode::Names,
                                "Names",
                            );
                            ui.selectable_value(
                                &mut self.search_mode,
                                DbSearchMode::Modifiers,
                                "Modifiers",
                            );
                        });
                    let types = if self.show_rares {
                        &self.rare_types
                    } else {
                        &self.unique_types
                    };
                    egui::ComboBox::from_id_salt("item_db_type_filter")
                        .selected_text(if self.type_filter.is_empty() {
                            "Any type"
                        } else {
                            self.type_filter.as_str()
                        })
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(self.type_filter.is_empty(), "Any type")
                                .clicked()
                            {
                                self.type_filter.clear();
                            }
                            for t in types {
                                if ui.selectable_label(self.type_filter == *t, t).clicked() {
                                    self.type_filter = t.clone();
                                }
                            }
                        });
                });
                ui.separator();

                // Filtered list (virtualized; the unique DB has ~1500 entries)
                let items = if self.show_rares {
                    &self.rares
                } else {
                    &self.uniques
                };
                let search = self.search.trim().to_lowercase();
                let filtered: Vec<&DbItem> = items
                    .iter()
                    .filter(|item| {
                        if !self.type_filter.is_empty() && item.item_type != self.type_filter {
                            return false;
                        }
                        if search.is_empty() {
                            return true;
                        }
                        match self.search_mode {
                            DbSearchMode::Names => item.search_name.contains(&search),
                            DbSearchMode::Modifiers => item.search_mods.contains(&search),
                            DbSearchMode::Anywhere => {
                                item.search_name.contains(&search)
                                    || item.search_mods.contains(&search)
                            }
                        }
                    })
                    .collect();

                ui.label(format!("{} items", filtered.len()));
                let color = items::rarity_color(if self.show_rares { "RARE" } else { "UNIQUE" });
                egui::ScrollArea::vertical()
                    .id_salt("item_db_scroll")
                    .show_rows(ui, 20.0, filtered.len(), |ui, range| {
                        for &item in &filtered[range] {
                            ui.horizontal(|ui| {
                                if ui.small_button("+").on_hover_text("Add to build").clicked() {
                                    match items::add_item_from_raw(bridge.lua(), &item.raw) {
                                        Ok(None) => changed = true,
                                        Ok(Some(err)) => {
                                            log::error!("Failed to add DB item: {err}")
                                        }
                                        Err(e) => log::error!("Failed to add DB item: {e}"),
                                    }
                                }
                                let resp = ui.label(egui::RichText::new(&item.name).color(color));
                                if resp.hovered() {
                                    if !self.tooltip_cache.contains_key(&item.name) {
                                        let lines =
                                            item_db::tooltip_from_raw(bridge.lua(), &item.raw)
                                                .unwrap_or_else(|e| {
                                                    log::error!("DB tooltip failed: {e}");
                                                    Vec::new()
                                                });
                                        self.tooltip_cache.insert(item.name.clone(), lines);
                                    }
                                    let lines = &self.tooltip_cache[&item.name];
                                    if !lines.is_empty() {
                                        resp.on_hover_ui(|ui| show_tooltip_lines(ui, lines));
                                    }
                                }
                            });
                        }
                    });
            });

        self.open = open;
        changed
    }
}

/// Distinct base types present in a DB list, sorted.
fn distinct_types(items: &[DbItem]) -> Vec<String> {
    let mut types: Vec<String> = items
        .iter()
        .map(|i| i.item_type.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    types.retain(|t| !t.is_empty());
    types
}

/// Draw variant, influence, and quality controls for the item edit dialog.
/// Returns the edit to apply this frame, if any.
fn show_item_edit_controls(
    ui: &mut egui::Ui,
    info: &items::ItemEditInfo,
) -> Option<items::ItemEditOp> {
    let mut op = None;

    // Variant dropdowns: the main one plus one per alt-variant slot
    if info.variants.len() > 1 {
        let variant_combo = |ui: &mut egui::Ui, label: &str, id: &str, selected: usize| {
            let mut picked = None;
            ui.horizontal(|ui| {
                ui.label(label);
                egui::ComboBox::from_id_salt(id)
                    .selected_text(
                        info.variants
                            .get(selected.wrapping_sub(1))
                            .map(String::as_str)
                            .unwrap_or("?"),
                    )
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in info.variants.iter().enumerate() {
                            if ui.selectable_label(i + 1 == selected, name).clicked()
                                && i + 1 != selected
                            {
                                picked = Some(i + 1);
                            }
                        }
                    });
            });
            picked
        };

        if let Some(v) = variant_combo(ui, "Variant:", "item_edit_variant", info.variant) {
            op = Some(items::ItemEditOp::Variant(v));
        }
        for (slot0, &selected) in info.alt_variants.iter().enumerate() {
            let label = format!("Variant {}:", slot0 + 2);
            let id = format!("item_edit_alt_variant_{slot0}");
            if let Some(v) = variant_combo(ui, &label, &id, selected) {
                op = Some(items::ItemEditOp::AltVariant(slot0 as u8 + 1, v));
            }
        }
    }

    // Influence dropdowns (Shaper/Elder/conqueror/exarch-eater)
    if info.can_be_influenced {
        ui.horizontal(|ui| {
            ui.label("Influence:");
            let influence_combo = |ui: &mut egui::Ui, id: &str, selected: usize| {
                let mut picked = None;
                egui::ComboBox::from_id_salt(id)
                    .selected_text(
                        info.influence_names
                            .get(selected.wrapping_sub(1))
                            .map(String::as_str)
                            .unwrap_or("None"),
                    )
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(selected == 0, "None").clicked() && selected != 0 {
                            picked = Some(0);
                        }
                        for (i, name) in info.influence_names.iter().enumerate() {
                            if ui.selectable_label(i + 1 == selected, name).clicked()
                                && i + 1 != selected
                            {
                                picked = Some(i + 1);
                            }
                        }
                    });
                picked
            };
            if let Some(v) = influence_combo(ui, "item_edit_influence1", info.influence1) {
                op = Some(items::ItemEditOp::Influence(v, info.influence2));
            }
            if let Some(v) = influence_combo(ui, "item_edit_influence2", info.influence2) {
                op = Some(items::ItemEditOp::Influence(info.influence1, v));
            }
        });
    }

    // Quality edit
    if let Some(quality) = info.quality {
        ui.horizontal(|ui| {
            ui.label("Quality:");
            let mut q = quality;
            let resp = ui.add(egui::DragValue::new(&mut q).range(0..=100).suffix("%"));
            if resp.changed() && q != quality {
                op = Some(items::ItemEditOp::Quality(q));
            }
        });
    }

    op
}

/// Attach the full upstream item tooltip to a response, computing and caching
/// the lines on first hover.
fn hover_tooltip(
    resp: egui::Response,
    cache: &mut HashMap<(i64, Option<String>), Vec<TooltipLine>>,
    bridge: &LuaBridge,
    item_id: i64,
    slot_name: Option<&str>,
) -> egui::Response {
    if !resp.hovered() {
        return resp;
    }
    let key = (item_id, slot_name.map(str::to_owned));
    if !cache.contains_key(&key) {
        let lines =
            items::item_tooltip_lines(bridge.lua(), item_id, slot_name).unwrap_or_else(|e| {
                log::error!("Tooltip failed for item {item_id}: {e}");
                Vec::new()
            });
        cache.insert(key.clone(), lines);
    }
    let lines = &cache[&key];
    if lines.is_empty() {
        return resp;
    }
    resp.on_hover_ui(|ui| show_tooltip_lines(ui, lines))
}

/// Shorten a long affix label for the closed combo box.
fn truncate_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        label.to_string()
    } else {
        let truncated: String = label.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

fn item_set_label(set: &ItemSetInfo) -> String {
    if set.title.is_empty() {
        "Default".to_string()
    } else {
        set.title.clone()
    }
}

/// Render item tooltip lines (shared by build items and DB items).
fn show_tooltip_lines(ui: &mut egui::Ui, lines: &[TooltipLine]) {
    ui.spacing_mut().item_spacing.y = 2.0;
    for line in lines {
        if line.is_separator {
            ui.separator();
        } else if line.text.is_empty() {
            ui.add_space(line.size * 0.4);
        } else {
            let size = (line.size * 0.75).clamp(10.0, 20.0);
            ui.label(theme::pob_layout_job(&line.text, size, Theme::TEXT_DEFAULT));
        }
    }
}

//! Items tab: equipment slots with equip dropdowns, the build's item list,
//! full upstream tooltips, paste-from-clipboard, and item deletion.

use std::collections::HashMap;

use pob_egui::data::item_db::{self, DbItem};
use pob_egui::data::items::{self, EquippedItem, ItemListEntry, TooltipLine};
use pob_egui::lua_bridge::LuaBridge;

use super::theme::{self, Theme};

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
        Self {
            equipped,
            item_list,
            error,
            paste_error: None,
            confirm_delete: None,
            edit_dialog: None,
            tooltip_cache: HashMap::new(),
            item_db: ItemDbBrowser::default(),
        }
    }

    /// Returns true if the build changed (items equipped/added/deleted).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if let Some(ref err) = self.error {
            ui.colored_label(Theme::ERROR, err);
            return false;
        }

        let mut changed = false;

        changed |= self.show_edit_dialog(ui, bridge);

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

    fn show_slots(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, label_span: f32) -> bool {
        // (slot_name, new_item_id) selected this frame
        let mut pending_equip: Option<(String, i64)> = None;

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

                    // Tooltip for the currently equipped item on hover
                    if slot.sel_item_id > 0 {
                        hover_tooltip(
                            combo_resp.response,
                            &mut self.tooltip_cache,
                            bridge,
                            slot.sel_item_id,
                            Some(&slot.slot_name),
                        );
                    }

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

                let resp = ui.label(
                    egui::RichText::new(&entry.name).color(items::rarity_color(&entry.rarity)),
                );
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

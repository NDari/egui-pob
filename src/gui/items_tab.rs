//! Items tab: equipment slots with equip dropdowns, the build's item list,
//! full upstream tooltips, paste-from-clipboard, and item deletion.

use std::collections::HashMap;

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
    /// Cached tooltip lines keyed by (item id, slot context).
    tooltip_cache: HashMap<(i64, Option<String>), Vec<TooltipLine>>,
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
            tooltip_cache: HashMap::new(),
        }
    }

    /// Returns true if the build changed (items equipped/added/deleted).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if let Some(ref err) = self.error {
            ui.colored_label(Theme::ERROR, err);
            return false;
        }

        let mut changed = false;

        // Top bar: paste from clipboard
        ui.horizontal(|ui| {
            if ui
                .button("Paste item")
                .on_hover_text("Add an item copied from the game or a trade site (Ctrl+V)")
                .clicked()
            {
                changed |= self.paste_item_from_clipboard(bridge);
            }
            if let Some(ref err) = self.paste_error {
                ui.colored_label(Theme::ERROR, err);
            }
        });

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
                ui.strong("All Items");
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
                } else if ui.small_button("🗑").on_hover_text("Delete item").clicked() {
                    self.confirm_delete = Some(entry.id);
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
    resp.on_hover_ui(|ui| {
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
    })
}

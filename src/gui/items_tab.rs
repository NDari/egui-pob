//! Items tab: displays equipped items by slot.

use pob_egui::data::items::{self, EquippedItem};
use pob_egui::lua_bridge::LuaBridge;

/// State for the items panel.
pub struct ItemsPanel {
    pub equipped: Vec<EquippedItem>,
    pub error: Option<String>,
}

impl ItemsPanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        match items::extract_equipped_items(lua) {
            Ok(equipped) => {
                log::info!("Loaded {} equipment slots", equipped.len());
                Self {
                    equipped,
                    error: None,
                }
            }
            Err(e) => {
                log::error!("Failed to load items: {e}");
                Self {
                    equipped: Vec::new(),
                    error: Some(format!("Failed to load items: {e}")),
                }
            }
        }
    }

    pub fn show(&self, ui: &mut egui::Ui, _bridge: &LuaBridge) {
        if let Some(ref err) = self.error {
            ui.colored_label(egui::Color32::RED, err);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for slot in &self.equipped {
                show_slot(ui, slot);
            }
        });
    }
}

fn show_slot(ui: &mut egui::Ui, slot: &EquippedItem) {
    let Some(ref item) = slot.item else {
        ui.horizontal(|ui| {
            ui.label(&slot.slot_name);
            ui.colored_label(egui::Color32::from_rgb(100, 100, 100), "(empty)");
        });
        return;
    };

    let header = egui::RichText::new(&item.name).color(item.rarity_color());
    egui::CollapsingHeader::new(header)
        .id_salt(&slot.slot_name)
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("{} — {}", slot.slot_name, item.base_name))
                    .small()
                    .color(egui::Color32::from_rgb(150, 150, 150)),
            );

            if item.quality > 0 {
                ui.label(format!("Quality: +{}%", item.quality));
            }
            if let Some(lvl) = item.level_req {
                ui.label(format!("Requires Level {lvl}"));
            }

            if !item.implicit_mods.is_empty() {
                ui.separator();
                for m in &item.implicit_mods {
                    ui.colored_label(egui::Color32::from_rgb(136, 136, 255), m);
                }
            }

            if !item.explicit_mods.is_empty() {
                ui.separator();
                for m in &item.explicit_mods {
                    ui.colored_label(egui::Color32::from_rgb(136, 136, 255), m);
                }
            }
        });
}

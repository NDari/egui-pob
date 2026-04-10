//! Config tab: displays build configuration options as egui widgets.

use mlua::prelude::*;
use pob_egui::data::config::{self, ConfigOption, LuaValueKind};
use pob_egui::lua_bridge::LuaBridge;

/// State for the config panel.
pub struct ConfigPanel {
    pub options: Vec<ConfigOption>,
    pub error: Option<String>,
}

impl ConfigPanel {
    pub fn new(lua: &Lua) -> Self {
        match config::extract_config_options(lua) {
            Ok(options) => {
                log::info!("Loaded {} config options", options.len());
                Self {
                    options,
                    error: None,
                }
            }
            Err(e) => Self {
                options: Vec::new(),
                error: Some(format!("Failed to load config options: {e}")),
            },
        }
    }

    /// Draw the config panel. Returns true if any value changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(super::theme::Theme::ERROR, err);
            return false;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for option in &mut self.options {
                match option {
                    ConfigOption::Section { label } => {
                        ui.separator();
                        ui.strong(label.as_str());
                    }
                    ConfigOption::Check { var, label, value } => {
                        if ui.checkbox(value, label.as_str()).changed() {
                            if let Err(e) = config::set_config_value(
                                bridge.lua(),
                                var,
                                LuaValue::Boolean(*value),
                            ) {
                                log::error!("Failed to set config {var}: {e}");
                            } else {
                                changed = true;
                            }
                        }
                    }
                    ConfigOption::Count { var, label, value } => {
                        ui.horizontal(|ui| {
                            ui.label(label.as_str());
                            let response =
                                ui.add(egui::TextEdit::singleline(value).desired_width(80.0));
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let lua_val = if let Ok(n) = value.parse::<f64>() {
                                    LuaValue::Number(n)
                                } else {
                                    LuaValue::Number(0.0)
                                };
                                if let Err(e) = config::set_config_value(bridge.lua(), var, lua_val)
                                {
                                    log::error!("Failed to set config {var}: {e}");
                                } else {
                                    changed = true;
                                }
                            }
                        });
                    }
                    ConfigOption::List {
                        var,
                        label,
                        options: list_options,
                        selected_index,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label(label.as_str());
                            let current_label = list_options
                                .get(*selected_index)
                                .map(|e| e.label.as_str())
                                .unwrap_or("—");
                            egui::ComboBox::from_id_salt(var.as_str())
                                .selected_text(current_label)
                                .show_ui(ui, |ui| {
                                    for (i, entry) in list_options.iter().enumerate() {
                                        if ui
                                            .selectable_label(i == *selected_index, &entry.label)
                                            .clicked()
                                        {
                                            *selected_index = i;
                                            let lua_val =
                                                kind_to_lua_value(bridge.lua(), &entry.val);
                                            if let Err(e) =
                                                config::set_config_value(bridge.lua(), var, lua_val)
                                            {
                                                log::error!("Failed to set config {var}: {e}");
                                            } else {
                                                changed = true;
                                            }
                                        }
                                    }
                                });
                        });
                    }
                    ConfigOption::Text { var, label, value } => {
                        ui.horizontal(|ui| {
                            ui.label(label.as_str());
                            let response =
                                ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let lua_val = bridge
                                    .lua()
                                    .create_string(value.as_str())
                                    .map(LuaValue::String)
                                    .unwrap_or(LuaValue::Nil);
                                if let Err(e) = config::set_config_value(bridge.lua(), var, lua_val)
                                {
                                    log::error!("Failed to set config {var}: {e}");
                                } else {
                                    changed = true;
                                }
                            }
                        });
                    }
                }
            }
        });

        changed
    }
}

fn kind_to_lua_value(lua: &Lua, kind: &LuaValueKind) -> LuaValue {
    match kind {
        LuaValueKind::String(s) => lua
            .create_string(s.as_str())
            .map(LuaValue::String)
            .unwrap_or(LuaValue::Nil),
        LuaValueKind::Number(n) => LuaValue::Number(*n),
        LuaValueKind::Integer(n) => LuaValue::Integer(*n),
        LuaValueKind::Bool(b) => LuaValue::Boolean(*b),
        LuaValueKind::Nil => LuaValue::Nil,
    }
}

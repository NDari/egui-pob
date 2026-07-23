//! Config tab: displays build configuration options grouped by section.

use mlua::prelude::*;
use pob_egui::data::config::{self, ConfigOption, LuaValueKind};
use pob_egui::lua_bridge::LuaBridge;

pub struct ConfigPanel {
    pub options: Vec<ConfigOption>,
    pub error: Option<String>,
    pub search: String,
    pub show_ineligible: bool,
}

impl ConfigPanel {
    pub fn new(lua: &Lua) -> Self {
        match config::extract_config_options(lua) {
            Ok(options) => {
                log::info!("Loaded {} config options", options.len());
                Self {
                    options,
                    error: None,
                    search: String::new(),
                    show_ineligible: false,
                }
            }
            Err(e) => Self {
                options: Vec::new(),
                error: Some(format!("Failed to load config options: {e}")),
                search: String::new(),
                show_ineligible: false,
            },
        }
    }

    /// Refresh option visibility from the current build state (tooltip + ifX predicates).
    /// Called after a value changes so conditional visibility updates live.
    fn refresh_visibility(&mut self, lua: &Lua) {
        if let Ok(refreshed) = config::extract_config_options(lua) {
            self.options = refreshed;
        }
    }

    /// Draw the config panel. Returns true if any value changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(super::theme::Theme::ERROR, err);
            return false;
        }

        // Toolbar: search + ineligible toggle
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(200.0));
            if !self.search.is_empty() && ui.button("x").clicked() {
                self.search.clear();
            }
            ui.separator();
            ui.checkbox(&mut self.show_ineligible, "Show ineligible options");
        });
        ui.separator();

        let search_lower = self.search.to_lowercase();
        let show_ineligible = self.show_ineligible;

        // Group options by section.
        let sections = group_by_section(&self.options);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (section_label, indices) in sections {
                // Filter visible items inside this section based on search + visibility.
                let visible_indices: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|&i| {
                        let opt = &self.options[i];
                        if matches!(opt, ConfigOption::Section { .. }) {
                            return false;
                        }
                        if !show_ineligible && !opt.is_visible() {
                            return false;
                        }
                        if !search_lower.is_empty() {
                            return opt.label().to_lowercase().contains(&search_lower)
                                || opt
                                    .var()
                                    .map(|v| v.to_lowercase().contains(&search_lower))
                                    .unwrap_or(false);
                        }
                        true
                    })
                    .collect();

                if visible_indices.is_empty() {
                    continue;
                }

                let id = ui.make_persistent_id(("config_section", section_label.as_str()));
                // Force-open sections when the user is actively searching.
                let default_open = !search_lower.is_empty() || section_label.is_empty();
                let header_text = if section_label.is_empty() {
                    "General".to_string()
                } else {
                    section_label.clone()
                };
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    default_open,
                )
                .show_header(ui, |ui| {
                    ui.strong(header_text);
                })
                .body(|ui| {
                    for i in visible_indices {
                        if self.show_option(ui, bridge, i) {
                            changed = true;
                        }
                    }
                });
            }
        });

        if changed {
            self.refresh_visibility(bridge.lua());
        }

        changed
    }

    fn show_option(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, index: usize) -> bool {
        let mut changed = false;
        let option = &mut self.options[index];

        // Grey out when ineligible and the user opted to see them.
        let ineligible = !option.is_visible();

        let draw = |ui: &mut egui::Ui, option: &mut ConfigOption, changed: &mut bool| match option {
            ConfigOption::Section { .. } => {}
            ConfigOption::Check {
                var,
                label,
                value,
                tooltip,
                ..
            } => {
                let resp = ui.checkbox(value, label.as_str());
                if let Some(t) = tooltip {
                    resp.clone().on_hover_text(t.as_str());
                }
                if resp.changed() {
                    if let Err(e) =
                        config::set_config_value(bridge.lua(), var, LuaValue::Boolean(*value))
                    {
                        log::error!("Failed to set config {var}: {e}");
                    } else {
                        *changed = true;
                    }
                }
            }
            ConfigOption::Count {
                var,
                label,
                value,
                tooltip,
                ..
            } => {
                ui.horizontal(|ui| {
                    let lbl = ui.label(label.as_str());
                    if let Some(t) = tooltip {
                        lbl.on_hover_text(t.as_str());
                    }
                    let response = ui.add(egui::TextEdit::singleline(value).desired_width(80.0));
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let lua_val = if let Ok(n) = value.parse::<f64>() {
                            LuaValue::Number(n)
                        } else {
                            LuaValue::Number(0.0)
                        };
                        if let Err(e) = config::set_config_value(bridge.lua(), var, lua_val) {
                            log::error!("Failed to set config {var}: {e}");
                        } else {
                            *changed = true;
                        }
                    }
                });
            }
            ConfigOption::List {
                var,
                label,
                options,
                selected_index,
                tooltip,
                ..
            } => {
                ui.horizontal(|ui| {
                    let lbl = ui.label(label.as_str());
                    if let Some(t) = tooltip {
                        lbl.on_hover_text(t.as_str());
                    }
                    let current_label = options
                        .get(*selected_index)
                        .map(|e| e.label.as_str())
                        .unwrap_or("—");
                    egui::ComboBox::from_id_salt(var.as_str())
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for (i, entry) in options.iter().enumerate() {
                                if ui
                                    .selectable_label(i == *selected_index, &entry.label)
                                    .clicked()
                                {
                                    *selected_index = i;
                                    let lua_val = kind_to_lua_value(bridge.lua(), &entry.val);
                                    if let Err(e) =
                                        config::set_config_value(bridge.lua(), var, lua_val)
                                    {
                                        log::error!("Failed to set config {var}: {e}");
                                    } else {
                                        *changed = true;
                                    }
                                }
                            }
                        });
                });
            }
            ConfigOption::Text {
                var,
                label,
                value,
                tooltip,
                ..
            } => {
                ui.horizontal(|ui| {
                    let lbl = ui.label(label.as_str());
                    if let Some(t) = tooltip {
                        lbl.on_hover_text(t.as_str());
                    }
                    let response = ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let lua_val = bridge
                            .lua()
                            .create_string(value.as_str())
                            .map(LuaValue::String)
                            .unwrap_or(LuaValue::Nil);
                        if let Err(e) = config::set_config_value(bridge.lua(), var, lua_val) {
                            log::error!("Failed to set config {var}: {e}");
                        } else {
                            *changed = true;
                        }
                    }
                });
            }
        };

        if ineligible {
            ui.add_enabled_ui(false, |ui| draw(ui, option, &mut changed));
        } else {
            draw(ui, option, &mut changed);
        }

        changed
    }
}

/// Group option indices by their preceding section header.
/// Returns `(section_label, indices_into_options)`.
fn group_by_section(options: &[ConfigOption]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    let mut current_section = String::new();
    let mut current_indices: Vec<usize> = Vec::new();

    for (i, option) in options.iter().enumerate() {
        if let ConfigOption::Section { label } = option {
            if !current_indices.is_empty() || !current_section.is_empty() {
                groups.push((
                    current_section.clone(),
                    std::mem::take(&mut current_indices),
                ));
            }
            current_section = label.clone();
        } else {
            current_indices.push(i);
        }
    }
    if !current_indices.is_empty() || !current_section.is_empty() {
        groups.push((current_section, current_indices));
    }
    groups
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

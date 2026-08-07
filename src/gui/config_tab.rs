//! Config tab: displays build configuration options grouped by section.

use mlua::prelude::*;
use pob_egui::data::config::{self, ConfigOption, LuaValueKind};
use pob_egui::data::config_sets::{self, ConfigSetInfo};
use pob_egui::lua_bridge::LuaBridge;

/// Pending name prompt in the config set manager.
enum SetAction {
    New,
    Copy(i64),
    Rename(i64),
}

struct SetPrompt {
    action: SetAction,
    text: String,
}

pub struct ConfigPanel {
    pub options: Vec<ConfigOption>,
    pub error: Option<String>,
    pub search: String,
    pub show_ineligible: bool,
    /// True while the reset-to-defaults confirmation popup is open.
    confirm_reset: bool,
    /// Config sets in order + the active set id.
    sets: Vec<ConfigSetInfo>,
    active_set: i64,
    manage_sets_open: bool,
    set_prompt: Option<SetPrompt>,
    confirm_delete_set: Option<i64>,
    /// Cached width of the right-aligned label column, measured from the
    /// widest label. Cleared whenever the option list is rebuilt.
    label_col_width: Option<f32>,
}

impl ConfigPanel {
    pub fn new(lua: &Lua) -> Self {
        let (sets, active_set) = config_sets::list_config_sets(lua).unwrap_or_else(|e| {
            log::error!("Failed to list config sets: {e}");
            (Vec::new(), 1)
        });
        match config::extract_config_options(lua) {
            Ok(options) => {
                log::info!("Loaded {} config options", options.len());
                Self {
                    options,
                    error: None,
                    search: String::new(),
                    show_ineligible: false,
                    confirm_reset: false,
                    sets,
                    active_set,
                    manage_sets_open: false,
                    set_prompt: None,
                    confirm_delete_set: None,
                    label_col_width: None,
                }
            }
            Err(e) => Self {
                options: Vec::new(),
                error: Some(format!("Failed to load config options: {e}")),
                search: String::new(),
                show_ineligible: false,
                confirm_reset: false,
                sets,
                active_set,
                manage_sets_open: false,
                set_prompt: None,
                confirm_delete_set: None,
                label_col_width: None,
            },
        }
    }

    /// Width of the label column, measured once so every control lands on the
    /// same x.
    ///
    /// Sized to the widest label that fits on one line (at most
    /// [`WRAP_CHAR_THRESHOLD`] characters). Longer labels wrap to a second
    /// line inside this same width rather than widening the column, which
    /// keeps the gutter tight instead of letting one 50-character outlier set
    /// it for all ~700 options.
    ///
    /// Upstream anchors every config control at a fixed x (234px from the
    /// section edge in `ConfigTab`) and right-aligns the label against it,
    /// shrinking the font from 14pt to 12pt for labels wider than the column.
    /// We wrap instead, keeping one font size and adapting to theme/DPI.
    fn label_column_width(&mut self, ui: &egui::Ui) -> f32 {
        if let Some(w) = self.label_col_width {
            return w;
        }
        let font = egui::TextStyle::Body.resolve(ui.style());
        let widest = self
            .options
            .iter()
            .filter(|o| !matches!(o, ConfigOption::Section { .. }))
            .map(ConfigOption::label)
            .filter(|l| l.chars().count() <= super::theme::WRAP_CHAR_THRESHOLD)
            .map(|l| {
                ui.fonts(|f| f.layout_no_wrap(l.to_string(), font.clone(), egui::Color32::WHITE))
                    .rect
                    .width()
            })
            .fold(0.0_f32, f32::max);
        // Leave a gap before the control, and keep a pathological label from
        // pushing every control off the right edge.
        let w = (widest + super::theme::LABEL_GAP).clamp(80.0, 520.0);
        self.label_col_width = Some(w);
        w
    }

    /// Refresh option visibility from the current build state (tooltip + ifX predicates).
    /// Called after a value changes so conditional visibility updates live.
    fn refresh_visibility(&mut self, lua: &Lua) {
        if let Ok(refreshed) = config::extract_config_options(lua) {
            self.options = refreshed;
            self.label_col_width = None;
        }
    }

    /// Draw the config panel. Returns true if any value changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(super::theme::Theme::ERROR, err);
            return false;
        }

        // Undo/redo (Ctrl+Z / Ctrl+Y), only when no widget (e.g. the search
        // field) has keyboard focus
        if ui.ctx().memory(|m| m.focused().is_none()) {
            let undo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            let redo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y));
            if undo_pressed || redo_pressed {
                let result = if undo_pressed {
                    config::undo(bridge.lua())
                } else {
                    config::redo(bridge.lua())
                };
                match result {
                    Ok(()) => {
                        self.refresh_visibility(bridge.lua());
                        changed = true;
                    }
                    Err(e) => log::error!("Config undo/redo failed: {e}"),
                }
            }
        }

        // Config set selector row
        if !self.sets.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Config set:");
                let active_label = self
                    .sets
                    .iter()
                    .find(|s| s.id == self.active_set)
                    .map(config_set_label)
                    .unwrap_or_else(|| "Default".to_string());
                egui::ComboBox::from_id_salt("config_set_select")
                    .selected_text(active_label)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for set in &self.sets {
                            if ui
                                .selectable_label(set.id == self.active_set, config_set_label(set))
                                .clicked()
                                && set.id != self.active_set
                            {
                                match config_sets::set_active_config_set(bridge.lua(), set.id) {
                                    Ok(()) => changed = true,
                                    Err(e) => log::error!("Failed to switch config set: {e}"),
                                }
                            }
                        }
                    });
                if ui.button("Manage...").clicked() {
                    self.manage_sets_open = true;
                }
            });
        }

        changed |= self.show_set_manager(ui, bridge);

        // Toolbar: search + ineligible toggle
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(200.0));
            // Ctrl+F focuses the search box (upstream ConfigTab key handling)
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                response.request_focus();
            }
            if !self.search.is_empty() && ui.button("x").clicked() {
                self.search.clear();
            }
            ui.separator();
            ui.checkbox(&mut self.show_ineligible, "Show ineligible options");
            ui.separator();
            if ui.button("Reset to defaults").clicked() {
                self.confirm_reset = true;
            }
        });
        ui.separator();

        // Reset-to-defaults confirmation popup
        if self.confirm_reset {
            egui::Window::new("Reset Configuration")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("Reset all configuration options to their default values?");
                    ui.horizontal(|ui| {
                        if ui.button("Reset").clicked() {
                            match config::reset_config_to_defaults(bridge.lua()) {
                                Ok(()) => changed = true,
                                Err(e) => log::error!("Failed to reset config: {e}"),
                            }
                            self.confirm_reset = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_reset = false;
                        }
                    });
                });
        }

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
            match config_sets::list_config_sets(bridge.lua()) {
                Ok((sets, active)) => {
                    self.sets = sets;
                    self.active_set = active;
                }
                Err(e) => log::error!("Failed to refresh config sets: {e}"),
            }
        }

        changed
    }

    /// Manage Config Sets dialog. Returns true if the sets changed.
    fn show_set_manager(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if !self.manage_sets_open {
            return false;
        }
        let mut changed = false;
        let mut close = false;
        let mut activate: Option<i64> = None;
        let mut delete: Option<i64> = None;

        egui::Window::new("Manage Config Sets")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                for set in &self.sets {
                    ui.horizontal(|ui| {
                        let is_active = set.id == self.active_set;
                        let label = if is_active {
                            egui::RichText::new(config_set_label(set))
                                .color(super::theme::Theme::MAIN_SKILL)
                        } else {
                            egui::RichText::new(config_set_label(set))
                        };
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.sets.len() > 1 && ui.small_button("Delete").clicked() {
                                self.confirm_delete_set = Some(set.id);
                            }
                            if ui.small_button("Rename").clicked() {
                                self.set_prompt = Some(SetPrompt {
                                    action: SetAction::Rename(set.id),
                                    text: set.title.clone(),
                                });
                            }
                            if ui.small_button("Copy").clicked() {
                                self.set_prompt = Some(SetPrompt {
                                    action: SetAction::Copy(set.id),
                                    text: format!("{} (copy)", config_set_label(set)),
                                });
                            }
                            if !is_active && ui.small_button("Activate").clicked() {
                                activate = Some(set.id);
                            }
                        });
                    });
                }
                ui.separator();

                if let Some(prompt) = &mut self.set_prompt {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut prompt.text).desired_width(200.0));
                    });
                }

                ui.horizontal(|ui| {
                    if let Some(prompt) = &self.set_prompt {
                        let name = prompt.text.trim().to_string();
                        if ui
                            .add_enabled(!name.is_empty(), egui::Button::new("OK"))
                            .clicked()
                        {
                            let result = match prompt.action {
                                SetAction::New => config_sets::new_config_set(bridge.lua(), &name),
                                SetAction::Copy(id) => {
                                    config_sets::copy_config_set(bridge.lua(), id, &name)
                                }
                                SetAction::Rename(id) => {
                                    config_sets::rename_config_set(bridge.lua(), id, &name)
                                }
                            };
                            match result {
                                Ok(()) => changed = true,
                                Err(e) => log::error!("Config set action failed: {e}"),
                            }
                            self.set_prompt = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.set_prompt = None;
                        }
                    } else {
                        if ui.button("New Set").clicked() {
                            self.set_prompt = Some(SetPrompt {
                                action: SetAction::New,
                                text: String::new(),
                            });
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    }
                });

                if let Some(id) = self.confirm_delete_set {
                    let title = self
                        .sets
                        .iter()
                        .find(|s| s.id == id)
                        .map(config_set_label)
                        .unwrap_or_default();
                    ui.separator();
                    ui.colored_label(
                        super::theme::Theme::ERROR,
                        format!("Delete '{title}'? Its configuration values are lost."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            delete = Some(id);
                            self.confirm_delete_set = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_set = None;
                        }
                    });
                }
            });

        if let Some(id) = activate {
            match config_sets::set_active_config_set(bridge.lua(), id) {
                Ok(()) => changed = true,
                Err(e) => log::error!("Failed to switch config set: {e}"),
            }
        }
        if let Some(id) = delete {
            match config_sets::delete_config_set(bridge.lua(), id) {
                Ok(()) => changed = true,
                Err(e) => log::error!("Failed to delete config set: {e}"),
            }
        }
        if close {
            self.manage_sets_open = false;
        }
        changed
    }

    fn show_option(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, index: usize) -> bool {
        let mut changed = false;
        let label_width = self.label_column_width(ui);
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
                let resp = labeled_row(ui, label_width, label.as_str(), tooltip.as_deref(), |ui| {
                    ui.checkbox(value, "")
                });
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
                labeled_row(ui, label_width, label.as_str(), tooltip.as_deref(), |ui| {
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
                labeled_row(ui, label_width, label.as_str(), tooltip.as_deref(), |ui| {
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
                labeled_row(ui, label_width, label.as_str(), tooltip.as_deref(), |ui| {
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

/// One config row: plain-text label right-aligned in the shared column, then
/// the control. See [`super::theme::right_aligned_row`].
fn labeled_row<R>(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    tooltip: Option<&str>,
    add_control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let job = egui::text::LayoutJob::simple(
        label.to_string(),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
        f32::INFINITY,
    );
    super::theme::right_aligned_row(ui, label_width, job, tooltip, add_control)
}

fn config_set_label(set: &ConfigSetInfo) -> String {
    if set.title.is_empty() {
        "Default".to_string()
    } else {
        set.title.clone()
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

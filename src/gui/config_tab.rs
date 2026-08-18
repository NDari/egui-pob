//! Config tab: displays build configuration options grouped by section.

use mlua::prelude::*;
use pob_egui::data::config::{self, ConfigOption, LuaValueKind};
use pob_egui::data::config_sets::{self, ConfigSetInfo};
use pob_egui::data::custom_mods::{self, CustomModGroup, LineStatus};
use pob_egui::lua_bridge::LuaBridge;

/// The section whose box holds the custom modifier groups. Upstream declares
/// it in `ConfigOptions.lua` with no options of its own - it is an anchor for
/// the group controls `ConfigTab` builds programmatically.
const CUSTOM_MODS_SECTION: &str = "Custom Modifiers";

/// A group edit that changes the list itself, applied once the draw loop is
/// done rather than while it is iterating.
enum GroupEdit {
    Add,
    Delete(usize),
    Title(usize, String),
    Enabled(usize, bool),
    Text(usize, String),
}

/// Order the section boxes are laid out in.
///
/// Ours, not upstream's: `ConfigOptions.lua` declares them General, Skill
/// Options, Map Modifiers, When In Combat, For Effective DPS, Enemy Stats,
/// Custom Modifiers, and upstream then re-flows them by a per-section `col`
/// hint. We lay them out in the order they are usually worked through instead.
/// A section not named here keeps its ConfigOptions position and follows these.
const SECTION_ORDER: &[&str] = &[
    "General",
    "When In Combat",
    "For Effective DPS",
    "Custom Modifiers",
    "Skill Options",
    "Map Modifiers and Player Debuffs",
    "Enemy Stats",
];

/// Narrowest a section box may get before the layout drops to fewer columns.
/// Upstream lays its config sections out in fixed 360px columns
/// (`ConfigTab:UpdateControls`, `maxCol = floor((viewPort.width - 10) / 370)`);
/// we keep its column count but stretch the boxes to consume the remainder.
const SECTION_MIN_WIDTH: f32 = 360.0;

/// Gap between section boxes, horizontally and vertically.
const COLUMN_GAP: f32 = 8.0;

/// Padding inside a section box.
const SECTION_MARGIN: f32 = 8.0;

/// Upper bound handed to a box's layout rect. Boxes size themselves to their
/// content, so this only has to be past the tallest section.
const SECTION_MAX_HEIGHT: f32 = 20_000.0;

/// Share of a box's inner width the label column may claim, leaving the rest
/// for the control. Upstream's 360px section puts its controls at x=234, i.e.
/// 65% for the label.
const LABEL_COLUMN_FRACTION: f32 = 0.65;

/// Visible rows in a custom modifier group's text editor. Upstream sizes its
/// box at 80px and lets the user drag it taller; ours grows with the text.
const CUSTOM_MOD_EDITOR_ROWS: usize = 4;

/// Warning upstream appends to the tooltip of an option that survives its own
/// `ifX` predicates only because its value is off-default (ConfigTab.lua).
const INVALID_OPTION_NOTE: &str =
    "This config option is conditional with missing source and is invalid.";

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
    pub show_all: bool,
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
    /// Height each section box took the last time it was drawn, keyed by
    /// section label. The box layout needs sizes before it can place anything,
    /// so it packs with these and re-packs when a measurement disagrees.
    section_heights: std::collections::HashMap<String, f32>,
    /// Editable copies of the active config set's custom modifier groups. Text
    /// and title edits are committed to Lua when the field loses focus, so
    /// these hold what the user has typed until then.
    custom_groups: Vec<CustomModGroup>,
    /// Per-group, per-line parse status for the group text, refreshed whenever
    /// the text changes. Drives the colouring in the editor.
    custom_line_status: Vec<Vec<LineStatus>>,
    /// What Lua currently holds, so a field losing focus without having been
    /// edited does not spend a recalculation and an undo state on the value it
    /// already has.
    custom_committed: Vec<CustomModGroup>,
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
                    show_all: false,
                    confirm_reset: false,
                    sets,
                    active_set,
                    manage_sets_open: false,
                    set_prompt: None,
                    confirm_delete_set: None,
                    label_col_width: None,
                    section_heights: Default::default(),
                    custom_groups: read_custom_groups(lua),
                    custom_line_status: Vec::new(),
                    custom_committed: read_custom_groups(lua),
                }
            }
            Err(e) => Self {
                options: Vec::new(),
                error: Some(format!("Failed to load config options: {e}")),
                search: String::new(),
                show_all: false,
                confirm_reset: false,
                sets,
                active_set,
                manage_sets_open: false,
                set_prompt: None,
                confirm_delete_set: None,
                label_col_width: None,
                section_heights: Default::default(),
                custom_groups: Vec::new(),
                custom_line_status: Vec::new(),
                custom_committed: Vec::new(),
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
        self.custom_groups = read_custom_groups(lua);
        self.custom_committed = self.custom_groups.clone();
        self.custom_line_status.clear();
    }

    /// Parse status for every group's text. Recomputed only when the cache is
    /// stale - each call runs the whole text through upstream's parser.
    fn sync_line_status(&mut self, lua: &Lua) {
        if self.custom_line_status.len() != self.custom_groups.len() {
            self.custom_line_status = self
                .custom_groups
                .iter()
                .map(|g| custom_mods::line_status(lua, &g.text).unwrap_or_default())
                .collect();
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
            ui.checkbox(&mut self.show_all, "Show all configurations");
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
        let show_all = self.show_all;

        // Sections that still have a row to show.
        let mut sections: Vec<(String, Vec<usize>)> = group_by_section(&self.options)
            .into_iter()
            .map(|(label, indices)| {
                let visible = indices
                    .into_iter()
                    .filter(|&i| {
                        let opt = &self.options[i];
                        if matches!(opt, ConfigOption::Section { .. }) {
                            return false;
                        }
                        if !opt.vis().shown(show_all) {
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
                    .collect::<Vec<_>>();
                (label, visible)
            })
            .filter(|(label, visible)| !visible.is_empty() || label == CUSTOM_MODS_SECTION)
            .collect();
        // Stable, so anything SECTION_ORDER does not name keeps its file order.
        sections.sort_by_key(|(label, _)| {
            SECTION_ORDER
                .iter()
                .position(|s| s == label)
                .unwrap_or(SECTION_ORDER.len())
        });

        // Height the boxes get to pack into. Taken out here because inside the
        // scroll area the content is free to grow past the viewport, which is
        // exactly what we are trying to avoid.
        let viewport_h = ui.available_height();

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Column geometry: as many SECTION_MIN_WIDTH columns as fit across
            // the panel, then share the leftover width equally so the boxes
            // always span it instead of leaving a ragged gutter.
            let avail_w = ui.available_width();
            let n_cols =
                (((avail_w + COLUMN_GAP) / (SECTION_MIN_WIDTH + COLUMN_GAP)) as usize).max(1);
            let col_w = (avail_w - COLUMN_GAP * (n_cols - 1) as f32) / n_cols as f32;
            let label_width = self
                .label_column_width(ui)
                .min((col_w - 2.0 * SECTION_MARGIN) * LABEL_COLUMN_FRACTION);

            let heights: Vec<f32> = sections
                .iter()
                .map(|(label, indices)| {
                    // Last frame's measurement if we have one, otherwise predict
                    // it from the row metrics. Either way the draw pass below
                    // stores what the box actually took, so a wrong guess
                    // self-corrects on the next frame.
                    self.section_heights.get(label).copied().unwrap_or_else(|| {
                        let rows =
                            predicted_section_height(ui, label_width, &self.options, indices);
                        if label == CUSTOM_MODS_SECTION {
                            rows + predicted_custom_mods_height(ui, self.custom_groups.len())
                        } else {
                            rows
                        }
                    })
                })
                .collect();

            // Fill a column top to bottom while the next box still fits the
            // viewport, then move right - so a wider panel pushes boxes sideways
            // and a taller one pulls them back underneath each other. Once
            // nothing fits anywhere the shortest column wins, keeping an
            // overfull panel balanced rather than piling the remainder into the
            // last column.
            let mut col_bottom = vec![0.0_f32; n_cols];
            let mut placement: Vec<(usize, f32)> = Vec::with_capacity(sections.len());
            for &h in &heights {
                let col = (0..n_cols)
                    .find(|&c| col_bottom[c] + h <= viewport_h)
                    .unwrap_or_else(|| {
                        (0..n_cols).fold(0, |best, c| {
                            if col_bottom[c] < col_bottom[best] {
                                c
                            } else {
                                best
                            }
                        })
                    });
                placement.push((col, col_bottom[col]));
                col_bottom[col] += h + COLUMN_GAP;
            }
            let content_h = col_bottom.iter().fold(0.0_f32, |a, &b| a.max(b));

            let origin = ui.cursor().min;
            ui.allocate_space(egui::vec2(avail_w, content_h));

            let mut measured = std::collections::HashMap::with_capacity(sections.len());
            for ((label, indices), (col, y)) in sections.iter().zip(placement) {
                let slot = egui::Rect::from_min_size(
                    origin + egui::vec2(col as f32 * (col_w + COLUMN_GAP), y),
                    // Height is left generous: the frame sizes itself to its
                    // content, and this is what we measure the box by.
                    egui::vec2(col_w, SECTION_MAX_HEIGHT),
                );
                let drawn = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
                        section_frame(ui)
                            .show(ui, |ui| {
                                ui.set_width(col_w - 2.0 * SECTION_MARGIN);
                                ui.strong(if label.is_empty() { "General" } else { label });
                                for &i in indices {
                                    if self.show_option(ui, bridge, i, label_width) {
                                        changed = true;
                                    }
                                }
                                if label == CUSTOM_MODS_SECTION {
                                    changed |= self.show_custom_mod_groups(ui, bridge);
                                }
                            })
                            .response
                            .rect
                            .height()
                    })
                    .inner;
                measured.insert(label.clone(), drawn);
            }

            // Placement above ran on last frame's numbers; if the box grew or
            // shrank, redo it now that we know the real size.
            if measured.len() != self.section_heights.len()
                || measured.iter().any(|(k, v)| {
                    self.section_heights
                        .get(k)
                        .is_none_or(|c| (c - v).abs() > 0.5)
                })
            {
                self.section_heights = measured;
                ui.ctx().request_repaint();
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

    /// Draw the custom modifier groups: an "Add Mod Group" button, then one
    /// block per group with a delete button, title, enable checkbox and a
    /// modifier text editor.
    ///
    /// Title and text commit when the field loses focus rather than on every
    /// keystroke (see DIVERGENCES.md) - each commit is a full recalculation.
    /// Line colouring updates as you type, so the parser feedback is still
    /// live. Returns true when something was committed.
    fn show_custom_mod_groups(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        self.sync_line_status(bridge.lua());

        // One frame can produce several edits at once: clicking a second
        // group's checkbox blurs the first group's editor, committing it. Both
        // have to be applied, so they are collected rather than overwritten.
        let mut edits: Vec<GroupEdit> = Vec::new();
        if ui.button("Add Mod Group").clicked() {
            edits.push(GroupEdit::Add);
        }

        // Split the borrow so the editor can hold the text mutably while the
        // layouter reads that group's line statuses.
        let Self {
            custom_groups,
            custom_line_status,
            custom_committed,
            ..
        } = self;

        for (i, group) in custom_groups.iter_mut().enumerate() {
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("X").color(super::theme::Theme::ERROR))
                    .on_hover_text("Delete this group")
                    .clicked()
                {
                    edits.push(GroupEdit::Delete(i));
                }
                let mut enabled = group.enabled;
                if ui
                    .checkbox(&mut enabled, "")
                    .on_hover_text("Disabled groups keep their text but apply no modifiers")
                    .changed()
                {
                    edits.push(GroupEdit::Enabled(i, enabled));
                }
                let title = ui.add(
                    egui::TextEdit::singleline(&mut group.title)
                        .desired_width(f32::INFINITY)
                        .hint_text("Group name"),
                );
                let committed_title = custom_committed.get(i).map(|g| g.title.as_str());
                if title.lost_focus() && committed_title != Some(group.title.as_str()) {
                    edits.push(GroupEdit::Title(i, group.title.clone()));
                }
            });

            let status = custom_line_status.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                mod_text_galley(ui, text, status, wrap_width)
            };
            let editor = ui.add(
                egui::TextEdit::multiline(&mut group.text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(CUSTOM_MOD_EDITOR_ROWS)
                    .hint_text("One modifier per line")
                    .layouter(&mut layouter),
            );
            if editor.changed() {
                // Re-colour as they type; the calc only runs on commit.
                custom_line_status[i] =
                    custom_mods::line_status(bridge.lua(), &group.text).unwrap_or_default();
            }
            let committed_text = custom_committed.get(i).map(|g| g.text.as_str());
            if editor.lost_focus() && committed_text != Some(group.text.as_str()) {
                edits.push(GroupEdit::Text(i, group.text.clone()));
            }
        }

        // Value edits first, so a pending commit is not lost to a delete that
        // shifts the indices out from under it; adds and deletes go last.
        let (structural, values): (Vec<_>, Vec<_>) = edits
            .into_iter()
            .partition(|e| matches!(e, GroupEdit::Add | GroupEdit::Delete(_)));

        let lua = bridge.lua();
        let mut committed = false;
        for edit in values.into_iter().chain(structural) {
            let result = match edit {
                GroupEdit::Add => custom_mods::add_group(lua),
                GroupEdit::Delete(i) => custom_mods::delete_group(lua, i),
                GroupEdit::Title(i, title) => custom_mods::set_title(lua, i, &title),
                GroupEdit::Enabled(i, on) => custom_mods::set_enabled(lua, i, on),
                GroupEdit::Text(i, text) => custom_mods::set_text(lua, i, &text),
            };
            match result {
                Ok(()) => committed = true,
                Err(e) => log::error!("Custom modifier group edit failed: {e}"),
            }
        }
        committed
    }

    fn show_option(
        &mut self,
        ui: &mut egui::Ui,
        bridge: &LuaBridge,
        index: usize,
        label_width: f32,
    ) -> bool {
        let mut changed = false;
        let show_all = self.show_all;
        let option = &mut self.options[index];
        let vis = option.vis();

        // Upstream keeps ineligible controls interactive - setting an option
        // before its source exists is the point of the toggle - and marks them
        // instead: red label plus a warning line for an option that is only
        // listed because its value is off-default, dim for the rest.
        let (label_color, note) = if vis.invalid(show_all) {
            (Some(super::theme::Theme::ERROR), Some(INVALID_OPTION_NOTE))
        } else if !vis.relevant {
            (Some(super::theme::Theme::TEXT_DIM), None)
        } else {
            (None, None)
        };

        let draw = |ui: &mut egui::Ui, option: &mut ConfigOption, changed: &mut bool| match option {
            ConfigOption::Section { .. } => {}
            ConfigOption::Check {
                var,
                label,
                value,
                tooltip,
                ..
            } => {
                let resp = labeled_row(
                    ui,
                    label_width,
                    label.as_str(),
                    tooltip.as_deref(),
                    label_color,
                    note,
                    |ui| ui.checkbox(value, ""),
                );
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
                labeled_row(
                    ui,
                    label_width,
                    label.as_str(),
                    tooltip.as_deref(),
                    label_color,
                    note,
                    |ui| {
                        // "-"/"+" buttons stay beside the field at all times
                        // (not just while editing) so a run of clicks works
                        // without the row's layout jumping between clicks.
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let response =
                                ui.add(egui::TextEdit::singleline(value).desired_width(50.0));
                            let mut commit = response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let current = value.parse::<f64>().unwrap_or(0.0);
                            let (minus, plus) = super::theme::step_buttons(ui);
                            if minus {
                                *value = format_config_count(current - 1.0);
                                commit = true;
                            }
                            if plus {
                                *value = format_config_count(current + 1.0);
                                commit = true;
                            }
                            if commit {
                                let lua_val = if let Ok(n) = value.parse::<f64>() {
                                    LuaValue::Number(n)
                                } else {
                                    LuaValue::Number(0.0)
                                };
                                if let Err(e) =
                                    config::set_config_value(bridge.lua(), var, lua_val)
                                {
                                    log::error!("Failed to set config {var}: {e}");
                                } else {
                                    *changed = true;
                                }
                            }
                        });
                    },
                );
            }
            ConfigOption::List {
                var,
                label,
                options,
                selected_index,
                tooltip,
                ..
            } => {
                labeled_row(
                    ui,
                    label_width,
                    label.as_str(),
                    tooltip.as_deref(),
                    label_color,
                    note,
                    |ui| {
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
                    },
                );
            }
            ConfigOption::Text {
                var,
                label,
                value,
                tooltip,
                ..
            } => {
                labeled_row(
                    ui,
                    label_width,
                    label.as_str(),
                    tooltip.as_deref(),
                    label_color,
                    note,
                    |ui| {
                        let response =
                            ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
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
                    },
                );
            }
        };

        draw(ui, option, &mut changed);

        changed
    }
}

/// Read the active config set's custom modifier groups, logging rather than
/// failing if the VM is not in a state to answer.
fn read_custom_groups(lua: &Lua) -> Vec<CustomModGroup> {
    custom_mods::list_groups(lua).unwrap_or_else(|e| {
        log::error!("Failed to read custom modifier groups: {e}");
        Vec::new()
    })
}

/// Colour a group's modifier text a line at a time, the way upstream's editor
/// does: recognised modifiers in the magic colour, everything else in the
/// unsupported colour, so a typo is visible without leaving the box.
fn mod_text_galley(
    ui: &egui::Ui,
    text: &str,
    status: &[LineStatus],
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;
    for (i, line) in text.split('\n').enumerate() {
        let color = match status.get(i) {
            Some(LineStatus::Parsed) => super::theme::Theme::MOD_TEXT,
            Some(LineStatus::Unsupported) => super::theme::Theme::MOD_UNSUPPORTED,
            // Blank lines, and anything not classified yet, stay neutral.
            _ => ui.visuals().text_color(),
        };
        job.append(
            line,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
        if i + 1 < text.split('\n').count() {
            job.append("\n", 0.0, egui::TextFormat::default());
        }
    }
    ui.fonts(|f| f.layout_job(job))
}

/// The box a section is drawn in: a bordered, padded panel, always open.
///
/// Upstream's `SectionControl` draws the same thing - a titled border around
/// the section's controls - and its config sections are never collapsible.
fn section_frame(ui: &egui::Ui) -> egui::Frame {
    egui::Frame::new()
        .inner_margin(SECTION_MARGIN)
        .corner_radius(ui.visuals().widgets.noninteractive.corner_radius)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
}

/// Height a section box is expected to take, used to place it before it has
/// ever been drawn. Sums the same per-row metric the rows lay themselves out
/// with ([`super::theme::row_height`]) plus the header and the frame padding.
fn predicted_section_height(
    ui: &egui::Ui,
    label_width: f32,
    options: &[ConfigOption],
    indices: &[usize],
) -> f32 {
    let spacing = ui.spacing().item_spacing.y;
    let header = ui.text_style_height(&egui::TextStyle::Body);
    let rows: f32 = indices
        .iter()
        .map(|&i| super::theme::row_height(ui, label_width, options[i].label()) + spacing)
        .sum();
    2.0 * SECTION_MARGIN + header + spacing + rows
}

/// Height the custom modifier controls add to their box: the "Add Mod Group"
/// button, then a separator, header row and text editor per group.
fn predicted_custom_mods_height(ui: &egui::Ui, groups: usize) -> f32 {
    let spacing = ui.spacing().item_spacing.y;
    let row = ui.spacing().interact_size.y + spacing;
    let editor =
        ui.text_style_height(&egui::TextStyle::Monospace) * CUSTOM_MOD_EDITOR_ROWS as f32 + spacing;
    row + groups as f32 * (spacing + row + editor)
}

/// One config row: plain-text label right-aligned in the shared column, then
/// the control. See [`super::theme::right_aligned_row`].
fn labeled_row<R>(
    ui: &mut egui::Ui,
    label_width: f32,
    label: &str,
    tooltip: Option<&str>,
    label_color: Option<egui::Color32>,
    note: Option<&str>,
    add_control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let job = egui::text::LayoutJob::simple(
        label.to_string(),
        egui::TextStyle::Body.resolve(ui.style()),
        label_color.unwrap_or_else(|| ui.visuals().text_color()),
        f32::INFINITY,
    );
    let combined = match (tooltip, note) {
        (Some(t), Some(n)) => Some(format!("{t}\n{n}")),
        (Some(t), None) => Some(t.to_string()),
        (None, Some(n)) => Some(n.to_string()),
        (None, None) => None,
    };
    super::theme::right_aligned_row(ui, label_width, job, combined.as_deref(), add_control)
}

/// Format a config Count value: as an integer when it is a whole number
/// (the common case), else with its fractional part.
fn format_config_count(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
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

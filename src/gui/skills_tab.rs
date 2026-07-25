//! Skills tab: socket group and gem editing, main skill selection.

use std::collections::HashMap;

use pob_egui::data::gems::{self, GemChoice};
use pob_egui::data::skill_sets::{self, SkillSetInfo};
use pob_egui::data::skills::{self, GemOptions, GemProperty, SocketGroup};
use pob_egui::lua_bridge::LuaBridge;

/// Pending name prompt in the skill set manager.
enum SetAction {
    New,
    Copy(i64),
    Rename(i64),
}

struct SetPrompt {
    action: SetAction,
    text: String,
}

/// Gem autocomplete state, shared across socket groups (only one add-gem
/// field has focus at a time).
#[derive(Default)]
struct GemSuggest {
    /// (group index, query, sort_by_dps) the current results were computed for.
    query_key: Option<(usize, String, bool)>,
    results: Vec<GemChoice>,
    /// True if the suggestion list was hovered last frame. Keeps the list
    /// alive during the frame where clicking it steals focus from the field.
    hovered: bool,
    /// Sort suggestions by DPS impact (upstream's Sort by DPS).
    sort_by_dps: bool,
}

/// A deferred mutation, collected during drawing and applied afterwards.
enum SkillAction {
    SetMain(usize),
    NewGroup,
    DeleteGroup(usize),
    DeleteAllGroups,
    SetEnabled(usize, bool),
    SetLabel(usize, String),
    SetSlot(usize, Option<String>),
    SetFullDps(usize, bool),
    SetGroupCount(usize, i64),
    SetGem(usize, usize, GemProperty),
    AddGem(usize, String),
    RemoveGem(usize, usize),
    ActivateSet(i64),
    NewSet(String),
    CopySet(i64, String),
    RenameSet(i64, String),
    DeleteSet(i64),
}

/// State for the skills panel.
pub struct SkillsPanel {
    pub groups: Vec<SocketGroup>,
    pub error: Option<String>,
    /// Per-group text buffers for the "add gem" field.
    add_gem_text: HashMap<usize, String>,
    /// Per-group error from the last add-gem attempt (unknown/ambiguous name).
    add_gem_error: HashMap<usize, String>,
    /// Per-group buffers for in-progress label edits.
    label_edits: HashMap<usize, String>,
    /// Group index awaiting delete confirmation (group has gems).
    confirm_delete: Option<usize>,
    /// True while the delete-all confirmation popup is open.
    confirm_delete_all: bool,
    /// Gem autocomplete state.
    suggest: GemSuggest,
    /// Skill sets in order + the active set id.
    sets: Vec<SkillSetInfo>,
    active_set: i64,
    manage_sets_open: bool,
    set_prompt: Option<SetPrompt>,
    /// Set id awaiting delete confirmation.
    confirm_delete_set: Option<i64>,
    /// Gem list/default options (mirrors upstream SkillsTab fields).
    options: GemOptions,
    show_options: bool,
}

impl SkillsPanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        let (sets, active_set) = skill_sets::list_skill_sets(lua).unwrap_or_else(|e| {
            log::error!("Failed to list skill sets: {e}");
            (Vec::new(), 1)
        });
        let options = skills::gem_options(lua).unwrap_or_else(|e| {
            log::error!("Failed to read gem options: {e}");
            GemOptions {
                sort_by_dps: true,
                sort_field: "CombinedDPS".to_string(),
                default_level: "normalMaximum".to_string(),
                default_quality: 0,
                show_support_types: "ALL".to_string(),
                show_alt_quality: false,
                show_legacy_gems: false,
            }
        });
        match skills::extract_skills(lua) {
            Ok(groups) => {
                log::info!("Loaded {} socket groups", groups.len());
                Self {
                    groups,
                    error: None,
                    add_gem_text: HashMap::new(),
                    add_gem_error: HashMap::new(),
                    label_edits: HashMap::new(),
                    confirm_delete: None,
                    confirm_delete_all: false,
                    suggest: GemSuggest::default(),
                    sets,
                    active_set,
                    manage_sets_open: false,
                    set_prompt: None,
                    confirm_delete_set: None,
                    options: options.clone(),
                    show_options: false,
                }
            }
            Err(e) => {
                log::error!("Failed to load skills: {e}");
                Self {
                    groups: Vec::new(),
                    error: Some(format!("Failed to load skills: {e}")),
                    add_gem_text: HashMap::new(),
                    add_gem_error: HashMap::new(),
                    label_edits: HashMap::new(),
                    confirm_delete: None,
                    confirm_delete_all: false,
                    suggest: GemSuggest::default(),
                    sets,
                    active_set,
                    manage_sets_open: false,
                    set_prompt: None,
                    confirm_delete_set: None,
                    options: options.clone(),
                    show_options: false,
                }
            }
        }
    }

    /// Draw the skills panel. Returns true if anything changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        if let Some(ref err) = self.error {
            ui.colored_label(super::theme::Theme::ERROR, err);
            return false;
        }

        let mut actions: Vec<SkillAction> = Vec::new();

        ui.horizontal(|ui| {
            // Skill set selector + manager
            if !self.sets.is_empty() {
                let active_label = self
                    .sets
                    .iter()
                    .find(|s| s.id == self.active_set)
                    .map(set_label)
                    .unwrap_or_else(|| "Default".to_string());
                egui::ComboBox::from_id_salt("skill_set_select")
                    .selected_text(active_label)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for set in &self.sets {
                            if ui
                                .selectable_label(set.id == self.active_set, set_label(set))
                                .clicked()
                                && set.id != self.active_set
                            {
                                actions.push(SkillAction::ActivateSet(set.id));
                            }
                        }
                    });
                if ui.button("Manage...").clicked() {
                    self.manage_sets_open = true;
                }
                ui.separator();
            }
            if ui.button("New Socket Group").clicked() {
                actions.push(SkillAction::NewGroup);
            }
            if self.groups.iter().any(|g| !g.from_item)
                && ui
                    .button("Delete All")
                    .on_hover_text("Delete every socket group (item-granted groups remain)")
                    .clicked()
            {
                self.confirm_delete_all = true;
            }
            ui.checkbox(&mut self.suggest.sort_by_dps, "Sort gems by DPS")
                .on_hover_text(
                    "Sort gem suggestions by their DPS impact on the socket group \
                     (slower on first use per group)",
                );
            if ui
                .selectable_label(self.show_options, "Gem options")
                .clicked()
            {
                self.show_options = !self.show_options;
            }
        });

        if self.show_options {
            let mut options_changed = false;
            ui.horizontal(|ui| {
                ui.label("Sort stat:");
                let current = skills::GEM_SORT_FIELDS
                    .iter()
                    .find(|(_, key)| *key == self.options.sort_field)
                    .map(|(label, _)| *label)
                    .unwrap_or("?");
                egui::ComboBox::from_id_salt("gem_sort_field")
                    .selected_text(current)
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        for (label, key) in skills::GEM_SORT_FIELDS {
                            if ui
                                .selectable_label(self.options.sort_field == key, label)
                                .clicked()
                                && self.options.sort_field != key
                            {
                                self.options.sort_field = key.to_string();
                                options_changed = true;
                            }
                        }
                    });
                ui.label("Default level:");
                let current = skills::GEM_DEFAULT_LEVELS
                    .iter()
                    .find(|(_, key)| *key == self.options.default_level)
                    .map(|(label, _)| *label)
                    .unwrap_or("?");
                egui::ComboBox::from_id_salt("gem_default_level")
                    .selected_text(current)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for (label, key) in skills::GEM_DEFAULT_LEVELS {
                            if ui
                                .selectable_label(self.options.default_level == key, label)
                                .clicked()
                                && self.options.default_level != key
                            {
                                self.options.default_level = key.to_string();
                                options_changed = true;
                            }
                        }
                    });
                ui.label("Default quality:");
                let resp =
                    ui.add(egui::DragValue::new(&mut self.options.default_quality).range(0..=23));
                if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                    options_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Supports:");
                let current = skills::GEM_SUPPORT_TYPES
                    .iter()
                    .find(|(_, key)| *key == self.options.show_support_types)
                    .map(|(label, _)| *label)
                    .unwrap_or("?");
                egui::ComboBox::from_id_salt("gem_support_types")
                    .selected_text(current)
                    .width(130.0)
                    .show_ui(ui, |ui| {
                        for (label, key) in skills::GEM_SUPPORT_TYPES {
                            if ui
                                .selectable_label(self.options.show_support_types == key, label)
                                .clicked()
                                && self.options.show_support_types != key
                            {
                                self.options.show_support_types = key.to_string();
                                options_changed = true;
                            }
                        }
                    });
                if ui
                    .checkbox(&mut self.options.show_alt_quality, "Show quality variants")
                    .on_hover_text(
                        "Include Anomalous/Divergent/Phantasmal variants in gem \
                         suggestions",
                    )
                    .changed()
                {
                    options_changed = true;
                }
                if ui
                    .checkbox(&mut self.options.show_legacy_gems, "Show legacy gems")
                    .on_hover_text("Include legacy (removed) gems in suggestions")
                    .changed()
                {
                    options_changed = true;
                }
            });
            if options_changed {
                self.options.sort_by_dps = self.suggest.sort_by_dps;
                if let Err(e) = skills::set_gem_options(bridge.lua(), &self.options) {
                    log::error!("Failed to set gem options: {e}");
                }
                // Filters affect suggestion contents
                self.suggest.query_key = None;
            }
        }

        if self.manage_sets_open {
            self.show_set_manager(ui, &mut actions);
        }

        // Delete-all confirmation
        if self.confirm_delete_all {
            egui::Window::new("Delete All Socket Groups")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("Delete every socket group? Item-granted groups are kept.");
                    ui.horizontal(|ui| {
                        if ui.button("Delete All").clicked() {
                            actions.push(SkillAction::DeleteAllGroups);
                            self.confirm_delete_all = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_all = false;
                        }
                    });
                });
        }
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in &mut self.groups {
                let add_text = self.add_gem_text.entry(group.index).or_default();
                let add_error = self.add_gem_error.get(&group.index);
                let label_buf = self
                    .label_edits
                    .entry(group.index)
                    .or_insert_with(|| group.label.clone());
                show_socket_group(
                    ui,
                    bridge,
                    group,
                    label_buf,
                    add_text,
                    add_error,
                    &mut actions,
                    &mut self.confirm_delete,
                    &mut self.suggest,
                );
            }
        });

        // Delete confirmation for groups that still contain gems
        if let Some(index) = self.confirm_delete {
            let title = self
                .groups
                .iter()
                .find(|g| g.index == index)
                .map(socket_group_title)
                .unwrap_or_else(|| format!("Group {index}"));
            let mut close = false;
            let modal =
                egui::Modal::new(egui::Id::new("delete_socket_group")).show(ui.ctx(), |ui| {
                    ui.set_max_width(400.0);
                    ui.heading("Delete Socket Group");
                    ui.label(format!("Are you sure you want to delete '{title}'?"));
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            actions.push(SkillAction::DeleteGroup(index));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if modal.should_close() || close {
                self.confirm_delete = None;
            }
        }

        let changed = self.apply_actions(bridge, actions);
        if changed {
            match skills::extract_skills(bridge.lua()) {
                Ok(groups) => self.groups = groups,
                Err(e) => log::error!("Failed to refresh skills: {e}"),
            }
            match skill_sets::list_skill_sets(bridge.lua()) {
                Ok((sets, active)) => {
                    self.sets = sets;
                    self.active_set = active;
                }
                Err(e) => log::error!("Failed to refresh skill sets: {e}"),
            }
            // Group indices may have shifted; drop stale edit buffers
            self.label_edits.clear();
            self.add_gem_text.clear();
        }
        changed
    }

    /// Manage Skill Sets dialog: activate, new, copy, rename, delete.
    fn show_set_manager(&mut self, ui: &mut egui::Ui, actions: &mut Vec<SkillAction>) {
        let mut close = false;
        egui::Window::new("Manage Skill Sets")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                for set in &self.sets {
                    ui.horizontal(|ui| {
                        let is_active = set.id == self.active_set;
                        let label = if is_active {
                            egui::RichText::new(set_label(set))
                                .color(super::theme::Theme::MAIN_SKILL)
                        } else {
                            egui::RichText::new(set_label(set))
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
                                    text: format!("{} (copy)", set_label(set)),
                                });
                            }
                            if !is_active && ui.small_button("Activate").clicked() {
                                actions.push(SkillAction::ActivateSet(set.id));
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
                            match prompt.action {
                                SetAction::New => actions.push(SkillAction::NewSet(name)),
                                SetAction::Copy(id) => actions.push(SkillAction::CopySet(id, name)),
                                SetAction::Rename(id) => {
                                    actions.push(SkillAction::RenameSet(id, name))
                                }
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

                // Delete confirmation
                if let Some(id) = self.confirm_delete_set {
                    let title = self
                        .sets
                        .iter()
                        .find(|s| s.id == id)
                        .map(set_label)
                        .unwrap_or_default();
                    ui.separator();
                    ui.colored_label(
                        super::theme::Theme::ERROR,
                        format!("Delete '{title}'? Its socket groups are lost."),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            actions.push(SkillAction::DeleteSet(id));
                            self.confirm_delete_set = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete_set = None;
                        }
                    });
                }
            });
        if close {
            self.manage_sets_open = false;
        }
    }

    fn apply_actions(&mut self, bridge: &LuaBridge, actions: Vec<SkillAction>) -> bool {
        let lua = bridge.lua();
        let mut changed = false;
        for action in actions {
            let result = match action {
                SkillAction::SetMain(index) => skills::set_main_socket_group(lua, index),
                SkillAction::NewGroup => skills::new_socket_group(lua),
                SkillAction::DeleteGroup(index) => skills::delete_socket_group(lua, index),
                SkillAction::SetEnabled(index, enabled) => {
                    skills::set_group_enabled(lua, index, enabled)
                }
                SkillAction::SetLabel(index, ref label) => {
                    skills::set_group_label(lua, index, label)
                }
                SkillAction::SetSlot(index, ref slot) => {
                    skills::set_group_slot(lua, index, slot.as_deref())
                }
                SkillAction::SetFullDps(index, include) => {
                    skills::set_group_full_dps(lua, index, include)
                }
                SkillAction::SetGroupCount(index, count) => {
                    skills::set_group_count(lua, index, count)
                }
                SkillAction::DeleteAllGroups => skills::delete_all_socket_groups(lua),
                SkillAction::SetGem(group, gem, property) => {
                    skills::set_gem_property(lua, group, gem, property)
                }
                SkillAction::AddGem(group, ref name) => match skills::add_gem(lua, group, name) {
                    Ok(None) => {
                        self.add_gem_text.remove(&group);
                        self.add_gem_error.remove(&group);
                        Ok(())
                    }
                    Ok(Some(err_msg)) => {
                        self.add_gem_error.insert(group, err_msg);
                        continue;
                    }
                    Err(e) => Err(e),
                },
                SkillAction::RemoveGem(group, gem) => skills::remove_gem(lua, group, gem),
                SkillAction::ActivateSet(id) => skill_sets::set_active_skill_set(lua, id),
                SkillAction::NewSet(ref name) => skill_sets::new_skill_set(lua, name),
                SkillAction::CopySet(id, ref name) => skill_sets::copy_skill_set(lua, id, name),
                SkillAction::RenameSet(id, ref name) => skill_sets::rename_skill_set(lua, id, name),
                SkillAction::DeleteSet(id) => skill_sets::delete_skill_set(lua, id),
            };
            match result {
                Ok(()) => changed = true,
                Err(e) => log::error!("Skill action failed: {e}"),
            }
        }
        changed
    }
}

#[allow(clippy::too_many_arguments)]
fn show_socket_group(
    ui: &mut egui::Ui,
    bridge: &LuaBridge,
    group: &mut SocketGroup,
    label_buf: &mut String,
    add_text: &mut String,
    add_error: Option<&String>,
    actions: &mut Vec<SkillAction>,
    confirm_delete: &mut Option<usize>,
    suggest: &mut GemSuggest,
) {
    let title = socket_group_title(group);
    let header_text = if group.is_main {
        egui::RichText::new(format!("* {title}")).color(super::theme::Theme::MAIN_SKILL)
    } else if !group.enabled {
        egui::RichText::new(title).color(super::theme::Theme::TEXT_DIM)
    } else {
        egui::RichText::new(title)
    };

    egui::CollapsingHeader::new(header_text)
        .id_salt(format!("skill_group_{}", group.index))
        .default_open(group.is_main)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut enabled = group.enabled;
                if ui.checkbox(&mut enabled, "Enabled").changed() {
                    actions.push(SkillAction::SetEnabled(group.index, enabled));
                }
                if !group.is_main && group.enabled && ui.small_button("Set Main").clicked() {
                    actions.push(SkillAction::SetMain(group.index));
                }
                if group.is_main {
                    ui.colored_label(super::theme::Theme::MAIN_SKILL, "Main Skill");
                }
                if let Some(ref slot) = group.slot {
                    ui.label(
                        egui::RichText::new(format!("({slot})"))
                            .small()
                            .color(super::theme::Theme::TEXT_MUTED),
                    );
                }
                let label_response = ui.add(
                    egui::TextEdit::singleline(label_buf)
                        .desired_width(140.0)
                        .hint_text("label"),
                );
                if label_response.lost_focus() && *label_buf != group.label {
                    actions.push(SkillAction::SetLabel(group.index, label_buf.clone()));
                }
                if group.from_item {
                    ui.label(
                        egui::RichText::new("(from item)")
                            .small()
                            .color(super::theme::Theme::TEXT_MUTED),
                    );
                } else if ui.small_button("Delete").clicked() {
                    if group.gems.is_empty() {
                        actions.push(SkillAction::DeleteGroup(group.index));
                    } else {
                        *confirm_delete = Some(group.index);
                    }
                }
            });

            // Second row: socket slot assignment, Full DPS, count multiplier
            ui.horizontal(|ui| {
                ui.label("Socketed in:");
                let current_label = group
                    .slot
                    .as_deref()
                    .and_then(|slot| {
                        skills::GROUP_SLOT_LIST
                            .iter()
                            .find(|(_, name)| *name == Some(slot))
                            .map(|(label, _)| *label)
                    })
                    .unwrap_or("None");
                ui.add_enabled_ui(!group.from_item, |ui| {
                    egui::ComboBox::from_id_salt(format!("group_slot_{}", group.index))
                        .selected_text(current_label)
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for (label, slot_name) in skills::GROUP_SLOT_LIST {
                                if ui.selectable_label(label == current_label, label).clicked()
                                    && label != current_label
                                {
                                    actions.push(SkillAction::SetSlot(
                                        group.index,
                                        slot_name.map(str::to_string),
                                    ));
                                }
                            }
                        });
                })
                .response
                .on_hover_text(
                    "The item this skill is socketed in; the skill benefits from that \
                     item's socketed-gem modifiers",
                );

                let mut full_dps = group.include_in_full_dps;
                if ui.checkbox(&mut full_dps, "Full DPS").changed() {
                    actions.push(SkillAction::SetFullDps(group.index, full_dps));
                }

                if group.from_item {
                    ui.label("Count:");
                    let mut count = group.group_count;
                    let resp = ui.add(egui::DragValue::new(&mut count).range(1..=99));
                    if drag_value_committed(&resp) {
                        actions.push(SkillAction::SetGroupCount(group.index, count));
                    }
                }
            });

            let group_index = group.index;
            for (i, gem) in group.gems.iter_mut().enumerate() {
                let gem_index = i + 1; // Lua is 1-based
                ui.horizontal(|ui| {
                    let mut enabled = gem.enabled;
                    if ui.checkbox(&mut enabled, "").changed() {
                        actions.push(SkillAction::SetGem(
                            group_index,
                            gem_index,
                            GemProperty::Enabled(enabled),
                        ));
                    }

                    let color = if !gem.enabled {
                        super::theme::Theme::TEXT_DIM
                    } else if gem.is_support {
                        super::theme::Theme::GEM_SUPPORT
                    } else {
                        super::theme::Theme::GEM_ACTIVE
                    };
                    ui.colored_label(color, &gem.name);

                    let level_response = ui.add(
                        egui::DragValue::new(&mut gem.level)
                            .range(1..=40)
                            .prefix("Lv "),
                    );
                    if drag_value_committed(&level_response) {
                        actions.push(SkillAction::SetGem(
                            group_index,
                            gem_index,
                            GemProperty::Level(gem.level),
                        ));
                    }

                    let quality_response = ui.add(
                        egui::DragValue::new(&mut gem.quality)
                            .range(0..=23)
                            .prefix("Q ")
                            .suffix("%"),
                    );
                    if drag_value_committed(&quality_response) {
                        actions.push(SkillAction::SetGem(
                            group_index,
                            gem_index,
                            GemProperty::Quality(gem.quality),
                        ));
                    }

                    // Quality variant (Anomalous/Divergent/...) when the gem
                    // has any beyond Default
                    if gem.alt_qualities.len() > 1 {
                        let current = gem
                            .alt_qualities
                            .iter()
                            .find(|(_, t)| *t == gem.quality_id)
                            .map(|(l, _)| l.as_str())
                            .unwrap_or("Default");
                        egui::ComboBox::from_id_salt(format!(
                            "gem_quality_id_{group_index}_{gem_index}"
                        ))
                        .selected_text(current)
                        .width(95.0)
                        .show_ui(ui, |ui| {
                            for (label, type_id) in &gem.alt_qualities {
                                if ui
                                    .selectable_label(*type_id == gem.quality_id, label)
                                    .clicked()
                                    && *type_id != gem.quality_id
                                {
                                    actions.push(SkillAction::SetGem(
                                        group_index,
                                        gem_index,
                                        GemProperty::QualityId(type_id.clone()),
                                    ));
                                }
                            }
                        });
                    }

                    // Skill copy count (totems, mirages, item-triggered copies)
                    if gem.has_count {
                        let count_response = ui.add(
                            egui::DragValue::new(&mut gem.count)
                                .range(1..=99)
                                .prefix("x "),
                        );
                        if drag_value_committed(&count_response) {
                            actions.push(SkillAction::SetGem(
                                group_index,
                                gem_index,
                                GemProperty::Count(gem.count),
                            ));
                        }
                    }

                    if ui.small_button("✕").clicked() {
                        actions.push(SkillAction::RemoveGem(group_index, gem_index));
                    }
                });
            }

            // Add gem row: autocomplete via upstream's GemSelectControl.
            // Supports tag search (":aura"), exclusion (":-cold"), and
            // abbreviations ("CtF"); Enter adds the fuzzy match directly.
            let mut field_focused = false;
            ui.horizontal(|ui| {
                let text_response = ui.add(
                    egui::TextEdit::singleline(add_text)
                        .desired_width(180.0)
                        .hint_text("gem name or :tag..."),
                );
                field_focused = text_response.has_focus();
                let submitted =
                    text_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.small_button("Add Gem").clicked() || submitted)
                    && !add_text.trim().is_empty()
                {
                    actions.push(SkillAction::AddGem(
                        group_index,
                        add_text.trim().to_string(),
                    ));
                }
            });
            if let Some(err) = add_error {
                ui.colored_label(super::theme::Theme::ERROR, err);
            }

            // Suggestion list, shown while the field has focus (or while the
            // list itself is being hovered/clicked)
            let show_list = !add_text.trim().is_empty()
                && (field_focused
                    || (suggest.hovered
                        && suggest
                            .query_key
                            .as_ref()
                            .is_some_and(|k| k.0 == group_index)));
            if show_list {
                let key = (group_index, add_text.clone(), suggest.sort_by_dps);
                if suggest.query_key.as_ref() != Some(&key) {
                    suggest.results = gems::search_gems(
                        bridge.lua(),
                        group_index,
                        add_text.trim(),
                        suggest.sort_by_dps,
                        12,
                    )
                    .unwrap_or_else(|e| {
                        log::error!("Gem search failed: {e}");
                        Vec::new()
                    });
                    suggest.query_key = Some(key);
                }

                let frame_resp = egui::Frame::group(ui.style())
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        if suggest.results.is_empty() {
                            ui.colored_label(super::theme::Theme::TEXT_DIM, "no matches");
                        }
                        for choice in &suggest.results {
                            ui.horizontal(|ui| {
                                let color = match choice.attribute.as_str() {
                                    "str" => egui::Color32::from_rgb(224, 80, 48),
                                    "dex" => egui::Color32::from_rgb(112, 255, 112),
                                    "int" => egui::Color32::from_rgb(112, 112, 255),
                                    _ => super::theme::Theme::TEXT_DEFAULT,
                                };
                                let tick = if choice.can_support { "✔ " } else { "    " };
                                let resp = ui.selectable_label(
                                    false,
                                    egui::RichText::new(format!("{tick}{}", choice.name))
                                        .color(color),
                                );
                                if suggest.sort_by_dps && choice.dps > 0.0 {
                                    ui.label(super::theme::pob_layout_job(
                                        &format!("{}{:.0}", choice.dps_color, choice.dps),
                                        12.0,
                                        super::theme::Theme::TEXT_MUTED,
                                    ));
                                }
                                if resp.clicked() {
                                    actions.push(SkillAction::AddGem(
                                        group_index,
                                        choice.name.clone(),
                                    ));
                                }
                            });
                        }
                    });
                suggest.hovered = frame_resp.response.contains_pointer();
            } else if suggest
                .query_key
                .as_ref()
                .is_some_and(|k| k.0 == group_index)
            {
                suggest.hovered = false;
            }
        });
}

/// True when a DragValue edit should be committed: on drag end, or on a
/// change made without dragging (typed edit / arrow buttons).
fn drag_value_committed(response: &egui::Response) -> bool {
    response.drag_stopped() || (response.changed() && !response.dragged())
}

fn set_label(set: &SkillSetInfo) -> String {
    if set.title.is_empty() {
        "Default".to_string()
    } else {
        set.title.clone()
    }
}

fn socket_group_title(group: &SocketGroup) -> String {
    if !group.label.is_empty() {
        return group.label.clone();
    }

    // Use the first active skill gem name as the title
    for gem in &group.gems {
        if gem.enabled && !gem.is_support && !gem.name.is_empty() {
            return gem.name.clone();
        }
    }

    // Fall back to first gem name
    group
        .gems
        .first()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| format!("Group {}", group.index))
}

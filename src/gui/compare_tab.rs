//! Compare tab: side-by-side build comparison (upstream CompareTab).
//! Covers entry management, the Summary stat comparison, tree/items/skills/
//! config sub-views with copy-to-primary actions, the calcs view with the
//! "only show differences" filter, and the compare power report.

use pob_egui::data::build_list::{self, BuildEntry, IndexKey, SortMode};
use pob_egui::data::compare::{self, CompareStatRow};
use pob_egui::data::node_power::{self, PowerStat};
use pob_egui::lua_bridge::LuaBridge;

use super::build_list::relative_prefix;
use super::theme::{self, Theme};

const BETTER: egui::Color32 = egui::Color32::from_rgb(120, 220, 120);
const WORSE: egui::Color32 = egui::Color32::from_rgb(230, 90, 90);
const COMPARE_COL: egui::Color32 = egui::Color32::from_rgb(255, 200, 100);
const EXTRA: egui::Color32 = egui::Color32::from_rgb(120, 180, 255);

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum CompareView {
    #[default]
    Summary,
    Tree,
    Items,
    Skills,
    Config,
    Calcs,
    Power,
}

/// State for the compare panel.
#[derive(Default)]
pub struct ComparePanel {
    entries: Vec<String>,
    /// Active entry (0 = none, else 1-based like Lua).
    active: usize,
    view: CompareView,
    rows: Vec<CompareStatRow>,
    /// (entry, primary calc revision) the summary rows were computed for.
    rows_key: Option<(usize, i64)>,
    /// (entry, revision, view) the sub-view data was computed for.
    view_key: Option<(usize, i64, CompareView)>,
    tree_diff: compare::TreeDiff,
    item_rows: Vec<compare::ItemRow>,
    skill_rows: Vec<compare::SkillRow>,
    config_rows: Vec<compare::ConfigSection>,
    calc_sections: Vec<compare::CalcCompareSection>,
    /// "Only show differences" filter (upstream defaults to on).
    calcs_only_diff: bool,
    calcs_only_diff_init: bool,
    // Power report state
    power_stats: Vec<PowerStat>,
    /// 0 = no metric, else 1-based into power_stats.
    power_stat_sel: usize,
    power_cats: [bool; 5],
    power_running: bool,
    power_progress: i64,
    power_rows: Vec<compare::PowerRow>,
    import: Option<ImportDialog>,
    error: Option<String>,
}

struct ImportDialog {
    /// Sub path relative to the build root; the root itself comes from
    /// `main.buildPath`, which upstream's scanner reads for itself.
    sub_path: String,
    entries: Vec<BuildEntry>,
    code: String,
    filter: String,
    /// The (sub path, filter) `entries` was produced for, so the cached index
    /// is re-filtered on change rather than every frame.
    filtered_as: Option<(String, String)>,
}

impl ImportDialog {
    fn new(bridge: &LuaBridge) -> Self {
        let mut dialog = Self {
            sub_path: String::new(),
            entries: Vec::new(),
            code: String::new(),
            filter: String::new(),
            filtered_as: None,
        };
        dialog.refresh(bridge);
        dialog
    }

    /// Re-index from disk. Same split as the build list panel: this is the only
    /// filesystem step, and `apply_filter` runs off the cached index.
    fn refresh(&mut self, bridge: &LuaBridge) {
        if let Err(e) =
            build_list::refresh_index(bridge.lua(), IndexKey::ComparePicker, &self.sub_path)
        {
            log::error!("Failed to index builds: {e}");
        }
        self.filtered_as = None;
    }

    fn apply_filter(&mut self, bridge: &LuaBridge) {
        let key = (self.sub_path.clone(), self.filter.trim().to_string());
        if self.filtered_as.as_ref() == Some(&key) {
            return;
        }
        match build_list::filter_index(
            bridge.lua(),
            IndexKey::ComparePicker,
            &key.0,
            &key.1,
            SortMode::Name,
        ) {
            Ok(entries) => self.entries = entries,
            Err(e) => log::error!("Failed to filter the build picker: {e}"),
        }
        self.filtered_as = Some(key);
    }
}

impl ComparePanel {
    fn refresh_entries(&mut self, bridge: &LuaBridge) {
        match compare::list_entries(bridge.lua()) {
            Ok((entries, active)) => {
                self.entries = entries;
                self.active = active;
            }
            Err(e) => {
                log::error!("Failed to list compare entries: {e}");
                self.error = Some(format!("Failed to list compare entries: {e}"));
            }
        }
        self.rows_key = None;
        self.view_key = None;
        self.power_rows.clear();
        self.power_running = false;
    }

    /// Draw the panel. Returns true when the primary build was modified
    /// (copy-to-primary actions, or a finished power run that touched it).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut primary_changed = false;
        let mut refresh = false;
        ui.horizontal(|ui| {
            if ui.button("Import build...").clicked() {
                self.import = Some(ImportDialog::new(bridge));
            }
            if !self.entries.is_empty() {
                ui.separator();
                ui.label("Comparing against:");
                let current = (self.active > 0)
                    .then(|| self.entries.get(self.active - 1))
                    .flatten()
                    .cloned()
                    .unwrap_or_else(|| "-".to_string());
                egui::ComboBox::from_id_salt("compare_entry")
                    .selected_text(current)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (i, label) in self.entries.iter().enumerate() {
                            if ui.selectable_label(self.active == i + 1, label).clicked() {
                                let _ = compare::set_active(bridge.lua(), i + 1);
                                refresh = true;
                            }
                        }
                    });
                if self.active > 0 && ui.button("Remove").clicked() {
                    if let Err(e) = compare::remove_entry(bridge.lua(), self.active) {
                        log::error!("Failed to remove compare entry: {e}");
                    }
                    refresh = true;
                }
            }
            if let Some(ref err) = self.error {
                ui.colored_label(Theme::ERROR, err);
            }
        });
        if refresh {
            self.refresh_entries(bridge);
        }

        self.show_import_dialog(ui, bridge);

        if self.active == 0 {
            ui.separator();
            ui.weak(
                "Import another build to compare it against this one. \
                 Comparison builds are not saved with the build (matching upstream).",
            );
            return false;
        }

        // Sub-view switcher
        ui.horizontal(|ui| {
            for (view, label) in [
                (CompareView::Summary, "Summary"),
                (CompareView::Tree, "Tree"),
                (CompareView::Items, "Items"),
                (CompareView::Skills, "Skills"),
                (CompareView::Config, "Config"),
                (CompareView::Calcs, "Calcs"),
                (CompareView::Power, "Power Report"),
            ] {
                ui.selectable_value(&mut self.view, view, label);
            }
        });
        ui.separator();

        // Refresh the active view's data when the entry, the primary's
        // calcs, or the view changed
        let revision = compare::primary_revision(bridge.lua()).unwrap_or(0);
        let key = (self.active, revision, self.view);
        if self.view_key != Some(key) {
            let result: Result<(), mlua::Error> = match self.view {
                CompareView::Summary => {
                    if self.rows_key != Some((self.active, revision)) {
                        compare::stat_rows(bridge.lua(), self.active).map(|rows| {
                            self.rows = rows;
                            self.rows_key = Some((self.active, revision));
                        })
                    } else {
                        Ok(())
                    }
                }
                CompareView::Tree => {
                    compare::tree_diff(bridge.lua(), self.active).map(|d| self.tree_diff = d)
                }
                CompareView::Items => {
                    compare::item_rows(bridge.lua(), self.active).map(|r| self.item_rows = r)
                }
                CompareView::Skills => {
                    compare::skill_rows(bridge.lua(), self.active).map(|r| self.skill_rows = r)
                }
                CompareView::Config => {
                    compare::config_rows(bridge.lua(), self.active).map(|r| self.config_rows = r)
                }
                CompareView::Calcs => {
                    if !self.calcs_only_diff_init {
                        // Upstream defaults the filter to on
                        self.calcs_only_diff = true;
                        self.calcs_only_diff_init = true;
                    }
                    compare::calc_sections(bridge.lua(), self.active, self.calcs_only_diff)
                        .map(|s| self.calc_sections = s)
                }
                CompareView::Power => Ok(()),
            };
            if let Err(e) = result {
                log::error!("Compare view refresh failed: {e}");
            }
            self.view_key = Some(key);
        }

        let compare_label = self
            .entries
            .get(self.active - 1)
            .cloned()
            .unwrap_or_else(|| "Compare".to_string());

        match self.view {
            CompareView::Summary => self.show_summary(ui, &compare_label),
            CompareView::Tree => primary_changed |= self.show_tree(ui, bridge, &compare_label),
            CompareView::Items => primary_changed |= self.show_items(ui, bridge, &compare_label),
            CompareView::Skills => self.show_skills(ui, &compare_label),
            CompareView::Config => primary_changed |= self.show_config(ui, bridge, &compare_label),
            CompareView::Calcs => self.show_calcs(ui, &compare_label),
            CompareView::Power => primary_changed |= self.show_power(ui, bridge),
        }
        if primary_changed {
            self.rows_key = None;
            self.view_key = None;
        }
        primary_changed
    }

    fn show_summary(&mut self, ui: &mut egui::Ui, compare_label: &str) {
        let rows = &self.rows;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("compare_stats")
                .num_columns(4)
                .spacing([24.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Stat");
                    ui.strong("This build");
                    ui.colored_label(COMPARE_COL, egui::RichText::new(compare_label).strong());
                    ui.strong("Difference");
                    ui.end_row();
                    for row in rows {
                        if row.spacer {
                            ui.add_space(6.0);
                            ui.end_row();
                            continue;
                        }
                        let label_text = match &row.label_color {
                            Some(code) => format!("{code}{}", row.label),
                            None => row.label.clone(),
                        };
                        ui.label(theme::pob_layout_job(
                            &label_text,
                            13.0,
                            ui.visuals().text_color(),
                        ));
                        ui.label(&row.primary);
                        ui.colored_label(COMPARE_COL, &row.compare);
                        match row.better {
                            0 => ui.label(""),
                            b if b > 0 => ui.colored_label(BETTER, &row.diff),
                            _ => ui.colored_label(WORSE, &row.diff),
                        };
                        ui.end_row();
                    }
                });
        });
    }

    fn show_tree(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, compare_label: &str) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui
                .button("Copy tree to this build")
                .on_hover_text(
                    "Add the compared build's tree as a new spec on this build \
                     (jewels are not copied, like upstream)",
                )
                .clicked()
            {
                match compare::copy_spec(bridge.lua(), false) {
                    Ok(()) => changed = true,
                    Err(e) => log::error!("Copy spec failed: {e}"),
                }
            }
            if !self.tree_diff.version.is_empty() {
                ui.weak(format!("Compared tree version: {}", self.tree_diff.version));
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(3, |cols| {
                let diff = &self.tree_diff;
                cols[0].colored_label(
                    BETTER,
                    egui::RichText::new(format!(
                        "Allocated in {compare_label} ({})",
                        diff.added.len()
                    ))
                    .strong(),
                );
                for name in &diff.added {
                    cols[0].label(name);
                }
                cols[1].colored_label(
                    WORSE,
                    egui::RichText::new(format!("Only in this build ({})", diff.removed.len()))
                        .strong(),
                );
                for name in &diff.removed {
                    cols[1].label(name);
                }
                cols[2].colored_label(
                    EXTRA,
                    egui::RichText::new(format!(
                        "Different mastery effect ({})",
                        diff.mastery.len()
                    ))
                    .strong(),
                );
                for name in &diff.mastery {
                    cols[2].label(name);
                }
            });
            if self.tree_diff.added.is_empty()
                && self.tree_diff.removed.is_empty()
                && self.tree_diff.mastery.is_empty()
            {
                ui.weak("The passive trees are identical.");
            }
        });
        changed
    }

    fn show_items(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, compare_label: &str) -> bool {
        let mut copy: Option<(String, bool)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("compare_items")
                .num_columns(5)
                .spacing([18.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Slot");
                    ui.strong("This build");
                    ui.colored_label(COMPARE_COL, egui::RichText::new(compare_label).strong());
                    ui.strong("Status");
                    ui.strong("");
                    ui.end_row();
                    for row in &self.item_rows {
                        ui.label(&row.slot);
                        let mut primary = row.primary.clone();
                        if row.primary_warn {
                            primary.push_str("  (tree missing allocated node)");
                        }
                        ui.colored_label(
                            pob_egui::data::items::rarity_color(&row.primary_rarity),
                            primary,
                        );
                        let mut compare_name = row.compare.clone();
                        if row.compare_warn {
                            compare_name.push_str("  (tree missing allocated node)");
                        }
                        ui.colored_label(
                            pob_egui::data::items::rarity_color(&row.compare_rarity),
                            compare_name,
                        );
                        let status_color = match row.status.as_str() {
                            "(match)" => BETTER,
                            "(missing)" => WORSE,
                            "(extra)" => EXTRA,
                            "(different)" => COMPARE_COL,
                            _ => Theme::TEXT_DIM,
                        };
                        ui.colored_label(status_color, &row.status);
                        ui.horizontal(|ui| {
                            if row.can_copy {
                                let slot =
                                    row.copy_slot.clone().unwrap_or_else(|| row.slot.clone());
                                if ui
                                    .small_button("Copy")
                                    .on_hover_text("Add this item to this build (unequipped)")
                                    .clicked()
                                {
                                    copy = Some((slot.clone(), false));
                                }
                                if ui
                                    .small_button("Equip")
                                    .on_hover_text("Add this item and equip it in the slot")
                                    .clicked()
                                {
                                    copy = Some((slot, true));
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        if let Some((slot, and_use)) = copy {
            match compare::copy_item(bridge.lua(), self.active, &slot, and_use) {
                Ok(()) => return true,
                Err(e) => log::error!("Copy item failed: {e}"),
            }
        }
        false
    }

    fn show_skills(&mut self, ui: &mut egui::Ui, compare_label: &str) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].strong("This build");
                cols[1].colored_label(COMPARE_COL, egui::RichText::new(compare_label).strong());
            });
            ui.separator();
            for row in &self.skill_rows {
                ui.columns(2, |cols| {
                    let (left, right) = cols.split_at_mut(1);
                    let sides = [
                        (&mut left[0], &row.primary_label, &row.primary_gems),
                        (&mut right[0], &row.compare_label, &row.compare_gems),
                    ];
                    for (col, label, gems) in sides {
                        if label.is_empty() {
                            col.weak("(no matching group)");
                        } else {
                            col.strong(label.as_str());
                        }
                        for gem in gems {
                            match gem.status.as_str() {
                                "missing" => {
                                    col.colored_label(WORSE, format!("- {}", gem.name));
                                }
                                "additional" => {
                                    col.colored_label(
                                        BETTER,
                                        format!("+ {} {}/{}", gem.name, gem.level, gem.quality),
                                    );
                                }
                                _ => {
                                    col.label(format!(
                                        "{} {}/{}",
                                        gem.name, gem.level, gem.quality
                                    ));
                                }
                            }
                        }
                    }
                });
                ui.add_space(8.0);
                ui.separator();
            }
            if self.skill_rows.is_empty() {
                ui.weak("No socket groups to compare.");
            }
        });
    }

    fn show_config(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge, compare_label: &str) -> bool {
        let mut changed = false;
        if ui
            .button("Copy config to this build")
            .on_hover_text("Merge the compared build's configuration into this build")
            .clicked()
        {
            match compare::copy_config(bridge.lua()) {
                Ok(()) => changed = true,
                Err(e) => log::error!("Copy config failed: {e}"),
            }
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for section in &self.config_rows {
                let title = if section.diffs.is_empty() {
                    section.name.clone()
                } else {
                    format!("{}  ({} diff)", section.name, section.diffs.len())
                };
                egui::CollapsingHeader::new(title)
                    .default_open(!section.diffs.is_empty())
                    .show(ui, |ui| {
                        egui::Grid::new(("compare_config", &section.name))
                            .num_columns(3)
                            .spacing([18.0, 2.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("Option");
                                ui.strong("This build");
                                ui.colored_label(
                                    COMPARE_COL,
                                    egui::RichText::new(compare_label).strong(),
                                );
                                ui.end_row();
                                for row in section.diffs.iter() {
                                    ui.colored_label(COMPARE_COL, &row.label);
                                    ui.label(&row.primary);
                                    ui.label(&row.compare);
                                    ui.end_row();
                                }
                                for row in section.commons.iter() {
                                    ui.label(&row.label);
                                    ui.label(&row.primary);
                                    ui.label(&row.compare);
                                    ui.end_row();
                                }
                            });
                    });
            }
            if self.config_rows.is_empty() {
                ui.weak("No configuration to compare.");
            }
        });
        changed
    }

    fn show_calcs(&mut self, ui: &mut egui::Ui, compare_label: &str) {
        if ui
            .checkbox(&mut self.calcs_only_diff, "Only show differences")
            .on_hover_text(
                "Hide rows whose value, modifier list, and breakdown are \
                 identical in both builds",
            )
            .changed()
        {
            // Recompute with the new filter on the next pass
            self.view_key = None;
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let text_color = ui.visuals().text_color();
            for section in &self.calc_sections {
                egui::CollapsingHeader::new(&section.id)
                    .default_open(true)
                    .show(ui, |ui| {
                        for sub in &section.subsections {
                            if !sub.label.is_empty() {
                                ui.strong(&sub.label);
                            }
                            if !sub.primary_extra.is_empty() || !sub.compare_extra.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label(theme::pob_layout_job(
                                        &sub.primary_extra,
                                        12.0,
                                        text_color,
                                    ));
                                    ui.weak("|");
                                    ui.colored_label(COMPARE_COL, "");
                                    ui.label(theme::pob_layout_job(
                                        &sub.compare_extra,
                                        12.0,
                                        COMPARE_COL,
                                    ));
                                });
                            }
                            egui::Grid::new(("compare_calcs", &section.id, &sub.label))
                                .num_columns(3)
                                .spacing([18.0, 1.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    for row in &sub.rows {
                                        ui.label(&row.label);
                                        ui.label(theme::pob_layout_job(
                                            &row.primary,
                                            12.0,
                                            text_color,
                                        ));
                                        ui.label(theme::pob_layout_job(
                                            &row.compare,
                                            12.0,
                                            COMPARE_COL,
                                        ));
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(4.0);
                        }
                    });
            }
            if self.calc_sections.is_empty() {
                ui.weak(if self.calcs_only_diff {
                    format!("No calculation differences against {compare_label}.")
                } else {
                    "No calculation sections available.".to_string()
                });
            }
        });
    }

    fn show_power(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut finished_run = false;
        if self.power_stats.is_empty() {
            self.power_stats = node_power::list_power_stats(bridge.lua()).unwrap_or_default();
        }
        ui.horizontal(|ui| {
            ui.label("Metric:");
            let current = (self.power_stat_sel > 0)
                .then(|| self.power_stats.get(self.power_stat_sel - 1))
                .flatten()
                .map(|s| s.label.clone())
                .unwrap_or_else(|| "-- Select Metric --".to_string());
            let mut restart = false;
            egui::ComboBox::from_id_salt("compare_power_stat")
                .selected_text(current)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (i, stat) in self.power_stats.iter().enumerate() {
                        if ui
                            .selectable_label(self.power_stat_sel == i + 1, &stat.label)
                            .clicked()
                        {
                            self.power_stat_sel = i + 1;
                            restart = true;
                        }
                    }
                });
            let cat_labels = ["Tree", "Items", "Skill gems", "Support gems", "Config"];
            for (i, label) in cat_labels.iter().enumerate() {
                if ui.checkbox(&mut self.power_cats[i], *label).changed() {
                    restart = true;
                }
            }
            if restart && self.power_stat_sel > 0 {
                let stat_index = self.power_stats[self.power_stat_sel - 1].index;
                match compare::power_set_stat(bridge.lua(), stat_index, self.power_cats) {
                    Ok(()) => {
                        self.power_running = true;
                        self.power_rows.clear();
                    }
                    Err(e) => log::error!("Power stat select failed: {e}"),
                }
            }
        });
        ui.weak(
            "What would this build gain from the compared build? One calc pass per \
             candidate; the report runs incrementally.",
        );
        ui.separator();

        if self.power_running {
            match compare::power_step(bridge.lua(), self.active) {
                Ok((done, progress)) => {
                    self.power_progress = progress;
                    if done {
                        self.power_running = false;
                        self.power_rows = compare::power_results(bridge.lua()).unwrap_or_default();
                        // The builder temporarily mutates the primary build;
                        // let the parent refresh primary-derived state
                        finished_run = true;
                    } else {
                        ui.ctx().request_repaint();
                    }
                }
                Err(e) => {
                    log::error!("Power step failed: {e}");
                    self.power_running = false;
                }
            }
        }
        if self.power_running {
            ui.add(
                egui::ProgressBar::new(self.power_progress as f32 / 100.0)
                    .text(format!("Analyzing... {}%", self.power_progress)),
            );
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("compare_power")
                .num_columns(4)
                .spacing([18.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Category");
                    ui.strong("Name");
                    ui.strong("Impact");
                    ui.strong("Per point");
                    ui.end_row();
                    for row in &self.power_rows {
                        ui.label(&row.category);
                        ui.label(&row.name);
                        let color = if row.impact > 0.0 { BETTER } else { WORSE };
                        ui.colored_label(color, &row.impact_str);
                        ui.label(&row.per_point);
                        ui.end_row();
                    }
                });
            if self.power_rows.is_empty() && !self.power_running {
                ui.weak(if self.power_stat_sel == 0 {
                    "Select a metric to analyze what this build could gain."
                } else {
                    "No improvements found from the compared build."
                });
            }
        });
        finished_run
    }

    fn show_import_dialog(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) {
        let Some(dialog) = &mut self.import else {
            return;
        };
        let mut close = false;
        let mut nav_to: Option<String> = None;
        let mut enter_folder: Option<String> = None;
        let mut import_file: Option<std::path::PathBuf> = None;
        let mut import_code = false;
        dialog.apply_filter(bridge);
        let here = dialog.sub_path.clone();

        egui::Window::new("Import Comparison Build")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_width(420.0);
                ui.horizontal(|ui| {
                    ui.label("Share code:");
                    ui.add(
                        egui::TextEdit::singleline(&mut dialog.code)
                            .hint_text("Paste a build code")
                            .desired_width(220.0),
                    );
                    if ui
                        .add_enabled(!dialog.code.trim().is_empty(), egui::Button::new("Import"))
                        .clicked()
                    {
                        import_code = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Folder:");
                    if ui.selectable_label(false, "Builds").clicked() {
                        nav_to = Some(String::new());
                    }
                    let components: Vec<&str> = dialog
                        .sub_path
                        .split('/')
                        .filter(|c| !c.is_empty())
                        .collect();
                    for (i, component) in components.iter().enumerate() {
                        ui.label("/");
                        if ui.selectable_label(false, *component).clicked() {
                            nav_to = Some(format!("{}/", components[..=i].join("/")));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add(
                        egui::TextEdit::singleline(&mut dialog.filter)
                            .hint_text("e.g. class:assassin myfilename")
                            .desired_width(240.0),
                    );
                    if !dialog.filter.is_empty() && ui.button("✖").clicked() {
                        dialog.filter.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("compare_import_list")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for entry in &dialog.entries {
                            match entry {
                                BuildEntry::Folder(f) => {
                                    let prefix = relative_prefix(&f.sub_path, &here);
                                    if ui.button(format!("📁 {prefix}{}", f.folder_name)).clicked()
                                    {
                                        // A search hit can sit below the folder
                                        // on screen, so navigate to where it is.
                                        enter_folder =
                                            Some(format!("{}{}", f.sub_path, f.folder_name));
                                    }
                                }
                                BuildEntry::Build(b) => {
                                    let prefix = relative_prefix(&b.sub_path, &here);
                                    if ui.button(format!("{prefix}{}", b.build_name)).clicked() {
                                        import_file = Some(b.full_path.clone());
                                    }
                                }
                            }
                        }
                        if dialog.entries.is_empty() {
                            if dialog.filter.trim().is_empty() {
                                ui.weak("(empty folder)");
                            } else {
                                ui.weak("(no matches)");
                            }
                        }
                    });
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });

        if let Some(sub_path) = nav_to {
            dialog.sub_path = sub_path;
            dialog.refresh(bridge);
        }
        if let Some(folder) = enter_folder {
            dialog.sub_path = format!("{folder}/");
            dialog.refresh(bridge);
        }
        let mut imported = false;
        if let Some(path) = import_file {
            match std::fs::read_to_string(&path) {
                Ok(xml) => {
                    let label = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Build".to_string());
                    match compare::import_build(bridge.lua(), &xml, &label) {
                        Ok(true) => imported = true,
                        Ok(false) => self.error = Some("Build failed to load.".to_string()),
                        Err(e) => log::error!("Compare import failed: {e}"),
                    }
                }
                Err(e) => self.error = Some(format!("Failed to read build: {e}")),
            }
        }
        if import_code {
            match compare::import_code(bridge.lua(), dialog.code.trim()) {
                Ok(true) => imported = true,
                Ok(false) => self.error = Some("Invalid build code.".to_string()),
                Err(e) => log::error!("Compare code import failed: {e}"),
            }
        }
        if imported {
            self.import = None;
            self.error = None;
            self.refresh_entries(bridge);
        } else if close {
            self.import = None;
        }
    }
}

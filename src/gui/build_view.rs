//! Build view: container for an open build with stat sidebar and tabs.

use std::collections::HashMap;

use pob_egui::data::CalcOutput;
use pob_egui::data::build_list::{self, BuildEntry};
use pob_egui::data::display_stats::{self, SidebarStats};
use pob_egui::data::items::TooltipLine;
use pob_egui::data::loadouts;
use pob_egui::data::spectres::{self, SpectreEntry};
use pob_egui::lua_bridge::LuaBridge;

use super::items_tab::show_tooltip_lines;

use super::calcs_tab::CalcsPanel;
use super::compare_tab::ComparePanel;
use super::config_tab::ConfigPanel;
use super::import_tab::ImportPanel;
use super::items_tab::ItemsPanel;
use super::notes_tab::NotesPanel;
use super::skills_tab::SkillsPanel;
use super::tree_tab::TreePanel;

/// Which tab is currently active in the build view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildTab {
    Tree,
    Skills,
    Items,
    Calcs,
    Config,
    Notes,
    Compare,
    Import,
}

/// A class entry with its ascendancies.
struct ClassEntry {
    class_id: u32,
    name: String,
    ascendancies: Vec<AscendEntry>,
}

struct AscendEntry {
    ascend_class_id: u32,
    name: String,
}

/// A secondary (alternate) ascendancy. id 0 is the synthetic "None" entry.
struct SecondaryAscendEntry {
    id: u32,
    name: String,
}

/// State for an open build.
pub struct BuildView {
    pub build_name: String,
    pub calc_output: Option<CalcOutput>,
    /// Full sidebar stat list + warnings, pre-formatted by upstream Lua.
    pub sidebar_stats: Option<SidebarStats>,
    pub config_panel: Option<ConfigPanel>,
    pub tree_panel: Option<TreePanel>,
    pub items_panel: Option<ItemsPanel>,
    pub skills_panel: Option<SkillsPanel>,
    pub calcs_panel: Option<CalcsPanel>,
    /// True when calc data changed and the calcs panel needs a refresh.
    calcs_stale: bool,
    pub notes_panel: Option<NotesPanel>,
    pub import_panel: ImportPanel,
    pub active_tab: BuildTab,
    /// True if this build has never been saved to disk (new build).
    pub is_unsaved_new: bool,
    /// When set, shows a save-as dialog before navigating away.
    save_as_dialog: Option<SaveAsDialog>,
    /// When true, shows a "save changes?" prompt before navigating away.
    save_prompt: bool,
    /// Available classes and ascendancies.
    classes: Vec<ClassEntry>,
    /// Currently selected class index (into `classes`).
    selected_class: usize,
    /// Currently selected ascendancy index (into the selected class's `ascendancies`).
    selected_ascend: usize,
    /// Available secondary ascendancies for this tree version (empty if unsupported).
    secondary_ascendancies: Vec<SecondaryAscendEntry>,
    /// Currently selected secondary ascendancy index (into `secondary_ascendancies`).
    selected_secondary: usize,
    // Character header state
    char_level: String,
    level_auto_mode: bool,
    /// Loadouts (matched tree/item/skill/config set names) + current match.
    loadouts: Vec<String>,
    selected_loadout: Option<String>,
    /// Name buffer for the New Loadout prompt (None = closed).
    loadout_prompt: Option<String>,
    /// Spectre library popup (staged list; committed on Save like upstream).
    spectre_library: Option<SpectreLibrary>,
    /// Compare tab state (entries live in Lua; dropped on build close).
    compare_panel: ComparePanel,
    /// Transient toast: (message, is_error, shown-at).
    toast: Option<(String, bool, std::time::Instant)>,
}

/// Spectre library popup state.
struct SpectreLibrary {
    /// Staged (id, name) build list; only Save commits it.
    dest: Vec<(String, String)>,
    available: Vec<SpectreEntry>,
    search: String,
    /// 0 = Names, 1 = Skills, 2 = Both (upstream's search modes).
    search_mode: usize,
    tooltip_cache: HashMap<String, Vec<TooltipLine>>,
}

struct SaveAsDialog {
    name: String,
    error: Option<String>,
    /// True when triggered by leaving the build (back button): offers
    /// Discard, and navigates back after saving.
    then_close: bool,
    /// Builds root directory (main.buildPath, trailing slash).
    build_path: String,
    /// Target folder relative to the builds root ("" or "a/b/"), like
    /// upstream's dbFileSubPath.
    sub_path: String,
    /// Subfolders of `sub_path` shown in the folder browser.
    folders: Vec<String>,
    /// New-folder input state: (name, error), Some while the input is open.
    new_folder: Option<(String, Option<String>)>,
}

impl SaveAsDialog {
    fn new(name: String, then_close: bool, bridge: &LuaBridge) -> Self {
        let mut dialog = Self {
            name,
            error: None,
            then_close,
            build_path: bridge.build_path().unwrap_or_default(),
            sub_path: bridge.build_file_sub_path(),
            folders: Vec::new(),
            new_folder: None,
        };
        dialog.refresh_folders(bridge);
        dialog
    }

    fn refresh_folders(&mut self, bridge: &LuaBridge) {
        self.folders =
            build_list::scan_builds_sorted(bridge.lua(), &self.build_path, &self.sub_path)
                .into_iter()
                .filter_map(|entry| match entry {
                    BuildEntry::Folder(f) => Some(f.folder_name),
                    BuildEntry::Build(_) => None,
                })
                .collect();
    }

    /// Target file for the current name, with illegal filename characters
    /// replaced the same way the Lua save path does.
    fn target_file(&self) -> Option<std::path::PathBuf> {
        let sanitized: String = self
            .name
            .trim()
            .chars()
            .map(|c| {
                if "\\/:*?\"<>|".contains(c) || c.is_control() {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        if sanitized.trim().is_empty() {
            return None;
        }
        Some(
            std::path::Path::new(&self.build_path)
                .join(&self.sub_path)
                .join(format!("{sanitized}.xml")),
        )
    }
}

impl BuildView {
    pub fn new(build_name: String, bridge: &LuaBridge) -> Self {
        let calc_output = CalcOutput::extract(bridge.lua())
            .map_err(|e| log::error!("Failed to extract calc output: {e}"))
            .ok();
        let sidebar_stats = display_stats::extract_sidebar_stats(bridge.lua())
            .map_err(|e| log::error!("Failed to extract sidebar stats: {e}"))
            .ok();

        let config_panel = Some(ConfigPanel::new(bridge.lua()));
        let tree_panel = Some(TreePanel::new(bridge.lua()));
        let items_panel = Some(ItemsPanel::new(bridge.lua()));
        let skills_panel = Some(SkillsPanel::new(bridge.lua()));
        let notes_panel = Some(NotesPanel::new(bridge.lua()));
        let import_panel = ImportPanel::new();

        let classes = load_class_list(bridge.lua());
        let (selected_class, selected_ascend) = find_current_selection(bridge.lua(), &classes);
        let secondary_ascendancies = load_secondary_ascendancies(bridge.lua());
        let selected_secondary = find_secondary_selection(bridge.lua(), &secondary_ascendancies);
        let header = load_char_header(bridge.lua());
        let (loadouts, selected_loadout) =
            loadouts::list_loadouts(bridge.lua()).unwrap_or_else(|e| {
                log::error!("Failed to list loadouts: {e}");
                (Vec::new(), None)
            });

        Self {
            build_name,
            calc_output,
            sidebar_stats,
            config_panel,
            tree_panel,
            items_panel,
            skills_panel,
            calcs_panel: None, // created lazily on first tab visit
            calcs_stale: false,
            notes_panel,
            import_panel,
            active_tab: BuildTab::Tree,
            is_unsaved_new: false,
            save_as_dialog: None,
            save_prompt: false,
            classes,
            selected_class,
            selected_ascend,
            secondary_ascendancies,
            selected_secondary,
            char_level: header.level.to_string(),
            level_auto_mode: header.level_auto,
            loadouts,
            selected_loadout,
            loadout_prompt: None,
            spectre_library: None,
            compare_panel: ComparePanel::default(),
            toast: None,
        }
    }

    /// Window title text for this build: "Name (Class)" like upstream's
    /// SetWindowTitleWithBuildClass (ascendancy when picked, plus secondary).
    pub fn window_title(&self) -> String {
        let mut class_text = self
            .classes
            .get(self.selected_class)
            .map(|c| {
                if self.selected_ascend > 0 {
                    c.ascendancies
                        .get(self.selected_ascend)
                        .map(|a| a.name.as_str())
                        .unwrap_or(c.name.as_str())
                } else {
                    c.name.as_str()
                }
            })
            .unwrap_or("")
            .to_string();
        if self.selected_secondary > 0
            && let Some(secondary) = self.secondary_ascendancies.get(self.selected_secondary)
        {
            class_text = format!("{class_text} + {}", secondary.name);
        }
        if class_text.is_empty() {
            self.build_name.clone()
        } else {
            format!("{} ({class_text})", self.build_name)
        }
    }

    /// Save the build, opening the Save As dialog if it has no file yet.
    /// Spectre Library popup (upstream OpenSpectreLibrary): staged in-build
    /// list on the left, searchable available list on the right; Save
    /// commits, Cancel discards.
    fn show_spectre_library(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) {
        let Some(lib) = &mut self.spectre_library else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let mut add: Option<String> = None;
        let mut remove: Option<usize> = None;

        egui::Window::new("Spectre Library")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_width(560.0);
                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.strong("Spectres in Build:");
                    egui::ScrollArea::vertical()
                        .id_salt("spectre_dest")
                        .max_height(250.0)
                        .show(ui, |ui| {
                            for (i, (id, name)) in lib.dest.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if ui.small_button("✖").on_hover_text("Remove").clicked() {
                                        remove = Some(i);
                                    }
                                    let resp = ui.label(name);
                                    spectre_hover(resp, &mut lib.tooltip_cache, bridge, id);
                                });
                            }
                            if lib.dest.is_empty() {
                                ui.weak("(none)");
                            }
                        });

                    let ui = &mut cols[1];
                    ui.strong("Available Spectres:");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut lib.search)
                                .hint_text("Search...")
                                .desired_width(150.0),
                        );
                        egui::ComboBox::from_id_salt("spectre_search_mode")
                            .selected_text(["Names", "Skills", "Both"][lib.search_mode.min(2)])
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                for (i, label) in ["Names", "Skills", "Both"].iter().enumerate() {
                                    ui.selectable_value(&mut lib.search_mode, i, *label);
                                }
                            });
                    });
                    let query = lib.search.to_lowercase();
                    egui::ScrollArea::vertical()
                        .id_salt("spectre_source")
                        .max_height(228.0)
                        .show(ui, |ui| {
                            for entry in &lib.available {
                                if !query.is_empty() {
                                    let name_match = lib.search_mode != 1
                                        && entry.name.to_lowercase().contains(&query);
                                    let skill_match = lib.search_mode != 0
                                        && entry
                                            .skills
                                            .iter()
                                            .any(|s| s.to_lowercase().contains(&query));
                                    if !name_match && !skill_match {
                                        continue;
                                    }
                                }
                                let in_build = lib.dest.iter().any(|(id, _)| id == &entry.id);
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(!in_build, egui::Button::new("+").small())
                                        .on_hover_text("Add to build")
                                        .clicked()
                                    {
                                        add = Some(entry.id.clone());
                                    }
                                    let resp = ui.label(&entry.name);
                                    spectre_hover(resp, &mut lib.tooltip_cache, bridge, &entry.id);
                                });
                            }
                        });
                });
                ui.add_space(4.0);
                ui.weak(
                    "Spectres in your Library must be assigned to an active Raise Spectre gem \
                     for their buffs and curses to activate.",
                );
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if let Some(i) = remove {
            lib.dest.remove(i);
        }
        if let Some(id) = add
            && !lib.dest.iter().any(|(existing, _)| *existing == id)
        {
            let name = lib
                .available
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            lib.dest.push((id, name));
        }
        if save {
            let ids: Vec<String> = lib.dest.iter().map(|(id, _)| id.clone()).collect();
            match spectres::set_spectre_list(bridge.lua(), &ids) {
                Ok(()) => {
                    self.spectre_library = None;
                    self.refresh_calc_output(bridge);
                }
                Err(e) => log::error!("Failed to save spectre list: {e}"),
            }
        } else if close {
            self.spectre_library = None;
        }
    }

    fn request_save(&mut self, bridge: &LuaBridge) {
        if bridge.build_file_name().is_none() {
            self.save_as_dialog = Some(SaveAsDialog::new(self.build_name.clone(), false, bridge));
        } else {
            match bridge.save_build() {
                Ok(()) => {
                    self.toast =
                        Some(("Build saved.".to_string(), false, std::time::Instant::now()));
                }
                Err(e) => {
                    log::error!("Save failed: {e}");
                    self.toast =
                        Some((format!("Save failed: {e}"), true, std::time::Instant::now()));
                }
            }
        }
    }

    /// Start closing the build: opens the save-as dialog or save prompt if
    /// there are unsaved changes. Returns true when it's safe to close now.
    fn request_close(&mut self, bridge: &LuaBridge) -> bool {
        let dirty = bridge.is_build_dirty();
        if bridge.build_file_name().is_none() && (self.is_unsaved_new || dirty) {
            // Never saved: needs a name before it can be kept
            self.save_as_dialog = Some(SaveAsDialog::new(self.build_name.clone(), true, bridge));
            false
        } else if dirty {
            self.save_prompt = true;
            false
        } else {
            true
        }
    }

    /// Draw the build view. Returns true if the user wants to go back to the build list.
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut go_back = false;

        // Keyboard shortcuts (skipped while a modal is open)
        let modal_open =
            self.save_as_dialog.is_some() || self.save_prompt || self.spectre_library.is_some();
        if !modal_open {
            // Ctrl+S: save (no-op when there is nothing to save, like the button)
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S))
                && (bridge.is_build_dirty() || bridge.build_file_name().is_none())
            {
                self.request_save(bridge);
            }

            // Ctrl+1-7: switch tabs
            let tab_keys = [
                (egui::Key::Num1, BuildTab::Tree),
                (egui::Key::Num2, BuildTab::Skills),
                (egui::Key::Num3, BuildTab::Items),
                (egui::Key::Num4, BuildTab::Calcs),
                (egui::Key::Num5, BuildTab::Config),
                (egui::Key::Num6, BuildTab::Notes),
                (egui::Key::Num7, BuildTab::Import),
            ];
            for (key, tab) in tab_keys {
                if ui.input(|i| i.modifiers.command && i.key_pressed(key)) {
                    self.active_tab = tab;
                }
            }

            // Ctrl+I: open Import/Export
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::I)) {
                self.active_tab = BuildTab::Import;
            }

            // Ctrl+W or Mouse4: close build (with save prompt)
            let close_pressed = ui.input(|i| {
                (i.modifiers.command && i.key_pressed(egui::Key::W))
                    || i.pointer.button_pressed(egui::PointerButton::Extra1)
            });
            if close_pressed && self.request_close(bridge) {
                return true;
            }
        }

        // Handle save-as dialog (modal)
        if let Some(dialog) = &mut self.save_as_dialog {
            let mut close_dialog = false;
            let mut do_save = false;
            let mut discard = false;
            let then_close = dialog.then_close;
            // Deferred folder actions (applied after the window closure)
            let mut nav_to: Option<String> = None;
            let mut enter_folder: Option<String> = None;
            let mut do_create_folder = false;
            let mut cancel_new_folder = false;

            // Upstream refuses to overwrite a different build's file; saving
            // over this build's own file is a normal save
            let target = dialog.target_file();
            let overwrite_blocked = target.as_ref().is_some_and(|t| {
                t.exists()
                    && bridge
                        .build_file_name()
                        .is_none_or(|f| std::path::Path::new(&f) != t)
            });

            egui::Window::new("Save Build As")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("Enter a name for this build:");
                    let response = ui.text_edit_singleline(&mut dialog.name);
                    if response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !overwrite_blocked
                    {
                        do_save = true;
                    }

                    // Folder breadcrumb (Builds / a / b, each segment jumps)
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
                    egui::ScrollArea::vertical()
                        .id_salt("save_as_folders")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for folder in &dialog.folders {
                                if ui.button(format!("📁 {folder}")).clicked() {
                                    enter_folder = Some(folder.clone());
                                }
                            }
                            if dialog.folders.is_empty() {
                                ui.weak("(no subfolders)");
                            }
                        });

                    // New folder input
                    if let Some((folder_name, folder_err)) = &mut dialog.new_folder {
                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(folder_name)
                                    .hint_text("Folder name")
                                    .desired_width(180.0),
                            );
                            let submit =
                                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("Create").clicked() || submit {
                                do_create_folder = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_new_folder = true;
                            }
                        });
                        if let Some(err) = folder_err {
                            ui.colored_label(egui::Color32::RED, err.as_str());
                        }
                    } else if ui.button("+ New Folder").clicked() {
                        dialog.new_folder = Some((String::new(), None));
                    }

                    if overwrite_blocked {
                        ui.colored_label(
                            egui::Color32::RED,
                            "A build with this name already exists in this folder.",
                        );
                    }
                    if let Some(ref err) = dialog.error {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!overwrite_blocked, egui::Button::new("Save"))
                            .clicked()
                        {
                            do_save = true;
                        }
                        if then_close && ui.button("Discard").clicked() {
                            discard = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                    });
                });

            // Apply deferred folder actions
            if cancel_new_folder {
                dialog.new_folder = None;
            }
            if let Some(sub_path) = nav_to {
                dialog.sub_path = sub_path;
                dialog.refresh_folders(bridge);
            }
            if let Some(folder) = enter_folder {
                dialog.sub_path = format!("{}{folder}/", dialog.sub_path);
                dialog.refresh_folders(bridge);
            }
            if do_create_folder && let Some((folder_name, _)) = dialog.new_folder.clone() {
                let folder_name = folder_name.trim().to_string();
                match build_list::create_folder(&dialog.build_path, &dialog.sub_path, &folder_name)
                {
                    Ok(()) => {
                        // Navigate into the new folder like upstream
                        dialog.sub_path = format!("{}{folder_name}/", dialog.sub_path);
                        dialog.new_folder = None;
                        dialog.refresh_folders(bridge);
                    }
                    Err(e) => dialog.new_folder = Some((folder_name, Some(e))),
                }
            }

            if do_save {
                let name = dialog.name.trim().to_string();
                if name.is_empty() {
                    dialog.error = Some("Name cannot be empty.".to_string());
                } else {
                    match bridge.save_build_as(&name, &dialog.sub_path) {
                        Ok(()) => {
                            self.build_name = name;
                            self.is_unsaved_new = false;
                            self.save_as_dialog = None;
                            go_back = then_close;
                        }
                        Err(e) => {
                            dialog.error = Some(format!("Save failed: {e}"));
                        }
                    }
                }
            } else if discard {
                self.save_as_dialog = None;
                go_back = true;
            } else if close_dialog {
                self.save_as_dialog = None;
            }

            if go_back {
                return true;
            }
        }

        // Handle "save changes?" prompt (modal, for builds that have a file)
        if self.save_prompt {
            let mut close_prompt = false;
            egui::Window::new("Save Changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!(
                        "'{}' has unsaved changes. Save before closing?",
                        self.build_name
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Save and Close").clicked() {
                            match bridge.save_build() {
                                Ok(()) => {
                                    go_back = true;
                                }
                                Err(e) => log::error!("Save failed: {e}"),
                            }
                            close_prompt = true;
                        }
                        if ui.button("Discard Changes").clicked() {
                            go_back = true;
                            close_prompt = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_prompt = true;
                        }
                    });
                });
            if close_prompt {
                self.save_prompt = false;
            }
            if go_back {
                return true;
            }
        }

        // Top bar: back button, build name, class/ascendancy, level, points
        let mut class_changed = false;
        let mut ascend_changed = false;
        let mut secondary_changed = false;
        let mut header_changed = false;

        let dirty = bridge.is_build_dirty();
        let mut request_save = false;
        let mut activate_loadout: Option<String> = None;
        let mut new_loadout_clicked = false;
        ui.horizontal(|ui| {
            if ui.button("< Builds").clicked() && self.request_close(bridge) {
                go_back = true;
            }
            let title = if dirty {
                format!("{} *", self.build_name)
            } else {
                self.build_name.clone()
            };
            ui.heading(title);

            ui.separator();

            // Save buttons (Save is inactive when there is nothing to save;
            // a never-saved build can always be saved to give it a file)
            let can_save = dirty || bridge.build_file_name().is_none();
            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .on_hover_text("Ctrl+S")
                .on_disabled_hover_text("No unsaved changes")
                .clicked()
            {
                request_save = true;
            }
            if ui.button("Save As").clicked() {
                self.save_as_dialog =
                    Some(SaveAsDialog::new(self.build_name.clone(), false, bridge));
            }

            ui.separator();

            // Points display
            let (points_used, points_max, asc_used, asc_max) = get_points_display(bridge.lua());
            let points_color = if points_used > points_max {
                egui::Color32::from_rgb(255, 80, 80)
            } else {
                egui::Color32::from_rgb(200, 200, 200)
            };
            ui.colored_label(points_color, format!("Points: {points_used}/{points_max}"));
            let asc_color = if asc_used > asc_max {
                egui::Color32::from_rgb(255, 80, 80)
            } else {
                egui::Color32::from_rgb(200, 200, 200)
            };
            ui.colored_label(asc_color, format!("Asc: {asc_used}/{asc_max}"));

            ui.separator();

            // Class dropdown
            if !self.classes.is_empty() {
                let prev_class = self.selected_class;
                egui::ComboBox::from_id_salt("class_select")
                    .selected_text(&self.classes[self.selected_class].name)
                    .show_ui(ui, |ui| {
                        for (i, class) in self.classes.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_class, i, &class.name);
                        }
                    });
                if self.selected_class != prev_class {
                    self.selected_ascend = 0;
                    class_changed = true;
                }

                // Ascendancy dropdown
                let ascendancies = &self.classes[self.selected_class].ascendancies;
                if !ascendancies.is_empty() {
                    let prev_ascend = self.selected_ascend;
                    egui::ComboBox::from_id_salt("ascend_select")
                        .selected_text(&ascendancies[self.selected_ascend].name)
                        .show_ui(ui, |ui| {
                            for (i, asc) in ascendancies.iter().enumerate() {
                                ui.selectable_value(&mut self.selected_ascend, i, &asc.name);
                            }
                        });
                    if self.selected_ascend != prev_ascend {
                        ascend_changed = true;
                    }
                }

                // Secondary ascendancy (only on trees that support it).
                if !self.secondary_ascendancies.is_empty() {
                    let prev_sec = self.selected_secondary;
                    let selected_label = self
                        .secondary_ascendancies
                        .get(self.selected_secondary)
                        .map(|e| e.name.as_str())
                        .unwrap_or("None");
                    egui::ComboBox::from_id_salt("secondary_ascend_select")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for (i, asc) in self.secondary_ascendancies.iter().enumerate() {
                                ui.selectable_value(&mut self.selected_secondary, i, &asc.name);
                            }
                        });
                    if self.selected_secondary != prev_sec {
                        secondary_changed = true;
                    }
                }
            }

            ui.separator();

            // Loadouts: activate a matching tree/item/skill/config set combo
            ui.label("Loadout:");
            let selected_text = self.selected_loadout.as_deref().unwrap_or("-");
            egui::ComboBox::from_id_salt("loadout_select")
                .selected_text(selected_text)
                .width(140.0)
                .show_ui(ui, |ui| {
                    if self.loadouts.is_empty() {
                        ui.label(
                            egui::RichText::new("No loadouts")
                                .color(egui::Color32::from_rgb(150, 150, 150)),
                        );
                    }
                    for loadout in &self.loadouts {
                        let is_selected = self.selected_loadout.as_deref() == Some(loadout);
                        if ui.selectable_label(is_selected, loadout).clicked() && !is_selected {
                            activate_loadout = Some(loadout.clone());
                        }
                    }
                })
                .response
                .on_hover_text(
                    "A loadout switches the tree, items, skills, and config together. \
                     Sets with the same name (or a shared {tag} in their names) form a loadout.",
                );
            if ui
                .button("+")
                .on_hover_text(
                    "New loadout: creates a linked tree spec, item set, skill set, \
                     and config set with the same name",
                )
                .clicked()
            {
                new_loadout_clicked = true;
            }

            ui.separator();

            // Character level
            ui.label("Lv");
            let level_response = ui.add(
                egui::TextEdit::singleline(&mut self.char_level)
                    .desired_width(30.0)
                    .char_limit(3),
            );
            if let Ok(level) = self.char_level.parse::<u32>() {
                let tooltip = experience_multiplier_tooltip(bridge.lua(), level);
                if !tooltip.is_empty() {
                    level_response.clone().on_hover_text(tooltip);
                }
            }
            if level_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                header_changed = true;
            }

            let mode_label = if self.level_auto_mode {
                "Auto"
            } else {
                "Manual"
            };
            if ui.button(mode_label).clicked() {
                self.level_auto_mode = !self.level_auto_mode;
                header_changed = true;
            }
        });
        ui.separator();

        if request_save {
            self.request_save(bridge);
        }

        // Loadout actions
        if let Some(name) = activate_loadout {
            match loadouts::activate_loadout(bridge.lua(), &name) {
                Ok(true) => self.refresh_all(bridge),
                Ok(false) => log::warn!("Loadout '{name}' no longer matches"),
                Err(e) => log::error!("Failed to activate loadout: {e}"),
            }
        }
        if new_loadout_clicked {
            self.loadout_prompt = Some("New Loadout".to_string());
        }
        if let Some(mut name) = self.loadout_prompt.take() {
            let mut keep_open = true;
            egui::Window::new("New Loadout")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label("Enter a name for this loadout:");
                    let response = ui.text_edit_singleline(&mut name);
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        let create = ui
                            .add_enabled(!name.trim().is_empty(), egui::Button::new("Create"))
                            .clicked()
                            || (submitted && !name.trim().is_empty());
                        if create {
                            match loadouts::new_loadout(bridge.lua(), name.trim()) {
                                Ok(()) => self.refresh_all(bridge),
                                Err(e) => log::error!("Failed to create loadout: {e}"),
                            }
                            keep_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep_open = false;
                        }
                    });
                });
            if keep_open {
                self.loadout_prompt = Some(name);
            }
        }

        // Apply class/ascendancy changes
        if class_changed || ascend_changed {
            let class_id = self.classes[self.selected_class].class_id;
            let ascend_id = self.classes[self.selected_class].ascendancies[self.selected_ascend]
                .ascend_class_id;
            if let Err(e) = select_class_and_ascendancy(bridge, class_id, ascend_id) {
                log::error!("Failed to change class/ascendancy: {e}");
            } else {
                self.refresh_all(bridge);
            }
        }

        // Apply secondary ascendancy change
        if secondary_changed {
            let id = self
                .secondary_ascendancies
                .get(self.selected_secondary)
                .map(|e| e.id)
                .unwrap_or(0);
            if let Err(e) = select_secondary_ascendancy(bridge, id) {
                log::error!("Failed to change secondary ascendancy: {e}");
            } else {
                self.refresh_all(bridge);
            }
        }

        // Apply header changes (level)
        if header_changed {
            if let Err(e) = apply_char_header(bridge, self) {
                log::error!("Failed to apply header changes: {e}");
            } else {
                self.refresh_calc_output(bridge);
            }
        }

        // Layout: stat sidebar on the left, tab content on the right
        egui::SidePanel::left("stat_sidebar")
            .default_width(250.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                self.show_stat_sidebar(ui, bridge);
            });

        // Tab bar + content
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, BuildTab::Tree, "Tree");
                ui.selectable_value(&mut self.active_tab, BuildTab::Skills, "Skills");
                ui.selectable_value(&mut self.active_tab, BuildTab::Items, "Items");
                ui.selectable_value(&mut self.active_tab, BuildTab::Calcs, "Calcs");
                ui.selectable_value(&mut self.active_tab, BuildTab::Config, "Config");
                ui.selectable_value(&mut self.active_tab, BuildTab::Notes, "Notes");
                ui.selectable_value(&mut self.active_tab, BuildTab::Compare, "Compare");
                ui.selectable_value(&mut self.active_tab, BuildTab::Import, "Import/Export");
            });
            ui.separator();

            match self.active_tab {
                BuildTab::Tree => {
                    if let Some(ref mut tree) = self.tree_panel {
                        let changed = tree.show(ui, bridge);
                        let rebuild = tree.request_rebuild;
                        let full_refresh = tree.request_full_refresh;
                        tree.request_full_refresh = false;
                        if tree.request_items_tab {
                            tree.request_items_tab = false;
                            self.active_tab = BuildTab::Items;
                        }
                        if full_refresh {
                            // A node click switched class/ascendancy: rebuild
                            // everything and resync the header dropdowns
                            self.refresh_all(bridge);
                        } else if changed {
                            self.refresh_calc_output(bridge);
                        }
                        if rebuild && !full_refresh {
                            // Active spec switched: tree data, sprites, and
                            // jewel sockets may all have changed
                            self.tree_panel = Some(TreePanel::new(bridge.lua()));
                            self.items_panel = Some(ItemsPanel::new(bridge.lua()));
                        }
                    }
                }
                BuildTab::Skills => {
                    if let Some(ref mut skills) = self.skills_panel
                        && skills.show(ui, bridge)
                    {
                        // Rebuild the panel but keep the selected socket group
                        let selected = skills.selection();
                        self.refresh_calc_output(bridge);
                        let mut panel = SkillsPanel::new(bridge.lua());
                        panel.restore_selection(selected);
                        self.skills_panel = Some(panel);
                    }
                }
                BuildTab::Items => {
                    if let Some(ref mut items) = self.items_panel
                        && items.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                        // Rebuild the panel but keep the DB browser and the
                        // set-manager UI (their open windows survive changes)
                        let carried = self
                            .items_panel
                            .take()
                            .map(|p| (p.item_db, p.sets_ui, p.craft_ui));
                        let mut panel = ItemsPanel::new(bridge.lua());
                        if let Some((db, sets_ui, craft_ui)) = carried {
                            panel.item_db = db;
                            panel.sets_ui = sets_ui;
                            panel.craft_ui = craft_ui;
                        }
                        self.items_panel = Some(panel);
                        // Items can grant skills and socket jewels into the tree;
                        // cluster jewels add/remove whole subgraphs of nodes
                        self.skills_panel = Some(SkillsPanel::new(bridge.lua()));
                        if let Some(ref mut tree) = self.tree_panel {
                            tree.refresh_tree_data(bridge);
                            tree.refresh_jewels(bridge);
                        }
                    }
                }
                BuildTab::Calcs => {
                    if self.calcs_panel.is_none() {
                        self.calcs_panel = Some(CalcsPanel::new(bridge.lua()));
                        self.calcs_stale = false;
                    } else if self.calcs_stale {
                        if let Some(ref mut calcs) = self.calcs_panel {
                            calcs.refresh(bridge.lua());
                        }
                        self.calcs_stale = false;
                    }
                    if let Some(ref mut calcs) = self.calcs_panel
                        && calcs.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                        // The panel refreshed itself; don't do it again
                        self.calcs_stale = false;
                    }
                }
                BuildTab::Config => {
                    if let Some(ref mut config) = self.config_panel
                        && config.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                    }
                }
                BuildTab::Notes => {
                    if let Some(ref mut notes) = self.notes_panel {
                        notes.show(ui, bridge);
                    }
                }
                BuildTab::Compare => {
                    if self.compare_panel.show(ui, bridge) {
                        // Copy-to-primary actions (or a finished power run)
                        // touched the primary build: refresh everything
                        self.refresh_all(bridge);
                    }
                }
                BuildTab::Import => {
                    if self.import_panel.show(ui, bridge) {
                        // Build was imported - refresh everything
                        if self.import_panel.new_build_imported {
                            self.import_panel.new_build_imported = false;
                            self.build_name = "Imported Build".to_string();
                            self.is_unsaved_new = true;
                        }
                        self.refresh_all(bridge);
                    }
                }
            }
        });

        // Transient toast (top right, ~2.5s)
        if let Some((msg, is_error, shown_at)) = &self.toast {
            if shown_at.elapsed().as_secs_f32() > 2.5 {
                self.toast = None;
            } else {
                let color = if *is_error {
                    egui::Color32::from_rgb(230, 90, 90)
                } else {
                    egui::Color32::from_rgb(120, 220, 120)
                };
                egui::Area::new(egui::Id::new("build_toast"))
                    .anchor(egui::Align2::RIGHT_TOP, [-16.0, 40.0])
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.colored_label(color, msg);
                        });
                    });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }
        }

        // Pinned calc breakdowns float above every tab (upstream's overlay
        // calc panes); keep them fresh when calcs changed on another tab
        if let Some(ref mut calcs) = self.calcs_panel
            && calcs.has_pinned()
        {
            if self.calcs_stale {
                calcs.refresh(bridge.lua());
                self.calcs_stale = false;
            }
            calcs.show_pinned_windows(ui.ctx());
        }

        go_back
    }

    fn refresh_calc_output(&mut self, bridge: &LuaBridge) {
        match CalcOutput::extract(bridge.lua()) {
            Ok(output) => {
                self.calc_output = Some(output);
            }
            Err(e) => {
                log::error!("Failed to refresh calc output: {e}");
            }
        }
        match display_stats::extract_sidebar_stats(bridge.lua()) {
            Ok(stats) => self.sidebar_stats = Some(stats),
            Err(e) => log::error!("Failed to refresh sidebar stats: {e}"),
        }
        match loadouts::list_loadouts(bridge.lua()) {
            Ok((list, selected)) => {
                self.loadouts = list;
                self.selected_loadout = selected;
            }
            Err(e) => log::error!("Failed to refresh loadouts: {e}"),
        }
        self.calcs_stale = true;
    }

    /// Refresh all panels after a major change (e.g., build import).
    fn refresh_all(&mut self, bridge: &LuaBridge) {
        self.refresh_calc_output(bridge);
        self.config_panel = Some(ConfigPanel::new(bridge.lua()));
        self.tree_panel = Some(TreePanel::new(bridge.lua()));
        self.items_panel = Some(ItemsPanel::new(bridge.lua()));
        self.skills_panel = Some(SkillsPanel::new(bridge.lua()));
        self.calcs_panel = None; // recreated lazily on next visit
        self.notes_panel = Some(NotesPanel::new(bridge.lua()));
        // The import may have changed class/ascendancy/level: resync the
        // header dropdowns with the Lua state
        let (selected_class, selected_ascend) = find_current_selection(bridge.lua(), &self.classes);
        self.selected_class = selected_class;
        self.selected_ascend = selected_ascend;
        self.secondary_ascendancies = load_secondary_ascendancies(bridge.lua());
        self.selected_secondary =
            find_secondary_selection(bridge.lua(), &self.secondary_ascendancies);
        let header = load_char_header(bridge.lua());
        self.char_level = header.level.to_string();
        self.level_auto_mode = header.level_auto;
    }

    fn show_stat_sidebar(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) {
        // Main skill selection
        ui.strong("Main Skill");

        let skill_info = load_skill_selection(bridge.lua());

        if !skill_info.socket_groups.is_empty() {
            // Socket group dropdown
            let mut group_idx = skill_info.selected_group;
            egui::ComboBox::from_id_salt("main_socket_group")
                .selected_text(&skill_info.socket_groups[group_idx])
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for (i, name) in skill_info.socket_groups.iter().enumerate() {
                        ui.selectable_value(&mut group_idx, i, name);
                    }
                });
            if group_idx != skill_info.selected_group {
                let _ = set_main_socket_group(bridge.lua(), group_idx);
                self.refresh_calc_output(bridge);
            }

            // Active skill dropdown (within selected group)
            if !skill_info.active_skills.is_empty() {
                let mut skill_idx = skill_info.selected_skill;
                egui::ComboBox::from_id_salt("main_active_skill")
                    .selected_text(&skill_info.active_skills[skill_idx])
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in skill_info.active_skills.iter().enumerate() {
                            ui.selectable_value(&mut skill_idx, i, name);
                        }
                    });
                if skill_idx != skill_info.selected_skill {
                    let _ = set_main_active_skill(bridge.lua(), skill_idx);
                    self.refresh_calc_output(bridge);
                }
            }

            // Skill part dropdown (only if skill has multiple parts)
            if skill_info.skill_parts.len() > 1 {
                let mut part_idx = skill_info.selected_part;
                egui::ComboBox::from_id_salt("main_skill_part")
                    .selected_text(&skill_info.skill_parts[part_idx])
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in skill_info.skill_parts.iter().enumerate() {
                            ui.selectable_value(&mut part_idx, i, name);
                        }
                    });
                if part_idx != skill_info.selected_part {
                    let _ = set_skill_part(bridge.lua(), part_idx);
                    self.refresh_calc_output(bridge);
                }
            }

            // Stage count (for multi-stage skills)
            if skill_info.show_stages {
                ui.horizontal(|ui| {
                    ui.label("Stages:");
                    let cur: i32 = skill_info.stage_count.parse().unwrap_or(1);
                    let mut buf = skill_info.stage_count.clone();
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut buf)
                            .desired_width(40.0)
                            .char_limit(4),
                    );
                    let mut changed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("-").clicked() {
                        buf = (cur - 1).max(1).to_string();
                        changed = true;
                    }
                    if ui.button("+").clicked() {
                        buf = (cur + 1).to_string();
                        changed = true;
                    }
                    if changed {
                        let _ = set_stage_count(bridge.lua(), &buf);
                        self.refresh_calc_output(bridge);
                    }
                });
            }

            // Minion type dropdown
            if skill_info.show_minions && !skill_info.minion_labels.is_empty() {
                let mut minion_idx = skill_info.selected_minion;
                egui::ComboBox::from_id_salt("main_minion")
                    .selected_text(
                        skill_info
                            .minion_labels
                            .get(minion_idx)
                            .cloned()
                            .unwrap_or_else(|| "—".to_string()),
                    )
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in skill_info.minion_labels.iter().enumerate() {
                            ui.selectable_value(&mut minion_idx, i, name);
                        }
                    });
                if minion_idx != skill_info.selected_minion
                    && let Some(id) = skill_info.minion_ids.get(minion_idx)
                {
                    let _ = set_minion(bridge.lua(), id);
                    self.refresh_calc_output(bridge);
                }
            }

            // Manage Spectres (upstream OpenSpectreLibrary popup)
            if skill_info.show_spectre_library && ui.button("Manage Spectres…").clicked() {
                match (
                    spectres::list_in_build(bridge.lua()),
                    spectres::list_available(bridge.lua()),
                ) {
                    (Ok(dest), Ok(available)) => {
                        self.spectre_library = Some(SpectreLibrary {
                            dest,
                            available,
                            search: String::new(),
                            search_mode: 0,
                            tooltip_cache: HashMap::new(),
                        });
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        log::error!("Failed to open spectre library: {e}")
                    }
                }
            }
            self.show_spectre_library(ui, bridge);

            // Minion skill dropdown
            if skill_info.show_minions && !skill_info.minion_skills.is_empty() {
                let mut skill_idx = skill_info.selected_minion_skill;
                egui::ComboBox::from_id_salt("main_minion_skill")
                    .selected_text(
                        skill_info
                            .minion_skills
                            .get(skill_idx)
                            .cloned()
                            .unwrap_or_else(|| "—".to_string()),
                    )
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in skill_info.minion_skills.iter().enumerate() {
                            ui.selectable_value(&mut skill_idx, i, name);
                        }
                    });
                if skill_idx != skill_info.selected_minion_skill {
                    let _ = set_minion_skill(bridge.lua(), skill_idx);
                    self.refresh_calc_output(bridge);
                }
            }

            // Mine count (for mine skills)
            if skill_info.show_mines {
                ui.horizontal(|ui| {
                    ui.label("Active Mines:");
                    let cur: i32 = skill_info.mine_count.parse().unwrap_or(1);
                    let mut buf = skill_info.mine_count.clone();
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut buf)
                            .desired_width(40.0)
                            .char_limit(4),
                    );
                    let mut changed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("-").clicked() {
                        buf = (cur - 1).max(1).to_string();
                        changed = true;
                    }
                    if ui.button("+").clicked() {
                        buf = (cur + 1).to_string();
                        changed = true;
                    }
                    if changed {
                        let _ = set_mine_count(bridge.lua(), &buf);
                        self.refresh_calc_output(bridge);
                    }
                });
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Stats");
            // Compact large numbers (upstream main.useCompactValues; the
            // stat list is rebuilt by upstream FormatStat on the next frame)
            let mut compact = bridge
                .lua()
                .load("return mainObject_ref.main.useCompactValues == true")
                .eval()
                .unwrap_or(false);
            if ui
                .checkbox(&mut compact, "Compact")
                .on_hover_text("Compact large numbers (e.g. 12.3K)")
                .changed()
            {
                let result = bridge
                    .lua()
                    .load(
                        r#"
                        mainObject_ref.main.useCompactValues = ...
                        _runCallback('OnFrame')
                    "#,
                    )
                    .call::<()>(compact);
                match result {
                    Ok(()) => self.refresh_calc_output(bridge),
                    Err(e) => log::error!("Failed to toggle compact values: {e}"),
                }
            }
        });
        ui.separator();

        // Warnings bar (always visible above the scrolling stat list)
        if let Some(ref stats) = self.sidebar_stats
            && !stats.warnings.is_empty()
        {
            let header = egui::RichText::new(format!(
                "⚠ {} warning{}",
                stats.warnings.len(),
                if stats.warnings.len() == 1 { "" } else { "s" }
            ))
            .color(egui::Color32::from_rgb(255, 100, 80));
            egui::CollapsingHeader::new(header)
                .id_salt("build_warnings")
                .show(ui, |ui| {
                    for warning in &stats.warnings {
                        // Warnings may carry PoB color codes
                        ui.label(super::theme::pob_layout_job(
                            warning,
                            13.0,
                            egui::Color32::from_rgb(255, 100, 80),
                        ));
                    }
                });
            ui.separator();
        }

        // Do not shrink horizontally to the content. The stat rows are only as
        // wide as their text, so with egui's default auto-shrink the scroll
        // area collapses to that width and draws the scrollbar at the end of
        // the text rather than at the panel edge - where it then sits still
        // while the panel is resized around it.
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                match self.sidebar_stats {
                    Some(ref stats) if !stats.lines.is_empty() => {
                        show_stat_lines(ui, stats);
                    }
                    // Fallback: hardcoded key-stat table from raw calc output
                    _ => {
                        if let Some(ref output) = self.calc_output {
                            super::show_stat_table(ui, output);
                        } else {
                            ui.label("No calc output available.");
                        }
                    }
                }
            });
    }
}

/// Render the upstream-formatted sidebar stat lines.
fn show_stat_lines(ui: &mut egui::Ui, stats: &SidebarStats) {
    use super::theme;

    // Tighten the list: rows are floored at `interact_size.y` (egui's default
    // 18 leaves ~5px of air around ~15px of text), so the floor, not the gap,
    // is what spreads the list out. At 14 the floor is below the text height
    // of the larger lines, leaving effectively no padding around them.
    ui.spacing_mut().item_spacing.y = 0.5;
    ui.spacing_mut().interact_size.y = 14.0;

    // Width of the right-aligned label column, measured from the widest label
    // that has a value beside it. Upstream hardcodes x=170 in a 300px control;
    // our sidebar is resizable, so measure and cap at a share of the panel to
    // leave room for the value.
    let avail = ui.available_width();
    let label_width = stats
        .lines
        .iter()
        .filter(|l| !l.is_spacer() && !l.center && l.rhs.is_some())
        .filter_map(|l| l.lhs.as_ref().map(|lhs| (lhs, l.height)))
        .map(|(lhs, height)| {
            let size = if height <= 14.0 { 11.0 } else { 13.0 };
            let job = theme::pob_layout_job(lhs, size, egui::Color32::WHITE);
            ui.fonts(|f| f.layout_job(job)).rect.width()
        })
        .fold(0.0_f32, f32::max)
        + theme::LABEL_GAP;
    let label_width = label_width.clamp(40.0, (avail * 0.7).max(40.0));

    for line in &stats.lines {
        if line.is_spacer() {
            // Section separator (upstream emits height-6 breaks between the
            // DPS, cost, attribute, defence, resistance, ... blocks); render
            // it as a visible empty line.
            ui.add_space(line.height * 0.75);
            continue;
        }
        let size = if line.height <= 14.0 { 11.0 } else { 13.0 };
        if line.center {
            // Centered annotation line (skill part, DPS source, info message)
            if let Some(ref lhs) = line.lhs {
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Label::new(theme::pob_layout_job(lhs, size, egui::Color32::GRAY))
                            .truncate(),
                    );
                });
            }
            continue;
        }
        match (&line.lhs, &line.rhs) {
            // Label right-aligned in the shared column, value left-aligned
            // just after it, matching upstream's TextListControl columns
            // ({x=170, RIGHT_X} then {x=174, LEFT}).
            (lhs, Some(rhs)) => {
                let label_job = lhs
                    .as_ref()
                    .map(|l| theme::pob_layout_job(l, size, egui::Color32::WHITE))
                    .unwrap_or_default();
                theme::right_aligned_row(ui, label_width, label_job, None, |ui| {
                    ui.add(
                        egui::Label::new(theme::pob_layout_job(rhs, size, egui::Color32::WHITE))
                            .truncate(),
                    );
                });
            }
            // Label-only lines (headers, notes) keep the full panel width.
            (Some(lhs), None) => {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(theme::pob_layout_job(lhs, size, egui::Color32::WHITE))
                            .truncate(),
                    );
                });
            }
            (None, None) => {}
        }
    }
}

/// Load the list of classes and their ascendancies from Lua.
fn load_class_list(lua: &mlua::Lua) -> Vec<ClassEntry> {
    let result: Result<mlua::Table, _> = lua
        .load(
            r#"
        local spec = mainObject_ref.main.modes['BUILD'].spec
        local classes = spec.tree.classes
        local result = {}
        for classId, class in pairs(classes) do
            local ascendancies = {}
            for i = 0, #class.classes do
                local asc = class.classes[i]
                table.insert(ascendancies, {
                    ascendClassId = i,
                    name = asc.name,
                })
            end
            table.insert(result, {
                classId = classId,
                name = class.name,
                ascendancies = ascendancies,
            })
        end
        table.sort(result, function(a, b) return a.name < b.name end)
        return result
    "#,
        )
        .eval();

    let Ok(table) = result else {
        log::error!("Failed to load class list from Lua");
        return Vec::new();
    };

    let mut classes = Vec::new();
    for entry in table.sequence_values::<mlua::Table>() {
        let Ok(t) = entry else { continue };
        let class_id: u32 = t.get("classId").unwrap_or(0);
        let name: String = t.get("name").unwrap_or_default();
        let ascendancies_table: mlua::Table = match t.get("ascendancies") {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut ascendancies = Vec::new();
        for asc_entry in ascendancies_table.sequence_values::<mlua::Table>() {
            let Ok(asc) = asc_entry else { continue };
            ascendancies.push(AscendEntry {
                ascend_class_id: asc.get("ascendClassId").unwrap_or(0),
                name: asc.get("name").unwrap_or_default(),
            });
        }
        classes.push(ClassEntry {
            class_id,
            name,
            ascendancies,
        });
    }
    classes
}

/// Find the currently selected class and ascendancy indices.
fn find_current_selection(lua: &mlua::Lua, classes: &[ClassEntry]) -> (usize, usize) {
    let cur_class_id: u32 = lua
        .load("return mainObject_ref.main.modes['BUILD'].spec.curClassId")
        .eval()
        .unwrap_or(0);
    let cur_ascend_id: u32 = lua
        .load("return mainObject_ref.main.modes['BUILD'].spec.curAscendClassId")
        .eval()
        .unwrap_or(0);

    let class_idx = classes
        .iter()
        .position(|c| c.class_id == cur_class_id)
        .unwrap_or(0);
    let ascend_idx = classes
        .get(class_idx)
        .and_then(|c| {
            c.ascendancies
                .iter()
                .position(|a| a.ascend_class_id == cur_ascend_id)
        })
        .unwrap_or(0);
    (class_idx, ascend_idx)
}

/// Load secondary (alternate) ascendancies for the current tree version.
/// Returns empty vec if the tree doesn't support them.
fn load_secondary_ascendancies(lua: &mlua::Lua) -> Vec<SecondaryAscendEntry> {
    let result: Result<mlua::Table, _> = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            local out = {}
            if spec.tree and spec.tree.alternate_ascendancies then
                for i, asc in ipairs(spec.tree.alternate_ascendancies) do
                    table.insert(out, { id = i, name = asc.name or ("Ascendancy "..i) })
                end
            end
            return out
            "#,
        )
        .eval();

    let Ok(table) = result else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for pair in table.sequence_values::<mlua::Table>() {
        let Ok(t) = pair else { continue };
        entries.push(SecondaryAscendEntry {
            id: t.get("id").unwrap_or(0),
            name: t.get("name").unwrap_or_default(),
        });
    }
    // If there are alternates, prepend a synthetic "None" entry (id 0).
    if !entries.is_empty() {
        entries.insert(
            0,
            SecondaryAscendEntry {
                id: 0,
                name: "None".to_string(),
            },
        );
    }
    entries
}

fn find_secondary_selection(lua: &mlua::Lua, entries: &[SecondaryAscendEntry]) -> usize {
    let cur: u32 = lua
        .load("return mainObject_ref.main.modes['BUILD'].spec.curSecondaryAscendClassId or 0")
        .eval()
        .unwrap_or(0);
    entries.iter().position(|e| e.id == cur).unwrap_or(0)
}

fn select_secondary_ascendancy(
    bridge: &LuaBridge,
    ascend_class_id: u32,
) -> Result<(), mlua::Error> {
    bridge
        .lua()
        .load(format!(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            build.spec:SelectSecondaryAscendClass({ascend_class_id})
            build.spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
            "#
        ))
        .exec()
}

/// Compute the experience multiplier tooltip for a given player level.
///
/// Reads `data.monsterExperienceLevelMap` from Lua and applies upstream's formula
/// (see upstream `Modules/Build.lua` tooltipFunc for `characterLevel`).
fn experience_multiplier_tooltip(lua: &mlua::Lua, player_level: u32) -> String {
    if !(1..=100).contains(&player_level) {
        return String::new();
    }
    let level_map: Vec<u32> = match lua
        .load(
            r#"
            local m = mainObject_ref.main.modes['BUILD'].data and mainObject_ref.main.modes['BUILD'].data.monsterExperienceLevelMap
            if not m then return {} end
            local out = {}
            for i, lvl in ipairs(m) do out[i] = lvl end
            return out
            "#,
        )
        .eval::<mlua::Table>()
    {
        Ok(t) => t.sequence_values::<u32>().filter_map(|r| r.ok()).collect(),
        Err(_) => return String::new(),
    };
    if level_map.is_empty() {
        return String::new();
    }

    let pl = player_level as f64;
    let safe_zone = 3.0 + (pl / 16.0).floor();
    let mut lines = vec!["Experience multiplier:".to_string()];
    for (idx, &exp_level) in level_map.iter().enumerate() {
        let area_level = (idx + 1) as u32;
        let diff = (pl - exp_level as f64).abs() - safe_zone;
        let mut mult = if diff <= 0.0 {
            1.0
        } else {
            ((pl + 5.0) / (pl + 5.0 + diff.powf(2.5))).powf(1.5)
        };
        if player_level >= 95 {
            let penalties = [0.935, 0.885, 0.813, 0.7175, 0.6];
            let p = penalties
                .get((player_level - 95) as usize)
                .copied()
                .unwrap_or(0.0);
            mult *= (1.0 / (1.0 + 0.1 * (pl - 94.0))) * p;
        }
        if mult > 0.01 {
            let mut line = area_level.to_string();
            if area_level >= 68 {
                line = format!("{line} (Tier {})", area_level - 67);
            }
            lines.push(format!("{line}: {:.1}%", mult * 100.0));
        }
    }
    lines.join("\n")
}

/// Change the class and ascendancy in Lua.
fn select_class_and_ascendancy(
    bridge: &LuaBridge,
    class_id: u32,
    ascend_class_id: u32,
) -> Result<(), mlua::Error> {
    bridge
        .lua()
        .load(format!(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        spec:SelectClass({class_id})
        spec:SelectAscendClass({ascend_class_id})
        spec:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
        ))
        .exec()
}

struct CharHeader {
    level: u32,
    level_auto: bool,
}

fn load_char_header(lua: &mlua::Lua) -> CharHeader {
    let level: u32 = lua
        .load("return mainObject_ref.main.modes['BUILD'].characterLevel")
        .eval()
        .unwrap_or(1);
    let level_auto: bool = lua
        .load("return mainObject_ref.main.modes['BUILD'].characterLevelAutoMode")
        .eval()
        .unwrap_or(true);

    CharHeader { level, level_auto }
}

/// Get points used/max for display.
fn get_points_display(lua: &mlua::Lua) -> (u32, u32, u32, u32) {
    let result: Result<mlua::Table, _> = lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local used, ascUsed = build.spec:CountAllocNodes()
        local extra = (build.calcsTab and build.calcsTab.mainOutput and build.calcsTab.mainOutput.ExtraPoints) or 0
        return { used = used, max = 99 + 23 + extra, ascUsed = ascUsed, ascMax = 8 }
    "#,
    ).eval();

    match result {
        Ok(t) => (
            t.get("used").unwrap_or(0),
            t.get("max").unwrap_or(122),
            t.get("ascUsed").unwrap_or(0),
            t.get("ascMax").unwrap_or(8),
        ),
        Err(e) => {
            log::error!("Failed to get points display: {e}");
            (0, 122, 0, 8)
        }
    }
}

/// Apply character header values to Lua and trigger recalc.
fn apply_char_header(bridge: &LuaBridge, view: &BuildView) -> Result<(), mlua::Error> {
    let level: u32 = view.char_level.parse().unwrap_or(1).clamp(1, 100);
    let auto_mode = view.level_auto_mode;

    bridge
        .lua()
        .load(format!(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.characterLevel = {level}
        build.characterLevelAutoMode = {auto_mode}
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
        ))
        .exec()
}

struct SkillSelectionInfo {
    socket_groups: Vec<String>,
    selected_group: usize,
    active_skills: Vec<String>,
    selected_skill: usize,
    skill_parts: Vec<String>,
    selected_part: usize,
    /// Show stage count input (for multi-stage skills or parts with stages)
    show_stages: bool,
    stage_count: String,
    /// Show mine count input (for mine skills)
    show_mines: bool,
    mine_count: String,
    /// Minion type dropdown (e.g. for Raise Zombie, Summon Skeletons).
    show_minions: bool,
    minion_labels: Vec<String>,
    minion_ids: Vec<String>,
    selected_minion: usize,
    /// Minion skill dropdown (selects which skill the minion uses).
    minion_skills: Vec<String>,
    selected_minion_skill: usize,
    /// "Manage Spectres..." button shown for spectres (minion list is user-curated).
    show_spectre_library: bool,
}

fn load_skill_selection(lua: &mlua::Lua) -> SkillSelectionInfo {
    let result: Result<mlua::Table, _> = lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local groupList = build.skillsTab.socketGroupList
        local groups = {}
        for i, group in ipairs(groupList) do
            local label = group.displayLabel or ("Group " .. i)
            table.insert(groups, label)
        end
        local selGroup = build.mainSocketGroup or 1
        local activeSkills = {}
        local selSkill = 1
        local skillParts = {}
        local selPart = 1
        local showStages = false
        local stageCount = ""
        local showMines = false
        local mineCount = ""
        local showMinions = false
        local minionLabels = {}
        local minionIds = {}
        local selMinion = 1
        local minionSkills = {}
        local selMinionSkill = 1
        local showSpectreLibrary = false
        if selGroup >= 1 and selGroup <= #groupList then
            local sg = groupList[selGroup]
            if sg.displaySkillList then
                for i, skill in ipairs(sg.displaySkillList) do
                    local name = skill.activeEffect and skill.activeEffect.grantedEffect and skill.activeEffect.grantedEffect.name or ("Skill " .. i)
                    table.insert(activeSkills, name)
                end
            end
            selSkill = sg.mainActiveSkill or 1
            if selSkill >= 1 and selSkill <= #activeSkills and sg.displaySkillList and sg.displaySkillList[selSkill] then
                local skill = sg.displaySkillList[selSkill]
                local ae = skill.activeEffect
                if ae and ae.grantedEffect then
                    local ge = ae.grantedEffect
                    local src = ae.srcInstance
                    -- Parts
                    if ge.parts then
                        for _, part in ipairs(ge.parts) do
                            table.insert(skillParts, part.name or "?")
                        end
                        selPart = src and src.skillPart or 1
                        -- Stage count from part
                        local selPartData = ge.parts[selPart]
                        if selPartData and selPartData.stages then
                            showStages = true
                            stageCount = tostring(src and src.skillStageCount or selPartData.stagesMin or 1)
                        end
                    end
                    -- Mine count
                    if skill.skillFlags and skill.skillFlags.mine then
                        showMines = true
                        mineCount = tostring(src and src.skillMineCount or "")
                    end
                    -- Multi-stage without parts
                    if not showStages and skill.skillFlags and skill.skillFlags.multiStage
                       and not (ge.parts and #ge.parts > 1) then
                        showStages = true
                        stageCount = tostring(src and src.skillStageCount or skill.skillData and skill.skillData.stagesMin or 1)
                    end
                    -- Minion list
                    local minionList = skill.minionList
                    local hasMinions = minionList and minionList[1]
                    local minionListFromGE = ge.minionList
                    if (hasMinions or minionListFromGE) and not (skill.skillFlags and skill.skillFlags.disable) then
                        showMinions = true
                        -- Spectre library button shows when skill has a minionList hook but no bound minions yet (e.g. Raise Spectre).
                        showSpectreLibrary = minionListFromGE and not (minionListFromGE[1])
                        local dataMinions = mainObject_ref.main.modes['BUILD'].data and mainObject_ref.main.modes['BUILD'].data.minions
                        if hasMinions and dataMinions then
                            for _, mid in ipairs(minionList) do
                                local mdata = dataMinions[mid]
                                table.insert(minionLabels, mdata and mdata.name or mid)
                                table.insert(minionIds, mid)
                            end
                            local curMinion = src and src.skillMinion
                            for i, mid in ipairs(minionIds) do
                                if mid == curMinion then selMinion = i break end
                            end
                        end
                        -- Active minion's skills
                        if skill.minion and skill.minion.activeSkillList then
                            for _, ms in ipairs(skill.minion.activeSkillList) do
                                local nm = ms.activeEffect and ms.activeEffect.grantedEffect and ms.activeEffect.grantedEffect.name or "?"
                                table.insert(minionSkills, nm)
                            end
                            selMinionSkill = src and src.skillMinionSkill or 1
                        end
                    end
                end
            end
        end
        return {
            groups = groups,
            selGroup = selGroup,
            activeSkills = activeSkills,
            selSkill = selSkill,
            skillParts = skillParts,
            selPart = selPart,
            showStages = showStages,
            stageCount = stageCount,
            showMines = showMines,
            mineCount = mineCount,
            showMinions = showMinions,
            minionLabels = minionLabels,
            minionIds = minionIds,
            selMinion = selMinion,
            minionSkills = minionSkills,
            selMinionSkill = selMinionSkill,
            showSpectreLibrary = showSpectreLibrary,
        }
    "#,
    ).eval();

    let Ok(t) = result else {
        return SkillSelectionInfo {
            socket_groups: Vec::new(),
            selected_group: 0,
            active_skills: Vec::new(),
            selected_skill: 0,
            skill_parts: Vec::new(),
            selected_part: 0,
            show_stages: false,
            stage_count: String::new(),
            show_mines: false,
            mine_count: String::new(),
            show_minions: false,
            minion_labels: Vec::new(),
            minion_ids: Vec::new(),
            selected_minion: 0,
            minion_skills: Vec::new(),
            selected_minion_skill: 0,
            show_spectre_library: false,
        };
    };

    let groups: Vec<String> = t
        .get::<mlua::Table>("groups")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let sel_group: usize = t.get::<u32>("selGroup").unwrap_or(1) as usize;
    let active_skills: Vec<String> = t
        .get::<mlua::Table>("activeSkills")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let sel_skill: usize = t.get::<u32>("selSkill").unwrap_or(1) as usize;
    let skill_parts: Vec<String> = t
        .get::<mlua::Table>("skillParts")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let sel_part: usize = t.get::<u32>("selPart").unwrap_or(1) as usize;
    let show_stages: bool = t.get("showStages").unwrap_or(false);
    let stage_count: String = t.get("stageCount").unwrap_or_default();
    let show_mines: bool = t.get("showMines").unwrap_or(false);
    let mine_count: String = t.get("mineCount").unwrap_or_default();
    let show_minions: bool = t.get("showMinions").unwrap_or(false);
    let minion_labels: Vec<String> = t
        .get::<mlua::Table>("minionLabels")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let minion_ids: Vec<String> = t
        .get::<mlua::Table>("minionIds")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let sel_minion: usize = t.get::<u32>("selMinion").unwrap_or(1) as usize;
    let minion_skills: Vec<String> = t
        .get::<mlua::Table>("minionSkills")
        .map(|tbl| {
            tbl.sequence_values::<String>()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();
    let sel_minion_skill: usize = t.get::<u32>("selMinionSkill").unwrap_or(1) as usize;
    let show_spectre_library: bool = t.get("showSpectreLibrary").unwrap_or(false);

    // Lua indices are 1-based, convert to 0-based
    SkillSelectionInfo {
        selected_group: sel_group
            .saturating_sub(1)
            .min(groups.len().saturating_sub(1)),
        socket_groups: groups,
        selected_skill: sel_skill
            .saturating_sub(1)
            .min(active_skills.len().saturating_sub(1)),
        active_skills,
        selected_part: sel_part
            .saturating_sub(1)
            .min(skill_parts.len().saturating_sub(1)),
        skill_parts,
        show_stages,
        stage_count,
        show_mines,
        mine_count,
        show_minions,
        selected_minion: sel_minion
            .saturating_sub(1)
            .min(minion_labels.len().saturating_sub(1)),
        selected_minion_skill: sel_minion_skill
            .saturating_sub(1)
            .min(minion_skills.len().saturating_sub(1)),
        minion_labels,
        minion_ids,
        minion_skills,
        show_spectre_library,
    }
}

fn set_main_socket_group(lua: &mlua::Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1; // 1-based
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.mainSocketGroup = {lua_idx}
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

fn set_main_active_skill(lua: &mlua::Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        sg.mainActiveSkill = {lua_idx}
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

fn set_skill_part(lua: &mlua::Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        local skill = sg.displaySkillList[sg.mainActiveSkill]
        skill.activeEffect.srcInstance.skillPart = {lua_idx}
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

fn set_stage_count(lua: &mlua::Lua, count: &str) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        local skill = sg.displaySkillList[sg.mainActiveSkill]
        skill.activeEffect.srcInstance.skillStageCount = tonumber("{count}") or 1
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

fn set_mine_count(lua: &mlua::Lua, count: &str) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        local skill = sg.displaySkillList[sg.mainActiveSkill]
        skill.activeEffect.srcInstance.skillMineCount = tonumber("{count}")
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

fn set_minion(lua: &mlua::Lua, minion_id: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        local skill = sg.displaySkillList[sg.mainActiveSkill]
        skill.activeEffect.srcInstance.skillMinion = ...
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
        "#,
    )
    .call::<()>(minion_id)
}

fn set_minion_skill(lua: &mlua::Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local sg = build.skillsTab.socketGroupList[build.mainSocketGroup]
        local skill = sg.displaySkillList[sg.mainActiveSkill]
        skill.activeEffect.srcInstance.skillMinionSkill = {lua_idx}
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

/// Attach the spectre tooltip (upstream MinionListControl:AddValueTooltip)
/// to a response, computing and caching the lines on first hover.
fn spectre_hover(
    resp: egui::Response,
    cache: &mut HashMap<String, Vec<TooltipLine>>,
    bridge: &LuaBridge,
    id: &str,
) {
    if !resp.hovered() {
        return;
    }
    if !cache.contains_key(id) {
        let lines = spectres::spectre_tooltip(bridge.lua(), id).unwrap_or_else(|e| {
            log::error!("Spectre tooltip failed: {e}");
            Vec::new()
        });
        cache.insert(id.to_string(), lines);
    }
    let lines = &cache[id];
    if !lines.is_empty() {
        resp.on_hover_ui(|ui| show_tooltip_lines(ui, lines));
    }
}

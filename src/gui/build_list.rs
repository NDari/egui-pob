//! Build list panel: displays saved builds, allows opening and managing them
//! (delete, rename, move to folder, new folder, sort, search).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pob_egui::data::build_list::{self, BuildEntry, BuildInfo, BuildPreview, FolderInfo, SortMode};
use pob_egui::lua_bridge::LuaBridge;

/// Modal popup state for build management actions.
enum Popup {
    ConfirmDelete {
        path: PathBuf,
        name: String,
        is_folder: bool,
    },
    Rename {
        path: PathBuf,
        is_folder: bool,
        name: String,
        error: Option<String>,
    },
    NewFolder {
        name: String,
        error: Option<String>,
    },
    Error(String),
}

/// Deferred row interaction, resolved after the entry loop.
enum RowAction {
    Open(BuildInfo),
    Enter(String),
    ConfirmDelete {
        path: PathBuf,
        name: String,
        is_folder: bool,
    },
    Rename {
        path: PathBuf,
        name: String,
        is_folder: bool,
    },
    Move {
        path: PathBuf,
        dest: PathBuf,
    },
}

/// State for the build list panel.
pub struct BuildListPanel {
    pub entries: Vec<BuildEntry>,
    pub sub_path: String,
    build_path: String,
    sort_mode: SortMode,
    /// The mode `entries` is currently ordered by. Sorting calls into Lua for
    /// upstream's comparator, so it runs when the list or mode changes rather
    /// than every frame.
    sorted_as: Option<SortMode>,
    filter: String,
    popup: Option<Popup>,
    /// Hover preview data, parsed from build XMLs on first hover.
    preview_cache: HashMap<PathBuf, BuildPreview>,
}

impl BuildListPanel {
    pub fn new(build_path: String) -> Self {
        let mut panel = Self {
            entries: Vec::new(),
            sub_path: String::new(),
            build_path,
            sort_mode: SortMode::Name,
            sorted_as: None,
            filter: String::new(),
            popup: None,
            preview_cache: HashMap::new(),
        };
        panel.refresh();
        panel
    }

    pub fn refresh(&mut self) {
        self.entries = build_list::scan_builds(&self.build_path, &self.sub_path);
        self.sorted_as = None;
        log::info!(
            "Scanned {} entries in {}{}",
            self.entries.len(),
            self.build_path,
            self.sub_path
        );
    }

    /// Navigate into a subfolder.
    pub fn enter_folder(&mut self, folder_name: &str) {
        self.sub_path = format!("{}{folder_name}/", self.sub_path);
        self.refresh();
    }

    /// Navigate up one folder level.
    pub fn go_up(&mut self) {
        if self.sub_path.is_empty() {
            return;
        }
        // Remove trailing slash, then remove last path component
        let trimmed = self.sub_path.trim_end_matches('/');
        self.sub_path = match trimmed.rfind('/') {
            Some(pos) => format!("{}/", &trimmed[..pos]),
            None => String::new(),
        };
        self.refresh();
    }

    /// The directory currently being displayed.
    fn current_dir(&self) -> PathBuf {
        Path::new(&self.build_path).join(&self.sub_path)
    }

    /// Returns the action the GUI should take, if any.
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> Option<BuildListAction> {
        let mut action = None;

        ui.heading("Builds");
        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("+ New Build").clicked() {
                action = Some(BuildListAction::NewBuild);
            }
            if ui.button("+ New Folder").clicked() {
                self.popup = Some(Popup::NewFolder {
                    name: String::new(),
                    error: None,
                });
            }
            ui.separator();
            if !self.sub_path.is_empty() {
                if ui.button("⬆ Up").clicked() {
                    self.go_up();
                }
                ui.label(format!("📁 {}", self.sub_path));
            }
            if ui.button("🔄 Refresh").clicked() {
                self.refresh();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Sort:");
            egui::ComboBox::from_id_salt("build_list_sort")
                .selected_text(match self.sort_mode {
                    SortMode::Name => "Name",
                    SortMode::Modified => "Date modified",
                })
                .width(110.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sort_mode, SortMode::Name, "Name");
                    ui.selectable_value(&mut self.sort_mode, SortMode::Modified, "Date modified");
                });
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter by name")
                    .desired_width(180.0),
            );
            if !self.filter.is_empty() && ui.button("✖").clicked() {
                self.filter.clear();
            }
        });

        ui.separator();

        self.show_popup(ui);

        if self.entries.is_empty() {
            ui.label("No builds found.");
            ui.label(format!(
                "Build directory: {}{}",
                self.build_path, self.sub_path
            ));
            return action;
        }

        // Folders available as move targets (collected before the row loop)
        let move_targets: Vec<(String, PathBuf)> = self
            .entries
            .iter()
            .filter_map(|e| match e {
                BuildEntry::Folder(f) => Some((f.folder_name.clone(), f.full_path.clone())),
                BuildEntry::Build(_) => None,
            })
            .collect();
        let parent_dir = if self.sub_path.is_empty() {
            None
        } else {
            self.current_dir().parent().map(Path::to_path_buf)
        };

        // Order the whole list once per change, then filter it - a filtered
        // subsequence of a sorted list is still sorted, so typing in the search
        // box costs nothing.
        if self.sorted_as != Some(self.sort_mode) {
            if let Err(e) =
                build_list::sort_entries(bridge.lua(), &mut self.entries, self.sort_mode)
            {
                log::error!("Failed to sort build list: {e}");
            }
            self.sorted_as = Some(self.sort_mode);
        }

        let filter = self.filter.trim().to_lowercase();
        let visible: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                filter.is_empty()
                    || entry_name(&self.entries[i])
                        .to_lowercase()
                        .contains(&filter)
            })
            .collect();

        let mut row_action = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for &i in &visible {
                match &self.entries[i] {
                    BuildEntry::Folder(folder) => {
                        let (response, delete) = show_folder_row(ui, folder);
                        if delete {
                            row_action = Some(RowAction::ConfirmDelete {
                                path: folder.full_path.clone(),
                                name: folder.folder_name.clone(),
                                is_folder: true,
                            });
                        }
                        if response.clicked() {
                            row_action = Some(RowAction::Enter(folder.folder_name.clone()));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                row_action = Some(RowAction::Rename {
                                    path: folder.full_path.clone(),
                                    name: folder.folder_name.clone(),
                                    is_folder: true,
                                });
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                row_action = Some(RowAction::ConfirmDelete {
                                    path: folder.full_path.clone(),
                                    name: folder.folder_name.clone(),
                                    is_folder: true,
                                });
                                ui.close_menu();
                            }
                        });
                    }
                    BuildEntry::Build(build) => {
                        let (response, delete) = show_build_row(ui, build);
                        if delete {
                            row_action = Some(RowAction::ConfirmDelete {
                                path: build.full_path.clone(),
                                name: build.build_name.clone(),
                                is_folder: false,
                            });
                        }
                        let response = preview_tooltip(response, &mut self.preview_cache, build);
                        if response.clicked() {
                            row_action = Some(RowAction::Open(build.clone()));
                        }
                        response.context_menu(|ui| {
                            if ui.button("Rename").clicked() {
                                row_action = Some(RowAction::Rename {
                                    path: build.full_path.clone(),
                                    name: build.build_name.clone(),
                                    is_folder: false,
                                });
                                ui.close_menu();
                            }
                            let has_targets = parent_dir.is_some() || !move_targets.is_empty();
                            if has_targets {
                                ui.menu_button("Move to", |ui| {
                                    if let Some(ref parent) = parent_dir
                                        && ui.button("⬆ (parent folder)").clicked()
                                    {
                                        row_action = Some(RowAction::Move {
                                            path: build.full_path.clone(),
                                            dest: parent.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    for (name, path) in &move_targets {
                                        if ui.button(format!("📁 {name}")).clicked() {
                                            row_action = Some(RowAction::Move {
                                                path: build.full_path.clone(),
                                                dest: path.clone(),
                                            });
                                            ui.close_menu();
                                        }
                                    }
                                });
                            }
                            if ui.button("Delete").clicked() {
                                row_action = Some(RowAction::ConfirmDelete {
                                    path: build.full_path.clone(),
                                    name: build.build_name.clone(),
                                    is_folder: false,
                                });
                                ui.close_menu();
                            }
                        });
                    }
                }
            }
        });

        match row_action {
            Some(RowAction::Open(build)) => action = Some(BuildListAction::OpenBuild(build)),
            Some(RowAction::Enter(name)) => {
                self.enter_folder(&name);
                action = Some(BuildListAction::EnterFolder);
            }
            Some(RowAction::ConfirmDelete {
                path,
                name,
                is_folder,
            }) => {
                self.popup = Some(Popup::ConfirmDelete {
                    path,
                    name,
                    is_folder,
                });
            }
            Some(RowAction::Rename {
                path,
                name,
                is_folder,
            }) => {
                self.popup = Some(Popup::Rename {
                    path,
                    is_folder,
                    name,
                    error: None,
                });
            }
            Some(RowAction::Move { path, dest }) => match build_list::move_build(&path, &dest) {
                Ok(()) => self.refresh(),
                Err(e) => self.popup = Some(Popup::Error(e)),
            },
            None => {}
        }

        action
    }

    /// Render the active management popup, if any.
    fn show_popup(&mut self, ui: &mut egui::Ui) {
        let Some(mut popup) = self.popup.take() else {
            return;
        };
        let mut close = false;
        let mut refresh = false;

        match &mut popup {
            Popup::ConfirmDelete {
                path,
                name,
                is_folder,
            } => {
                let mut result = None;
                egui::Window::new("Confirm Delete")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        if *is_folder {
                            ui.label(format!("Delete folder \"{name}\" and everything in it?"));
                        } else {
                            ui.label(format!("Delete build \"{name}\"?"));
                        }
                        ui.label("This cannot be undone.");
                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked() {
                                result = Some(if *is_folder {
                                    build_list::delete_folder(path)
                                } else {
                                    build_list::delete_build(path)
                                });
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                match result {
                    Some(Ok(())) => {
                        close = true;
                        refresh = true;
                    }
                    Some(Err(e)) => popup = Popup::Error(e),
                    None => {}
                }
            }
            Popup::Rename {
                path,
                is_folder,
                name,
                error,
            } => {
                let mut do_rename = false;
                let title = if *is_folder {
                    "Rename Folder"
                } else {
                    "Rename Build"
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.label("New name:");
                        let response = ui.text_edit_singleline(name);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            do_rename = true;
                        }
                        if let Some(err) = error {
                            ui.colored_label(egui::Color32::RED, err.as_str());
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked() {
                                do_rename = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if do_rename {
                    match build_list::rename_entry(path, name, *is_folder) {
                        Ok(_) => {
                            close = true;
                            refresh = true;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
            }
            Popup::NewFolder { name, error } => {
                let mut do_create = false;
                egui::Window::new("New Folder")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.label("Folder name:");
                        let response = ui.text_edit_singleline(name);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            do_create = true;
                        }
                        if let Some(err) = error {
                            ui.colored_label(egui::Color32::RED, err.as_str());
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                do_create = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
                if do_create {
                    match build_list::create_folder(&self.build_path, &self.sub_path, name) {
                        Ok(()) => {
                            close = true;
                            refresh = true;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
            }
            Popup::Error(message) => {
                egui::Window::new("Error")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.colored_label(egui::Color32::RED, message.as_str());
                        if ui.button("OK").clicked() {
                            close = true;
                        }
                    });
            }
        }

        if !close {
            self.popup = Some(popup);
        }
        if refresh {
            self.refresh();
        }
    }
}

/// What the build list wants the app to do.
pub enum BuildListAction {
    EnterFolder,
    OpenBuild(BuildInfo),
    NewBuild,
}

fn entry_name(entry: &BuildEntry) -> &str {
    match entry {
        BuildEntry::Build(b) => &b.build_name,
        BuildEntry::Folder(f) => &f.folder_name,
    }
}

/// Rows are centered in the panel at this width rather than stretched across it.
const ROW_WIDTH: f32 = 460.0;
const ROW_HEIGHT: f32 = 24.0;
const DELETE_WIDTH: f32 = 26.0;

/// Lay out one centered row: the main button plus a trailing delete button.
/// Returns the main button's response and whether delete was clicked.
fn centered_row(ui: &mut egui::Ui, label: String) -> (egui::Response, bool) {
    let width = ROW_WIDTH.min(ui.available_width());
    let indent = ((ui.available_width() - width) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        // Right-to-left so the delete button claims its width first and the
        // main button truncates into whatever is left.
        ui.allocate_ui_with_layout(
            egui::vec2(width, ROW_HEIGHT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let delete = ui
                    .add(egui::Button::new("🗑").min_size(egui::vec2(DELETE_WIDTH, ROW_HEIGHT)))
                    .on_hover_text("Delete")
                    .clicked();
                let main = ui.add(
                    egui::Button::new(label)
                        .truncate()
                        .min_size(egui::vec2(ui.available_width(), ROW_HEIGHT)),
                );
                (main, delete)
            },
        )
        .inner
    })
    .inner
}

fn show_folder_row(ui: &mut egui::Ui, folder: &FolderInfo) -> (egui::Response, bool) {
    centered_row(ui, format!("📁 {}", folder.folder_name))
}

/// Attach the build preview tooltip (class, level, headline stats) to a row,
/// parsing the XML on first hover.
fn preview_tooltip(
    resp: egui::Response,
    cache: &mut HashMap<PathBuf, BuildPreview>,
    build: &BuildInfo,
) -> egui::Response {
    if !resp.hovered() {
        return resp;
    }
    let preview = cache
        .entry(build.full_path.clone())
        .or_insert_with(|| build_list::build_preview(&build.full_path));
    resp.on_hover_ui(|ui| {
        ui.strong(&build.build_name);
        let class = preview
            .ascend_class_name
            .as_deref()
            .filter(|c| *c != "None" && !c.is_empty())
            .or(preview.class_name.as_deref());
        match (class, preview.level) {
            (Some(class), Some(level)) => {
                ui.label(format!("Level {level} {class}"));
            }
            (Some(class), None) => {
                ui.label(class.to_string());
            }
            (None, Some(level)) => {
                ui.label(format!("Level {level}"));
            }
            (None, None) => {}
        }
        if preview.stats.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(150, 150, 150),
                "No saved stats (build not calculated yet)",
            );
        } else {
            ui.separator();
            for (label, value) in &preview.stats {
                ui.label(format!("{label}: {}", format_stat(*value)));
            }
        }
    })
}

/// Compact number formatting for preview stats.
fn format_stat(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.2}M", value / 1_000_000.0)
    } else if value >= 10_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn show_build_row(ui: &mut egui::Ui, build: &BuildInfo) -> (egui::Response, bool) {
    centered_row(ui, build_summary(build))
}

fn build_summary(build: &BuildInfo) -> String {
    let mut parts = vec![build.build_name.clone()];
    if let Some(ref class) = build.ascend_class_name {
        if class != "None" && !class.is_empty() {
            parts.push(class.clone());
        } else if let Some(ref c) = build.class_name {
            parts.push(c.clone());
        }
    } else if let Some(ref class) = build.class_name {
        parts.push(class.clone());
    }
    if let Some(level) = build.level {
        parts.push(format!("Lv{level}"));
    }
    parts.join(" - ")
}

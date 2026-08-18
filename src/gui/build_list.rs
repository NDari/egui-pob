//! Build list panel: displays saved builds, allows opening and managing them
//! (delete, rename, move to folder, new folder, sort, search).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pob_egui::data::build_list::{
    self, BuildEntry, BuildInfo, BuildPreview, FolderInfo, IndexKey, SortMode,
};
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
    Enter {
        sub_path: String,
        name: String,
    },
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
        file_name: String,
        from_sub_path: String,
        to_sub_path: String,
    },
}

/// The sub path one level above `sub_path`, or `None` at the build root.
fn parent_of(sub_path: &str) -> Option<String> {
    let trimmed = sub_path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(match trimmed.rfind('/') {
        Some(pos) => format!("{}/", &trimmed[..pos]),
        None => String::new(),
    })
}

/// State for the build list panel.
pub struct BuildListPanel {
    pub entries: Vec<BuildEntry>,
    pub sub_path: String,
    build_path: String,
    sort_mode: SortMode,
    filter: String,
    /// The (sub path, filter, sort mode) `entries` was produced for. Filtering
    /// and ordering both run in Lua against the cached index, so they repeat
    /// only when one of the three changes rather than every frame.
    filtered_as: Option<(String, String, SortMode)>,
    popup: Option<Popup>,
    /// Hover preview data, parsed from build XMLs on first hover.
    preview_cache: HashMap<PathBuf, BuildPreview>,
}

impl BuildListPanel {
    pub fn new(build_path: String, bridge: &LuaBridge) -> Self {
        let mut panel = Self {
            entries: Vec::new(),
            sub_path: String::new(),
            build_path,
            sort_mode: SortMode::Name,
            filter: String::new(),
            filtered_as: None,
            popup: None,
            preview_cache: HashMap::new(),
        };
        panel.refresh(bridge);
        panel
    }

    /// Re-index the build tree from disk. Filtering runs off the cached index,
    /// so this is the only step that touches the filesystem.
    pub fn refresh(&mut self, bridge: &LuaBridge) {
        if let Err(e) = build_list::refresh_index(bridge.lua(), IndexKey::BuildList, &self.sub_path)
        {
            log::error!("Failed to index builds: {e}");
        }
        self.filtered_as = None;
    }

    /// Navigate into a subfolder. `sub_path` is the folder's own location,
    /// which is not necessarily the directory on screen: a search result can
    /// sit several levels down.
    pub fn enter_folder(&mut self, sub_path: &str, folder_name: &str, bridge: &LuaBridge) {
        self.sub_path = format!("{sub_path}{folder_name}/");
        self.refresh(bridge);
    }

    /// Navigate up one folder level.
    pub fn go_up(&mut self, bridge: &LuaBridge) {
        if self.sub_path.is_empty() {
            return;
        }
        // Remove trailing slash, then remove last path component
        let trimmed = self.sub_path.trim_end_matches('/');
        self.sub_path = match trimmed.rfind('/') {
            Some(pos) => format!("{}/", &trimmed[..pos]),
            None => String::new(),
        };
        self.refresh(bridge);
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
                    self.go_up(bridge);
                }
                ui.label(format!("📁 {}", self.sub_path));
            }
            if ui.button("🔄 Refresh").clicked() {
                self.refresh(bridge);
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
                    .hint_text("e.g. class:assassin myfilename")
                    .desired_width(220.0),
            )
            .on_hover_text(
                "Searches every folder below this one. Terms match the build \
                 name, folder, class or path; a class:<name> term matches only \
                 the class or ascendancy.",
            );
            if !self.filter.is_empty() && ui.button("✖").clicked() {
                self.filter.clear();
            }
        });

        ui.separator();

        self.show_popup(ui, bridge);

        // Re-filter only when the inputs change; the index itself is cached.
        let filter_key = (
            self.sub_path.clone(),
            self.filter.trim().to_string(),
            self.sort_mode,
        );
        if self.filtered_as.as_ref() != Some(&filter_key) {
            match build_list::filter_index(
                bridge.lua(),
                IndexKey::BuildList,
                &filter_key.0,
                &filter_key.1,
                self.sort_mode,
            ) {
                Ok(entries) => self.entries = entries,
                Err(e) => log::error!("Failed to filter build list: {e}"),
            }
            self.filtered_as = Some(filter_key);
        }

        if self.entries.is_empty() {
            if self.filter.trim().is_empty() {
                ui.label("No builds found.");
                ui.label(format!(
                    "Build directory: {}{}",
                    self.build_path, self.sub_path
                ));
            } else {
                ui.label("No builds match that search.");
            }
            return action;
        }

        // Folders available as move targets, as sub paths - the form upstream's
        // move guard and destination-name helper both work in.
        let move_targets: Vec<(String, String)> = self
            .entries
            .iter()
            .filter_map(|e| match e {
                BuildEntry::Folder(f) => Some((
                    f.folder_name.clone(),
                    format!("{}{}/", f.sub_path, f.folder_name),
                )),
                BuildEntry::Build(_) => None,
            })
            .collect();
        let parent_sub_path = parent_of(&self.sub_path);

        // Search results can come from anywhere below the current folder, so
        // rows carry their location when it is not the folder on screen.
        let here = self.sub_path.clone();

        let mut row_action = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..self.entries.len() {
                match &self.entries[i] {
                    BuildEntry::Folder(folder) => {
                        let (response, delete) = show_folder_row(ui, folder, &here);
                        if delete {
                            row_action = Some(RowAction::ConfirmDelete {
                                path: folder.full_path.clone(),
                                name: folder.folder_name.clone(),
                                is_folder: true,
                            });
                        }
                        if response.clicked() {
                            row_action = Some(RowAction::Enter {
                                sub_path: folder.sub_path.clone(),
                                name: folder.folder_name.clone(),
                            });
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
                        let (response, delete) = show_build_row(ui, build, &here);
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
                            let has_targets = parent_sub_path.is_some() || !move_targets.is_empty();
                            if has_targets {
                                ui.menu_button("Move to", |ui| {
                                    if let Some(ref parent) = parent_sub_path
                                        && ui.button("⬆ (parent folder)").clicked()
                                    {
                                        row_action = Some(RowAction::Move {
                                            path: build.full_path.clone(),
                                            file_name: build.file_name.clone(),
                                            from_sub_path: build.sub_path.clone(),
                                            to_sub_path: parent.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    for (name, target) in &move_targets {
                                        if ui.button(format!("📁 {name}")).clicked() {
                                            row_action = Some(RowAction::Move {
                                                path: build.full_path.clone(),
                                                file_name: build.file_name.clone(),
                                                from_sub_path: build.sub_path.clone(),
                                                to_sub_path: target.clone(),
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
            Some(RowAction::Enter { sub_path, name }) => {
                self.enter_folder(&sub_path, &name, bridge);
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
            Some(RowAction::Move {
                path,
                file_name,
                from_sub_path,
                to_sub_path,
            }) => match self.move_entry(bridge, &path, &file_name, &from_sub_path, &to_sub_path) {
                Ok(()) => self.refresh(bridge),
                Err(e) => self.popup = Some(Popup::Error(e)),
            },
            None => {}
        }

        action
    }

    /// Move a build into another folder, upstream's way.
    ///
    /// `CanMoveToSubPath` rejects a move that goes nowhere (and, once folders
    /// can be moved too, one whose destination lies inside the folder being
    /// moved). `GetDestName` then picks the landing name, appending `[2]`,
    /// `[3]`, ... ahead of the extension when the plain one is taken, so a
    /// collision renames rather than failing.
    fn move_entry(
        &self,
        bridge: &LuaBridge,
        path: &Path,
        file_name: &str,
        from_sub_path: &str,
        to_sub_path: &str,
    ) -> Result<(), String> {
        let lua = bridge.lua();
        let allowed = build_list::can_move_to_sub_path(lua, from_sub_path, None, to_sub_path)
            .map_err(|e| format!("Failed to check the move target: {e}"))?;
        if !allowed {
            return Err("That build is already in this folder.".to_string());
        }
        let target = build_list::dest_name(lua, to_sub_path, file_name)
            .map_err(|e| format!("Failed to resolve the destination name: {e}"))?;
        build_list::move_build_to(path, &target)
    }

    /// Render the active management popup, if any.
    fn show_popup(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) {
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
            self.refresh(bridge);
        }
    }
}

/// What the build list wants the app to do.
pub enum BuildListAction {
    EnterFolder,
    OpenBuild(BuildInfo),
    NewBuild,
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

/// The part of an entry's location that lies below the folder on screen.
///
/// Empty for entries in that folder, so ordinary browsing looks unchanged;
/// search results reaching into subfolders show where they came from, the way
/// upstream prefixes a hit with its relative subpath.
pub(crate) fn relative_prefix(entry_sub_path: &str, here: &str) -> String {
    match entry_sub_path.strip_prefix(here) {
        Some("") | None => String::new(),
        Some(rest) => rest.to_string(),
    }
}

fn show_folder_row(ui: &mut egui::Ui, folder: &FolderInfo, here: &str) -> (egui::Response, bool) {
    let prefix = relative_prefix(&folder.sub_path, here);
    centered_row(ui, format!("📁 {prefix}{}", folder.folder_name))
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

fn show_build_row(ui: &mut egui::Ui, build: &BuildInfo, here: &str) -> (egui::Response, bool) {
    centered_row(ui, build_summary(build, here))
}

fn build_summary(build: &BuildInfo, here: &str) -> String {
    let prefix = relative_prefix(&build.sub_path, here);
    let mut parts = vec![format!("{prefix}{}", build.build_name)];
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

//! Build list panel: displays saved builds, allows opening them.

use pob_egui::data::build_list::{self, BuildEntry, BuildInfo, FolderInfo};

/// State for the build list panel.
pub struct BuildListPanel {
    pub entries: Vec<BuildEntry>,
    pub sub_path: String,
    build_path: String,
}

impl BuildListPanel {
    pub fn new(build_path: String) -> Self {
        let mut panel = Self {
            entries: Vec::new(),
            sub_path: String::new(),
            build_path,
        };
        panel.refresh();
        panel
    }

    pub fn refresh(&mut self) {
        self.entries = build_list::scan_builds(&self.build_path, &self.sub_path);
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

    /// Returns the action the GUI should take, if any.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<BuildListAction> {
        let mut action = None;

        ui.heading("Builds");
        ui.separator();

        ui.horizontal(|ui| {
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

        ui.separator();

        if self.entries.is_empty() {
            ui.label("No builds found.");
            ui.label(format!(
                "Build directory: {}{}",
                self.build_path, self.sub_path
            ));
            return None;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in &self.entries {
                match entry {
                    BuildEntry::Folder(folder) => {
                        if show_folder_row(ui, folder) {
                            action = Some(BuildListAction::EnterFolder(folder.folder_name.clone()));
                        }
                    }
                    BuildEntry::Build(build) => {
                        if show_build_row(ui, build) {
                            action = Some(BuildListAction::OpenBuild(build.clone()));
                        }
                    }
                }
            }
        });

        // Process folder navigation (deferred to avoid borrow conflicts)
        if let Some(BuildListAction::EnterFolder(ref name)) = action {
            self.enter_folder(name);
        }

        action
    }
}

/// What the build list wants the app to do.
pub enum BuildListAction {
    EnterFolder(String),
    OpenBuild(BuildInfo),
}

fn show_folder_row(ui: &mut egui::Ui, folder: &FolderInfo) -> bool {
    let response = ui.add(
        egui::Button::new(format!("📁 {}", folder.folder_name))
            .min_size(egui::vec2(ui.available_width(), 24.0)),
    );
    response.double_clicked()
}

fn show_build_row(ui: &mut egui::Ui, build: &BuildInfo) -> bool {
    let summary = build_summary(build);
    let response =
        ui.add(egui::Button::new(&summary).min_size(egui::vec2(ui.available_width(), 24.0)));
    response.double_clicked()
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
    parts.join(" — ")
}

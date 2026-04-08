//! Build view: container for an open build with stat sidebar and tabs.

use pob_egui::data::CalcOutput;
use pob_egui::lua_bridge::LuaBridge;

use super::config_tab::ConfigPanel;
use super::import_tab::ImportPanel;
use super::items_tab::ItemsPanel;
use super::skills_tab::SkillsPanel;
use super::tree_tab::TreePanel;

/// Which tab is currently active in the build view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildTab {
    Tree,
    Skills,
    Items,
    Config,
    Import,
}

/// State for an open build.
pub struct BuildView {
    pub build_name: String,
    pub calc_output: Option<CalcOutput>,
    pub config_panel: Option<ConfigPanel>,
    pub tree_panel: Option<TreePanel>,
    pub items_panel: Option<ItemsPanel>,
    pub skills_panel: Option<SkillsPanel>,
    pub import_panel: ImportPanel,
    pub active_tab: BuildTab,
}

impl BuildView {
    pub fn new(build_name: String, bridge: &LuaBridge) -> Self {
        let calc_output = CalcOutput::extract(bridge.lua())
            .map_err(|e| log::error!("Failed to extract calc output: {e}"))
            .ok();

        let config_panel = Some(ConfigPanel::new(bridge.lua()));
        let tree_panel = Some(TreePanel::new(bridge.lua()));
        let items_panel = Some(ItemsPanel::new(bridge.lua()));
        let skills_panel = Some(SkillsPanel::new(bridge.lua()));
        let import_panel = ImportPanel::new();

        Self {
            build_name,
            calc_output,
            config_panel,
            tree_panel,
            items_panel,
            skills_panel,
            import_panel,
            active_tab: BuildTab::Tree,
        }
    }

    /// Draw the build view. Returns true if the user wants to go back to the build list.
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut go_back = false;

        // Top bar: build name + back button
        ui.horizontal(|ui| {
            if ui.button("< Builds").clicked() {
                go_back = true;
            }
            ui.heading(&self.build_name);
        });
        ui.separator();

        // Layout: stat sidebar on the left, tab content on the right
        egui::SidePanel::left("stat_sidebar")
            .default_width(250.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                self.show_stat_sidebar(ui);
            });

        // Tab bar + content
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, BuildTab::Tree, "Tree");
                ui.selectable_value(&mut self.active_tab, BuildTab::Skills, "Skills");
                ui.selectable_value(&mut self.active_tab, BuildTab::Items, "Items");
                ui.selectable_value(&mut self.active_tab, BuildTab::Config, "Config");
                ui.selectable_value(&mut self.active_tab, BuildTab::Import, "Import/Export");
            });
            ui.separator();

            match self.active_tab {
                BuildTab::Tree => {
                    if let Some(ref mut tree) = self.tree_panel
                        && tree.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                    }
                }
                BuildTab::Skills => {
                    if let Some(ref mut skills) = self.skills_panel
                        && skills.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                        self.skills_panel = Some(SkillsPanel::new(bridge.lua()));
                    }
                }
                BuildTab::Items => {
                    if let Some(ref items) = self.items_panel {
                        items.show(ui, bridge);
                    }
                }
                BuildTab::Config => {
                    if let Some(ref mut config) = self.config_panel
                        && config.show(ui, bridge)
                    {
                        self.refresh_calc_output(bridge);
                    }
                }
                BuildTab::Import => {
                    if self.import_panel.show(ui, bridge) {
                        // Build was imported — refresh everything
                        self.refresh_all(bridge);
                    }
                }
            }
        });

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
    }

    /// Refresh all panels after a major change (e.g., build import).
    fn refresh_all(&mut self, bridge: &LuaBridge) {
        self.refresh_calc_output(bridge);
        self.config_panel = Some(ConfigPanel::new(bridge.lua()));
        self.tree_panel = Some(TreePanel::new(bridge.lua()));
        self.items_panel = Some(ItemsPanel::new(bridge.lua()));
        self.skills_panel = Some(SkillsPanel::new(bridge.lua()));
    }

    fn show_stat_sidebar(&self, ui: &mut egui::Ui) {
        ui.strong("Stats");
        ui.separator();

        let Some(ref output) = self.calc_output else {
            ui.label("No calc output available.");
            return;
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            super::show_stat_table(ui, output);
        });
    }
}

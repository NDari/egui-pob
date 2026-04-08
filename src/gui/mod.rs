//! egui GUI — top-level App and panel modules.

mod build_list;
mod build_view;
mod config_tab;
mod items_tab;
mod skills_tab;
mod tree_renderer;
mod tree_tab;

use pob_egui::data::CalcOutput;
use pob_egui::lua_bridge::LuaBridge;

use build_list::{BuildListAction, BuildListPanel};
use build_view::BuildView;

/// What screen the app is showing.
enum AppScreen {
    BuildList(BuildListPanel),
    BuildView(Box<BuildView>),
}

/// Main application state.
pub struct PobApp {
    bridge: LuaBridge,
    status: AppStatus,
    screen: Option<AppScreen>,
}

enum AppStatus {
    Running,
    Error(String),
}

impl PobApp {
    pub fn new(
        bridge: Result<LuaBridge, anyhow::Error>,
        _cc: &eframe::CreationContext<'_>,
    ) -> Self {
        match bridge {
            Ok(b) => {
                if let Err(e) = b.verify_boot() {
                    Self {
                        bridge: b,
                        status: AppStatus::Error(format!("Boot verification failed: {e}")),
                        screen: None,
                    }
                } else {
                    // Initialize build list
                    let screen = match b.build_path() {
                        Ok(path) => {
                            log::info!("Build path: {path}");
                            Some(AppScreen::BuildList(BuildListPanel::new(path)))
                        }
                        Err(e) => {
                            log::error!("Failed to get build path: {e}");
                            None
                        }
                    };
                    Self {
                        bridge: b,
                        status: AppStatus::Running,
                        screen,
                    }
                }
            }
            Err(e) => Self {
                bridge: LuaBridge::new_dummy(),
                status: AppStatus::Error(format!("Failed to initialize Lua: {e}")),
                screen: None,
            },
        }
    }

    fn open_build(&mut self, build_info: &pob_egui::data::build_list::BuildInfo) {
        let xml_text = match std::fs::read_to_string(&build_info.full_path) {
            Ok(text) => text,
            Err(e) => {
                log::error!("Failed to read build file: {e}");
                self.status =
                    AppStatus::Error(format!("Failed to read {}: {e}", build_info.file_name));
                return;
            }
        };

        if let Err(e) = self
            .bridge
            .load_build_from_xml(&xml_text, &build_info.build_name)
        {
            log::error!("Failed to load build: {e}");
            self.status = AppStatus::Error(format!("Failed to load build: {e}"));
            return;
        }

        self.screen = Some(AppScreen::BuildView(Box::new(BuildView::new(
            build_info.build_name.clone(),
            &self.bridge,
        ))));
    }

    fn go_to_build_list(&mut self) {
        match self.bridge.build_path() {
            Ok(path) => {
                self.screen = Some(AppScreen::BuildList(BuildListPanel::new(path)));
            }
            Err(e) => {
                log::error!("Failed to get build path: {e}");
            }
        }
    }
}

impl eframe::App for PobApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| match &self.status {
            AppStatus::Error(msg) => {
                ui.heading("Path of Building — Error");
                ui.separator();
                ui.colored_label(egui::Color32::RED, msg);
            }
            AppStatus::Running => {
                // Take ownership of screen to avoid borrow conflicts
                let mut screen = self.screen.take();
                let mut transition = None;

                match &mut screen {
                    Some(AppScreen::BuildList(panel)) => {
                        if let Some(action) = panel.show(ui) {
                            match action {
                                BuildListAction::OpenBuild(build_info) => {
                                    transition = Some(build_info);
                                }
                                BuildListAction::EnterFolder(_) => {
                                    // Already handled inside panel.show()
                                }
                            }
                        }
                    }
                    Some(AppScreen::BuildView(view)) => {
                        if view.show(ui, &self.bridge) {
                            // User wants to go back
                            self.go_to_build_list();
                            return;
                        }
                    }
                    None => {
                        ui.heading("Path of Building");
                        ui.label("Initializing...");
                    }
                }

                // Restore screen if no transition
                if let Some(build_info) = transition {
                    self.screen = screen; // temporarily restore
                    self.open_build(&build_info);
                } else {
                    self.screen = screen;
                }
            }
        });
    }
}

/// Display calc output stats in an egui table.
pub fn show_stat_table(ui: &mut egui::Ui, output: &CalcOutput) {
    let key_stats = [
        ("TotalDPS", "Hit DPS"),
        ("CombinedDPS", "Combined DPS"),
        ("TotalDot", "DoT DPS"),
        ("Life", "Life"),
        ("EnergyShield", "Energy Shield"),
        ("Mana", "Mana"),
        ("TotalEHP", "Effective Hit Pool"),
        ("Str", "Strength"),
        ("Dex", "Dexterity"),
        ("Int", "Intelligence"),
        ("Speed", "Attack/Cast Rate"),
        ("HitChance", "Hit Chance"),
        ("CritChance", "Crit Chance"),
        ("CritMultiplier", "Crit Multiplier"),
        ("Armour", "Armour"),
        ("Evasion", "Evasion"),
        ("FireResist", "Fire Resistance"),
        ("ColdResist", "Cold Resistance"),
        ("LightningResist", "Lightning Resistance"),
        ("ChaosResist", "Chaos Resistance"),
    ];

    egui::Grid::new("key_stats")
        .num_columns(2)
        .striped(true)
        .min_col_width(100.0)
        .show(ui, |ui| {
            for (stat, label) in &key_stats {
                if let Some(value) = output.stats.get(*stat) {
                    ui.label(*label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format_stat_value(stat, *value));
                    });
                    ui.end_row();
                }
            }
        });
}

/// Format a stat value for display.
fn format_stat_value(stat: &str, value: f64) -> String {
    if stat.contains("Resist")
        || stat.contains("Chance")
        || stat.contains("Percent")
        || stat == "CritMultiplier"
    {
        return format!("{value:.1}%");
    }

    if stat == "Speed" || stat.contains("Rate") {
        return format!("{value:.2}");
    }

    if value.fract().abs() < 0.001
        || stat == "Life"
        || stat == "Mana"
        || stat == "EnergyShield"
        || stat == "Armour"
        || stat == "Evasion"
        || stat == "Str"
        || stat == "Dex"
        || stat == "Int"
    {
        return format_number(value as i64);
    }

    if value.abs() >= 1000.0 {
        return format_number_f64(value);
    }

    format!("{value:.1}")
}

fn format_number(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

fn format_number_f64(n: f64) -> String {
    let integer = n.trunc() as i64;
    let frac = (n.fract().abs() * 10.0).round() as u8;
    format!("{}.{}", format_number(integer), frac)
}

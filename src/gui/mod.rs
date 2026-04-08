//! egui GUI — top-level App and panel modules.

use pob_egui::data::CalcOutput;
use pob_egui::lua_bridge::LuaBridge;

/// Main application state.
pub struct PobApp {
    bridge: LuaBridge,
    status: AppStatus,
    calc_output: Option<CalcOutput>,
    build_load_attempted: bool,
}

enum AppStatus {
    /// Lua bridge loaded successfully, app is running.
    Running,
    /// An error occurred during boot or build loading.
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
                        calc_output: None,
                        build_load_attempted: false,
                    }
                } else {
                    Self {
                        bridge: b,
                        status: AppStatus::Running,
                        calc_output: None,
                        build_load_attempted: false,
                    }
                }
            }
            Err(e) => Self {
                bridge: LuaBridge::new_dummy(),
                status: AppStatus::Error(format!("Failed to initialize Lua: {e}")),
                calc_output: None,
                build_load_attempted: false,
            },
        }
    }

    /// Try to load the test build and extract calc output.
    fn try_load_test_build(&mut self) {
        self.build_load_attempted = true;

        // Look for test build XML relative to the base directory
        let test_build_path = find_test_build();
        let xml_text = match test_build_path {
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(text) => {
                    log::info!("Found test build: {}", path.display());
                    text
                }
                Err(e) => {
                    self.status = AppStatus::Error(format!("Failed to read test build: {e}"));
                    return;
                }
            },
            None => {
                log::info!("No test build found — showing empty state");
                return;
            }
        };

        if let Err(e) = self.bridge.load_build_from_xml(&xml_text, "Test Build") {
            self.status = AppStatus::Error(format!("Failed to load build: {e}"));
            return;
        }

        match CalcOutput::extract(self.bridge.lua()) {
            Ok(output) => {
                log::info!("Extracted {} stats from calc output", output.stats.len());
                self.calc_output = Some(output);
            }
            Err(e) => {
                self.status = AppStatus::Error(format!("Failed to extract calc output: {e}"));
            }
        }
    }
}

/// Search for a test build XML file.
fn find_test_build() -> Option<std::path::PathBuf> {
    // Try relative to exe (dev mode: walk up to repo root)
    let exe = std::env::current_exe().ok()?;
    let mut candidate = exe.parent()?.to_path_buf();
    for _ in 0..5 {
        let test_dir = candidate.join("test_builds");
        if test_dir.is_dir() {
            // Find first XML file recursively
            for entry in walkdir::WalkDir::new(&test_dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.path().extension().is_some_and(|ext| ext == "xml") {
                    return Some(entry.path().to_path_buf());
                }
            }
        }
        if !candidate.pop() {
            break;
        }
    }
    None
}

impl eframe::App for PobApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Try to load test build on first frame
        if !self.build_load_attempted && matches!(self.status, AppStatus::Running) {
            self.try_load_test_build();
        }

        egui::CentralPanel::default().show(ctx, |ui| match &self.status {
            AppStatus::Error(msg) => {
                ui.heading("Path of Building — Error");
                ui.separator();
                ui.colored_label(egui::Color32::RED, msg);
            }
            AppStatus::Running => {
                ui.heading("Path of Building");
                ui.separator();

                if let Some(output) = &self.calc_output {
                    show_stat_table(ui, output);
                } else {
                    ui.label("No build loaded. Place a build XML in test_builds/.");
                }
            }
        });
    }
}

/// Display calc output stats in an egui table.
fn show_stat_table(ui: &mut egui::Ui, output: &CalcOutput) {
    ui.label(format!("{} stats extracted", output.stats.len()));
    ui.separator();

    // Show key stats at the top in a prominent way
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

    ui.heading("Key Stats");
    egui::Grid::new("key_stats")
        .num_columns(2)
        .striped(true)
        .min_col_width(200.0)
        .show(ui, |ui| {
            for (stat, label) in &key_stats {
                if let Some(value) = output.stats.get(*stat) {
                    ui.label(*label);
                    ui.label(format_stat_value(stat, *value));
                    ui.end_row();
                }
            }
        });

    ui.separator();

    // Collapsible section with all stats
    egui::CollapsingHeader::new("All Stats")
        .default_open(false)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .show(ui, |ui| {
                    let mut stats: Vec<_> = output.stats.iter().collect();
                    stats.sort_by(|(a, _), (b, _)| a.cmp(b));

                    egui::Grid::new("all_stats")
                        .num_columns(2)
                        .striped(true)
                        .min_col_width(200.0)
                        .show(ui, |ui| {
                            for (stat, value) in &stats {
                                ui.label(stat.as_str());
                                ui.label(format_stat_value(stat, **value));
                                ui.end_row();
                            }
                        });
                });
        });
}

/// Format a stat value for display. Uses appropriate precision based on stat type.
fn format_stat_value(stat: &str, value: f64) -> String {
    // Percentage-like stats
    if stat.contains("Resist")
        || stat.contains("Chance")
        || stat.contains("Percent")
        || stat == "CritMultiplier"
    {
        return format!("{value:.1}%");
    }

    // Rate stats
    if stat == "Speed" || stat.contains("Rate") {
        return format!("{value:.2}");
    }

    // Integer-like stats (life, mana, armour, etc.)
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

    // DPS and other large numbers
    if value.abs() >= 1000.0 {
        return format_number_f64(value);
    }

    format!("{value:.1}")
}

/// Format an integer with thousands separators.
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

/// Format a float with thousands separators and one decimal place.
fn format_number_f64(n: f64) -> String {
    let integer = n.trunc() as i64;
    let frac = (n.fract().abs() * 10.0).round() as u8;
    format!("{}.{}", format_number(integer), frac)
}

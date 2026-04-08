//! egui GUI — top-level App and panel modules.

use crate::lua_bridge::LuaBridge;

/// Main application state.
pub struct PobApp {
    bridge: LuaBridge,
    status: AppStatus,
}

enum AppStatus {
    /// Lua bridge loaded successfully, app is running.
    Running,
    /// An error occurred during boot.
    Error(String),
}

impl PobApp {
    pub fn new(bridge: Result<LuaBridge, anyhow::Error>, _cc: &eframe::CreationContext<'_>) -> Self {
        match bridge {
            Ok(b) => {
                if let Err(e) = b.verify_boot() {
                    Self {
                        bridge: b,
                        status: AppStatus::Error(format!("Boot verification failed: {e}")),
                    }
                } else {
                    Self {
                        bridge: b,
                        status: AppStatus::Running,
                    }
                }
            }
            Err(e) => {
                // Create a dummy bridge — we can't run, but we can show the error
                Self {
                    bridge: LuaBridge::new_dummy(),
                    status: AppStatus::Error(format!("Failed to initialize Lua: {e}")),
                }
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
                ui.heading("Path of Building");
                ui.separator();
                ui.label("Lua engine loaded successfully.");
                ui.label("GUI panels coming soon.");

                // TODO: build list, build view, stat display, etc.
            }
        });
    }
}

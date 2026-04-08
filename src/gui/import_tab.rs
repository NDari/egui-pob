//! Import/Export tab: build code import, export, and saving.

use pob_egui::lua_bridge::LuaBridge;

/// State for the import/export panel.
pub struct ImportPanel {
    pub import_code: String,
    pub export_code: String,
    pub status_message: Option<(String, bool)>, // (message, is_error)
}

impl ImportPanel {
    pub fn new() -> Self {
        Self {
            import_code: String::new(),
            export_code: String::new(),
            status_message: None,
        }
    }

    /// Draw the import/export panel. Returns true if a build was imported (full reload needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut imported = false;

        // Status message
        if let Some((ref msg, is_error)) = self.status_message {
            let color = if is_error {
                egui::Color32::RED
            } else {
                egui::Color32::from_rgb(100, 200, 100)
            };
            ui.colored_label(color, msg.as_str());
            ui.separator();
        }

        // Export section
        ui.heading("Export");
        ui.horizontal(|ui| {
            if ui.button("Generate Code").clicked() {
                match generate_export_code(bridge) {
                    Ok(code) => {
                        self.export_code = code;
                        self.status_message = Some(("Code generated.".to_string(), false));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Export failed: {e}"), true));
                    }
                }
            }
            if !self.export_code.is_empty()
                && ui.button("Copy to Clipboard").clicked()
                && let Ok(mut clip) = arboard::Clipboard::new()
            {
                let _ = clip.set_text(&self.export_code);
                self.status_message = Some(("Copied to clipboard.".to_string(), false));
            }
        });
        if !self.export_code.is_empty() {
            ui.add(
                egui::TextEdit::multiline(&mut self.export_code.as_str())
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .font(egui::TextStyle::Monospace),
            );
        }

        ui.add_space(16.0);
        ui.separator();

        // Import section
        ui.heading("Import");
        ui.label("Paste a build code:");
        ui.add(
            egui::TextEdit::multiline(&mut self.import_code)
                .desired_width(f32::INFINITY)
                .desired_rows(3)
                .hint_text("Paste build code here...")
                .font(egui::TextStyle::Monospace),
        );
        if ui.button("Import").clicked() && !self.import_code.is_empty() {
            match import_build_code(bridge, &self.import_code) {
                Ok(()) => {
                    self.status_message = Some(("Build imported.".to_string(), false));
                    self.import_code.clear();
                    imported = true;
                }
                Err(e) => {
                    self.status_message = Some((format!("Import failed: {e}"), true));
                }
            }
        }

        ui.add_space(16.0);
        ui.separator();

        // Save section
        ui.heading("Save");
        if ui.button("Save Build").clicked() {
            match save_build(bridge) {
                Ok(()) => {
                    self.status_message = Some(("Build saved.".to_string(), false));
                }
                Err(e) => {
                    self.status_message = Some((format!("Save failed: {e}"), true));
                }
            }
        }

        imported
    }
}

/// Generate an export code from the current build.
fn generate_export_code(bridge: &LuaBridge) -> anyhow::Result<String> {
    let code: String = bridge
        .lua()
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local xmlText = build:SaveDB("code")
            if not xmlText then
                return ""
            end
            local compressed = Deflate(xmlText)
            local encoded = common.base64.encode(compressed)
            return encoded:gsub("+", "-"):gsub("/", "_")
        "#,
        )
        .eval()
        .map_err(|e| anyhow::anyhow!("Lua error: {e}"))?;

    if code.is_empty() {
        anyhow::bail!("Failed to generate build XML");
    }

    Ok(code)
}

/// Import a build from a build code string.
fn import_build_code(bridge: &LuaBridge, code: &str) -> anyhow::Result<()> {
    // Decode: URL-safe base64 → standard base64 → decode → inflate → XML
    let lua = bridge.lua();

    let xml_text: String = lua
        .load(
            r#"
            local code = ...
            local decoded = common.base64.decode(code:gsub("-", "+"):gsub("_", "/"))
            if not decoded then
                return nil
            end
            return Inflate(decoded)
        "#,
        )
        .call(code)
        .map_err(|e| anyhow::anyhow!("Failed to decode build code: {e}"))?;

    if xml_text.is_empty() {
        anyhow::bail!("Failed to decode build code — invalid or corrupted");
    }

    bridge.load_build_from_xml(&xml_text, "Imported Build")?;
    Ok(())
}

/// Save the current build to disk.
fn save_build(bridge: &LuaBridge) -> anyhow::Result<()> {
    bridge
        .lua()
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            if not build.dbFileName or build.dbFileName == "" then
                error("No filename set — use Save As first")
            end
            build:SaveDBFile()
        "#,
        )
        .exec()
        .map_err(|e| anyhow::anyhow!("Lua error: {e}"))?;

    Ok(())
}

//! Import/Export tab: build code import, export, URL import, character
//! import from a PoE account, and saving.

use pob_egui::data::char_import::{self, CharacterInfo, REALMS};
use pob_egui::lua_bridge::LuaBridge;

/// State for the import/export panel.
pub struct ImportPanel {
    pub import_code: String,
    pub export_code: String,
    pub status_message: Option<(String, bool)>, // (message, is_error)
    // Character import state
    account_name: String,
    realm_index: usize,
    sessid: String,
    characters: Vec<CharacterInfo>,
    /// Leagues present in `characters`, for the filter dropdown.
    leagues: Vec<String>,
    /// Selected league filter (index into `leagues`; 0 = all).
    league_index: usize,
    /// Selected character (index into the filtered list).
    char_index: usize,
    char_status: Option<(String, bool)>, // (message, is_error)
    clear_jewels: bool,
    clear_items: bool,
    clear_skills: bool,
    ignore_weapon_swap: bool,
}

impl ImportPanel {
    pub fn new() -> Self {
        Self {
            import_code: String::new(),
            export_code: String::new(),
            status_message: None,
            account_name: String::new(),
            realm_index: 0,
            sessid: String::new(),
            characters: Vec::new(),
            leagues: Vec::new(),
            league_index: 0,
            char_index: 0,
            char_status: None,
            clear_jewels: true,
            clear_items: true,
            clear_skills: true,
            ignore_weapon_swap: false,
        }
    }

    /// Draw the import/export panel. Returns true if a build was imported (full reload needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut imported = false;

        // Status message
        if let Some((ref msg, is_error)) = self.status_message {
            let color = if is_error {
                super::theme::Theme::ERROR
            } else {
                super::theme::Theme::SUCCESS
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
        ui.label(
            "Paste a build code or URL (pobb.in, pastebin, poe.ninja, maxroll, rentry, poedb):",
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.import_code)
                .desired_width(f32::INFINITY)
                .desired_rows(3)
                .hint_text("Paste build code or URL here...")
                .font(egui::TextStyle::Monospace),
        );
        if ui.button("Import").clicked() && !self.import_code.is_empty() {
            let input = self.import_code.trim().to_string();
            let result = if looks_like_url(&input) {
                import_from_url(bridge, &input)
            } else {
                import_build_code(bridge, &input)
            };
            match result {
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

        // Character import section
        imported |= self.show_char_import(ui, bridge);

        ui.add_space(16.0);
        ui.separator();

        // Save section
        ui.heading("Save");
        if ui.button("Save Build").clicked() {
            match bridge.save_build() {
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

    /// Characters matching the current league filter.
    fn filtered_characters(&self) -> Vec<CharacterInfo> {
        self.characters
            .iter()
            .filter(|c| self.league_index == 0 || c.league == self.leagues[self.league_index - 1])
            .cloned()
            .collect()
    }

    /// Draw the "Import from PoE Account" section. Returns true if a
    /// character was imported into the build.
    fn show_char_import(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut imported = false;

        ui.heading("Import from PoE Account");
        ui.horizontal(|ui| {
            ui.label("Account name:");
            ui.add(
                egui::TextEdit::singleline(&mut self.account_name)
                    .hint_text("Name#1234")
                    .desired_width(180.0),
            );
            egui::ComboBox::from_id_salt("realm_select")
                .selected_text(REALMS[self.realm_index].label)
                .width(70.0)
                .show_ui(ui, |ui| {
                    for (i, realm) in REALMS.iter().enumerate() {
                        ui.selectable_value(&mut self.realm_index, i, realm.label);
                    }
                });
            if ui.button("Get characters").clicked() {
                self.fetch_characters(bridge);
            }
        });
        ui.horizontal(|ui| {
            ui.label("POESESSID:");
            ui.add(
                egui::TextEdit::singleline(&mut self.sessid)
                    .password(true)
                    .hint_text("only needed for private profiles")
                    .desired_width(260.0),
            );
            ui.hyperlink_to(
                "Privacy settings",
                "https://www.pathofexile.com/my-account/privacy",
            );
        });

        if !self.characters.is_empty() {
            let filtered = self.filtered_characters();
            ui.horizontal(|ui| {
                ui.label("League:");
                let league_label = if self.league_index == 0 {
                    "All"
                } else {
                    &self.leagues[self.league_index - 1]
                };
                egui::ComboBox::from_id_salt("league_select")
                    .selected_text(league_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.league_index == 0, "All").clicked() {
                            self.league_index = 0;
                            self.char_index = 0;
                        }
                        for (i, league) in self.leagues.iter().enumerate() {
                            if ui
                                .selectable_label(self.league_index == i + 1, league)
                                .clicked()
                            {
                                self.league_index = i + 1;
                                self.char_index = 0;
                            }
                        }
                    });

                ui.label("Character:");
                let char_label = filtered
                    .get(self.char_index)
                    .map(|c| format!("{} ({}, {})", c.name, c.class, c.level))
                    .unwrap_or_else(|| "-".to_string());
                egui::ComboBox::from_id_salt("char_select")
                    .selected_text(char_label)
                    .width(240.0)
                    .show_ui(ui, |ui| {
                        for (i, c) in filtered.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.char_index,
                                i,
                                format!("{} ({}, {}, {})", c.name, c.class, c.league, c.level),
                            );
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.clear_jewels, "Delete jewels")
                    .on_hover_text("Delete all existing jewels when importing the tree");
                ui.checkbox(&mut self.clear_items, "Delete equipment")
                    .on_hover_text("Delete all equipped items when importing items");
                ui.checkbox(&mut self.clear_skills, "Delete skills")
                    .on_hover_text("Delete all existing skills when importing items");
                ui.checkbox(&mut self.ignore_weapon_swap, "Ignore weapon swap");
            });

            if let Some(character) = filtered.get(self.char_index).cloned() {
                ui.horizontal(|ui| {
                    if ui.button("Import passive tree and jewels").clicked() {
                        imported |= self.import_tree(bridge, &character);
                    }
                    if ui.button("Import items and skills").clicked() {
                        imported |= self.import_items(bridge, &character);
                    }
                });
            }
        }

        if let Some((ref msg, is_error)) = self.char_status {
            let color = if is_error {
                super::theme::Theme::ERROR
            } else {
                super::theme::Theme::SUCCESS
            };
            ui.colored_label(color, msg.as_str());
        }

        imported
    }

    fn fetch_characters(&mut self, bridge: &LuaBridge) {
        self.characters.clear();
        self.leagues.clear();
        self.league_index = 0;
        self.char_index = 0;
        if self.account_name.trim().is_empty() {
            self.char_status = Some(("Enter an account name.".to_string(), true));
            return;
        }
        let realm = &REALMS[self.realm_index];
        let result =
            char_import::fetch_character_list(&self.account_name, realm.code, &self.sessid)
                .and_then(|json| char_import::parse_character_list(bridge.lua(), &json));
        match result {
            Ok(chars) => {
                for c in &chars {
                    if !self.leagues.contains(&c.league) {
                        self.leagues.push(c.league.clone());
                    }
                }
                self.char_status = Some((format!("Found {} characters.", chars.len()), false));
                self.characters = chars;
            }
            Err(e) => {
                self.char_status = Some((format!("{e}"), true));
            }
        }
    }

    fn import_tree(&mut self, bridge: &LuaBridge, character: &CharacterInfo) -> bool {
        let realm = &REALMS[self.realm_index];
        let result = char_import::fetch_passive_tree(
            &self.account_name,
            &character.name,
            realm.code,
            &self.sessid,
        )
        .and_then(|json| {
            char_import::import_passive_tree_and_jewels(
                bridge.lua(),
                &json,
                character,
                self.clear_jewels,
            )
            .map_err(|e| anyhow::anyhow!("Import failed: {e}"))
        });
        self.finish_import(result)
    }

    fn import_items(&mut self, bridge: &LuaBridge, character: &CharacterInfo) -> bool {
        let realm = &REALMS[self.realm_index];
        let result = char_import::fetch_items(
            &self.account_name,
            &character.name,
            realm.code,
            &self.sessid,
        )
        .and_then(|json| {
            char_import::import_items_and_skills(
                bridge.lua(),
                &json,
                self.clear_items,
                self.clear_skills,
                self.ignore_weapon_swap,
            )
            .map_err(|e| anyhow::anyhow!("Import failed: {e}"))
        });
        self.finish_import(result)
    }

    /// Store the import status (stripping PoB colour codes) and report
    /// whether the build changed.
    fn finish_import(&mut self, result: anyhow::Result<String>) -> bool {
        match result {
            Ok(status) => {
                let plain: String = super::theme::parse_pob_colors(&status, egui::Color32::WHITE)
                    .into_iter()
                    .map(|(_, text)| text)
                    .collect();
                let is_error = plain.contains("Error");
                self.char_status = Some((plain, is_error));
                !is_error
            }
            Err(e) => {
                self.char_status = Some((format!("{e}"), true));
                false
            }
        }
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

/// Import a build from a raw build code string.
fn import_build_code(bridge: &LuaBridge, code: &str) -> anyhow::Result<()> {
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

    bridge.load_build_from_xml(&xml_text, "Imported Build", None)?;
    Ok(())
}

/// Import a build from a URL by fetching the build code from the site.
fn import_from_url(bridge: &LuaBridge, url: &str) -> anyhow::Result<()> {
    let download_url = resolve_download_url(url)?;

    log::info!("Fetching build from: {download_url}");
    let response = reqwest::blocking::Client::new()
        .get(&download_url)
        .header("User-Agent", "pob-egui")
        .send()
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {} from {download_url}", response.status());
    }

    let body = response
        .text()
        .map_err(|e| anyhow::anyhow!("Failed to read response: {e}"))?;

    let code = body.trim();
    if code.is_empty() {
        anyhow::bail!("Empty response from {download_url}");
    }

    import_build_code(bridge, code)
}

/// Supported build sites and their URL → download URL mappings.
struct BuildSite {
    pattern: &'static str,
    download_prefix: &'static str,
}

const BUILD_SITES: &[BuildSite] = &[
    BuildSite {
        pattern: "pobb.in/",
        download_prefix: "https://pobb.in/pob/",
    },
    BuildSite {
        pattern: "poe.ninja/poe1/pob/",
        download_prefix: "https://poe.ninja/poe1/pob/raw/",
    },
    BuildSite {
        pattern: "poe.ninja/pob/",
        download_prefix: "https://poe.ninja/poe1/pob/raw/",
    },
    BuildSite {
        pattern: "pastebin.com/",
        download_prefix: "https://pastebin.com/raw/",
    },
    BuildSite {
        pattern: "pastebinp.com/",
        download_prefix: "https://pastebinp.com/raw/",
    },
    BuildSite {
        pattern: "rentry.co/",
        download_prefix: "https://rentry.co/paste/",
    },
    BuildSite {
        pattern: "maxroll.gg/poe/pob/",
        download_prefix: "https://maxroll.gg/poe/api/pob/",
    },
    BuildSite {
        pattern: "poedb.tw/pob/",
        download_prefix: "https://poedb.tw/pob/",
    },
];

/// Resolve a user-provided URL to the raw download URL for the build code.
fn resolve_download_url(url: &str) -> anyhow::Result<String> {
    // Strip protocol prefix
    let path = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    for site in BUILD_SITES {
        if let Some(rest) = path.strip_prefix(site.pattern) {
            // Extract the ID (first path segment, no trailing slashes or query params)
            let id = rest
                .split(&['/', '?', '#'][..])
                .next()
                .unwrap_or(rest)
                .trim();
            if id.is_empty() {
                anyhow::bail!("No build ID found in URL");
            }

            let mut download_url = format!("{}{id}", site.download_prefix);

            // rentry.co needs /raw suffix
            if site.pattern == "rentry.co/" {
                download_url.push_str("/raw");
            }
            // poedb.tw needs /raw suffix
            if site.pattern == "poedb.tw/pob/" {
                download_url.push_str("/raw");
            }

            return Ok(download_url);
        }
    }

    anyhow::bail!(
        "Unrecognized URL. Supported sites: pobb.in, pastebin.com, poe.ninja, maxroll.gg, rentry.co, poedb.tw"
    )
}

/// Check if input looks like a URL rather than a raw build code.
fn looks_like_url(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || BUILD_SITES.iter().any(|s| trimmed.starts_with(s.pattern))
}

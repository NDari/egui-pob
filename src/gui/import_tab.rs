//! Import/Export tab: build code import, export, URL import, character
//! import from a PoE account, and saving.

use pob_egui::data::char_import::{self, CharacterInfo, REALMS};
use pob_egui::lua_bridge::LuaBridge;

/// State for the import/export panel.
pub struct ImportPanel {
    pub import_code: String,
    pub export_code: String,
    pub status_message: Option<(String, bool)>, // (message, is_error)
    /// Selected share site (index into EXPORT_SITES).
    export_site_idx: usize,
    // Character import state
    account_name: String,
    /// Past account names (persisted in the app data dir, upstream's
    /// account history dropdown).
    account_history: Vec<String>,
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
    /// Import into a fresh build instead of overwriting the open one.
    import_to_new: bool,
    /// Set after a successful new-build import; the build view resets its
    /// name/unsaved state when it sees this.
    pub new_build_imported: bool,
}

impl ImportPanel {
    pub fn new() -> Self {
        Self {
            import_code: String::new(),
            export_code: String::new(),
            status_message: None,
            export_site_idx: 0,
            account_name: String::new(),
            account_history: char_import::load_account_history(),
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
            import_to_new: false,
            new_build_imported: false,
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

            // Share: upload the code to a build site and replace it with the
            // share URL (upstream's export dropdown + Share button)
            ui.separator();
            let site_idx = self.export_site_idx.min(EXPORT_SITES.len() - 1);
            egui::ComboBox::from_id_salt("export_site")
                .selected_text(EXPORT_SITES[site_idx].label)
                .width(100.0)
                .show_ui(ui, |ui| {
                    for (i, site) in EXPORT_SITES.iter().enumerate() {
                        ui.selectable_value(&mut self.export_site_idx, i, site.label);
                    }
                });
            let already_shared = self.export_code.trim().starts_with("http");
            if ui
                .add_enabled(
                    !self.export_code.is_empty() && !already_shared,
                    egui::Button::new("Share"),
                )
                .on_hover_text(
                    "Upload the build to the selected website and turn the code \
                     into a short link (network request)",
                )
                .on_disabled_hover_text("Generate a code first")
                .clicked()
            {
                match upload_build(&self.export_code, &EXPORT_SITES[site_idx]) {
                    Ok(url) => {
                        self.export_code = url;
                        self.status_message = Some(("Share link created.".to_string(), false));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Share failed: {e}"), true));
                    }
                }
            }

            // Party-play export toggle (upstream enablePartyExportBuffs;
            // persists as exportParty via upstream's ImportTab saver)
            let mut export_support = export_support_enabled(bridge);
            if ui
                .checkbox(&mut export_support, "Export Support")
                .on_hover_text(
                    "For party play: include this character's auras, curses and \
                     enemy modifiers in the export so it can be used as a \
                     support character",
                )
                .changed()
                && let Err(e) = set_export_support(bridge, export_support)
            {
                log::error!("Failed to toggle Export Support: {e}");
            }
        });
        if !self.export_code.is_empty() {
            // Single line on purpose: the code is huge and a multiline box
            // would grow until the buttons below are unreachable
            ui.add(
                egui::TextEdit::singleline(&mut self.export_code.as_str())
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        }

        ui.add_space(16.0);
        ui.separator();

        // Import section
        ui.heading("Import");
        ui.label(
            "Paste a build code or URL (pobb.in, pastebin, poe.ninja, maxroll, rentry, poedb, \
             pob.codes):",
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.import_code)
                .desired_width(f32::INFINITY)
                .hint_text("Paste build code or URL here...")
                .font(egui::TextStyle::Monospace),
        );
        ui.horizontal(|ui| {
            ui.label("Import to:");
            ui.radio_value(&mut self.import_to_new, false, "This build");
            ui.radio_value(&mut self.import_to_new, true, "A new build");
        });
        if ui.button("Import").clicked() && !self.import_code.is_empty() {
            // Unwrap youtube.com/redirect and google.com/url wrappers to the
            // q= target first, like upstream's import handler
            let input = unwrap_redirect_url(self.import_code.trim());
            // For a new-build import, switch the VM to a fresh build first so
            // the current one is untouched
            let prepared = if self.import_to_new {
                bridge
                    .create_new_build()
                    .map_err(|e| anyhow::anyhow!("failed to create new build: {e}"))
            } else {
                Ok(())
            };
            let result = prepared.and_then(|()| {
                if looks_like_url(&input) {
                    import_from_url(bridge, &input)
                } else {
                    import_build_code(bridge, &input)
                }
            });
            match result {
                Ok(()) => {
                    self.status_message = Some(("Build imported.".to_string(), false));
                    self.import_code.clear();
                    if self.import_to_new {
                        self.new_build_imported = true;
                    }
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
            // Account history (persisted across sessions)
            if !self.account_history.is_empty() {
                egui::ComboBox::from_id_salt("account_history")
                    .selected_text("History")
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for name in self.account_history.clone() {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(self.account_name == name, &name)
                                    .clicked()
                                {
                                    self.account_name = name.clone();
                                }
                                if ui
                                    .small_button("✕")
                                    .on_hover_text("Remove from history")
                                    .clicked()
                                {
                                    self.account_history =
                                        char_import::remove_account_history(&name);
                                }
                            });
                        }
                    });
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
                // Successful fetch: remember the account (upstream
                // SaveAccountHistory) and preselect the build's last league
                self.account_history = char_import::add_account_history(self.account_name.trim());
                if let Some(last) = char_import::last_league(bridge.lua())
                    && let Some(pos) = self.leagues.iter().position(|l| *l == last)
                {
                    self.league_index = pos + 1;
                }
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
    BuildSite {
        pattern: "pob.codes/b/",
        download_prefix: "https://api.pob.codes/",
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
            // pob.codes raw downloads live at api.pob.codes/<id>/raw
            if site.pattern == "pob.codes/b/" {
                download_url.push_str("/raw");
            }

            return Ok(download_url);
        }
    }

    anyhow::bail!(
        "Unrecognized URL. Supported sites: pobb.in, pastebin.com, poe.ninja, maxroll.gg, rentry.co, poedb.tw, pob.codes"
    )
}

/// Unwrap youtube.com/redirect and google.com/url indirection to the q=
/// target (upstream's textual unwrap; no HTTP involved).
fn unwrap_redirect_url(input: &str) -> String {
    let trimmed = input.trim().trim_matches('?');
    if (trimmed.contains("youtube.com/redirect?") || trimmed.contains("google.com/url?"))
        && let Some(q) = trimmed
            .split(&['?', '&'][..])
            .find_map(|part| part.strip_prefix("q="))
    {
        return url_decode(q);
    }
    trimmed.to_string()
}

/// Percent-decode a URL query value ('+' becomes space, %XX becomes a byte).
fn url_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Check if input looks like a URL rather than a raw build code.
fn looks_like_url(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || BUILD_SITES.iter().any(|s| trimmed.starts_with(s.pattern))
}

/// Export-capable share sites: upstream buildSites entries that carry
/// postUrl + postFields + codeOut, in the same order as upstream's filtered
/// dropdown (Maxroll is the default there too).
struct ExportSite {
    label: &'static str,
    post_url: &'static str,
    /// Raw body prefix prepended to the build code.
    post_fields: &'static str,
    /// Prefix turning the response into a share URL ("" when the API already
    /// returns a full URL).
    code_out: &'static str,
}

const EXPORT_SITES: &[ExportSite] = &[
    ExportSite {
        label: "Maxroll",
        post_url: "https://maxroll.gg/poe/api/pob",
        post_fields: "pobCode=",
        code_out: "https://maxroll.gg/poe/pob/",
    },
    ExportSite {
        label: "pob.codes",
        post_url: "https://api.pob.codes/pob/plain",
        post_fields: "",
        code_out: "https://pob.codes/b/",
    },
    ExportSite {
        label: "pobb.in",
        post_url: "https://pobb.in/pob/",
        post_fields: "",
        code_out: "https://pobb.in/",
    },
    ExportSite {
        label: "PoeNinja",
        post_url: "https://poe.ninja/poe1/pob/api/upload",
        post_fields: "code=",
        code_out: "",
    },
    ExportSite {
        label: "poedb.tw",
        post_url: "https://poedb.tw/pob/api/gen",
        post_fields: "",
        code_out: "",
    },
];

/// Upload a build code to a share site (upstream buildSites.UploadBuild):
/// POST postFields..code as a form body; a 200 response body is the share
/// id/URL, prefixed with codeOut.
fn upload_build(code: &str, site: &ExportSite) -> anyhow::Result<String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow::anyhow!("HTTP client init failed: {e}"))?
        .post(site.post_url)
        .header("User-Agent", "PathOfBuildingCommunity (egui-pob)")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("{}{}", site.post_fields, code))
        .send()
        .map_err(|e| anyhow::anyhow!("Upload to {} failed: {e}", site.label))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if status.as_u16() != 200 {
        let short: String = body.chars().take(200).collect();
        anyhow::bail!("{} returned {status}: {short}", site.label);
    }
    Ok(format!("{}{}", site.code_out, body.trim()))
}

/// Read upstream's party-play export toggle (partyTab.enableExportBuffs).
fn export_support_enabled(bridge: &LuaBridge) -> bool {
    bridge
        .lua()
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            return (build.partyTab and build.partyTab.enableExportBuffs == true) or false
        "#,
        )
        .eval()
        .unwrap_or(false)
}

/// Set the party-play export toggle. The ImportTab control state is mirrored
/// so upstream's saver persists it as the exportParty attribute.
fn set_export_support(bridge: &LuaBridge, state: bool) -> Result<(), mlua::Error> {
    bridge
        .lua()
        .load(
            r#"
            local state = ...
            local build = mainObject_ref.main.modes['BUILD']
            if build.partyTab then
                build.partyTab.enableExportBuffs = state
            end
            local ctrl = build.importTab and build.importTab.controls
                and build.importTab.controls.enablePartyExportBuffs
            if ctrl then
                ctrl.state = state
            end
            build.buildFlag = true
            _runCallback('OnFrame')
        "#,
        )
        .call(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_redirect_urls() {
        assert_eq!(
            unwrap_redirect_url(
                "https://www.youtube.com/redirect?event=desc&q=https%3A%2F%2Fpobb.in%2Fabc123&v=x"
            ),
            "https://pobb.in/abc123"
        );
        assert_eq!(
            unwrap_redirect_url(
                "https://www.google.com/url?sa=t&q=https%3A%2F%2Fpob.codes%2Fb%2Fxyz"
            ),
            "https://pob.codes/b/xyz"
        );
        // Non-wrapped URLs pass through untouched
        assert_eq!(
            unwrap_redirect_url("https://pobb.in/abc"),
            "https://pobb.in/abc"
        );
    }
}

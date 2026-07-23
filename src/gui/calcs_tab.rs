//! Calcs tab: full calculation breakdown display, mirroring upstream's
//! section layout. Cell text comes pre-formatted from the Lua helper;
//! clicking a highlighted cell opens the breakdown panel on the right.

use pob_egui::data::calcs::{self, BreakdownSection, CalcSection, CalcsInput};
use pob_egui::data::skills;
use pob_egui::lua_bridge::LuaBridge;

use super::theme::{self, Theme};

/// An open breakdown: the cell it came from and the resolved sections.
struct OpenBreakdown {
    title: String,
    cell: (usize, usize, usize, usize),
    sections: Vec<BreakdownSection>,
}

/// State for the calcs panel.
pub struct CalcsPanel {
    sections: Vec<CalcSection>,
    input: Option<CalcsInput>,
    socket_group_names: Vec<String>,
    breakdown: Option<OpenBreakdown>,
    search: String,
    error: Option<String>,
}

impl CalcsPanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        let mut panel = Self {
            sections: Vec::new(),
            input: None,
            socket_group_names: Vec::new(),
            breakdown: None,
            search: String::new(),
            error: None,
        };
        panel.refresh(lua);
        panel
    }

    /// Re-extract sections, input state, and any open breakdown from Lua.
    pub fn refresh(&mut self, lua: &mlua::Lua) {
        match calcs::extract_sections(lua) {
            Ok(sections) => {
                self.sections = sections;
                self.error = None;
            }
            Err(e) => {
                log::error!("Failed to extract calc sections: {e}");
                self.error = Some(format!("Failed to extract calc sections: {e}"));
            }
        }
        self.input = calcs::get_input(lua)
            .map_err(|e| log::error!("Failed to get calcs input: {e}"))
            .ok();
        self.socket_group_names = skills::extract_skills(lua)
            .map(|groups| groups.iter().map(socket_group_name).collect())
            .unwrap_or_default();
        if let Some(ref mut open) = self.breakdown {
            let (si, ui_idx, ri, ci) = open.cell;
            match calcs::fetch_breakdown(lua, si, ui_idx, ri, ci) {
                Ok(sections) if !sections.is_empty() => open.sections = sections,
                _ => self.breakdown = None,
            }
        }
    }

    /// Draw the calcs panel. Returns true if inputs changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(Theme::ERROR, err);
            return false;
        }

        changed |= self.show_selectors(ui, bridge);

        ui.separator();

        // Breakdown panel on the right, sections on the left
        if self.breakdown.is_some() {
            egui::SidePanel::right("calc_breakdown_panel")
                .resizable(true)
                .default_width(420.0)
                .show_inside(ui, |ui| {
                    self.show_breakdown_panel(ui);
                });
        }

        let mut clicked_cell: Option<(usize, usize, usize, usize, String)> = None;
        egui::ScrollArea::vertical()
            .id_salt("calc_sections")
            .show(ui, |ui| {
                // Two columns: group 1 (offence) left, group 2+ (defence) right
                ui.columns(2, |cols| {
                    for (col, group_filter) in cols.iter_mut().zip([1..=1, 2..=i64::MAX]) {
                        for section in self
                            .sections
                            .iter()
                            .filter(|s| group_filter.contains(&s.group))
                        {
                            show_section(col, section, &self.search, &mut clicked_cell);
                        }
                    }
                });
            });

        if let Some((si, ui_idx, ri, ci, title)) = clicked_cell {
            match calcs::fetch_breakdown(bridge.lua(), si, ui_idx, ri, ci) {
                Ok(sections) if !sections.is_empty() => {
                    self.breakdown = Some(OpenBreakdown {
                        title,
                        cell: (si, ui_idx, ri, ci),
                        sections,
                    });
                }
                Ok(_) => {}
                Err(e) => log::error!("Failed to fetch breakdown: {e}"),
            }
        }

        if changed {
            self.refresh(bridge.lua());
        }
        changed
    }

    fn show_selectors(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;
        let Some(input) = self.input.clone() else {
            return false;
        };

        ui.horizontal(|ui| {
            // Socket group selector
            if !self.socket_group_names.is_empty() {
                let current = input
                    .skill_number
                    .checked_sub(1)
                    .and_then(|i| self.socket_group_names.get(i))
                    .cloned()
                    .unwrap_or_else(|| "<none>".to_string());
                egui::ComboBox::from_id_salt("calcs_socket_group")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (i, name) in self.socket_group_names.iter().enumerate() {
                            if ui
                                .selectable_label(input.skill_number == i + 1, name)
                                .clicked()
                            {
                                if let Err(e) = calcs::set_skill_number(bridge.lua(), i + 1) {
                                    log::error!("Failed to set skill number: {e}");
                                } else {
                                    changed = true;
                                }
                            }
                        }
                    });
            }

            // Buff mode selector
            let mode_label = calcs::BUFF_MODES
                .iter()
                .find(|(mode, _)| *mode == input.buff_mode)
                .map(|(_, label)| *label)
                .unwrap_or("Effective DPS");
            egui::ComboBox::from_id_salt("calcs_buff_mode")
                .selected_text(mode_label)
                .show_ui(ui, |ui| {
                    for (mode, label) in calcs::BUFF_MODES {
                        if ui
                            .selectable_label(input.buff_mode == *mode, *label)
                            .clicked()
                            && input.buff_mode != *mode
                        {
                            match calcs::set_buff_mode(bridge.lua(), mode) {
                                Ok(()) => changed = true,
                                Err(e) => log::error!("Failed to set buff mode: {e}"),
                            }
                        }
                    }
                });

            // Minion toggle
            if input.has_minion {
                let mut show_minion = input.show_minion;
                if ui.checkbox(&mut show_minion, "Show minion stats").changed() {
                    if let Err(e) = calcs::set_show_minion(bridge.lua(), show_minion) {
                        log::error!("Failed to toggle minion stats: {e}");
                    } else {
                        changed = true;
                    }
                }
            }

            // Search filter
            ui.label("Search:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(160.0)
                    .hint_text("filter sections..."),
            );
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                response.request_focus();
            }
            if !self.search.is_empty() && ui.small_button("✕").clicked() {
                self.search.clear();
            }
        });

        changed
    }

    fn show_breakdown_panel(&mut self, ui: &mut egui::Ui) {
        let Some(open) = &self.breakdown else {
            return;
        };
        let mut close = false;
        ui.horizontal(|ui| {
            ui.heading(&open.title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✕").clicked() {
                    close = true;
                }
            });
        });
        ui.separator();
        egui::ScrollArea::both()
            .id_salt("calc_breakdown_scroll")
            .show(ui, |ui| {
                for (idx, section) in open.sections.iter().enumerate() {
                    match section {
                        BreakdownSection::Text { lines } => {
                            for line in lines {
                                ui.label(theme::pob_layout_job(line, 14.0, Theme::TEXT_DEFAULT));
                            }
                        }
                        BreakdownSection::Table {
                            label,
                            footer,
                            columns,
                            rows,
                        } => {
                            if let Some(label) = label {
                                ui.label(theme::pob_layout_job(label, 14.0, Theme::TEXT_DEFAULT));
                            }
                            egui::Grid::new(format!("breakdown_table_{idx}"))
                                .striped(true)
                                .spacing([10.0, 2.0])
                                .show(ui, |ui| {
                                    for column in columns {
                                        ui.label(
                                            egui::RichText::new(column)
                                                .strong()
                                                .size(12.0)
                                                .color(Theme::TEXT_MUTED),
                                        );
                                    }
                                    ui.end_row();
                                    for row in rows {
                                        for value in row {
                                            ui.label(theme::pob_layout_job(
                                                value,
                                                12.0,
                                                Theme::TEXT_DEFAULT,
                                            ));
                                        }
                                        ui.end_row();
                                    }
                                });
                            if let Some(footer) = footer {
                                ui.label(theme::pob_layout_job(footer, 12.0, Theme::TEXT_MUTED));
                            }
                        }
                    }
                    ui.add_space(8.0);
                }
            });
        if close {
            self.breakdown = None;
        }
    }
}

fn show_section(
    ui: &mut egui::Ui,
    section: &CalcSection,
    search: &str,
    clicked_cell: &mut Option<(usize, usize, usize, usize, String)>,
) {
    let search_lower = search.trim().to_lowercase();
    let border = theme::parse_pob_colors(&section.colour, Theme::TEXT_DEFAULT)
        .first()
        .map(|(c, _)| *c)
        .unwrap_or(Theme::TEXT_DEFAULT);

    // Filter by search: keep subsections whose label or row labels match
    let visible: Vec<_> = section
        .subsections
        .iter()
        .filter(|sub| {
            if search_lower.is_empty() {
                return true;
            }
            sub.label.to_lowercase().contains(&search_lower)
                || sub.rows.iter().any(|r| {
                    r.label
                        .as_deref()
                        .is_some_and(|l| l.to_lowercase().contains(&search_lower))
                })
        })
        .collect();
    if visible.is_empty() {
        return;
    }

    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.5_f32, border))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for sub in visible {
                let mut title = egui::text::LayoutJob::default();
                title.append(
                    &sub.label,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(14.0),
                        color: border,
                        ..Default::default()
                    },
                );
                if let Some(ref extra) = sub.extra {
                    title.append(
                        "  ",
                        0.0,
                        egui::TextFormat::simple(
                            egui::FontId::proportional(13.0),
                            Theme::TEXT_DEFAULT,
                        ),
                    );
                    for (color, segment) in theme::parse_pob_colors(extra, Theme::TEXT_DEFAULT) {
                        title.append(
                            &segment,
                            0.0,
                            egui::TextFormat::simple(egui::FontId::proportional(13.0), color),
                        );
                    }
                }
                egui::CollapsingHeader::new(title)
                    .id_salt(format!("calc_sub_{}_{}", section.si, sub.ui))
                    .default_open(!sub.collapsed)
                    .show(ui, |ui| {
                        // Wide grids scroll inside their column instead of
                        // overflowing into the neighbouring one
                        egui::ScrollArea::horizontal()
                            .id_salt(format!("calc_scroll_{}_{}", section.si, sub.ui))
                            .show(ui, |ui| {
                                egui::Grid::new(format!("calc_grid_{}_{}", section.si, sub.ui))
                                    .spacing([8.0, 2.0])
                                    .show(ui, |ui| {
                                        for row in &sub.rows {
                                            ui.label(
                                                egui::RichText::new(
                                                    row.label.as_deref().unwrap_or(""),
                                                )
                                                .size(12.0)
                                                .color(Theme::TEXT_MUTED),
                                            );
                                            for cell in &row.cells {
                                                if cell.text.is_empty() {
                                                    ui.label("");
                                                } else if cell.has_breakdown {
                                                    let job = theme::pob_layout_job(
                                                        &cell.text,
                                                        12.0,
                                                        Theme::MOD_TEXT,
                                                    );
                                                    if ui
                                                        .add(
                                                            egui::Label::new(job)
                                                                .sense(egui::Sense::click()),
                                                        )
                                                        .on_hover_cursor(
                                                            egui::CursorIcon::PointingHand,
                                                        )
                                                        .clicked()
                                                    {
                                                        let title = match &row.label {
                                                            Some(l) => {
                                                                format!("{}: {l}", sub.label)
                                                            }
                                                            None => sub.label.clone(),
                                                        };
                                                        *clicked_cell = Some((
                                                            section.si, sub.ui, row.ri, cell.ci,
                                                            title,
                                                        ));
                                                    }
                                                } else {
                                                    ui.label(theme::pob_layout_job(
                                                        &cell.text,
                                                        12.0,
                                                        Theme::TEXT_DEFAULT,
                                                    ));
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
            }
        });
    ui.add_space(6.0);
}

fn socket_group_name(group: &skills::SocketGroup) -> String {
    if !group.label.is_empty() {
        return group.label.clone();
    }
    for gem in &group.gems {
        if gem.enabled && !gem.is_support && !gem.name.is_empty() {
            return gem.name.clone();
        }
    }
    group
        .gems
        .first()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| format!("Group {}", group.index))
}

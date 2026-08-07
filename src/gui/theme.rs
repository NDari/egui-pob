//! Centralized color palette and styling constants for the GUI.

use egui::Color32;

/// Semantic colors used across the GUI.
pub struct Theme;

impl Theme {
    // -- Text colors --
    pub const TEXT_DIM: Color32 = Color32::from_rgb(100, 100, 100);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(150, 150, 150);
    pub const TEXT_DEFAULT: Color32 = Color32::from_rgb(200, 200, 200);

    // -- Status colors --
    pub const ERROR: Color32 = Color32::from_rgb(255, 80, 80);
    pub const SUCCESS: Color32 = Color32::from_rgb(100, 200, 100);

    // -- Gem colors --
    pub const GEM_ACTIVE: Color32 = Color32::from_rgb(200, 50, 50);
    pub const GEM_SUPPORT: Color32 = Color32::from_rgb(120, 160, 255);

    // -- Highlight colors --
    pub const MAIN_SKILL: Color32 = Color32::from_rgb(255, 200, 50);

    // -- Item mod colors --
    pub const MOD_TEXT: Color32 = Color32::from_rgb(136, 136, 255);

    // -- Stat sidebar colors --
    pub const STAT_FIRE: Color32 = Color32::from_rgb(210, 80, 60);
    pub const STAT_COLD: Color32 = Color32::from_rgb(80, 140, 230);
    pub const STAT_LIGHTNING: Color32 = Color32::from_rgb(255, 215, 0);
    pub const STAT_CHAOS: Color32 = Color32::from_rgb(190, 50, 210);
    pub const STAT_LIFE: Color32 = Color32::from_rgb(200, 60, 60);
    pub const STAT_ES: Color32 = Color32::from_rgb(110, 140, 210);
    pub const STAT_MANA: Color32 = Color32::from_rgb(80, 120, 220);
    pub const STAT_DPS: Color32 = Color32::from_rgb(255, 200, 50);
}

/// Parse a PoB colour-coded string (`^xRRGGBB` and `^0`-`^9` escapes) into
/// (colour, text) segments. Codes with no following text are dropped.
pub fn parse_pob_colors(text: &str, default: Color32) -> Vec<(Color32, String)> {
    // ^0-^9 palette from SimpleGraphic
    const DIGIT_COLORS: [Color32; 10] = [
        Color32::from_rgb(0, 0, 0),       // ^0 black
        Color32::from_rgb(255, 0, 0),     // ^1 red
        Color32::from_rgb(0, 255, 0),     // ^2 green
        Color32::from_rgb(0, 0, 255),     // ^3 blue
        Color32::from_rgb(255, 255, 0),   // ^4 yellow
        Color32::from_rgb(255, 0, 255),   // ^5 purple
        Color32::from_rgb(0, 255, 255),   // ^6 aqua
        Color32::from_rgb(255, 255, 255), // ^7 white
        Color32::from_rgb(160, 160, 160), // ^8 gray
        Color32::from_rgb(96, 96, 96),    // ^9 dark gray
    ];

    let mut segments: Vec<(Color32, String)> = Vec::new();
    let mut color = default;
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '^' && i + 1 < chars.len() {
            let next = chars[i + 1];
            let new_color = if let Some(d) = next.to_digit(10) {
                i += 2;
                Some(DIGIT_COLORS[d as usize])
            } else if (next == 'x' || next == 'X') && i + 7 < chars.len() {
                let hex: String = chars[i + 2..i + 8].iter().collect();
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    i += 8;
                    Some(Color32::from_rgb(r, g, b))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(new_color) = new_color {
                if !current.is_empty() {
                    segments.push((color, std::mem::take(&mut current)));
                }
                color = new_color;
                continue;
            }
        }
        current.push(chars[i]);
        i += 1;
    }
    if !current.is_empty() {
        segments.push((color, current));
    }
    segments
}

/// Build an egui LayoutJob from a PoB colour-coded string.
pub fn pob_layout_job(text: &str, size: f32, default: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (color, segment) in parse_pob_colors(text, default) {
        job.append(
            &segment,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(size),
                color,
                ..Default::default()
            },
        );
    }
    job
}

/// Gap between the end of a right-aligned label and its value/control.
pub const LABEL_GAP: f32 = 8.0;

/// Labels longer than this wrap onto a further line instead of widening the
/// label column for every row.
pub const WRAP_CHAR_THRESHOLD: usize = 30;

/// Lay out a row the way upstream does: `label_job` right-aligned inside a
/// fixed-width column, the value or control immediately after it. Because the
/// column is the same width for every row, trailing "?" / ":" line up and
/// every control shares one vertical axis.
///
/// Upstream builds this from fixed pixel columns: `ConfigTab` anchors controls
/// at x=234 with right-aligned labels, and the sidebar's `TextListControl` uses
/// `{{x=170, align="RIGHT_X"}, {x=174, align="LEFT"}}`.
///
/// Labels over [`WRAP_CHAR_THRESHOLD`] characters wrap within the column, and
/// the trailing content aligns with the *last* line so it stays attached to
/// where the label ends. The column is allocated at an exact size, so a label
/// that still overflows overhangs to the left (as upstream's right-anchored
/// labels do) rather than pushing its control out of line.
pub fn right_aligned_row<R>(
    ui: &mut egui::Ui,
    label_width: f32,
    mut label_job: egui::text::LayoutJob,
    tooltip: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if label_job.text.chars().count() > WRAP_CHAR_THRESHOLD {
        label_job.wrap.max_width = label_width - LABEL_GAP;
    }
    // Right-align every wrapped line against the column edge, so the trailing
    // "?" / ":" of the last line sits directly beside the value.
    label_job.halign = egui::Align::RIGHT;
    let tooltip_id = ui.id().with(&label_job.text);
    let galley = ui.fonts(|f| f.layout_job(label_job));

    let trailing_h = ui.spacing().interact_size.y;
    let line_h = galley
        .rows
        .last()
        .map_or(trailing_h, |r| r.rect.height().max(1.0));
    // Distance from the top of the text block to the centre of its last line.
    let last_line_centre = galley.rect.height() - line_h / 2.0;

    ui.horizontal_top(|ui| {
        let row_h = galley.rect.height().max(trailing_h);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(label_width, row_h), egui::Sense::hover());
        let color = ui.visuals().text_color();
        ui.painter().galley(
            egui::pos2(rect.right() - LABEL_GAP, rect.top()),
            galley,
            color,
        );
        if let Some(t) = tooltip {
            ui.interact(rect, tooltip_id, egui::Sense::hover())
                .on_hover_text(t);
        }
        ui.vertical(|ui| {
            // Drop the trailing content so its centre matches the last line's.
            let offset = last_line_centre - trailing_h / 2.0;
            if offset > 0.0 {
                ui.add_space(offset);
            }
            add_trailing(ui)
        })
        .inner
    })
    .inner
}

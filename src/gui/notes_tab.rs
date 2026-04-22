//! Notes tab: multiline editor for build notes with PoB color-code support.

use mlua::prelude::*;
use pob_egui::lua_bridge::LuaBridge;

/// A PoB color code button definition.
struct ColorButton {
    label: &'static str,
    /// The `^xRRGGBB` token inserted into the buffer.
    code: &'static str,
    /// The button's own display color in the GUI.
    display: egui::Color32,
}

const COLOR_BUTTONS: &[ColorButton] = &[
    ColorButton { label: "NORMAL",       code: "^xC8C8C8", display: egui::Color32::from_rgb(200, 200, 200) },
    ColorButton { label: "MAGIC",        code: "^x8888FF", display: egui::Color32::from_rgb(136, 136, 255) },
    ColorButton { label: "RARE",         code: "^xFFFF77", display: egui::Color32::from_rgb(255, 255, 119) },
    ColorButton { label: "UNIQUE",       code: "^xAF6025", display: egui::Color32::from_rgb(175, 96, 37) },
    ColorButton { label: "FIRE",         code: "^xB97123", display: egui::Color32::from_rgb(185, 113, 35) },
    ColorButton { label: "COLD",         code: "^x3F6DB3", display: egui::Color32::from_rgb(63, 109, 179) },
    ColorButton { label: "LIGHTNING",    code: "^xADAA47", display: egui::Color32::from_rgb(173, 170, 71) },
    ColorButton { label: "CHAOS",        code: "^xD02090", display: egui::Color32::from_rgb(208, 32, 144) },
    ColorButton { label: "STRENGTH",     code: "^xE05030", display: egui::Color32::from_rgb(224, 80, 48) },
    ColorButton { label: "DEXTERITY",    code: "^x70FF70", display: egui::Color32::from_rgb(112, 255, 112) },
    ColorButton { label: "INTELLIGENCE", code: "^x7070FF", display: egui::Color32::from_rgb(112, 112, 255) },
    ColorButton { label: "DEFAULT",      code: "^7",       display: egui::Color32::from_rgb(230, 230, 230) },
];

pub struct NotesPanel {
    buffer: String,
    show_color_codes: bool,
    /// Character offsets (byte index) of the current selection, captured last frame.
    last_selection: Option<(usize, usize)>,
}

impl NotesPanel {
    pub fn new(lua: &Lua) -> Self {
        let buffer = load_notes(lua).unwrap_or_default();
        Self {
            buffer,
            show_color_codes: false,
            last_selection: None,
        }
    }

    /// Draw the notes tab. Returns true if the notes buffer changed (save flag).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        ui.label(
            "You can use color codes in notes. Type ^ followed by a hex code (^xRRGGBB) \
             or a digit 0-9 (^7 for default) to change color. Select text before clicking \
             a color button to wrap the selection.",
        );
        ui.add_space(4.0);

        // Color code buttons in a grid (4 per row)
        ui.horizontal_wrapped(|ui| {
            for btn in COLOR_BUTTONS {
                let rich = egui::RichText::new(btn.label).color(btn.display).monospace();
                if ui.button(rich).clicked() {
                    self.apply_color(btn.code);
                    changed = true;
                }
            }
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let toggle_label = if self.show_color_codes {
                "Hide Color Codes"
            } else {
                "Show Color Codes"
            };
            if ui.button(toggle_label).clicked() {
                self.show_color_codes = !self.show_color_codes;
                self.buffer = if self.show_color_codes {
                    reveal_color_codes(&self.buffer)
                } else {
                    hide_color_codes(&self.buffer)
                };
            }
        });
        ui.separator();

        // Multiline editor
        let available = ui.available_size();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let id = ui.make_persistent_id("notes_editor");
                let mut output = egui::TextEdit::multiline(&mut self.buffer)
                    .id(id)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(20)
                    .min_size(available)
                    .show(ui);

                // Capture selection for next color button insertion.
                if let Some(cursor_range) = output.cursor_range {
                    let [a, b] = [
                        cursor_range.primary.ccursor.index,
                        cursor_range.secondary.ccursor.index,
                    ];
                    let (start, end) = if a <= b { (a, b) } else { (b, a) };
                    self.last_selection = Some((
                        char_index_to_byte(&self.buffer, start),
                        char_index_to_byte(&self.buffer, end),
                    ));
                }

                if output.response.changed() {
                    changed = true;
                }
                // Silence unused mutable warning (TextEdit output is mutable for re-emit)
                let _ = &mut output;
            });

        if changed {
            // Push buffer to Lua. Always store with real color codes (not hidden form).
            let to_store = if self.show_color_codes {
                hide_color_codes(&self.buffer)
            } else {
                self.buffer.clone()
            };
            if let Err(e) = save_notes(bridge.lua(), &to_store) {
                log::error!("Failed to save notes: {e}");
            }
        }

        changed
    }

    /// Insert a color code at the caret, or wrap the current selection.
    fn apply_color(&mut self, code: &str) {
        let insert = if self.show_color_codes {
            reveal_color_codes(code)
        } else {
            code.to_string()
        };

        if let Some((start, end)) = self.last_selection
            && start != end
            && end <= self.buffer.len()
        {
            // Wrap: <code><selection><default>
            let default = if self.show_color_codes { "^_7" } else { "^7" };
            let selected = &self.buffer[start..end];
            let replacement = format!("{insert}{selected}{default}");
            self.buffer.replace_range(start..end, &replacement);
        } else {
            // Insert at end if no cursor info, otherwise at caret.
            let pos = self
                .last_selection
                .map(|(_, c)| c.min(self.buffer.len()))
                .unwrap_or(self.buffer.len());
            self.buffer.insert_str(pos, &insert);
        }
    }
}

fn load_notes(lua: &Lua) -> LuaResult<String> {
    lua.load(
        r#"
        local nt = mainObject_ref.main.modes['BUILD'].notesTab
        if nt and nt.controls and nt.controls.edit and nt.controls.edit.buf then
            return nt.controls.edit.buf
        end
        return ""
        "#,
    )
    .eval::<String>()
}

fn save_notes(lua: &Lua, text: &str) -> LuaResult<()> {
    lua.load(
        r#"
        local nt = mainObject_ref.main.modes['BUILD'].notesTab
        if nt and nt.controls and nt.controls.edit then
            nt.controls.edit:SetText(...)
            nt.modFlag = true
        end
        "#,
    )
    .call::<()>(text)
}

/// Convert stored color codes into a visible form: `^x` → `^_x`, `^N` → `^_N`.
/// This lets users see the codes literally in the editor.
fn reveal_color_codes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'x'
                && i + 8 <= bytes.len()
                && bytes[i + 2..i + 8].iter().all(|b| b.is_ascii_hexdigit())
            {
                out.push_str("^_x");
                out.push_str(&s[i + 2..i + 8]);
                i += 8;
                continue;
            } else if next.is_ascii_digit() {
                out.push_str("^_");
                out.push(next as char);
                i += 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Inverse of `reveal_color_codes`: `^_x` → `^x`, `^_N` → `^N`.
fn hide_color_codes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' && i + 2 < bytes.len() && bytes[i + 1] == b'_' {
            let third = bytes[i + 2];
            if third == b'x'
                && i + 9 <= bytes.len()
                && bytes[i + 3..i + 9].iter().all(|b| b.is_ascii_hexdigit())
            {
                out.push('^');
                out.push('x');
                out.push_str(&s[i + 3..i + 9]);
                i += 9;
                continue;
            } else if third.is_ascii_digit() {
                out.push('^');
                out.push(third as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn char_index_to_byte(s: &str, ch_idx: usize) -> usize {
    s.char_indices()
        .nth(ch_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

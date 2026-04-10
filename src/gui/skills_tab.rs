//! Skills tab: displays socket groups and gems, allows main skill selection.

use pob_egui::data::skills::{self, GemInfo, SocketGroup};
use pob_egui::lua_bridge::LuaBridge;

/// State for the skills panel.
pub struct SkillsPanel {
    pub groups: Vec<SocketGroup>,
    pub error: Option<String>,
}

impl SkillsPanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        match skills::extract_skills(lua) {
            Ok(groups) => {
                log::info!("Loaded {} socket groups", groups.len());
                Self {
                    groups,
                    error: None,
                }
            }
            Err(e) => {
                log::error!("Failed to load skills: {e}");
                Self {
                    groups: Vec::new(),
                    error: Some(format!("Failed to load skills: {e}")),
                }
            }
        }
    }

    /// Draw the skills panel. Returns true if the main skill changed (recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(super::theme::Theme::ERROR, err);
            return false;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for group in &mut self.groups {
                changed |= show_socket_group(ui, group, bridge);
            }
        });

        changed
    }
}

fn show_socket_group(ui: &mut egui::Ui, group: &mut SocketGroup, bridge: &LuaBridge) -> bool {
    let mut changed = false;

    let title = socket_group_title(group);
    let header_text = if group.is_main {
        egui::RichText::new(format!("* {title}")).color(super::theme::Theme::MAIN_SKILL)
    } else if !group.enabled {
        egui::RichText::new(title).color(super::theme::Theme::TEXT_DIM)
    } else {
        egui::RichText::new(title)
    };

    egui::CollapsingHeader::new(header_text)
        .id_salt(format!("skill_group_{}", group.index))
        .default_open(group.is_main)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if !group.is_main && group.enabled && ui.small_button("Set Main").clicked() {
                    if let Err(e) = skills::set_main_socket_group(bridge.lua(), group.index) {
                        log::error!("Failed to set main skill: {e}");
                    } else {
                        changed = true;
                    }
                }
                if group.is_main {
                    ui.colored_label(super::theme::Theme::MAIN_SKILL, "Main Skill");
                }
                if let Some(ref slot) = group.slot {
                    ui.label(
                        egui::RichText::new(format!("({slot})"))
                            .small()
                            .color(super::theme::Theme::TEXT_MUTED),
                    );
                }
            });

            for gem in &group.gems {
                show_gem(ui, gem);
            }
        });

    changed
}

fn show_gem(ui: &mut egui::Ui, gem: &GemInfo) {
    let color = if !gem.enabled {
        super::theme::Theme::TEXT_DIM
    } else if gem.is_support {
        super::theme::Theme::GEM_SUPPORT
    } else {
        super::theme::Theme::GEM_ACTIVE
    };

    let text = if gem.quality > 0 {
        format!("  {} (Lv{}, {}% qual)", gem.name, gem.level, gem.quality)
    } else {
        format!("  {} (Lv{})", gem.name, gem.level)
    };

    ui.colored_label(color, text);
}

fn socket_group_title(group: &SocketGroup) -> String {
    if !group.label.is_empty() {
        return group.label.clone();
    }

    // Use the first active skill gem name as the title
    for gem in &group.gems {
        if gem.enabled && !gem.is_support && !gem.name.is_empty() {
            return gem.name.clone();
        }
    }

    // Fall back to first gem name
    group
        .gems
        .first()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| format!("Group {}", group.index))
}

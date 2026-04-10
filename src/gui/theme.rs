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

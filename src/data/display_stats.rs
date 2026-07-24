//! Sidebar stat list and warnings, read from upstream's pre-built display data.
//!
//! Upstream's `buildMode:RefreshStatList()` (Modules/Build.lua) evaluates the
//! full display-stat list (flags, condFunc, formatting, colors) and stores the
//! result in `build.controls.statBox.list`, plus warning strings in
//! `build.controls.warnings.lines`. It runs on every calc rebuild, so we just
//! read the result instead of reimplementing the ~200 stat definitions.

use mlua::prelude::*;

/// One line of the sidebar stat list. Strings contain PoB color codes
/// (`^7`, `^xRRGGBB`), to be rendered with `theme::parse_pob_colors`.
#[derive(Debug, Clone)]
pub struct StatLine {
    /// Upstream line height (16 = stat, 14 = annotation, 6/10 = spacer).
    pub height: f32,
    /// True for centered annotation lines (skill part names, DPS sources).
    pub center: bool,
    /// Left-hand text (usually "Label:"). None for spacer lines.
    pub lhs: Option<String>,
    /// Right-hand value text.
    pub rhs: Option<String>,
}

impl StatLine {
    pub fn is_spacer(&self) -> bool {
        self.lhs.is_none() && self.rhs.is_none()
    }
}

/// The full sidebar display data.
#[derive(Debug, Clone, Default)]
pub struct SidebarStats {
    pub lines: Vec<StatLine>,
    /// Build warnings (too many points, unaffordable skill costs, etc.).
    pub warnings: Vec<String>,
}

/// Extract the sidebar stat list and warnings from the Lua VM.
pub fn extract_sidebar_stats(lua: &Lua) -> Result<SidebarStats, mlua::Error> {
    let table: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local out = { lines = {}, warnings = {} }
            local list = build.controls.statBox and build.controls.statBox.list or {}
            for _, entry in ipairs(list) do
                table.insert(out.lines, {
                    height = entry.height or 16,
                    center = entry.align == "CENTER_X",
                    lhs = entry[1],
                    rhs = entry[2],
                })
            end
            local warn = build.controls.warnings and build.controls.warnings.lines or {}
            for _, line in ipairs(warn) do
                table.insert(out.warnings, line)
            end
            return out
            "#,
        )
        .eval()?;

    let mut result = SidebarStats::default();

    let lines: LuaTable = table.get("lines")?;
    for entry in lines.sequence_values::<LuaTable>() {
        let entry = entry?;
        result.lines.push(StatLine {
            height: entry.get("height").unwrap_or(16.0),
            center: entry.get("center").unwrap_or(false),
            lhs: entry.get("lhs").ok(),
            rhs: entry.get("rhs").ok(),
        });
    }

    let warnings: LuaTable = table.get("warnings")?;
    for line in warnings.sequence_values::<String>() {
        let line = line?;
        result.warnings.push(line);
    }

    Ok(result)
}

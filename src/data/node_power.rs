//! Node power: upstream's per-node stat-impact calculation (heatmap + report).
//!
//! The heavy lifting is upstream's `CalcsTab:PowerBuilder`, a coroutine that
//! runs one throwaway calc pass per candidate node and yields every ~100ms.
//! Upstream resumes it once per frame from the tree view; we do the same via
//! [`power_step`], then read the per-node results for the heatmap overlay and
//! the power report.

use std::collections::HashMap;

use mlua::prelude::*;

/// A selectable power stat (entry from `data.powerStatList`, minus the
/// item-only entries). `index` is 1-based into the unfiltered upstream list.
#[derive(Debug, Clone)]
pub struct PowerStat {
    pub index: usize,
    pub label: String,
}

/// One row of the power report.
#[derive(Debug, Clone)]
pub struct ReportRow {
    /// Node id (0 for unplaced cluster notables, which cannot be panned to).
    pub id: u32,
    pub name: String,
    pub power: f64,
    /// Formatted power value with PoB color codes.
    pub power_str: String,
    pub path_power: f64,
    /// Formatted path power per point with PoB color codes.
    pub path_power_str: String,
    pub allocated: bool,
    pub x: f64,
    pub y: f64,
    pub path_dist: i64,
    /// The node's stat description lines (upstream node.sd, copied into the
    /// report entry; shown as the row's hover tooltip).
    pub sd: Vec<String>,
}

/// List the power stats available for the heatmap dropdown (upstream's
/// TreeTab filter: entries not flagged `ignoreForNodes`).
pub fn list_power_stats(lua: &Lua) -> Result<Vec<PowerStat>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local out = {}
        for i, stat in ipairs(data.powerStatList) do
            if not stat.ignoreForNodes then
                table.insert(out, { index = i, label = stat.label })
            end
        end
        return out
    "#,
        )
        .eval()?;

    let mut stats = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        stats.push(PowerStat {
            index: entry.get("index").unwrap_or(0),
            label: entry.get("label").unwrap_or_default(),
        });
    }
    Ok(stats)
}

/// Select the power stat (1-based index into `data.powerStatList`) and max
/// path depth (None = unlimited), and flag the power data for a rebuild.
pub fn set_power_stat(
    lua: &Lua,
    stat_index: usize,
    max_depth: Option<i64>,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local statIndex, maxDepth = ...
        local build = mainObject_ref.main.modes['BUILD']
        local calcsTab = build.calcsTab
        calcsTab.powerStat = data.powerStatList[statIndex]
        calcsTab.nodePowerMaxDepth = maxDepth
        calcsTab.powerBuildFlag = true
    "#,
    )
    .call((stat_index, max_depth))
}

/// True when the power data needs (re)building: the build flag is set (e.g.
/// after any recalc) or the builder coroutine is mid-run.
pub fn power_dirty(lua: &Lua) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local calcsTab = mainObject_ref.main.modes['BUILD'].calcsTab
        return (calcsTab.powerBuildFlag or calcsTab.powerBuilder ~= nil) and true or false
    "#,
    )
    .eval()
}

/// Resume the power builder coroutine once (up to ~100ms of work). Returns
/// `(done, progress_percent)`.
pub fn power_step(lua: &Lua) -> Result<(bool, i64), mlua::Error> {
    let t: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local calcsTab = build.calcsTab
        build.powerBuilderProgressCallback = function(p)
            build._powerProgress = p
        end
        calcsTab:BuildPower()
        local done = calcsTab.powerBuilder == nil and not calcsTab.powerBuildFlag
        if done then
            build._powerProgress = nil
        end
        return { done = done, progress = build._powerProgress or 0 }
    "#,
        )
        .eval()?;
    Ok((
        t.get("done").unwrap_or(true),
        t.get("progress").unwrap_or(0),
    ))
}

/// Compute the heatmap tint per unallocated node from the finished power
/// data, using upstream's color formula (RED/BLUE theme: red = offence,
/// blue = defence; single-stat mode uses red only).
pub fn heatmap_colors(lua: &Lua) -> Result<HashMap<u32, egui::Color32>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local calcsTab = build.calcsTab
        local powerMax = calcsTab.powerMax or {}
        local statMode = calcsTab.powerStat and calcsTab.powerStat.stat
            and not calcsTab.powerStat.ignoreForNodes
        local out = {}
        for id, node in pairs(build.spec.nodes) do
            if not node.alloc and node.type ~= "ClassStart"
               and node.type ~= "AscendClassStart" and node.power then
                local r, g, b = 0, 0, 0
                if statMode then
                    local stat = math.max(node.power.singleStat or 0, 0)
                    local max = powerMax.singleStat or 0
                    r = max > 0 and (stat / max * 1.5) ^ 0.5 or 0
                else
                    local offence = math.max(node.power.offence or 0, 0)
                    local defence = math.max(node.power.defence or 0, 0)
                    local maxOff = powerMax.offence or 0
                    local maxDef = powerMax.defence or 0
                    local dpsCol = maxOff > 0 and (offence / maxOff * 1.5) ^ 0.5 or 0
                    local defCol = maxDef > 0 and (defence / maxDef * 1.5) ^ 0.5 or 0
                    r = dpsCol
                    g = (math.max(dpsCol - 0.5, 0) + math.max(defCol - 0.5, 0)) / 2
                    b = defCol
                end
                table.insert(out, {
                    id = id,
                    r = math.min(r, 1), g = math.min(g, 1), b = math.min(b, 1),
                })
            end
        end
        return out
    "#,
        )
        .eval()?;

    let mut colors = HashMap::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let id: u32 = entry.get("id").unwrap_or(0);
        let r: f32 = entry.get("r").unwrap_or(0.0);
        let g: f32 = entry.get("g").unwrap_or(0.0);
        let b: f32 = entry.get("b").unwrap_or(0.0);
        colors.insert(
            id,
            egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
        );
    }
    Ok(colors)
}

/// Build the power report via upstream's `TreeTab:BuildPowerReportList`.
pub fn power_report(lua: &Lua) -> Result<Vec<ReportRow>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local powerStat = build.calcsTab.powerStat or data.powerStatList[1]
        return build.treeTab:BuildPowerReportList(powerStat)
    "#,
        )
        .eval()?;

    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        rows.push(ReportRow {
            id: entry.get("id").unwrap_or(0),
            name: entry.get("name").unwrap_or_default(),
            power: entry.get("power").unwrap_or(0.0),
            power_str: entry.get("powerStr").unwrap_or_default(),
            path_power: entry.get("pathPower").unwrap_or(0.0),
            path_power_str: entry.get("pathPowerStr").unwrap_or_default(),
            allocated: entry.get("allocated").unwrap_or(false),
            x: entry.get("x").unwrap_or(0.0),
            y: entry.get("y").unwrap_or(0.0),
            path_dist: entry.get("pathDist").unwrap_or(1),
            sd: entry
                .get::<LuaTable>("sd")
                .map(|t| t.sequence_values::<String>().flatten().collect())
                .unwrap_or_default(),
        });
    }
    Ok(rows)
}

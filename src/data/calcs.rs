//! Calcs tab data: formatted calculation sections and per-stat breakdowns,
//! extracted via a Lua helper that reuses upstream's own formatting logic.

use mlua::prelude::*;

/// One top-level section (e.g. "Skill Hit Damage").
#[derive(Debug, Clone)]
pub struct CalcSection {
    /// Original index into Lua's sectionList (for breakdown addressing).
    pub si: usize,
    pub id: String,
    /// Layout group from upstream: 1 = offence column, 2+ = defence columns.
    pub group: i64,
    /// PoB colour code for the section border (e.g. "^xE05030").
    pub colour: String,
    pub subsections: Vec<CalcSubsection>,
}

#[derive(Debug, Clone)]
pub struct CalcSubsection {
    pub ui: usize,
    pub label: String,
    /// Extra header text (e.g. the combined damage display).
    pub extra: Option<String>,
    /// Collapsed by default in upstream.
    pub collapsed: bool,
    pub rows: Vec<CalcRow>,
}

#[derive(Debug, Clone)]
pub struct CalcRow {
    pub ri: usize,
    pub label: Option<String>,
    pub cells: Vec<CalcCell>,
}

#[derive(Debug, Clone)]
pub struct CalcCell {
    pub ci: usize,
    /// Formatted display text (may contain PoB colour codes).
    pub text: String,
    /// True if clicking this cell can show a breakdown.
    pub has_breakdown: bool,
}

/// One section of a stat breakdown: either formula text lines or a table.
#[derive(Debug, Clone)]
pub enum BreakdownSection {
    Text {
        lines: Vec<String>,
    },
    Table {
        label: Option<String>,
        footer: Option<String>,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// Current calcs input state.
#[derive(Debug, Clone)]
pub struct CalcsInput {
    pub skill_number: usize,
    pub buff_mode: String,
    pub show_minion: bool,
    pub has_minion: bool,
}

/// Buff modes accepted by the calcs environment, with display labels.
pub const BUFF_MODES: &[(&str, &str)] = &[
    ("UNBUFFED", "Unbuffed"),
    ("BUFFED", "Buffed"),
    ("COMBAT", "In Combat"),
    ("EFFECTIVE", "Effective DPS"),
];

/// Load the Lua helper functions into the VM (idempotent).
fn ensure_helper(lua: &Lua) -> Result<(), mlua::Error> {
    let loaded: bool = lua
        .load("return pob_calcs_extract ~= nil")
        .eval()
        .unwrap_or(false);
    if !loaded {
        lua.load(include_str!("calcs_helper.lua")).exec()?;
    }
    Ok(())
}

/// Extract all visible calc sections with formatted cell text.
pub fn extract_sections(lua: &Lua) -> Result<Vec<CalcSection>, mlua::Error> {
    ensure_helper(lua)?;
    let sections_table: LuaTable = lua.load("return pob_calcs_extract()").eval()?;

    let mut sections = Vec::new();
    for section_entry in sections_table.sequence_values::<LuaTable>() {
        let section = section_entry?;
        let subs_table: LuaTable = section.get("subsections")?;
        let mut subsections = Vec::new();
        for sub_entry in subs_table.sequence_values::<LuaTable>() {
            let sub = sub_entry?;
            let rows_table: LuaTable = sub.get("rows")?;
            let mut rows = Vec::new();
            for row_entry in rows_table.sequence_values::<LuaTable>() {
                let row = row_entry?;
                let cells_table: LuaTable = row.get("cells")?;
                let mut cells = Vec::new();
                for cell_entry in cells_table.sequence_values::<LuaTable>() {
                    let cell = cell_entry?;
                    cells.push(CalcCell {
                        ci: cell.get("ci")?,
                        text: cell.get("text").unwrap_or_default(),
                        has_breakdown: cell.get("hasBreakdown").unwrap_or(false),
                    });
                }
                rows.push(CalcRow {
                    ri: row.get("ri")?,
                    label: row.get("label").ok(),
                    cells,
                });
            }
            subsections.push(CalcSubsection {
                ui: sub.get("ui")?,
                label: sub.get("label").unwrap_or_default(),
                extra: sub.get("extra").ok(),
                collapsed: sub.get("collapsed").unwrap_or(false),
                rows,
            });
        }
        sections.push(CalcSection {
            si: section.get("si")?,
            id: section.get("id").unwrap_or_default(),
            group: section.get("group").unwrap_or(1),
            colour: section.get("colour").unwrap_or_else(|_| "^7".to_string()),
            subsections,
        });
    }
    Ok(sections)
}

/// Fetch the breakdown for a cell, addressed by original Lua indices.
pub fn fetch_breakdown(
    lua: &Lua,
    si: usize,
    ui: usize,
    ri: usize,
    ci: usize,
) -> Result<Vec<BreakdownSection>, mlua::Error> {
    ensure_helper(lua)?;
    let sections_table: LuaTable = lua
        .load(format!(
            "return pob_calcs_breakdown({si}, {ui}, {ri}, {ci})"
        ))
        .eval()?;

    let mut sections = Vec::new();
    for entry in sections_table.sequence_values::<LuaTable>() {
        let section = entry?;
        let section_type: String = section.get("type")?;
        match section_type.as_str() {
            "TEXT" => {
                let lines_table: LuaTable = section.get("lines")?;
                let lines = lines_table.sequence_values::<String>().flatten().collect();
                sections.push(BreakdownSection::Text { lines });
            }
            "TABLE" => {
                let cols_table: LuaTable = section.get("cols")?;
                let columns: Vec<String> =
                    cols_table.sequence_values::<String>().flatten().collect();
                let rows_table: LuaTable = section.get("rows")?;
                let mut rows = Vec::new();
                for row_entry in rows_table.sequence_values::<LuaTable>() {
                    let row = row_entry?;
                    rows.push(row.sequence_values::<String>().flatten().collect());
                }
                sections.push(BreakdownSection::Table {
                    label: section.get("label").ok(),
                    footer: section.get("footer").ok(),
                    columns,
                    rows,
                });
            }
            _ => {}
        }
    }
    Ok(sections)
}

/// Read the current calcs input state.
pub fn get_input(lua: &Lua) -> Result<CalcsInput, mlua::Error> {
    ensure_helper(lua)?;
    let input: LuaTable = lua.load("return pob_calcs_get_input()").eval()?;
    Ok(CalcsInput {
        skill_number: input.get("skillNumber").unwrap_or(1),
        buff_mode: input
            .get("buffMode")
            .unwrap_or_else(|_| "EFFECTIVE".to_string()),
        show_minion: input.get("showMinion").unwrap_or(false),
        has_minion: input.get("hasMinion").unwrap_or(false),
    })
}

/// Set the socket group used by the calcs environment.
pub fn set_skill_number(lua: &Lua, skill_number: usize) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_calcs_set_input(...)")
        .call(("skill_number", skill_number))
}

/// Set the calculation buff mode (see BUFF_MODES).
pub fn set_buff_mode(lua: &Lua, mode: &str) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_calcs_set_input(...)")
        .call(("misc_buffMode", mode))
}

/// The calcs view's active skill and skill part selection for the currently
/// selected socket group. The calcs tab has its own selection, independent of
/// the sidebar (upstream's `mainActiveSkillCalcs` / `skillPartCalcs`).
#[derive(Debug, Clone, Default)]
pub struct CalcsSkillSelection {
    pub skills: Vec<String>,
    /// Selected skill, 0-based.
    pub selected_skill: usize,
    pub parts: Vec<String>,
    /// Selected part, 0-based.
    pub selected_part: usize,
}

/// Read the calcs-mode active skill / part selection.
pub fn skill_selection(lua: &Lua) -> Result<CalcsSkillSelection, mlua::Error> {
    let t: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local groupIdx = (build.calcsTab.input and build.calcsTab.input.skill_number)
            or build.mainSocketGroup or 1
        local group = build.skillsTab.socketGroupList[groupIdx]
        local out = { skills = {}, selSkill = 1, parts = {}, selPart = 1 }
        if group and group.displaySkillListCalcs then
            for _, skill in ipairs(group.displaySkillListCalcs) do
                local name = skill.activeEffect and skill.activeEffect.grantedEffect
                    and skill.activeEffect.grantedEffect.name or "?"
                table.insert(out.skills, name)
            end
            out.selSkill = group.mainActiveSkillCalcs or 1
            local skill = group.displaySkillListCalcs[out.selSkill]
            if skill and skill.activeEffect and skill.activeEffect.grantedEffect then
                local ge = skill.activeEffect.grantedEffect
                local src = skill.activeEffect.srcInstance
                if ge.parts then
                    for _, part in ipairs(ge.parts) do
                        table.insert(out.parts, part.name or "?")
                    end
                    out.selPart = src and (src.skillPartCalcs or src.skillPart) or 1
                end
            end
        end
        return out
    "#,
        )
        .eval()?;

    let get_vec = |key: &str| -> Vec<String> {
        t.get::<LuaTable>(key)
            .map(|tbl| {
                tbl.sequence_values::<String>()
                    .filter_map(|r| r.ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    let skills = get_vec("skills");
    let parts = get_vec("parts");
    Ok(CalcsSkillSelection {
        selected_skill: t
            .get::<usize>("selSkill")
            .unwrap_or(1)
            .saturating_sub(1)
            .min(skills.len().saturating_sub(1)),
        skills,
        selected_part: t
            .get::<usize>("selPart")
            .unwrap_or(1)
            .saturating_sub(1)
            .min(parts.len().saturating_sub(1)),
        parts,
    })
}

/// Select the calcs-mode active skill (0-based index).
pub fn set_active_skill(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local groupIdx = (build.calcsTab.input and build.calcsTab.input.skill_number)
            or build.mainSocketGroup or 1
        local group = build.skillsTab.socketGroupList[groupIdx]
        if group then
            group.mainActiveSkillCalcs = {lua_idx}
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

/// Select the calcs-mode skill part (0-based index).
pub fn set_skill_part(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    let lua_idx = index + 1;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local groupIdx = (build.calcsTab.input and build.calcsTab.input.skill_number)
            or build.mainSocketGroup or 1
        local group = build.skillsTab.socketGroupList[groupIdx]
        local skill = group and group.displaySkillListCalcs
            and group.displaySkillListCalcs[group.mainActiveSkillCalcs or 1]
        if skill and skill.activeEffect and skill.activeEffect.srcInstance then
            skill.activeEffect.srcInstance.skillPartCalcs = {lua_idx}
            build.calcsTab:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

/// Toggle between player and minion stats.
pub fn set_show_minion(lua: &Lua, show: bool) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_calcs_set_input(...)")
        .call(("showMinion", show))
}

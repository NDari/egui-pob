//! Compare tab data: wrappers over upstream CompareTab/CompareEntry (a
//! second full calc environment in the same VM) plus the ported summary
//! stat-list (see compare_helper.lua and ports.toml).

use mlua::prelude::*;

/// One row of the summary stat comparison.
#[derive(Debug, Clone)]
pub struct CompareStatRow {
    /// Section spacer (no content).
    pub spacer: bool,
    pub label: String,
    /// PoB color code from the stat definition, when set.
    pub label_color: Option<String>,
    pub primary: String,
    pub compare: String,
    pub diff: String,
    /// 1 = compare is better, -1 = worse, 0 = no difference.
    pub better: i32,
}

/// Load the Lua helper functions into the VM (idempotent).
fn ensure_helper(lua: &Lua) -> Result<(), mlua::Error> {
    let loaded: bool = lua
        .load("return pob_compare ~= nil")
        .eval()
        .unwrap_or(false);
    if !loaded {
        lua.load(include_str!("compare_helper.lua")).exec()?;
    }
    Ok(())
}

/// Import a build XML as a comparison entry (upstream CompareTab:ImportBuild:
/// creates a CompareEntry with its own tabs and calc output). Returns false
/// when the XML failed to load.
pub fn import_build(lua: &Lua, xml: &str, label: &str) -> Result<bool, mlua::Error> {
    ensure_helper(lua)?;
    lua.load("return pob_compare.import(...)")
        .call((xml, label))
}

/// Import a comparison build from a share code (upstream ImportFromCode).
pub fn import_code(lua: &Lua, code: &str) -> Result<bool, mlua::Error> {
    ensure_helper(lua)?;
    lua.load("return pob_compare.importCode(...)").call(code)
}

/// Comparison entry labels + the active entry (0 = none, else 1-based).
pub fn list_entries(lua: &Lua) -> Result<(Vec<String>, usize), mlua::Error> {
    ensure_helper(lua)?;
    let result: LuaTable = lua.load("return pob_compare.list()").eval()?;
    let entries: LuaTable = result.get("entries")?;
    Ok((
        entries.sequence_values::<String>().flatten().collect(),
        result.get("active").unwrap_or(0),
    ))
}

/// Remove a comparison entry (1-based).
pub fn remove_entry(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_compare.remove(...)").call(index)
}

/// Select the active comparison entry (1-based).
pub fn set_active(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_compare.setActive(...)").call(index)
}

/// The primary build's calc revision, for cheap staleness checks.
pub fn primary_revision(lua: &Lua) -> Result<i64, mlua::Error> {
    ensure_helper(lua)?;
    lua.load("return pob_compare.revision()").eval()
}

/// The summary stat comparison rows for an entry (1-based), in upstream's
/// display-stat order with upstream's filtering, formatting, and
/// better/worse judgement.
pub fn stat_rows(lua: &Lua, index: usize) -> Result<Vec<CompareStatRow>, mlua::Error> {
    ensure_helper(lua)?;
    let list: LuaTable = lua.load("return pob_compare.statRows(...)").call(index)?;
    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        if entry.get("spacer").unwrap_or(false) {
            rows.push(CompareStatRow {
                spacer: true,
                label: String::new(),
                label_color: None,
                primary: String::new(),
                compare: String::new(),
                diff: String::new(),
                better: 0,
            });
            continue;
        }
        rows.push(CompareStatRow {
            spacer: false,
            label: entry.get("label").unwrap_or_default(),
            label_color: entry.get("labelColor").ok(),
            primary: entry.get("primaryStr").unwrap_or_default(),
            compare: entry.get("compareStr").unwrap_or_default(),
            diff: entry.get("diffStr").unwrap_or_default(),
            better: entry.get("better").unwrap_or(0),
        });
    }
    Ok(rows)
}

/// Named tree diff for the compare tree view.
#[derive(Debug, Clone, Default)]
pub struct TreeDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub mastery: Vec<String>,
    /// The compare spec's tree version (may differ from the primary's).
    pub version: String,
}

/// Node-name diff between the primary spec and a compare entry's spec.
pub fn tree_diff(lua: &Lua, index: usize) -> Result<TreeDiff, mlua::Error> {
    ensure_helper(lua)?;
    let t: LuaTable = lua.load("return pob_compare.treeDiff(...)").call(index)?;
    let get_list = |key: &str| -> Vec<String> {
        t.get::<LuaTable>(key)
            .map(|l| l.sequence_values::<String>().flatten().collect())
            .unwrap_or_default()
    };
    Ok(TreeDiff {
        added: get_list("added"),
        removed: get_list("removed"),
        mastery: get_list("mastery"),
        version: t.get("version").unwrap_or_default(),
    })
}

/// Copy the compare entry's active spec into the primary build (upstream
/// CopyCompareSpecToPrimary; jewels are not copied).
pub fn copy_spec(lua: &Lua, and_use: bool) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_compare.copySpec(...)").call(and_use)
}

/// One row of the item slot comparison.
#[derive(Debug, Clone)]
pub struct ItemRow {
    pub slot: String,
    /// Slot name to pass to copy_item (differs for jewels).
    pub copy_slot: Option<String>,
    pub is_jewel: bool,
    pub primary: String,
    pub primary_rarity: String,
    pub compare: String,
    pub compare_rarity: String,
    /// "(match)" / "(missing)" / "(extra)" / "(different)" / "(both empty)".
    pub status: String,
    pub can_copy: bool,
    pub primary_warn: bool,
    pub compare_warn: bool,
}

/// Item comparison rows: slot union incl. Ring 3, abyss sockets, and jewels.
pub fn item_rows(lua: &Lua, index: usize) -> Result<Vec<ItemRow>, mlua::Error> {
    ensure_helper(lua)?;
    let list: LuaTable = lua.load("return pob_compare.itemRows(...)").call(index)?;
    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        rows.push(ItemRow {
            slot: entry.get("slot").unwrap_or_default(),
            copy_slot: entry.get("copySlot").ok(),
            is_jewel: entry.get("isJewel").unwrap_or(false),
            primary: entry.get("primary").unwrap_or_default(),
            primary_rarity: entry.get("primaryRarity").unwrap_or_default(),
            compare: entry.get("compare").unwrap_or_default(),
            compare_rarity: entry.get("compareRarity").unwrap_or_default(),
            status: entry.get("status").unwrap_or_default(),
            can_copy: entry.get("canCopy").unwrap_or(false),
            primary_warn: entry.get("primaryWarn").unwrap_or(false),
            compare_warn: entry.get("compareWarn").unwrap_or(false),
        });
    }
    Ok(rows)
}

/// Copy the compare entry's item in a slot into the primary build (upstream
/// CopyCompareItemToPrimary); `and_use` also equips it.
pub fn copy_item(lua: &Lua, index: usize, slot: &str, and_use: bool) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_compare.copyItem(...)")
        .call((index, slot, and_use))
}

/// A gem in the skills comparison ("common", "additional", or "missing").
#[derive(Debug, Clone)]
pub struct CompareGem {
    pub name: String,
    pub status: String,
    pub level: i64,
    pub quality: i64,
}

/// A paired socket-group row in the skills comparison.
#[derive(Debug, Clone)]
pub struct SkillRow {
    pub primary_label: String,
    pub compare_label: String,
    pub primary_gems: Vec<CompareGem>,
    pub compare_gems: Vec<CompareGem>,
}

/// Skills comparison rows (upstream DrawSkills' Jaccard group pairing).
pub fn skill_rows(lua: &Lua, index: usize) -> Result<Vec<SkillRow>, mlua::Error> {
    ensure_helper(lua)?;
    let list: LuaTable = lua.load("return pob_compare.skillRows(...)").call(index)?;
    let read_gems = |t: &LuaTable, key: &str| -> Vec<CompareGem> {
        t.get::<LuaTable>(key)
            .map(|l| {
                l.sequence_values::<LuaTable>()
                    .flatten()
                    .map(|g| CompareGem {
                        name: g.get("name").unwrap_or_default(),
                        status: g.get("status").unwrap_or_default(),
                        level: g.get("level").unwrap_or(0),
                        quality: g.get("quality").unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        rows.push(SkillRow {
            primary_label: entry.get("primaryLabel").unwrap_or_default(),
            compare_label: entry.get("compareLabel").unwrap_or_default(),
            primary_gems: read_gems(&entry, "primaryGems"),
            compare_gems: read_gems(&entry, "compareGems"),
        });
    }
    Ok(rows)
}

/// One config row (formatted values per side).
#[derive(Debug, Clone)]
pub struct ConfigRow {
    pub label: String,
    pub primary: String,
    pub compare: String,
}

/// A config section with differing rows first.
#[derive(Debug, Clone)]
pub struct ConfigSection {
    pub name: String,
    pub diffs: Vec<ConfigRow>,
    pub commons: Vec<ConfigRow>,
}

/// Config comparison grouped by section (upstream LayoutConfigView pass 1).
pub fn config_rows(lua: &Lua, index: usize) -> Result<Vec<ConfigSection>, mlua::Error> {
    ensure_helper(lua)?;
    let list: LuaTable = lua.load("return pob_compare.configRows(...)").call(index)?;
    let read_rows = |t: &LuaTable, key: &str| -> Vec<ConfigRow> {
        t.get::<LuaTable>(key)
            .map(|l| {
                l.sequence_values::<LuaTable>()
                    .flatten()
                    .map(|r| ConfigRow {
                        label: r.get("label").unwrap_or_default(),
                        primary: r.get("primary").unwrap_or_default(),
                        compare: r.get("compare").unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut sections = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        sections.push(ConfigSection {
            name: entry.get("name").unwrap_or_default(),
            diffs: read_rows(&entry, "diffs"),
            commons: read_rows(&entry, "commons"),
        });
    }
    Ok(sections)
}

/// Merge the compare entry's config into the primary (upstream
/// CopyCompareConfig; keys only in the primary survive).
pub fn copy_config(lua: &Lua) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    lua.load("pob_compare.copyConfig()").exec()
}

/// Configure the compare power report: `stat_index` is 1-based into
/// data.powerStatList (0 clears), plus the five category toggles.
pub fn power_set_stat(
    lua: &Lua,
    stat_index: usize,
    categories: [bool; 5],
) -> Result<(), mlua::Error> {
    ensure_helper(lua)?;
    let [tree, items, skills, supports, config] = categories;
    lua.load("pob_compare.powerSetStat(...)")
        .call((stat_index, tree, items, skills, supports, config))
}

/// Advance the compare power report one step (upstream RunComparePowerReport
/// coroutine). Returns (done, progress 0-100). NOTE: the builder temporarily
/// mutates the primary build; refresh primary-derived panels after a run.
pub fn power_step(lua: &Lua, index: usize) -> Result<(bool, i64), mlua::Error> {
    ensure_helper(lua)?;
    let t: LuaTable = lua.load("return pob_compare.powerStep(...)").call(index)?;
    Ok((
        t.get("done").unwrap_or(true),
        t.get("progress").unwrap_or(0),
    ))
}

/// One compare power report row.
#[derive(Debug, Clone)]
pub struct PowerRow {
    pub category: String,
    pub name: String,
    pub impact: f64,
    pub impact_str: String,
    pub per_point: String,
    pub path_dist: i64,
}

/// The finished power report, sorted by impact descending.
pub fn power_results(lua: &Lua) -> Result<Vec<PowerRow>, mlua::Error> {
    ensure_helper(lua)?;
    let list: LuaTable = lua.load("return pob_compare.powerResults()").eval()?;
    let mut rows = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        rows.push(PowerRow {
            category: entry.get("category").unwrap_or_default(),
            name: entry.get("name").unwrap_or_default(),
            impact: entry.get("impact").unwrap_or(0.0),
            impact_str: entry.get("impactStr").unwrap_or_default(),
            per_point: entry.get("perPoint").unwrap_or_default(),
            path_dist: entry.get("pathDist").unwrap_or(0),
        });
    }
    Ok(rows)
}

/// One row of the calcs comparison (column-1 text per side).
#[derive(Debug, Clone)]
pub struct CalcCompareRow {
    pub label: String,
    pub primary: String,
    pub compare: String,
}

/// A subsection of the calcs comparison.
#[derive(Debug, Clone)]
pub struct CalcCompareSubsection {
    pub label: String,
    pub primary_extra: String,
    pub compare_extra: String,
    pub rows: Vec<CalcCompareRow>,
}

/// A section card of the calcs comparison.
#[derive(Debug, Clone)]
pub struct CalcCompareSection {
    pub id: String,
    pub subsections: Vec<CalcCompareSubsection>,
}

/// The calcs comparison (upstream DrawCalcs filter); `only_diff` keeps only
/// rows whose value/mods/breakdown differ between the builds.
pub fn calc_sections(
    lua: &Lua,
    index: usize,
    only_diff: bool,
) -> Result<Vec<CalcCompareSection>, mlua::Error> {
    // The port lives in calcs_helper.lua (it shares the FormatStr port)
    let loaded: bool = lua
        .load("return pob_compare_calc_sections ~= nil")
        .eval()
        .unwrap_or(false);
    if !loaded {
        lua.load(include_str!("calcs_helper.lua")).exec()?;
    }
    let list: LuaTable = lua
        .load("return pob_compare_calc_sections(...)")
        .call((index, only_diff))?;
    let mut sections = Vec::new();
    for section in list.sequence_values::<LuaTable>() {
        let section = section?;
        let subs: LuaTable = section.get("subsections")?;
        let mut subsections = Vec::new();
        for sub in subs.sequence_values::<LuaTable>() {
            let sub = sub?;
            let rows_table: LuaTable = sub.get("rows")?;
            let mut rows = Vec::new();
            for row in rows_table.sequence_values::<LuaTable>() {
                let row = row?;
                rows.push(CalcCompareRow {
                    label: row.get("label").unwrap_or_default(),
                    primary: row.get("primary").unwrap_or_default(),
                    compare: row.get("compare").unwrap_or_default(),
                });
            }
            subsections.push(CalcCompareSubsection {
                label: sub.get("label").unwrap_or_default(),
                primary_extra: sub.get("primaryExtra").unwrap_or_default(),
                compare_extra: sub.get("compareExtra").unwrap_or_default(),
                rows,
            });
        }
        sections.push(CalcCompareSection {
            id: section.get("id").unwrap_or_default(),
            subsections,
        });
    }
    Ok(sections)
}

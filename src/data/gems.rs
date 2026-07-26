//! Gem search backed by upstream's GemSelectControl: name patterns
//! (exact, abbreviation, prefix, contains), tag filters (`:tag`, `:-tag`),
//! support-compatibility, and optional DPS-impact sorting.

use mlua::prelude::*;

/// A gem returned by the search.
#[derive(Debug, Clone)]
pub struct GemChoice {
    pub name: String,
    /// Primary attribute: "str", "dex", "int", or "" (white).
    pub attribute: String,
    pub is_support: bool,
    /// True if this support gem can support the socket group's active skills.
    pub can_support: bool,
    /// DPS with this gem added (only meaningful when sorted by DPS).
    pub dps: f64,
    /// Colour code for the DPS value ("^x228866" better, "^xFF4422" worse).
    pub dps_color: String,
}

/// A gem search pass. While `pending` is true, DPS values are still being
/// computed incrementally: call [`search_gems`] again next frame to resume
/// the work and get updated results (upstream's progressive DPSBuilder,
/// resumed one ~50ms slice per call like upstream's Draw).
#[derive(Debug, Clone, Default)]
pub struct GemSearchResults {
    pub choices: Vec<GemChoice>,
    pub pending: bool,
    /// DPS-sort progress 0-100 while pending.
    pub progress: i64,
}

/// Search gems for a socket group using upstream's GemSelectControl matching.
/// `sort_by_dps` enables upstream's DPS-impact sort (runs one calc per
/// candidate support; computed incrementally, see [`GemSearchResults`];
/// cached per group and build revision).
/// `imbued` switches to upstream's imbued-select mode: only non-exceptional
/// supports that can support the group's active skills (a separate cached
/// control, since imbuedSelect changes list/filter semantics).
pub fn search_gems(
    lua: &Lua,
    group_index: usize,
    query: &str,
    sort_by_dps: bool,
    limit: usize,
    imbued: bool,
) -> Result<GemSearchResults, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local groupIndex, query, sortByDPS, limit, imbued = ...
            local build = mainObject_ref.main.modes['BUILD']
            local skillsTab = build.skillsTab
            local group = skillsTab.socketGroupList[groupIndex]
            if not group then
                return {}
            end
            skillsTab.displayGroup = group
            skillsTab.sortGemsByDPS = sortByDPS
            local ctrl
            if imbued then
                ctrl = skillsTab._eguiImbuedSelect
                if not ctrl then
                    ctrl = new("GemSelectControl", nil, {0, 0, 300, 20}, skillsTab,
                               1, function() end, true, true)
                    skillsTab._eguiImbuedSelect = ctrl
                end
            else
                ctrl = skillsTab._eguiGemSelect
                if not ctrl then
                    ctrl = new("GemSelectControl", nil, {0, 0, 300, 20}, skillsTab,
                               #group.gemList + 1, function() end)
                    skillsTab._eguiGemSelect = ctrl
                end
            end
            ctrl.index = #group.gemList + 1
            ctrl.searchStr = query
            ctrl:UpdateSortCache()
            -- v2.66+ computes DPS progressively in the DPSBuilder coroutine.
            -- Mirror upstream's Draw: create it when UpdateSortCache raised
            -- dpsBuildFlag, resume exactly one ~50ms slice per call, and let
            -- the GUI poll again next frame while work remains.
            if ctrl.dpsBuildFlag then
                ctrl.dpsBuildFlag = false
                ctrl.dpsBuilder = coroutine.create(ctrl.DPSBuilder)
                ctrl.dpsBuilderCallback = function(percentage)
                    ctrl._eguiSortProgress = percentage
                end
            end
            if ctrl.dpsBuilder then
                local ok, err = coroutine.resume(ctrl.dpsBuilder, ctrl)
                if not ok then
                    error(err)
                end
                if coroutine.status(ctrl.dpsBuilder) == "dead" then
                    ctrl.dpsBuilder = nil
                    ctrl._eguiSortProgress = nil
                end
            end
            local pending = ctrl.dpsBuilder ~= nil
            ctrl:BuildList(query)
            local sortCache = ctrl.sortCache or { canSupport = {}, dps = {}, dpsColor = {} }
            local result = {}
            for i, gemId in ipairs(ctrl.list) do
                if i > limit then break end
                local gemData = ctrl.gems[gemId]
                if gemData then
                    local attribute = ""
                    if gemData.tags.strength then
                        attribute = "str"
                    elseif gemData.tags.dexterity then
                        attribute = "dex"
                    elseif gemData.tags.intelligence then
                        attribute = "int"
                    end
                    table.insert(result, {
                        name = gemData.name,
                        attribute = attribute,
                        isSupport = gemData.grantedEffect.support == true,
                        canSupport = sortCache.canSupport[gemId] == true,
                        dps = sortCache.dps[gemId] or 0,
                        dpsColor = sortCache.dpsColor[gemId] or "",
                    })
                end
            end
            return {
                list = result,
                pending = pending,
                progress = ctrl._eguiSortProgress or 0,
            }
        "#,
        )
        .call((group_index, query, sort_by_dps, limit, imbued))?;

    let mut choices = Vec::new();
    if let Ok(list) = result.get::<LuaTable>("list") {
        for pair in list.sequence_values::<LuaTable>() {
            let entry = pair?;
            choices.push(GemChoice {
                name: entry.get("name").unwrap_or_default(),
                attribute: entry.get("attribute").unwrap_or_default(),
                is_support: entry.get("isSupport").unwrap_or(false),
                can_support: entry.get("canSupport").unwrap_or(false),
                dps: entry.get("dps").unwrap_or(0.0),
                dps_color: entry.get("dpsColor").unwrap_or_default(),
            });
        }
    }
    Ok(GemSearchResults {
        choices,
        pending: result.get("pending").unwrap_or(false),
        progress: result.get("progress").unwrap_or(0),
    })
}

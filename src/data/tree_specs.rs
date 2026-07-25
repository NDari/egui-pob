//! Multiple passive tree specs per build: list/switch/create/copy/rename/
//! delete, tree URL import/export, and spec comparison. All mutations mirror
//! upstream's PassiveSpecListControl and TreeTab popups.

use std::collections::{HashMap, HashSet};

use mlua::prelude::*;

/// A passive tree spec in the build's spec list.
#[derive(Debug, Clone)]
pub struct SpecInfo {
    pub title: String,
    /// Tree version like "3_28"; shown when it differs from the latest.
    pub tree_version: String,
    pub is_latest_version: bool,
}

/// List all specs and the active spec index (1-based, matching Lua).
pub fn list_specs(lua: &Lua) -> Result<(Vec<SpecInfo>, usize), mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local treeTab = build.treeTab
            local list = {}
            for _, spec in ipairs(treeTab.specList) do
                table.insert(list, {
                    title = spec.title or "Default",
                    version = spec.treeVersion or "",
                    -- Prefix match like upstream's showConvert: subtype trees
                    -- (e.g. 3_28_ruthless) still count as the latest version
                    isLatest = spec.treeVersion:match("^" .. latestTreeVersion) ~= nil,
                })
            end
            return { specs = list, active = treeTab.activeSpec or 1 }
        "#,
        )
        .eval()?;

    let specs_table: LuaTable = result.get("specs")?;
    let mut specs = Vec::new();
    for pair in specs_table.sequence_values::<LuaTable>() {
        let entry = pair?;
        specs.push(SpecInfo {
            title: entry.get("title").unwrap_or_default(),
            tree_version: entry.get("version").unwrap_or_default(),
            is_latest_version: entry.get("isLatest").unwrap_or(true),
        });
    }
    let active: usize = result.get("active").unwrap_or(1);
    Ok((specs, active))
}

/// Switch the active spec (1-based index). Handles jewel re-socketing via
/// upstream's SetActiveSpec.
pub fn set_active_spec(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        if treeTab.specList[index] then
            treeTab:SetActiveSpec(index)
            _runCallback('OnFrame')
        end
    "#,
    )
    .call(index)
}

/// Create a new empty spec with the build's current class selection and
/// switch to it.
pub fn new_spec(lua: &Lua, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        local newSpec = new("PassiveSpec", build, latestTreeVersion)
        newSpec:SelectClass(build.spec.curClassId)
        newSpec:SelectAscendClass(build.spec.curAscendClassId)
        newSpec:SelectSecondaryAscendClass(build.spec.curSecondaryAscendClassId)
        newSpec.title = title
        table.insert(treeTab.specList, newSpec)
        treeTab.modFlag = true
        treeTab:SetActiveSpec(#treeTab.specList)
        build.spec:AddUndoState()
        _runCallback('OnFrame')
    "#,
    )
    .call(title)
}

/// Copy an existing spec (1-based index) and switch to the copy.
pub fn copy_spec(lua: &Lua, index: usize, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        local src = treeTab.specList[index]
        if not src then return end
        local newSpec = new("PassiveSpec", build, src.treeVersion)
        newSpec.title = title
        newSpec.jewels = copyTable(src.jewels)
        newSpec:RestoreUndoState(src:CreateUndoState())
        newSpec:BuildClusterJewelGraphs()
        table.insert(treeTab.specList, newSpec)
        treeTab.modFlag = true
        treeTab:SetActiveSpec(#treeTab.specList)
        build.spec:AddUndoState()
        _runCallback('OnFrame')
    "#,
    )
    .call((index, title))
}

/// Rename a spec (1-based index).
pub fn rename_spec(lua: &Lua, index: usize, title: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        if treeTab.specList[index] then
            treeTab.specList[index].title = title
            treeTab.modFlag = true
        end
    "#,
    )
    .call((index, title))
}

/// Delete a spec (1-based index). The last remaining spec cannot be deleted.
/// Mirrors PassiveSpecListControl:OnSelDelete.
pub fn delete_spec(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        local list = treeTab.specList
        if not list[index] or #list <= 1 then return end
        table.remove(list, index)
        if index == treeTab.activeSpec then
            treeTab:SetActiveSpec(math.max(1, index - 1))
        else
            for i, spec in ipairs(list) do
                if spec == build.spec then
                    treeTab.activeSpec = i
                    break
                end
            end
        end
        treeTab.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call(index)
}

/// Move a spec up or down in the list (1-based index, delta -1 or +1).
/// The active spec index is re-derived from the current spec's new position,
/// mirroring PassiveSpecListControl:OnOrderChange.
pub fn move_spec(lua: &Lua, index: usize, delta: i64) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local index, delta = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab
        local list = treeTab.specList
        local target = index + delta
        if not list[index] or not list[target] then return end
        list[index], list[target] = list[target], list[index]
        for i, spec in ipairs(list) do
            if spec == build.spec then
                treeTab.activeSpec = i
                break
            end
        end
        treeTab.modFlag = true
    "#,
    )
    .call((index, delta))
}

/// A selectable passive tree version.
#[derive(Debug, Clone)]
pub struct TreeVersion {
    /// Internal id, e.g. "3_28".
    pub id: String,
    /// Display name, e.g. "3.28".
    pub display: String,
    pub is_latest: bool,
}

/// List all known tree versions, oldest first (upstream's treeVersionList).
pub fn list_tree_versions(lua: &Lua) -> Result<Vec<TreeVersion>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local list = {}
            for _, id in ipairs(treeVersionList) do
                table.insert(list, {
                    id = id,
                    display = treeVersions[id].display,
                    isLatest = id == latestTreeVersion,
                })
            end
            return list
        "#,
        )
        .eval()?;

    let mut versions = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        versions.push(TreeVersion {
            id: entry.get("id").unwrap_or_default(),
            display: entry.get("display").unwrap_or_default(),
            is_latest: entry.get("isLatest").unwrap_or(false),
        });
    }
    Ok(versions)
}

/// Convert the ACTIVE spec to another tree version via upstream's
/// ConvertToVersion. With `remove` the current spec is replaced; without it
/// a converted copy is inserted after it (Copy + Convert). Nodes that no
/// longer exist in the target version are de-allocated by upstream.
pub fn convert_to_version(
    lua: &Lua,
    version: &str,
    remove: bool,
    ignore_sub_type: bool,
) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local version, remove, ignoreSubType = ...
        local build = mainObject_ref.main.modes['BUILD']
        build.treeTab:ConvertToVersion(version, remove, false, ignoreSubType)
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((version, remove, ignore_sub_type))
}

/// Convert ALL specs to the given tree version (upstream ConvertAllToVersion;
/// replaces each non-matching spec in place).
pub fn convert_all_to_version(lua: &Lua, version: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local version = ...
        local build = mainObject_ref.main.modes['BUILD']
        build.treeTab:ConvertAllToVersion(version)
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call(version)
}

/// A spec's allocation state, for comparison.
#[derive(Debug, Clone, Default)]
pub struct SpecAllocation {
    pub allocated: HashSet<u32>,
    /// Mastery node id → selected effect id.
    pub mastery_selections: HashMap<u32, u32>,
}

/// Extract a spec's allocated nodes and mastery selections (1-based index).
pub fn spec_allocation(lua: &Lua, index: usize) -> Result<SpecAllocation, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local index = ...
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.treeTab.specList[index]
            if not spec then
                return { alloc = {}, masteries = {} }
            end
            local alloc = {}
            for id in pairs(spec.allocNodes) do
                table.insert(alloc, id)
            end
            local masteries = {}
            for nodeId, effectId in pairs(spec.masterySelections) do
                masteries[tostring(nodeId)] = effectId
            end
            return { alloc = alloc, masteries = masteries }
        "#,
        )
        .call(index)?;

    let mut allocation = SpecAllocation::default();
    let alloc_table: LuaTable = result.get("alloc")?;
    for id in alloc_table.sequence_values::<u32>() {
        allocation.allocated.insert(id?);
    }
    let mastery_table: LuaTable = result.get("masteries")?;
    for pair in mastery_table.pairs::<String, u32>() {
        let (node_id, effect_id) = pair?;
        if let Ok(node_id) = node_id.parse::<u32>() {
            allocation.mastery_selections.insert(node_id, effect_id);
        }
    }
    Ok(allocation)
}

/// Node-level diff between the active spec and a comparison spec, matching
/// upstream's compare-mode colouring.
#[derive(Debug, Clone, Default)]
pub struct CompareDiff {
    /// Allocated in the compare spec but not the current one (drawn green).
    pub to_allocate: HashSet<u32>,
    /// Allocated in the current spec but not the compare one (drawn red).
    pub to_deallocate: HashSet<u32>,
    /// Mastery allocated in both, but with a different effect selected.
    pub mastery_diff: HashSet<u32>,
}

/// Compute the comparison diff between the current and compare specs.
pub fn compare_diff(current: &SpecAllocation, compare: &SpecAllocation) -> CompareDiff {
    let mut diff = CompareDiff::default();
    for &id in &compare.allocated {
        if !current.allocated.contains(&id) {
            diff.to_allocate.insert(id);
        }
    }
    for &id in &current.allocated {
        if !compare.allocated.contains(&id) {
            diff.to_deallocate.insert(id);
        }
    }
    for (&node_id, &effect_id) in &current.mastery_selections {
        if compare.allocated.contains(&node_id)
            && current.allocated.contains(&node_id)
            && compare
                .mastery_selections
                .get(&node_id)
                .is_some_and(|&other| other != effect_id)
        {
            diff.mastery_diff.insert(node_id);
        }
    }
    diff
}

/// Export the active spec as an official pathofexile.com tree URL.
pub fn export_tree_url(lua: &Lua) -> Result<String, mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        return build.spec:EncodeURL("https://www.pathofexile.com/passive-skill-tree/")
    "#,
    )
    .eval()
}

/// Expand a poeurl.com shortlink by following redirects. Other URLs pass
/// through unchanged.
pub fn expand_shortlink(url: &str) -> anyhow::Result<String> {
    if !url.contains("poeurl.com") {
        return Ok(url.to_string());
    }
    let response = reqwest::blocking::Client::new()
        .get(url.trim())
        .header("User-Agent", "PathOfBuildingCommunity (egui-pob)")
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to expand shortlink: {e}"))?;
    Ok(response.url().to_string())
}

/// Import a tree URL (official site, PoePlanner, or pre-expanded shortlink)
/// as a NEW spec and switch to it. Returns Some(error) on decode failure.
/// Mirrors upstream's OpenImportPopup decode logic.
pub fn import_tree_url(lua: &Lua, url: &str, title: &str) -> Result<Option<String>, mlua::Error> {
    lua.load(
        r#"
        local url, title = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeTab = build.treeTab

        local function activateNew(newSpec)
            table.insert(treeTab.specList, newSpec)
            treeTab.modFlag = true
            treeTab:SetActiveSpec(#treeTab.specList)
            build.spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end

        if url:match("poeplanner%.com") then
            local tmpSpec = new("PassiveSpec", build, latestTreeVersion)
            local verOrErr = tmpSpec:DecodePoePlannerURL(url, true)
            if verOrErr:find("Invalid") then
                return verOrErr
            end
            local newSpec = new("PassiveSpec", build, verOrErr)
            newSpec.title = title
            newSpec:DecodePoePlannerURL(url, false)
            activateNew(newSpec)
            return nil
        end

        -- Official site (and compatible) URLs: detect embedded tree version
        local ver = latestTreeVersion
        local major, minor = url:match("tree/([0-9]+)%.([0-9]+)%.[0-9]+/")
        if major and minor then
            local num = tonumber(string.format("%d.%02d", major, minor))
            if num >= treeVersions[defaultTreeVersion].num
               and num <= treeVersions[latestTreeVersion].num then
                ver = string.format("%s_%s", major, minor)
            end
        end
        local newSpec = new("PassiveSpec", build, ver)
        newSpec.title = title
        local errMsg = newSpec:DecodeURL(url)
        if errMsg then
            return errMsg
        end
        activateNew(newSpec)
        return nil
    "#,
    )
    .call((url, title))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc(ids: &[u32], masteries: &[(u32, u32)]) -> SpecAllocation {
        SpecAllocation {
            allocated: ids.iter().copied().collect(),
            mastery_selections: masteries.iter().copied().collect(),
        }
    }

    #[test]
    fn diff_partitions_allocations() {
        let current = alloc(&[1, 2, 3], &[]);
        let compare = alloc(&[2, 3, 4], &[]);
        let diff = compare_diff(&current, &compare);
        assert_eq!(diff.to_allocate, [4].into_iter().collect());
        assert_eq!(diff.to_deallocate, [1].into_iter().collect());
        assert!(diff.mastery_diff.is_empty());
    }

    #[test]
    fn diff_flags_changed_mastery_effects() {
        // Node 10: same effect — no diff. Node 11: different effect — diff.
        // Node 12: only allocated in current — counts as deallocate, not
        // mastery diff.
        let current = alloc(&[10, 11, 12], &[(10, 100), (11, 200), (12, 300)]);
        let compare = alloc(&[10, 11], &[(10, 100), (11, 201)]);
        let diff = compare_diff(&current, &compare);
        assert_eq!(diff.mastery_diff, [11].into_iter().collect());
        assert_eq!(diff.to_deallocate, [12].into_iter().collect());
        assert!(diff.to_allocate.is_empty());
    }
}

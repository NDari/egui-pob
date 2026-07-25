//! Passive tree data: node positions, types, connections, and allocation state.

use std::collections::{HashMap, HashSet};

use mlua::prelude::*;

/// The full passive tree data extracted from Lua after a build is loaded.
#[derive(Debug, Clone)]
pub struct TreeData {
    pub nodes: HashMap<u32, TreeNode>,
    pub connections: Vec<TreeConnection>,
    pub groups: Vec<TreeGroup>,
    pub allocated: HashSet<u32>,
    pub bounds: TreeBounds,
    /// Current class ID (0=Scion, 1=Marauder, 2=Ranger, 3=Witch, 4=Duelist, 5=Templar, 6=Shadow).
    pub class_id: u32,
    /// Current ascendancy name (e.g. "Berserker"), or None if no ascendancy selected.
    pub ascendancy_name: Option<String>,
}

/// A single passive tree node.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub node_type: NodeType,
    pub icon: String,
    pub inactive_icon: Option<String>,
    pub active_icon: Option<String>,
    pub active_effect_image: Option<String>,
    /// Group center coordinates (for arc connections between same-orbit nodes).
    pub group_x: f32,
    pub group_y: f32,
    /// Orbit index (0 = center, 1-6 = rings).
    pub orbit: u32,
    /// Maximum orbit in this node's group (determines group background size).
    pub group_max_orbit: u32,
    pub stats: Vec<String>,
    pub ascendancy_name: Option<String>,
    pub is_allocated: bool,
    /// For ClassStart nodes: the art asset name when allocated (e.g. "centertemplar").
    pub start_art: Option<String>,
    /// Gray reminder text (e.g. "Modifiers to Claw Damage also apply to...").
    pub reminder_text: Vec<String>,
    /// Oil recipe for anointing (notable nodes only, e.g. ["CrimsonOil", "CrimsonOil", "OpalescentOil"]).
    pub recipe: Vec<String>,
    /// Flavour text (italic lore text).
    pub flavour_text: Vec<String>,
}

/// A node group with a center position and background info (for background rendering).
#[derive(Debug, Clone)]
pub struct TreeGroup {
    pub x: f32,
    pub y: f32,
    pub is_ascendancy: bool,
    /// True if this is the starting group for an ascendancy class (draws class background art).
    pub is_ascendancy_start: bool,
    /// The ascendancy name (e.g. "Berserker") — used to look up class background sprite.
    pub ascendancy_name: Option<String>,
    /// True if this group belongs to a bloodline (alternate ascendancy), not a regular ascendancy.
    pub is_bloodline: bool,
    /// Background type from tree data — None means no background art for this group.
    pub background: Option<GroupBackground>,
}

/// Which background sprite to use for a group.
#[derive(Debug, Clone, Copy)]
pub enum GroupBackground {
    Small,
    Medium,
    Large,
}

/// A connection between two nodes — either straight or arc.
#[derive(Debug, Clone)]
pub struct TreeConnection {
    pub from_id: u32,
    pub to_id: u32,
    /// If both nodes share the same group and orbit, this holds arc info.
    pub arc: Option<ArcInfo>,
}

/// Arc connection info — both nodes sit on a circle.
#[derive(Debug, Clone, Copy)]
pub struct ArcInfo {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Normal,
    Notable,
    Keystone,
    Socket,
    Mastery,
    ClassStart,
    AscendClassStart,
}

impl NodeType {
    /// Radius for rendering (in tree coordinates).
    pub fn radius(self) -> f32 {
        // Half of artWidth * 1.33, matching upstream PoB's nodeOverlay sizes
        match self {
            NodeType::Normal => 26.6,           // 40 * 1.33 / 2
            NodeType::Notable => 38.6,          // 58 * 1.33 / 2
            NodeType::Keystone => 55.9,         // 84 * 1.33 / 2
            NodeType::Socket => 38.6,           // 58 * 1.33 / 2
            NodeType::Mastery => 43.2,          // 65 * 1.33 / 2
            NodeType::ClassStart => 55.9,       // same as Keystone
            NodeType::AscendClassStart => 38.6, // same as Notable
        }
    }
}

/// Bounding box of the tree in world coordinates.
#[derive(Debug, Clone, Copy)]
pub struct TreeBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl TreeBounds {
    pub fn center(&self) -> (f32, f32) {
        (
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        )
    }

    pub fn size(&self) -> f32 {
        (self.max_x - self.min_x).max(self.max_y - self.min_y)
    }
}

impl TreeData {
    /// Extract tree data from a loaded build in the Lua VM.
    /// Reads from `build.spec` where nodes already have calculated x/y positions.
    pub fn extract(lua: &Lua) -> Result<Self, mlua::Error> {
        let spec: LuaTable = lua
            .load("return mainObject_ref.main.modes['BUILD'].spec")
            .eval()?;

        let nodes_table: LuaTable = spec.get("nodes")?;
        let alloc_nodes: LuaTable = spec.get("allocNodes")?;
        let class_id: u32 = spec.get("curClassId").unwrap_or(0);
        let ascendancy_name: Option<String> = spec.get("curAscendClassBaseName").ok();

        // Collect allocated node IDs
        let mut allocated = HashSet::new();
        for pair in alloc_nodes.pairs::<LuaValue, LuaValue>() {
            let (key, _) = pair?;
            if let Some(id) = lua_value_to_u32(&key) {
                allocated.insert(id);
            }
        }

        // Extract group data for background rendering
        let groups: Vec<TreeGroup> = lua
            .load(
                r#"
                local tree = mainObject_ref.main.modes['BUILD'].spec.tree
                local altAsc = tree.alternate_ascendancies or {}
                local bloodlineNames = {}
                for _, asc in pairs(altAsc) do
                    bloodlineNames[asc.id] = true
                end
                local result = {}
                for _, group in pairs(tree.groups) do
                    if not group.isProxy then
                        local bgImage = nil
                        if group.background then
                            bgImage = group.background.image
                        end
                        table.insert(result, {
                            x = group.x,
                            y = group.y,
                            isAscendancy = group.ascendancyName ~= nil,
                            isAscendancyStart = group.isAscendancyStart or false,
                            ascendancyName = group.ascendancyName,
                            isBloodline = group.ascendancyName and bloodlineNames[group.ascendancyName] or false,
                            bgImage = bgImage,
                        })
                    end
                end
                return result
            "#,
            )
            .eval::<LuaTable>()
            .and_then(|table| {
                let mut groups = Vec::new();
                for entry in table.sequence_values::<LuaTable>() {
                    let t = entry?;
                    let background = t
                        .get::<Option<String>>("bgImage")
                        .ok()
                        .flatten()
                        .and_then(|img| match img.as_str() {
                            "PSGroupBackground3" => Some(GroupBackground::Large),
                            "PSGroupBackground2" => Some(GroupBackground::Medium),
                            "PSGroupBackground1" => Some(GroupBackground::Small),
                            _ => None,
                        });
                    groups.push(TreeGroup {
                        x: t.get("x")?,
                        y: t.get("y")?,
                        is_ascendancy: t.get("isAscendancy").unwrap_or(false),
                        is_ascendancy_start: t.get("isAscendancyStart").unwrap_or(false),
                        ascendancy_name: t.get("ascendancyName").ok(),
                        is_bloodline: t.get("isBloodline").unwrap_or(false),
                        background,
                    });
                }
                Ok(groups)
            })
            .unwrap_or_default();

        // Extract all nodes
        let mut nodes = HashMap::new();
        let mut raw_connections = Vec::new();
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;

        for pair in nodes_table.pairs::<LuaValue, LuaTable>() {
            let (key, node_table) = pair?;
            let Some(id) = lua_value_to_u32(&key) else {
                continue;
            };

            // Get x/y — skip nodes without positions (e.g., unprocessed)
            let x: f32 = match node_table.get("x") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let y: f32 = match node_table.get("y") {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Prefer the display name: node overrides (tattoos) set `dn` but
            // not `name`
            let name: String = node_table
                .get("dn")
                .or_else(|_| node_table.get("name"))
                .unwrap_or_default();
            let type_str: String = node_table.get("type").unwrap_or_default();
            let node_type = parse_node_type(&type_str);

            // Skip certain node types we can't render meaningfully
            if type_str.is_empty() {
                continue;
            }

            let icon: String = node_table.get("icon").unwrap_or_default();
            let inactive_icon: Option<String> = node_table.get("inactiveIcon").ok();
            let active_icon: Option<String> = node_table.get("activeIcon").ok();
            let active_effect_image: Option<String> = node_table.get("activeEffectImage").ok();
            let ascendancy_name: Option<String> = node_table.get("ascendancyName").ok();
            let orbit: u32 = node_table.get("o").unwrap_or(0);

            // Get group center coordinates and max orbit
            let (group_x, group_y, group_max_orbit) = match node_table.get::<LuaTable>("group") {
                Ok(group) => {
                    let gx: f32 = group.get("x").unwrap_or(x);
                    let gy: f32 = group.get("y").unwrap_or(y);
                    // Get max orbit from group.oo table (keys are orbit indices)
                    let max_orbit = group
                        .get::<LuaTable>("oo")
                        .map(|oo| {
                            let mut max = 0u32;
                            for (k, _) in oo.pairs::<u32, LuaValue>().flatten() {
                                max = max.max(k);
                            }
                            max
                        })
                        .unwrap_or(0);
                    (gx, gy, max_orbit)
                }
                Err(_) => (x, y, 0),
            };

            // Read stats
            let stats = read_string_list(&node_table, "sd");

            let is_allocated = allocated.contains(&id);
            let start_art: Option<String> = node_table.get("startArt").ok();
            let reminder_text = read_string_list(&node_table, "reminderText");
            let recipe = read_string_list(&node_table, "recipe");
            let flavour_text = read_string_list(&node_table, "flavourText");

            // Update bounds
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);

            // Extract outgoing connections
            if let Ok(out_table) = node_table.get::<LuaTable>("linked") {
                for (_, linked_node) in out_table.pairs::<LuaValue, LuaTable>().flatten() {
                    if let Ok(linked_id) = linked_node.get::<u32>("id") {
                        // Only add each connection once (from lower to higher ID)
                        if id < linked_id {
                            raw_connections.push((id, linked_id));
                        }
                    }
                }
            }

            nodes.insert(
                id,
                TreeNode {
                    id,
                    name,
                    x,
                    y,
                    node_type,
                    icon,
                    inactive_icon,
                    active_icon,
                    active_effect_image,
                    group_x,
                    group_y,
                    orbit,
                    group_max_orbit,
                    stats,
                    ascendancy_name,
                    is_allocated,
                    start_art,
                    reminder_text,
                    recipe,
                    flavour_text,
                },
            );
        }

        // Build connections with arc detection, filtering out clutter
        let connections: Vec<TreeConnection> = raw_connections
            .into_iter()
            .filter_map(|(from_id, to_id)| {
                let from = nodes.get(&from_id)?;
                let to = nodes.get(&to_id)?;
                // Skip connections between main tree and ascendancy nodes
                if from.ascendancy_name.is_some() != to.ascendancy_name.is_some() {
                    return None;
                }
                // Skip connections to/from mastery nodes
                if from.node_type == NodeType::Mastery || to.node_type == NodeType::Mastery {
                    return None;
                }
                // Detect arc: same group center and same orbit (non-zero)
                let arc = if from.orbit == to.orbit
                    && from.orbit > 0
                    && (from.group_x - to.group_x).abs() < 0.1
                    && (from.group_y - to.group_y).abs() < 0.1
                {
                    let dx = from.x - from.group_x;
                    let dy = from.y - from.group_y;
                    let radius = (dx * dx + dy * dy).sqrt();
                    Some(ArcInfo {
                        center_x: from.group_x,
                        center_y: from.group_y,
                        radius,
                    })
                } else {
                    None
                };
                Some(TreeConnection {
                    from_id,
                    to_id,
                    arc,
                })
            })
            .collect();

        // Add padding to bounds
        let padding = 100.0;
        let bounds = TreeBounds {
            min_x: min_x - padding,
            max_x: max_x + padding,
            min_y: min_y - padding,
            max_y: max_y + padding,
        };

        log::info!(
            "Extracted tree: {} nodes, {} connections, {} allocated",
            nodes.len(),
            connections.len(),
            allocated.len()
        );

        Ok(TreeData {
            nodes,
            connections,
            groups,
            allocated,
            bounds,
            class_id,
            ascendancy_name,
        })
    }

    /// Refresh stats and reminder text for mastery nodes. Their `sd` changes
    /// when an effect is selected or the node is deallocated (Lua resets it
    /// to the full option list).
    pub fn refresh_mastery_stats(&mut self, lua: &Lua) -> Result<(), mlua::Error> {
        let table: LuaTable = lua
            .load(
                r#"
                local result = {}
                for id, node in pairs(mainObject_ref.main.modes['BUILD'].spec.nodes) do
                    if node.type == "Mastery" then
                        result[id] = { sd = node.sd, reminderText = node.reminderText }
                    end
                end
                return result
            "#,
            )
            .eval()?;
        for pair in table.pairs::<LuaValue, LuaTable>() {
            let (key, t) = pair?;
            let Some(id) = lua_value_to_u32(&key) else {
                continue;
            };
            if let Some(node) = self.nodes.get_mut(&id) {
                node.stats = read_string_list(&t, "sd");
                node.reminder_text = read_string_list(&t, "reminderText");
            }
        }
        Ok(())
    }

    /// Find nodes matching a search query, mirroring upstream's search semantics:
    /// terms are split on whitespace (quoted phrases stay together) and a node
    /// matches when every term matches its name, a stat line, or its type.
    /// An `oil:` first term switches to anoint-recipe matching.
    pub fn search_matches(&self, query: &str) -> HashSet<u32> {
        let query = query.to_lowercase();
        let terms = parse_search_terms(&query);
        let mut out = HashSet::new();
        if terms.is_empty() {
            return out;
        }
        let oil_mode = terms[0] == "oil:";
        for node in self.nodes.values() {
            if matches!(
                node.node_type,
                NodeType::ClassStart | NodeType::AscendClassStart
            ) {
                continue;
            }
            let matched = if oil_mode {
                node_matches_oil(node, &terms[1..])
            } else {
                node_matches(node, &terms)
            };
            if matched {
                out.insert(node.id);
            }
        }
        out
    }

    /// Refresh allocation state from Lua (after a node toggle).
    pub fn refresh_allocation(&mut self, lua: &Lua) -> Result<(), mlua::Error> {
        let alloc_nodes: LuaTable = lua
            .load("return mainObject_ref.main.modes['BUILD'].spec.allocNodes")
            .eval()?;

        self.allocated.clear();
        for pair in alloc_nodes.pairs::<LuaValue, LuaValue>() {
            let (key, _) = pair?;
            if let Some(id) = lua_value_to_u32(&key) {
                self.allocated.insert(id);
            }
        }

        // Update is_allocated on each node
        for (id, node) in &mut self.nodes {
            node.is_allocated = self.allocated.contains(id);
        }

        Ok(())
    }
}

/// Hover pathing info for one node: the shortest path to reach it (if
/// unallocated) and the nodes that depend on it (if allocated). Kept current
/// by Lua's BuildAllDependsAndPaths, which runs on every alloc change.
#[derive(Debug, Clone, Default)]
pub struct HoverInfo {
    /// Nodes on the shortest path from the allocated tree to this node.
    pub path: HashSet<u32>,
    /// Allocated nodes that would disconnect if this node were deallocated.
    pub depends: HashSet<u32>,
    /// Stat difference preview lines ("Allocating this node will give you:
    /// ..."), pre-formatted with PoB color codes. Empty when diffs are off.
    pub diff: Vec<String>,
}

/// Fetch path and dependency info for a node from Lua. With `with_diffs`,
/// also runs upstream's stat-difference comparison (one or two throwaway calc
/// passes: the node alone, and the whole path to/from it) and returns the
/// formatted tooltip lines, mirroring PassiveTreeView:AddNodeTooltip.
pub fn fetch_hover_info(
    lua: &Lua,
    node_id: u32,
    with_diffs: bool,
) -> Result<HoverInfo, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local nodeId, withDiffs = ...
            local build = mainObject_ref.main.modes['BUILD']
            local node = build.spec.nodes[nodeId]
            local path, depends, diff = {}, {}, {}
            if node then
                local leap = node.intuitiveLeapLikesAffecting
                if node.path and (leap == nil or #leap == 0) then
                    for _, p in ipairs(node.path) do
                        table.insert(path, p.id)
                    end
                end
                if node.depends then
                    for _, d in ipairs(node.depends) do
                        table.insert(depends, d.id)
                    end
                end
                if withDiffs and node.type ~= "ClassStart" and node.type ~= "AscendClassStart" then
                    local ok, err = pcall(function()
                        local calcFunc, calcBase = build.calcsTab:GetMiscCalculator(build)
                        if not calcFunc then
                            return
                        end
                        local tt = new("Tooltip")
                        local pathList = (node.alloc and node.depends) or node.path or { }
                        local pathLength = #pathList
                        local pathNodes = { }
                        for _, n in pairs(pathList) do
                            pathNodes[n] = true
                        end
                        local nodeOutput, pathOutput
                        local isGranted = build.calcsTab.mainEnv.grantedPassives[node.id]
                        if node.alloc then
                            nodeOutput = calcFunc({ removeNodes = { [node] = true } })
                            if pathLength > 1 then
                                pathOutput = calcFunc({ removeNodes = pathNodes })
                            end
                        elseif isGranted then
                            nodeOutput = calcFunc({ removeNodes = { [node.id] = true } })
                        else
                            if node.type == "Mastery" and node.allMasteryOptions then
                                pathNodes[node] = nil
                                nodeOutput = calcFunc()
                            else
                                nodeOutput = calcFunc({ addNodes = { [node] = true } })
                            end
                            if pathLength > 1 then
                                pathOutput = calcFunc({ addNodes = pathNodes })
                            end
                        end
                        local count = build:AddStatComparesToTooltip(tt, calcBase, nodeOutput,
                            node.alloc and "^7Unallocating this node will give you:"
                            or isGranted and "^7This node is granted by an item. Removing it will give you:"
                            or "^7Allocating this node will give you:")
                        if pathLength > 1 and not isGranted
                           and (#(node.intuitiveLeapLikesAffecting or { }) == 0 or node.alloc) then
                            count = count + build:AddStatComparesToTooltip(tt, calcBase, pathOutput,
                                node.alloc and "^7Unallocating this node and all nodes depending on it will give you:"
                                or "^7Allocating this node and all nodes leading to it will give you:", pathLength)
                        end
                        if count == 0 then
                            if isGranted then
                                tt:AddLine(14, "^7This node is granted by an item. Removing it will cause no changes")
                            else
                                tt:AddLine(14, string.format("^7No changes from %s this node%s.",
                                    node.alloc and "unallocating" or "allocating",
                                    (#(node.intuitiveLeapLikesAffecting or { }) == 0 and pathLength > 1)
                                        and " or the nodes leading to it" or ""))
                            end
                        end
                        for _, line in ipairs(tt.lines) do
                            if line.text then
                                table.insert(diff, line.text)
                            end
                        end
                    end)
                    if not ok then
                        table.insert(diff, "^1Stat comparison failed: " .. tostring(err))
                    end
                end
            end
            return { path = path, depends = depends, diff = diff }
        "#,
        )
        .call((node_id, with_diffs))?;

    let read_ids = |key: &str| -> Result<HashSet<u32>, mlua::Error> {
        let list: LuaTable = result.get(key)?;
        Ok(list.sequence_values::<u32>().flatten().collect())
    };
    let diff_list: LuaTable = result.get("diff")?;
    Ok(HoverInfo {
        path: read_ids("path")?,
        depends: read_ids("depends")?,
        diff: diff_list.sequence_values::<String>().flatten().collect(),
    })
}

/// Undo the last tree change (Ctrl+Z).
pub fn undo(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.spec:Undo()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
}

/// Redo the last undone tree change (Ctrl+Y).
pub fn redo(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.spec:Redo()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
}

/// One selectable mastery effect.
#[derive(Debug, Clone)]
pub struct MasteryEffect {
    pub id: u32,
    pub label: String,
}

/// The selectable mastery effects for a node, plus the currently assigned one.
#[derive(Debug, Clone)]
pub struct MasteryEffectList {
    pub node_name: String,
    pub effects: Vec<MasteryEffect>,
    /// Effect currently selected on this node, if any.
    pub current: Option<u32>,
}

/// Toggle a node allocation in Lua and trigger recalc.
pub fn toggle_node(lua: &Lua, node_id: u32) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[{node_id}]
        if node then
            if spec.allocNodes[{node_id}] then
                spec:DeallocNode(node)
            else
                spec:AllocNode(node)
            end
            spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

/// Outcome of a left-click on a node routed through upstream's ascendancy
/// switching logic.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeClickOutcome {
    /// The click toggled allocation (or did nothing); no structural change.
    Toggled,
    /// The click switched ascendancy, bloodline, or class; tree data and the
    /// header dropdowns need a full refresh.
    Switched,
    /// A cross-class switch would reset the tree; the user must confirm
    /// (Continue = reset, Connect Path = path to the class start first).
    NeedsConfirm { class_name: String },
}

/// Handle a left-click on a node, porting upstream PassiveTreeView's click
/// routing: allocated nodes deallocate; nodes of another ascendancy switch
/// ascendancy (bloodlines switch the secondary ascendancy; same-class
/// switches are immediate; cross-class switches happen only when the tree is
/// empty or already connected to the target class, otherwise
/// [`NodeClickOutcome::NeedsConfirm`] is returned); then the node is
/// allocated if reachable.
pub fn click_node(lua: &Lua, node_id: u32) -> Result<NodeClickOutcome, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
        local nodeId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[nodeId]
        if not node then
            return { }
        end
        if node.alloc then
            spec:DeallocNode(node)
            spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
            return { }
        end
        local switched = false
        if node.ascendancyName then
            if node.isBloodline and spec.tree.alternate_ascendancies then
                local isDifferentBloodline = not spec.curSecondaryAscendClass
                    or node.ascendancyName ~= spec.curSecondaryAscendClass.id
                if isDifferentBloodline then
                    for bloodlineId, bloodlineData in pairs(spec.tree.alternate_ascendancies) do
                        if bloodlineData.id == node.ascendancyName then
                            spec:SelectSecondaryAscendClass(bloodlineId)
                            spec:AddUndoState()
                            build.buildFlag = true
                            switched = true
                            break
                        end
                    end
                end
            else
                local isDifferentAscendancy = false
                if spec.curAscendClassId == 0
                   or node.ascendancyName ~= spec.curAscendClassBaseName then
                    if not (spec.curSecondaryAscendClass
                            and node.ascendancyName == spec.curSecondaryAscendClass.id) then
                        isDifferentAscendancy = true
                    end
                end
                if isDifferentAscendancy then
                    -- Same-class switching is always allowed
                    local targetAscendClassId
                    for ascendClassId, ascendClass in pairs(spec.curClass.classes) do
                        if ascendClass.id == node.ascendancyName then
                            targetAscendClassId = ascendClassId
                            break
                        end
                    end
                    if targetAscendClassId then
                        spec:SelectAscendClass(targetAscendClassId)
                        spec:AddUndoState()
                        build.buildFlag = true
                        switched = true
                    else
                        -- Cross-class switching
                        local targetBaseClassId, targetBaseClass
                        for classId, classData in pairs(spec.tree.classes) do
                            for ascendClassId, ascendClass in pairs(classData.classes) do
                                if ascendClass.id == node.ascendancyName then
                                    targetBaseClassId = classId
                                    targetBaseClass = classData
                                    targetAscendClassId = ascendClassId
                                    break
                                end
                            end
                            if targetBaseClassId then break end
                        end
                        if targetBaseClassId then
                            local used = spec:CountAllocNodes()
                            if used == 0 or spec:IsClassConnected(targetBaseClassId) then
                                spec:SelectClass(targetBaseClassId)
                                spec:SelectAscendClass(targetAscendClassId)
                                local targetNode = spec.nodes[nodeId]
                                if targetNode and not targetNode.alloc then
                                    spec:AllocNode(targetNode)
                                end
                                spec:AddUndoState()
                                build.buildFlag = true
                                switched = true
                            else
                                return {
                                    needsConfirm = true,
                                    className = targetBaseClass.name,
                                }
                            end
                        end
                    end
                end
            end
        end
        -- Normal allocation (non-ascendancy, same ascendancy, or after a
        -- switch); upstream only allocates reachable nodes
        local targetNode = spec.nodes[nodeId]
        if targetNode and targetNode.path and not targetNode.alloc then
            spec:AllocNode(targetNode)
            spec:AddUndoState()
            build.buildFlag = true
        end
        _runCallback('OnFrame')
        return { switched = switched }
    "#,
        )
        .call(node_id)?;

    if result.get::<bool>("needsConfirm").unwrap_or(false) {
        return Ok(NodeClickOutcome::NeedsConfirm {
            class_name: result.get("className").unwrap_or_default(),
        });
    }
    if result.get::<bool>("switched").unwrap_or(false) {
        return Ok(NodeClickOutcome::Switched);
    }
    Ok(NodeClickOutcome::Toggled)
}

/// Complete a confirmed cross-class switch for a clicked ascendancy node
/// (the "Continue" / "Connect Path" choices of upstream's Class Change
/// popup). With `connect`, the tree is first pathed to the target class
/// start (keeping allocations); otherwise the switch resets the tree.
/// Returns false if `connect` was requested but no connecting path exists.
pub fn confirm_class_switch(lua: &Lua, node_id: u32, connect: bool) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local nodeId, connect = ...
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[nodeId]
        if not node or not node.ascendancyName then
            return false
        end
        local targetBaseClassId, targetAscendClassId
        for classId, classData in pairs(spec.tree.classes) do
            for ascendClassId, ascendClass in pairs(classData.classes) do
                if ascendClass.id == node.ascendancyName then
                    targetBaseClassId = classId
                    targetAscendClassId = ascendClassId
                    break
                end
            end
            if targetBaseClassId then break end
        end
        if not targetBaseClassId then
            return false
        end
        if connect and not spec:ConnectToClass(targetBaseClassId) then
            return false
        end
        spec:SelectClass(targetBaseClassId)
        spec:SelectAscendClass(targetAscendClassId)
        local targetNode = spec.nodes[nodeId]
        if targetNode and not targetNode.alloc then
            spec:AllocNode(targetNode)
        end
        spec:AddUndoState()
        build.buildFlag = true
        _runCallback('OnFrame')
        return true
    "#,
    )
    .call((node_id, connect))
}

/// Fetch the selectable mastery effects for a node. Effects already assigned
/// to a different mastery node are excluded (matching upstream's
/// OpenMasteryPopup). Returns None if the node has no selectable effects.
pub fn fetch_mastery_effects(
    lua: &Lua,
    node_id: u32,
) -> Result<Option<MasteryEffectList>, mlua::Error> {
    let result: LuaTable = lua
        .load(format!(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            local node = spec.nodes[{node_id}]
            local result = {{ effects = {{}} }}
            if node and node.masteryEffects then
                result.name = node.name
                result.current = spec.masterySelections[{node_id}]
                for _, effect in pairs(node.masteryEffects) do
                    local assigned = isValueInTable(spec.masterySelections, effect.effect)
                    if not assigned or assigned == {node_id} then
                        table.insert(result.effects, {{
                            id = effect.effect,
                            label = table.concat(effect.stats, " / "),
                        }})
                    end
                end
            end
            return result
        "#
        ))
        .eval()?;

    let effects_table: LuaTable = result.get("effects")?;
    let mut effects = Vec::new();
    for entry in effects_table.sequence_values::<LuaTable>() {
        let t = entry?;
        effects.push(MasteryEffect {
            id: t.get("id")?,
            label: t.get("label")?,
        });
    }
    if effects.is_empty() {
        return Ok(None);
    }

    Ok(Some(MasteryEffectList {
        node_name: result.get::<String>("name").unwrap_or_default(),
        effects,
        current: result.get("current").ok(),
    }))
}

/// Apply a mastery effect selection and allocate the node - a port of
/// upstream's TreeTab:SaveMasteryPopup.
pub fn select_mastery_effect(lua: &Lua, node_id: u32, effect_id: u32) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[{node_id}]
        local effect = spec.tree.masteryEffects[{effect_id}]
        if node and effect then
            node.sd = effect.sd
            node.allMasteryOptions = false
            node.reminderText = {{ "Tip: Right click to select a different effect" }}
            spec.tree:ProcessStats(node)
            spec.masterySelections[{node_id}] = effect.id
            if not node.alloc then
                spec:AllocNode(node)
            end
            spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

fn lua_value_to_u32(val: &LuaValue) -> Option<u32> {
    match val {
        LuaValue::Integer(n) => Some(*n as u32),
        LuaValue::Number(n) => Some(*n as u32),
        LuaValue::String(s) => s.to_str().ok()?.parse().ok(),
        _ => None,
    }
}

fn parse_node_type(s: &str) -> NodeType {
    match s {
        "Notable" => NodeType::Notable,
        "Keystone" => NodeType::Keystone,
        "Socket" => NodeType::Socket,
        "Mastery" => NodeType::Mastery,
        "ClassStart" => NodeType::ClassStart,
        "AscendClassStart" => NodeType::AscendClassStart,
        _ => NodeType::Normal,
    }
}

fn read_string_list(table: &LuaTable, key: &str) -> Vec<String> {
    let Ok(list) = table.get::<LuaTable>(key) else {
        return Vec::new();
    };
    list.sequence_values::<String>()
        .filter_map(|r| r.ok())
        .collect()
}

/// Split a (lowercased) search query into terms: quoted phrases first, then
/// bare whitespace-separated words.
fn parse_search_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut rest = String::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in query.chars() {
        if c == '"' {
            if in_quote {
                if !cur.is_empty() {
                    terms.push(std::mem::take(&mut cur));
                }
                in_quote = false;
            } else {
                in_quote = true;
            }
        } else if in_quote {
            cur.push(c);
        } else {
            rest.push(c);
        }
    }
    if in_quote && !cur.is_empty() {
        terms.push(cur);
    }
    terms.extend(rest.split_whitespace().map(str::to_string));
    terms
}

fn node_search_type(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Normal => "normal",
        NodeType::Notable => "notable",
        NodeType::Keystone => "keystone",
        NodeType::Socket => "socket",
        NodeType::Mastery => "mastery",
        NodeType::ClassStart | NodeType::AscendClassStart => "",
    }
}

fn node_matches(node: &TreeNode, terms: &[String]) -> bool {
    let name = node.name.to_lowercase();
    let type_str = node_search_type(node.node_type);
    terms.iter().all(|t| {
        name.contains(t.as_str())
            || type_str.contains(t.as_str())
            || node
                .stats
                .iter()
                .any(|s| s.to_lowercase().contains(t.as_str()))
    })
}

fn node_matches_oil(node: &TreeNode, terms: &[String]) -> bool {
    if node.recipe.is_empty() {
        return false;
    }
    let oils: Vec<String> = node
        .recipe
        .iter()
        .map(|r| r.replace("Oil", "").to_lowercase())
        .collect();
    terms
        .iter()
        .all(|t| oils.iter().any(|o| o.contains(t.as_str())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(name: &str, node_type: NodeType, stats: &[&str], recipe: &[&str]) -> TreeNode {
        TreeNode {
            id: 1,
            name: name.to_string(),
            x: 0.0,
            y: 0.0,
            node_type,
            icon: String::new(),
            inactive_icon: None,
            active_icon: None,
            active_effect_image: None,
            group_x: 0.0,
            group_y: 0.0,
            orbit: 0,
            group_max_orbit: 0,
            stats: stats.iter().map(|s| s.to_string()).collect(),
            ascendancy_name: None,
            is_allocated: false,
            start_art: None,
            reminder_text: Vec::new(),
            recipe: recipe.iter().map(|s| s.to_string()).collect(),
            flavour_text: Vec::new(),
        }
    }

    #[test]
    fn parse_terms_words_and_phrases() {
        assert_eq!(parse_search_terms("fire damage"), vec!["fire", "damage"]);
        assert_eq!(
            parse_search_terms("\"maximum life\" fire"),
            vec!["maximum life", "fire"]
        );
        assert!(parse_search_terms("  ").is_empty());
    }

    #[test]
    fn match_requires_all_terms() {
        let node = make_node(
            "Heart of Flame",
            NodeType::Notable,
            &["10% increased Fire Damage", "+10 to maximum Life"],
            &[],
        );
        let terms = |q: &str| parse_search_terms(&q.to_lowercase());
        assert!(node_matches(&node, &terms("fire life")));
        assert!(node_matches(&node, &terms("heart notable")));
        assert!(!node_matches(&node, &terms("fire cold")));
        assert!(node_matches(&node, &terms("\"maximum life\"")));
        assert!(!node_matches(&node, &terms("\"maximum fire\"")));
    }

    #[test]
    fn oil_prefix_matches_recipe() {
        let node = make_node(
            "Heart of Flame",
            NodeType::Notable,
            &[],
            &["CrimsonOil", "GoldenOil"],
        );
        let terms = |q: &str| parse_search_terms(&q.to_lowercase());
        // "oil:" alone matches any node with a recipe
        assert!(node_matches_oil(&node, &terms("oil:")[1..]));
        assert!(node_matches_oil(&node, &terms("oil: golden")[1..]));
        assert!(!node_matches_oil(&node, &terms("oil: silver")[1..]));
        let no_recipe = make_node("Other", NodeType::Notable, &[], &[]);
        assert!(!node_matches_oil(&no_recipe, &terms("oil:")[1..]));
    }
}

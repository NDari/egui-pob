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
}

/// Node type determines rendering size and color.
/// A node group with a center position and orbit info (for background rendering).
#[derive(Debug, Clone)]
pub struct TreeGroup {
    pub x: f32,
    pub y: f32,
    pub max_orbit: u32,
    pub is_ascendancy: bool,
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
        match self {
            NodeType::Normal => 20.0,
            NodeType::Notable => 29.0,
            NodeType::Keystone => 42.0,
            NodeType::Socket => 29.0,
            NodeType::Mastery => 55.0,
            NodeType::ClassStart => 42.0,
            NodeType::AscendClassStart => 29.0,
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
                local result = {}
                for _, group in pairs(tree.groups) do
                    if not group.isProxy then
                        local maxOrbit = 0
                        if group.oo then
                            for k, _ in pairs(group.oo) do
                                if k > maxOrbit then maxOrbit = k end
                            end
                        end
                        table.insert(result, {
                            x = group.x,
                            y = group.y,
                            maxOrbit = maxOrbit,
                            isAscendancy = group.ascendancyName ~= nil
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
                    groups.push(TreeGroup {
                        x: t.get("x")?,
                        y: t.get("y")?,
                        max_orbit: t.get("maxOrbit").unwrap_or(0),
                        is_ascendancy: t.get("isAscendancy").unwrap_or(false),
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

            let name: String = node_table.get("name").unwrap_or_default();
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
        })
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

//! Passive tree data: node positions, types, connections, and allocation state.

use std::collections::{HashMap, HashSet};

use mlua::prelude::*;

/// The full passive tree data extracted from Lua after a build is loaded.
#[derive(Debug, Clone)]
pub struct TreeData {
    pub nodes: HashMap<u32, TreeNode>,
    pub connections: Vec<(u32, u32)>,
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
    pub stats: Vec<String>,
    pub ascendancy_name: Option<String>,
    pub is_allocated: bool,
}

/// Node type determines rendering size and color.
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
            NodeType::Mastery => 32.0,
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

        // Extract all nodes
        let mut nodes = HashMap::new();
        let mut connections = Vec::new();
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

            let ascendancy_name: Option<String> = node_table.get("ascendancyName").ok();

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
                            connections.push((id, linked_id));
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
                    stats,
                    ascendancy_name,
                    is_allocated,
                },
            );
        }

        // Filter out connections that clutter the view:
        // - Cross-tree connections (class start → ascendancy start)
        // - Notable → Mastery connections (masteries sit visually inside clusters)
        connections.retain(|&(from_id, to_id)| {
            let (Some(from), Some(to)) = (nodes.get(&from_id), nodes.get(&to_id)) else {
                return false;
            };
            // Skip connections between main tree and ascendancy nodes
            if from.ascendancy_name.is_some() != to.ascendancy_name.is_some() {
                return false;
            }
            // Skip connections to/from mastery nodes
            if from.node_type == NodeType::Mastery || to.node_type == NodeType::Mastery {
                return false;
            }
            true
        });

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

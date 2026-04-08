//! Tree tab: passive tree view with pan/zoom and node interaction.

use pob_egui::data::tree::TreeData;
use pob_egui::lua_bridge::LuaBridge;

use super::tree_renderer::{self, TreeCamera};

/// State for the passive tree tab.
pub struct TreePanel {
    pub tree_data: Option<TreeData>,
    pub camera: Option<TreeCamera>,
    pub error: Option<String>,
}

impl TreePanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        match TreeData::extract(lua) {
            Ok(tree_data) => {
                let camera = TreeCamera::new(&tree_data);
                log::info!(
                    "Tree loaded: {} nodes, {} connections",
                    tree_data.nodes.len(),
                    tree_data.connections.len()
                );
                Self {
                    tree_data: Some(tree_data),
                    camera: Some(camera),
                    error: None,
                }
            }
            Err(e) => {
                log::error!("Failed to load tree data: {e}");
                Self {
                    tree_data: None,
                    camera: None,
                    error: Some(format!("Failed to load tree: {e}")),
                }
            }
        }
    }

    /// Draw the tree tab. Returns true if the tree changed (node toggled → recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(egui::Color32::RED, err);
            return false;
        }

        let (Some(tree_data), Some(camera)) = (&mut self.tree_data, &mut self.camera) else {
            ui.label("No tree data loaded.");
            return false;
        };

        // Draw the tree and check for node clicks
        if let Some(clicked_id) = tree_renderer::draw_tree(ui, tree_data, camera) {
            if let Err(e) = toggle_node(bridge.lua(), clicked_id) {
                log::error!("Failed to toggle node {clicked_id}: {e}");
            } else if let Err(e) = tree_data.refresh_allocation(bridge.lua()) {
                log::error!("Failed to refresh allocation: {e}");
            } else {
                changed = true;
            }
        }

        changed
    }
}

/// Toggle a node allocation in Lua and trigger recalc.
fn toggle_node(lua: &mlua::Lua, node_id: u32) -> Result<(), mlua::Error> {
    // Use upstream's allocation logic via spec methods
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[{node_id}]
        if node then
            if spec.allocNodes[{node_id}] then
                spec:DeallocNode({node_id})
            else
                spec:AllocNode({node_id})
            end
            spec:AddUndoState()
            build.buildFlag = true
            _runCallback('OnFrame')
        end
    "#
    ))
    .exec()
}

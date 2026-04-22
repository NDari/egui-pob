//! Tree tab: passive tree view with pan/zoom and node interaction.

use std::path::PathBuf;

use pob_egui::data::tree::TreeData;
use pob_egui::data::tree_sprites::TreeSpriteAtlas;
use pob_egui::lua_bridge::LuaBridge;

use super::tree_renderer::{self, TooltipHeaders, TreeCamera};

/// State for the passive tree tab.
pub struct TreePanel {
    pub tree_data: Option<TreeData>,
    pub camera: Option<TreeCamera>,
    pub atlas: Option<TreeSpriteAtlas>,
    pub tooltip_headers: Option<TooltipHeaders>,
    pub tree_data_dir: Option<PathBuf>,
    pub textures_uploaded: bool,
    pub error: Option<String>,
}

impl TreePanel {
    pub fn new(lua: &mlua::Lua) -> Self {
        let tree_data = match TreeData::extract(lua) {
            Ok(td) => {
                log::info!(
                    "Tree loaded: {} nodes, {} connections",
                    td.nodes.len(),
                    td.connections.len()
                );
                Some(td)
            }
            Err(e) => {
                log::error!("Failed to load tree data: {e}");
                return Self {
                    tree_data: None,
                    camera: None,
                    atlas: None,
                    tooltip_headers: None,
                    tree_data_dir: None,
                    textures_uploaded: false,
                    error: Some(format!("Failed to load tree: {e}")),
                };
            }
        };

        let camera = tree_data.as_ref().map(TreeCamera::new);

        // Try to load sprite atlas — get tree version from spec
        let tree_data_dir = get_tree_version(lua).and_then(|version| find_tree_data_dir(&version));
        let atlas = tree_data_dir.as_ref().and_then(|dir| {
            log::info!("Loading tree sprites from: {}", dir.display());
            TreeSpriteAtlas::load(lua, dir)
                .map_err(|e| log::warn!("Failed to load tree sprites: {e}"))
                .ok()
        });

        Self {
            tree_data,
            camera,
            atlas,
            tooltip_headers: None,
            tree_data_dir,
            textures_uploaded: false,
            error: None,
        }
    }

    /// Draw the tree tab. Returns true if the tree changed (node toggled → recalc needed).
    pub fn show(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;

        if let Some(ref err) = self.error {
            ui.colored_label(egui::Color32::RED, err);
            return false;
        }

        // Upload textures on first frame (needs egui context)
        if !self.textures_uploaded {
            if let Some(ref mut atlas) = self.atlas {
                atlas.upload_textures(ui.ctx());
            }
            // Load tooltip header images and oil icons
            if self.tooltip_headers.is_none() {
                if let Some(dir) = find_assets_dir() {
                    log::info!("Loading tooltip headers from: {}", dir.display());
                    self.tooltip_headers = Some(TooltipHeaders::load(
                        ui.ctx(),
                        &dir,
                        self.tree_data_dir.as_deref(),
                    ));
                }
            }
            self.textures_uploaded = true;
        }

        let (Some(tree_data), Some(camera)) = (&mut self.tree_data, &mut self.camera) else {
            ui.label("No tree data loaded.");
            return false;
        };

        let atlas_ref = self.atlas.as_ref();
        let headers_ref = self.tooltip_headers.as_ref();

        if let Some(clicked_id) = tree_renderer::draw_tree(ui, tree_data, camera, atlas_ref, headers_ref) {
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

/// Get the current tree version from the loaded build's spec.
fn get_tree_version(lua: &mlua::Lua) -> Option<String> {
    lua.load("return mainObject_ref.main.modes['BUILD'].spec.treeVersion")
        .eval::<String>()
        .map_err(|e| log::warn!("Failed to get tree version: {e}"))
        .ok()
}

/// Find the tree data directory for a specific tree version.
fn find_tree_data_dir(version: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidate = exe.parent()?.to_path_buf();
    for _ in 0..5 {
        let tree_dir = candidate
            .join("upstream")
            .join("src")
            .join("TreeData")
            .join(version);
        if tree_dir.is_dir() {
            return Some(tree_dir);
        }
        if !candidate.pop() {
            break;
        }
    }
    log::warn!("Tree data directory not found for version {version}");
    None
}

/// Find the upstream Assets directory (contains tooltip header images).
fn find_assets_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut candidate = exe.parent()?.to_path_buf();
    for _ in 0..5 {
        let assets = candidate.join("upstream").join("src").join("Assets");
        if assets.is_dir() {
            return Some(assets);
        }
        if !candidate.pop() {
            break;
        }
    }
    log::warn!("Assets directory not found for tooltip headers");
    None
}

/// Toggle a node allocation in Lua and trigger recalc.
fn toggle_node(lua: &mlua::Lua, node_id: u32) -> Result<(), mlua::Error> {
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

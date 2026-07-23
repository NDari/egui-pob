//! Tree tab: passive tree view with pan/zoom and node interaction.

use std::collections::HashSet;
use std::path::PathBuf;

use pob_egui::data::tree::{self, HoverInfo, MasteryEffectList, NodeType, TreeData};
use pob_egui::data::tree_sprites::TreeSpriteAtlas;
use pob_egui::lua_bridge::LuaBridge;

use super::tree_renderer::{self, TooltipHeaders, TreeCamera};

/// State for the mastery effect selection popup.
struct MasteryPopup {
    node_id: u32,
    list: MasteryEffectList,
}

/// State for the passive tree tab.
pub struct TreePanel {
    pub tree_data: Option<TreeData>,
    pub camera: Option<TreeCamera>,
    pub atlas: Option<TreeSpriteAtlas>,
    pub tooltip_headers: Option<TooltipHeaders>,
    pub tree_data_dir: Option<PathBuf>,
    pub textures_uploaded: bool,
    pub error: Option<String>,
    search: String,
    search_matches: HashSet<u32>,
    /// Index (into the sorted match list) of the match last jumped to.
    search_cycle: Option<usize>,
    mastery_popup: Option<MasteryPopup>,
    /// Path/depends info for the currently hovered node, fetched from Lua
    /// once per hover change.
    hover_cache: Option<(u32, HoverInfo)>,
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
                    search: String::new(),
                    search_matches: HashSet::new(),
                    search_cycle: None,
                    mastery_popup: None,
                    hover_cache: None,
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
            search: String::new(),
            search_matches: HashSet::new(),
            search_cycle: None,
            mastery_popup: None,
            hover_cache: None,
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
            if self.tooltip_headers.is_none()
                && let Some(dir) = find_assets_dir()
            {
                log::info!("Loading tooltip headers from: {}", dir.display());
                self.tooltip_headers = Some(TooltipHeaders::load(
                    ui.ctx(),
                    &dir,
                    self.tree_data_dir.as_deref(),
                ));
            }
            self.textures_uploaded = true;
        }

        let (Some(tree_data), Some(camera)) = (&mut self.tree_data, &mut self.camera) else {
            ui.label("No tree data loaded.");
            return false;
        };

        // Search bar. Enter / Shift+Enter (or the arrow buttons) cycle through
        // matches, centering the camera on each.
        let mut jump: Option<i32> = None;
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(260.0)
                    .hint_text("name, stat, type, or oil:..."),
            );
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                response.request_focus();
            }
            if response.changed() {
                self.search_matches = tree_data.search_matches(&self.search);
                self.search_cycle = None;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                jump = Some(if ui.input(|i| i.modifiers.shift) {
                    -1
                } else {
                    1
                });
                // Keep focus so repeated Enter keeps cycling
                response.request_focus();
            }
            if !self.search.trim().is_empty() {
                let count = self.search_matches.len();
                if count > 0 {
                    if ui.small_button("◀").clicked() {
                        jump = Some(-1);
                    }
                    if ui.small_button("▶").clicked() {
                        jump = Some(1);
                    }
                    match self.search_cycle {
                        Some(i) => ui.label(format!("{} / {count} matches", i + 1)),
                        None => ui.label(format!("{count} matches")),
                    };
                } else {
                    ui.label("0 matches");
                }
                if ui.small_button("✕").clicked() {
                    self.search.clear();
                    self.search_matches.clear();
                    self.search_cycle = None;
                }
            }
        });

        // Cycle to the next/previous match and center the camera on it
        if let Some(dir) = jump
            && !self.search_matches.is_empty()
        {
            // Sort by position for a left-to-right sweep across the tree
            let mut order: Vec<u32> = self.search_matches.iter().copied().collect();
            order.sort_unstable_by(|a, b| {
                let (na, nb) = (&tree_data.nodes[a], &tree_data.nodes[b]);
                na.x.total_cmp(&nb.x).then(na.y.total_cmp(&nb.y))
            });
            let len = order.len();
            let next = match (self.search_cycle, dir) {
                (Some(i), d) if d > 0 => (i + 1) % len,
                (Some(i), _) => (i + len - 1) % len,
                (None, d) if d > 0 => 0,
                (None, _) => len - 1,
            };
            self.search_cycle = Some(next);
            if let Some(node) = tree_data.nodes.get(&order[next]) {
                camera.center_x = node.x;
                camera.center_y = node.y;
                // Zoom in enough to read the node if currently zoomed way out
                camera.zoom = camera.zoom.max(0.25);
            }
        }

        // Undo/redo (Ctrl+Z / Ctrl+Y), only when no widget (e.g. the search
        // field) has keyboard focus
        if ui.ctx().memory(|m| m.focused().is_none()) {
            let undo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            let redo_pressed =
                ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y));
            if undo_pressed || redo_pressed {
                let result = if undo_pressed {
                    tree::undo(bridge.lua())
                } else {
                    tree::redo(bridge.lua())
                };
                match result {
                    Ok(()) => {
                        if let Err(e) = tree_data.refresh_allocation(bridge.lua()) {
                            log::error!("Failed to refresh allocation: {e}");
                        }
                        if let Err(e) = tree_data.refresh_mastery_stats(bridge.lua()) {
                            log::error!("Failed to refresh mastery stats: {e}");
                        }
                        changed = true;
                    }
                    Err(e) => log::error!("Undo/redo failed: {e}"),
                }
            }
        }

        let atlas_ref = self.atlas.as_ref();
        let headers_ref = self.tooltip_headers.as_ref();

        let empty = HashSet::new();
        let (hover_node, hover_path, hover_depends) = match &self.hover_cache {
            Some((id, info)) => (Some(*id), &info.path, &info.depends),
            None => (None, &empty, &empty),
        };
        let view = tree_renderer::draw_tree(
            ui,
            tree_data,
            camera,
            atlas_ref,
            headers_ref,
            &tree_renderer::TreeOverlays {
                search_matches: &self.search_matches,
                hover_node,
                hover_path,
                hover_depends,
            },
        );

        // Route clicks: masteries get the effect-selection popup, everything
        // else toggles allocation directly.
        if let Some(click) = view.clicked
            && self.mastery_popup.is_none()
        {
            let node = tree_data.nodes.get(&click.node_id);
            let is_mastery = node.is_some_and(|n| n.node_type == NodeType::Mastery);
            let is_allocated = node.is_some_and(|n| n.is_allocated);

            if is_mastery && (!is_allocated || click.is_right) {
                // Unallocated mastery click, or right-click to change the
                // effect on an allocated one: open the popup.
                match tree::fetch_mastery_effects(bridge.lua(), click.node_id) {
                    Ok(Some(list)) => {
                        self.mastery_popup = Some(MasteryPopup {
                            node_id: click.node_id,
                            list,
                        })
                    }
                    Ok(None) => {} // mastery with no selectable effects - inert
                    Err(e) => log::error!("Failed to fetch mastery effects: {e}"),
                }
            } else if !click.is_right {
                if let Err(e) = tree::toggle_node(bridge.lua(), click.node_id) {
                    log::error!("Failed to toggle node {}: {e}", click.node_id);
                } else if let Err(e) = tree_data.refresh_allocation(bridge.lua()) {
                    log::error!("Failed to refresh allocation: {e}");
                } else {
                    if is_mastery && let Err(e) = tree_data.refresh_mastery_stats(bridge.lua()) {
                        log::error!("Failed to refresh mastery stats: {e}");
                    }
                    changed = true;
                }
            }
        }

        // Mastery effect selection popup
        if let Some(popup) = &self.mastery_popup {
            let mut selected: Option<u32> = None;
            let mut close = false;

            let modal = egui::Modal::new(egui::Id::new("mastery_popup")).show(ui.ctx(), |ui| {
                ui.set_max_width(520.0);
                ui.heading(&popup.list.node_name);
                ui.separator();
                for effect in &popup.list.effects {
                    let is_current = popup.list.current == Some(effect.id);
                    let text = egui::RichText::new(&effect.label)
                        .color(egui::Color32::from_rgb(136, 136, 255));
                    if ui.selectable_label(is_current, text).clicked() {
                        selected = Some(effect.id);
                    }
                }
                ui.separator();
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
            if modal.should_close() {
                close = true;
            }

            if let Some(effect_id) = selected {
                let node_id = popup.node_id;
                if let Err(e) = tree::select_mastery_effect(bridge.lua(), node_id, effect_id) {
                    log::error!(
                        "Failed to select mastery effect {effect_id} on node {node_id}: {e}"
                    );
                } else {
                    if let Err(e) = tree_data.refresh_allocation(bridge.lua()) {
                        log::error!("Failed to refresh allocation: {e}");
                    }
                    if let Err(e) = tree_data.refresh_mastery_stats(bridge.lua()) {
                        log::error!("Failed to refresh mastery stats: {e}");
                    }
                    changed = true;
                }
                close = true;
            }

            if close {
                self.mastery_popup = None;
            }
        }

        // Recompute search matches when the tree changed (mastery stats can shift)
        if changed && !self.search.trim().is_empty() {
            self.search_matches = tree_data.search_matches(&self.search);
        }

        // Keep the hover path/depends cache in sync: refetch when the hovered
        // node changed, or when allocations changed (paths shift).
        if changed {
            self.hover_cache = None;
        }
        let cached_id = self.hover_cache.as_ref().map(|(id, _)| *id);
        if view.hovered != cached_id {
            self.hover_cache =
                view.hovered
                    .and_then(|id| match tree::fetch_hover_info(bridge.lua(), id) {
                        Ok(info) => Some((id, info)),
                        Err(e) => {
                            log::error!("Failed to fetch hover info for node {id}: {e}");
                            None
                        }
                    });
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

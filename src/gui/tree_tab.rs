//! Tree tab: passive tree view with pan/zoom and node interaction.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use pob_egui::data::jewels::{self, RadiusDef, SocketInfo};
use pob_egui::data::node_power;
use pob_egui::data::tree::{self, HoverInfo, MasteryEffectList, NodeType, TreeData};
use pob_egui::data::tree_specs::{self, CompareDiff, SpecAllocation, SpecInfo, TreeVersion};
use pob_egui::data::tree_sprites::TreeSpriteAtlas;
use pob_egui::lua_bridge::LuaBridge;

use super::tree_renderer::{self, TooltipHeaders, TreeCamera};

/// State for the mastery effect selection popup.
struct MasteryPopup {
    node_id: u32,
    list: MasteryEffectList,
}

/// A pending name prompt in the spec manager.
enum SpecAction {
    New,
    Copy(usize),
    Rename(usize),
}

/// Name-entry prompt state for New/Copy/Rename.
struct SpecPrompt {
    action: SpecAction,
    text: String,
}

/// Pending tree version conversion awaiting confirmation.
struct ConvertPopup {
    /// Target version id, e.g. "3_27".
    version: String,
    display: String,
    /// True to convert every spec instead of just the active one.
    all: bool,
    /// Passed through to upstream: ignore the active spec's subtype
    /// (ruthless/alternate) when matching the target version.
    ignore_sub_type: bool,
}

/// Node power heatmap + report state.
struct NodePowerState {
    enabled: bool,
    /// Available power stats (entry 0 is the Offence/Defence default).
    stats: Vec<node_power::PowerStat>,
    /// Selected index into `stats`.
    stat_sel: usize,
    /// Selected index into `POWER_DEPTHS`.
    depth_sel: usize,
    /// True while the builder coroutine is running.
    building: bool,
    /// Builder progress percentage (0-100).
    progress: i64,
    /// Heatmap tint per unallocated node id.
    colors: HashMap<u32, egui::Color32>,
    report_open: bool,
    report: Vec<node_power::ReportRow>,
    /// Report sort column (0 name, 1 power, 2 power/point, 3 distance).
    sort_col: usize,
    sort_desc: bool,
}

impl Default for NodePowerState {
    fn default() -> Self {
        Self {
            enabled: false,
            stats: Vec::new(),
            stat_sel: 0,
            depth_sel: 0,
            building: false,
            progress: 0,
            colors: HashMap::new(),
            report_open: false,
            report: Vec::new(),
            sort_col: 1,
            sort_desc: true,
        }
    }
}

/// Max path depth options for the node power calculation.
const POWER_DEPTHS: [(&str, Option<i64>); 4] = [
    ("All", None),
    ("5", Some(5)),
    ("10", Some(10)),
    ("15", Some(15)),
];

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
    // Tree spec management
    specs: Vec<SpecInfo>,
    /// Active spec index (1-based, matching Lua).
    active_spec: usize,
    manage_specs_open: bool,
    spec_prompt: Option<SpecPrompt>,
    import_url: String,
    import_error: Option<String>,
    export_url: Option<String>,
    /// Set when the active spec changed: the whole panel (tree data, atlas)
    /// must be rebuilt by the parent.
    pub request_rebuild: bool,
    /// Set when the user right-clicked a jewel socket: the parent should
    /// switch to the Items tab.
    pub request_items_tab: bool,
    // Jewel radius overlays
    jewel_radii: Vec<RadiusDef>,
    jewel_sockets: Vec<SocketInfo>,
    // Tree version conversion
    tree_versions: Vec<TreeVersion>,
    convert_popup: Option<ConvertPopup>,
    // Spec comparison
    compare_enabled: bool,
    /// Compare spec index (1-based; 0 = not yet chosen).
    compare_index: usize,
    /// Cached allocation of the compare spec, keyed by its index.
    compare_cache: Option<(usize, SpecAllocation)>,
    /// Cached allocation of the active spec (invalidated on tree changes).
    current_cache: Option<SpecAllocation>,
    /// Computed diff shown by the renderer.
    compare_diff: Option<CompareDiff>,
    /// Node power heatmap + report.
    power: NodePowerState,
    /// Show stat difference previews in node tooltips (Ctrl+D toggles).
    show_stat_diffs: bool,
    /// Item tooltip of the jewel in the hovered socket (node id, lines).
    socket_tooltip: Option<(u32, Vec<pob_egui::data::items::TooltipLine>)>,
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
                    specs: Vec::new(),
                    active_spec: 1,
                    manage_specs_open: false,
                    spec_prompt: None,
                    import_url: String::new(),
                    import_error: None,
                    export_url: None,
                    request_rebuild: false,
                    request_items_tab: false,
                    jewel_radii: Vec::new(),
                    jewel_sockets: Vec::new(),
                    tree_versions: Vec::new(),
                    convert_popup: None,
                    compare_enabled: false,
                    compare_index: 0,
                    compare_cache: None,
                    current_cache: None,
                    compare_diff: None,
                    power: NodePowerState::default(),
                    show_stat_diffs: true,
                    socket_tooltip: None,
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

        let (specs, active_spec) = tree_specs::list_specs(lua).unwrap_or_else(|e| {
            log::error!("Failed to list tree specs: {e}");
            (Vec::new(), 1)
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
            specs,
            active_spec,
            manage_specs_open: false,
            spec_prompt: None,
            import_url: String::new(),
            import_error: None,
            export_url: None,
            request_rebuild: false,
            request_items_tab: false,
            jewel_radii: jewels::radius_defs(lua).unwrap_or_else(|e| {
                log::error!("Failed to load jewel radii: {e}");
                Vec::new()
            }),
            jewel_sockets: jewels::socket_jewels(lua).unwrap_or_else(|e| {
                log::error!("Failed to load jewel sockets: {e}");
                Vec::new()
            }),
            tree_versions: tree_specs::list_tree_versions(lua).unwrap_or_else(|e| {
                log::error!("Failed to list tree versions: {e}");
                Vec::new()
            }),
            convert_popup: None,
            compare_enabled: false,
            compare_index: 0,
            compare_cache: None,
            current_cache: None,
            compare_diff: None,
            power: NodePowerState::default(),
            show_stat_diffs: true,
            socket_tooltip: None,
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

        if self.manage_specs_open {
            changed |= self.show_manage_dialog(ui, bridge);
        }
        if self.convert_popup.is_some() {
            changed |= self.show_convert_popup(ui, bridge);
        }

        let (Some(tree_data), Some(camera)) = (&mut self.tree_data, &mut self.camera) else {
            ui.label("No tree data loaded.");
            return false;
        };

        // Search bar. Enter / Shift+Enter (or the arrow buttons) cycle through
        // matches, centering the camera on each.
        let mut jump: Option<i32> = None;
        let mut spec_switch: Option<usize> = None;
        ui.horizontal(|ui| {
            // Tree spec selector
            if !self.specs.is_empty() {
                let active_label = self
                    .specs
                    .get(self.active_spec - 1)
                    .map(spec_label)
                    .unwrap_or_else(|| "Default".to_string());
                egui::ComboBox::from_id_salt("spec_select")
                    .selected_text(active_label)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for (i, spec) in self.specs.iter().enumerate() {
                            if ui
                                .selectable_label(self.active_spec == i + 1, spec_label(spec))
                                .clicked()
                                && self.active_spec != i + 1
                            {
                                spec_switch = Some(i + 1);
                            }
                        }
                    });
                if ui.button("Manage...").clicked() {
                    self.manage_specs_open = true;
                }

                // Tree version dropdown: selecting a different version opens
                // the conversion confirmation popup (like upstream)
                if !self.tree_versions.is_empty() {
                    let active_version = self
                        .specs
                        .get(self.active_spec - 1)
                        .map(|s| s.tree_version.clone())
                        .unwrap_or_default();
                    let current_display = self
                        .tree_versions
                        .iter()
                        .find(|v| v.id == active_version)
                        .map(|v| v.display.clone())
                        .unwrap_or_else(|| active_version.replace('_', "."));
                    ui.label("Version:");
                    egui::ComboBox::from_id_salt("tree_version_select")
                        .selected_text(current_display)
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for version in self.tree_versions.iter().rev() {
                                if ui
                                    .selectable_label(
                                        version.id == active_version,
                                        &version.display,
                                    )
                                    .clicked()
                                    && version.id != active_version
                                {
                                    self.convert_popup = Some(ConvertPopup {
                                        version: version.id.clone(),
                                        display: version.display.clone(),
                                        all: false,
                                        ignore_sub_type: true,
                                    });
                                }
                            }
                        });
                }

                // Spec comparison toggle + compare-spec selector
                if ui
                    .checkbox(&mut self.compare_enabled, "Compare")
                    .on_hover_text(
                        "Highlight differences against another tree: green = allocate, \
                         red = deallocate, blue = different mastery effect",
                    )
                    .changed()
                {
                    self.compare_diff = None;
                    if self.compare_enabled && self.compare_index == 0 {
                        self.compare_index = self.active_spec;
                    }
                }
                if self.compare_enabled {
                    let compare_label = self
                        .compare_index
                        .checked_sub(1)
                        .and_then(|i| self.specs.get(i))
                        .map(spec_label)
                        .unwrap_or_else(|| "-".to_string());
                    egui::ComboBox::from_id_salt("compare_select")
                        .selected_text(compare_label)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            for (i, spec) in self.specs.iter().enumerate() {
                                if ui
                                    .selectable_label(self.compare_index == i + 1, spec_label(spec))
                                    .clicked()
                                {
                                    self.compare_index = i + 1;
                                }
                            }
                        });
                }
                ui.separator();
            }

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

        // Version mismatch banner: offer conversion when the active spec is
        // from an older tree version
        let active_outdated = self
            .specs
            .get(self.active_spec - 1)
            .is_some_and(|s| !s.is_latest_version);
        if active_outdated
            && let Some(latest) = self.tree_versions.iter().find(|v| v.is_latest).cloned()
        {
            ui.horizontal(|ui| {
                let version_display = self
                    .specs
                    .get(self.active_spec - 1)
                    .map(|s| s.tree_version.replace('_', "."))
                    .unwrap_or_default();
                ui.colored_label(
                    super::theme::Theme::MAIN_SKILL,
                    format!("⚠ This tree is from version {version_display}."),
                );
                if ui
                    .button(format!("Convert to {}", latest.display))
                    .clicked()
                {
                    self.convert_popup = Some(ConvertPopup {
                        version: latest.id.clone(),
                        display: latest.display.clone(),
                        all: false,
                        ignore_sub_type: false,
                    });
                }
                if self.specs.len() > 1 && ui.button("Convert all trees").clicked() {
                    self.convert_popup = Some(ConvertPopup {
                        version: latest.id.clone(),
                        display: latest.display.clone(),
                        all: true,
                        ignore_sub_type: false,
                    });
                }
            });
        }

        // Apply a spec switch from the dropdown
        if let Some(index) = spec_switch {
            match tree_specs::set_active_spec(bridge.lua(), index) {
                Ok(()) => {
                    self.active_spec = index;
                    self.request_rebuild = true;
                    changed = true;
                }
                Err(e) => log::error!("Failed to switch spec: {e}"),
            }
        }

        // Node power heatmap controls
        ui.horizontal(|ui| {
            let was_enabled = self.power.enabled;
            ui.checkbox(&mut self.power.enabled, "Node power")
                .on_hover_text(
                    "Color unallocated nodes by their impact on the selected stat \
                     (default: red = offence, blue = defence)",
                );
            if self.power.enabled && !was_enabled {
                if self.power.stats.is_empty() {
                    match node_power::list_power_stats(bridge.lua()) {
                        Ok(stats) => self.power.stats = stats,
                        Err(e) => log::error!("Failed to list power stats: {e}"),
                    }
                }
                apply_power_selection(&self.power, bridge);
            }
            if self.power.enabled {
                let mut selection_changed = false;
                let sel_label = self
                    .power
                    .stats
                    .get(self.power.stat_sel)
                    .map(|s| s.label.as_str())
                    .unwrap_or("?");
                egui::ComboBox::from_id_salt("power_stat_select")
                    .selected_text(sel_label)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for i in 0..self.power.stats.len() {
                            let label = self.power.stats[i].label.clone();
                            if ui
                                .selectable_label(i == self.power.stat_sel, label)
                                .clicked()
                                && i != self.power.stat_sel
                            {
                                self.power.stat_sel = i;
                                selection_changed = true;
                            }
                        }
                    });
                ui.label("Depth:");
                egui::ComboBox::from_id_salt("power_depth_select")
                    .selected_text(POWER_DEPTHS[self.power.depth_sel].0)
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for (i, (label, _)) in POWER_DEPTHS.iter().enumerate() {
                            if ui
                                .selectable_label(i == self.power.depth_sel, *label)
                                .clicked()
                                && i != self.power.depth_sel
                            {
                                self.power.depth_sel = i;
                                selection_changed = true;
                            }
                        }
                    });
                if selection_changed {
                    apply_power_selection(&self.power, bridge);
                }
                if ui.button("Power report").clicked() {
                    self.power.report_open = !self.power.report_open;
                    if self.power.report_open && !self.power.building {
                        refresh_power_report(&mut self.power, bridge);
                    }
                }
                if self.power.building {
                    ui.spinner();
                    ui.label(format!("Calculating... {}%", self.power.progress));
                }
            }
        });

        // Drive the power builder coroutine (one ~100ms slice per frame)
        if self.power.enabled {
            match node_power::power_dirty(bridge.lua()) {
                Ok(true) => {
                    self.power.building = true;
                    match node_power::power_step(bridge.lua()) {
                        Ok((done, progress)) => {
                            self.power.progress = progress;
                            if done {
                                self.power.building = false;
                                match node_power::heatmap_colors(bridge.lua()) {
                                    Ok(colors) => self.power.colors = colors,
                                    Err(e) => log::error!("Failed to read heatmap: {e}"),
                                }
                                if self.power.report_open {
                                    refresh_power_report(&mut self.power, bridge);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Power build step failed: {e}");
                            self.power.enabled = false;
                            self.power.building = false;
                        }
                    }
                    ui.ctx().request_repaint();
                }
                Ok(false) => self.power.building = false,
                Err(e) => log::error!("Power dirty check failed: {e}"),
            }
        }

        // Power report window; clicking a row pans the camera to that node
        if let Some((x, y)) = show_power_report(&mut self.power, ui) {
            camera.center_x = x;
            camera.center_y = y;
            camera.zoom = camera.zoom.max(0.25);
        }

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
            // Ctrl+D: toggle stat difference previews in node tooltips
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::D)) {
                self.show_stat_diffs = !self.show_stat_diffs;
                self.hover_cache = None;
            }
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

        // Refresh comparison caches and recompute the diff when stale
        if self.compare_enabled && self.compare_index > 0 {
            if self
                .compare_cache
                .as_ref()
                .is_none_or(|(i, _)| *i != self.compare_index)
            {
                match tree_specs::spec_allocation(bridge.lua(), self.compare_index) {
                    Ok(alloc) => self.compare_cache = Some((self.compare_index, alloc)),
                    Err(e) => log::error!("Failed to read compare spec: {e}"),
                }
                self.compare_diff = None;
            }
            if self.current_cache.is_none() {
                match tree_specs::spec_allocation(bridge.lua(), self.active_spec) {
                    Ok(alloc) => self.current_cache = Some(alloc),
                    Err(e) => log::error!("Failed to read active spec: {e}"),
                }
                self.compare_diff = None;
            }
            if self.compare_diff.is_none()
                && let (Some(current), Some((_, compare))) =
                    (&self.current_cache, &self.compare_cache)
            {
                self.compare_diff = Some(tree_specs::compare_diff(current, compare));
            }
        }

        let atlas_ref = self.atlas.as_ref();
        let headers_ref = self.tooltip_headers.as_ref();

        let empty = HashSet::new();
        let (hover_node, hover_path, hover_depends) = match &self.hover_cache {
            Some((id, info)) => (Some(*id), &info.path, &info.depends),
            None => (None, &empty, &empty),
        };
        let compare = if self.compare_enabled {
            self.compare_diff.as_ref()
        } else {
            None
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
                compare,
                jewel_radii: &self.jewel_radii,
                jewel_sockets: &self.jewel_sockets,
                heatmap: self.power.enabled.then_some(&self.power.colors),
                hover_diff: self
                    .hover_cache
                    .as_ref()
                    .map(|(_, info)| info.diff.as_slice())
                    .unwrap_or(&[]),
                hover_jewel: self
                    .socket_tooltip
                    .as_ref()
                    .map(|(_, lines)| lines.as_slice())
                    .unwrap_or(&[]),
            },
        );

        // Route clicks: masteries get the effect-selection popup, right-
        // clicking a jewel socket jumps to the Items tab, everything else
        // toggles allocation directly.
        if let Some(click) = view.clicked
            && self.mastery_popup.is_none()
        {
            let node = tree_data.nodes.get(&click.node_id);
            let is_mastery = node.is_some_and(|n| n.node_type == NodeType::Mastery);
            let is_socket = node.is_some_and(|n| n.node_type == NodeType::Socket);
            let is_allocated = node.is_some_and(|n| n.is_allocated);

            if is_socket && click.is_right {
                self.request_items_tab = true;
            } else if is_mastery && (!is_allocated || click.is_right) {
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
            self.socket_tooltip = None;
        }
        let cached_id = self.hover_cache.as_ref().map(|(id, _)| *id);
        if view.hovered != cached_id {
            self.hover_cache = view.hovered.and_then(|id| {
                match tree::fetch_hover_info(bridge.lua(), id, self.show_stat_diffs) {
                    Ok(info) => Some((id, info)),
                    Err(e) => {
                        log::error!("Failed to fetch hover info for node {id}: {e}");
                        None
                    }
                }
            });
            // Allocated sockets with a jewel show the jewel's item tooltip
            self.socket_tooltip = view.hovered.and_then(|id| {
                let is_filled_socket = tree_data
                    .nodes
                    .get(&id)
                    .is_some_and(|n| n.node_type == NodeType::Socket && n.is_allocated);
                if !is_filled_socket {
                    return None;
                }
                match jewels::socket_jewel_tooltip(bridge.lua(), id) {
                    Ok(lines) if !lines.is_empty() => Some((id, lines)),
                    Ok(_) => None,
                    Err(e) => {
                        log::error!("Failed to fetch socket jewel tooltip: {e}");
                        None
                    }
                }
            });
        }

        // Any tree change invalidates the comparison snapshot of the active
        // spec and the jewel socket info (allocating a socket changes it)
        if changed {
            self.current_cache = None;
            self.compare_diff = None;
            self.refresh_jewels(bridge);
        }

        changed
    }

    /// Re-read jewel socket contents from Lua. Called after tree changes and
    /// by the parent after item changes (equipping jewels).
    pub fn refresh_jewels(&mut self, bridge: &LuaBridge) {
        match jewels::socket_jewels(bridge.lua()) {
            Ok(sockets) => self.jewel_sockets = sockets,
            Err(e) => log::error!("Failed to refresh jewel sockets: {e}"),
        }
    }

    /// Re-extract the tree data while keeping the camera and sprite atlas.
    /// Needed when item changes rebuild cluster jewel subgraphs (socketing or
    /// removing a cluster jewel adds/removes tree nodes).
    pub fn refresh_tree_data(&mut self, bridge: &LuaBridge) {
        match TreeData::extract(bridge.lua()) {
            Ok(td) => {
                self.search_matches = td.search_matches(&self.search);
                self.tree_data = Some(td);
                self.search_cycle = None;
                self.hover_cache = None;
                self.current_cache = None;
                self.compare_diff = None;
            }
            Err(e) => log::error!("Failed to refresh tree data: {e}"),
        }
    }

    /// Draw the "Manage Passive Trees" dialog. Returns true if the build
    /// changed (spec created/copied/deleted/imported).
    fn show_manage_dialog(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let mut changed = false;
        let mut close = false;
        // (action closures collected to run after the UI pass)
        let mut activate: Option<usize> = None;
        let mut delete: Option<usize> = None;

        egui::Window::new("Manage Passive Trees")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                for (i, spec) in self.specs.iter().enumerate() {
                    let index = i + 1;
                    ui.horizontal(|ui| {
                        let is_active = index == self.active_spec;
                        let label = if is_active {
                            egui::RichText::new(spec_label(spec))
                                .color(super::theme::Theme::MAIN_SKILL)
                        } else {
                            egui::RichText::new(spec_label(spec))
                        };
                        ui.label(label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.specs.len() > 1 && ui.small_button("Delete").clicked() {
                                delete = Some(index);
                            }
                            if ui.small_button("Rename").clicked() {
                                self.spec_prompt = Some(SpecPrompt {
                                    action: SpecAction::Rename(index),
                                    text: spec.title.clone(),
                                });
                            }
                            if ui.small_button("Copy").clicked() {
                                self.spec_prompt = Some(SpecPrompt {
                                    action: SpecAction::Copy(index),
                                    text: format!("{} (copy)", spec.title),
                                });
                            }
                            if !is_active && ui.small_button("Activate").clicked() {
                                activate = Some(index);
                            }
                        });
                    });
                }
                ui.separator();

                // Name prompt for New/Copy/Rename
                if let Some(prompt) = &mut self.spec_prompt {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.add(egui::TextEdit::singleline(&mut prompt.text).desired_width(200.0));
                    });
                }

                ui.horizontal(|ui| {
                    if let Some(prompt) = &self.spec_prompt {
                        let name = prompt.text.trim().to_string();
                        if ui
                            .add_enabled(!name.is_empty(), egui::Button::new("OK"))
                            .clicked()
                        {
                            let result = match prompt.action {
                                SpecAction::New => tree_specs::new_spec(bridge.lua(), &name),
                                SpecAction::Copy(i) => {
                                    tree_specs::copy_spec(bridge.lua(), i, &name)
                                }
                                SpecAction::Rename(i) => {
                                    tree_specs::rename_spec(bridge.lua(), i, &name)
                                }
                            };
                            match result {
                                Ok(()) => {
                                    if !matches!(prompt.action, SpecAction::Rename(_)) {
                                        self.request_rebuild = true;
                                        changed = true;
                                    }
                                    self.refresh_specs(bridge);
                                    self.spec_prompt = None;
                                }
                                Err(e) => log::error!("Spec action failed: {e}"),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.spec_prompt = None;
                        }
                    } else if ui.button("New Tree").clicked() {
                        self.spec_prompt = Some(SpecPrompt {
                            action: SpecAction::New,
                            text: "Default".to_string(),
                        });
                    }
                });
                ui.separator();

                // Import tree URL as a new spec
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_url)
                            .desired_width(260.0)
                            .hint_text("pathofexile.com / poeplanner / poeurl link"),
                    );
                    if ui.button("Import Tree").clicked() && !self.import_url.trim().is_empty() {
                        let result =
                            tree_specs::expand_shortlink(self.import_url.trim()).and_then(|url| {
                                tree_specs::import_tree_url(bridge.lua(), &url, "Imported tree")
                                    .map_err(|e| anyhow::anyhow!("Import failed: {e}"))
                            });
                        match result {
                            Ok(None) => {
                                self.import_error = None;
                                self.import_url.clear();
                                self.request_rebuild = true;
                                changed = true;
                                self.refresh_specs(bridge);
                            }
                            Ok(Some(err)) => self.import_error = Some(err),
                            Err(e) => self.import_error = Some(format!("{e}")),
                        }
                    }
                });
                if let Some(ref err) = self.import_error {
                    ui.colored_label(super::theme::Theme::ERROR, err);
                }

                // Export the active spec as a URL
                ui.horizontal(|ui| {
                    if ui.button("Export Tree URL").clicked() {
                        match tree_specs::export_tree_url(bridge.lua()) {
                            Ok(url) => self.export_url = Some(url),
                            Err(e) => log::error!("Export failed: {e}"),
                        }
                    }
                    if let Some(url) = &self.export_url
                        && ui.button("Copy to Clipboard").clicked()
                        && let Ok(mut clip) = arboard::Clipboard::new()
                    {
                        let _ = clip.set_text(url);
                    }
                });
                if let Some(ref url) = self.export_url {
                    ui.add(
                        egui::TextEdit::multiline(&mut url.as_str())
                            .desired_width(340.0)
                            .desired_rows(2)
                            .font(egui::TextStyle::Monospace),
                    );
                }

                ui.separator();
                if ui.button("Done").clicked() {
                    close = true;
                }
            });

        if let Some(index) = activate {
            match tree_specs::set_active_spec(bridge.lua(), index) {
                Ok(()) => {
                    self.request_rebuild = true;
                    changed = true;
                    self.refresh_specs(bridge);
                }
                Err(e) => log::error!("Failed to switch spec: {e}"),
            }
        }
        if let Some(index) = delete {
            match tree_specs::delete_spec(bridge.lua(), index) {
                Ok(()) => {
                    self.request_rebuild = true;
                    changed = true;
                    self.refresh_specs(bridge);
                }
                Err(e) => log::error!("Failed to delete spec: {e}"),
            }
        }
        if close {
            self.manage_specs_open = false;
            self.export_url = None;
            self.import_error = None;
            self.spec_prompt = None;
        }
        changed
    }

    /// Draw the version conversion confirmation popup. Returns true if a
    /// conversion ran (the panel must be rebuilt by the parent).
    fn show_convert_popup(&mut self, ui: &mut egui::Ui, bridge: &LuaBridge) -> bool {
        let Some(popup) = &self.convert_popup else {
            return false;
        };
        let (version, display, all, ignore_sub_type) = (
            popup.version.clone(),
            popup.display.clone(),
            popup.all,
            popup.ignore_sub_type,
        );

        let mut changed = false;
        let mut close = false;
        let title = if all {
            format!("Convert all to Version {display}")
        } else {
            format!("Convert to Version {display}")
        };

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    "Warning: some or all of the passives may be de-allocated \
                     due to changes in the tree.",
                );
                if all {
                    ui.label(format!(
                        "Convert will replace all trees that are not version {display}. \
                         This action cannot be undone."
                    ));
                } else {
                    ui.label(
                        "Convert will replace your current tree. \
                         Copy + Convert will keep the original as a separate tree.",
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Convert").clicked() {
                        let result = if all {
                            tree_specs::convert_all_to_version(bridge.lua(), &version)
                        } else {
                            tree_specs::convert_to_version(
                                bridge.lua(),
                                &version,
                                true,
                                ignore_sub_type,
                            )
                        };
                        match result {
                            Ok(()) => changed = true,
                            Err(e) => log::error!("Conversion failed: {e}"),
                        }
                        close = true;
                    }
                    if !all && ui.button("Copy + Convert").clicked() {
                        match tree_specs::convert_to_version(
                            bridge.lua(),
                            &version,
                            false,
                            ignore_sub_type,
                        ) {
                            Ok(()) => changed = true,
                            Err(e) => log::error!("Conversion failed: {e}"),
                        }
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.convert_popup = None;
        }
        if changed {
            self.request_rebuild = true;
            self.refresh_specs(bridge);
        }
        changed
    }

    fn refresh_specs(&mut self, bridge: &LuaBridge) {
        match tree_specs::list_specs(bridge.lua()) {
            Ok((specs, active)) => {
                self.specs = specs;
                self.active_spec = active;
            }
            Err(e) => log::error!("Failed to refresh specs: {e}"),
        }
    }
}

/// Display label for a spec: "[3.25] Title" for non-latest tree versions.
/// Push the current power stat/depth selection to Lua and flag a rebuild.
fn apply_power_selection(power: &NodePowerState, bridge: &LuaBridge) {
    let Some(stat) = power.stats.get(power.stat_sel) else {
        return;
    };
    let depth = POWER_DEPTHS[power.depth_sel].1;
    if let Err(e) = node_power::set_power_stat(bridge.lua(), stat.index, depth) {
        log::error!("Failed to set power stat: {e}");
    }
}

/// Rebuild the power report rows from Lua and apply the current sort.
fn refresh_power_report(power: &mut NodePowerState, bridge: &LuaBridge) {
    match node_power::power_report(bridge.lua()) {
        Ok(mut rows) => {
            sort_power_report(&mut rows, power.sort_col, power.sort_desc);
            power.report = rows;
        }
        Err(e) => log::error!("Failed to build power report: {e}"),
    }
}

fn sort_power_report(report: &mut [node_power::ReportRow], col: usize, desc: bool) {
    report.sort_by(|a, b| {
        let ord = match col {
            0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            2 => a.path_power.total_cmp(&b.path_power),
            3 => a.path_dist.cmp(&b.path_dist),
            _ => a.power.total_cmp(&b.power),
        };
        if desc { ord.reverse() } else { ord }
    });
}

/// Draw the power report window. Returns the tree position to pan to when a
/// row was clicked.
fn show_power_report(power: &mut NodePowerState, ui: &mut egui::Ui) -> Option<(f32, f32)> {
    if !power.report_open {
        return None;
    }
    let mut pan = None;
    let mut open = power.report_open;
    let mut new_sort: Option<usize> = None;

    const COLS: [(&str, f32); 4] = [
        ("Node", 200.0),
        ("Power", 90.0),
        ("Power/Point", 90.0),
        ("Dist", 40.0),
    ];

    egui::Window::new("Power Report")
        .open(&mut open)
        .default_size([470.0, 420.0])
        .resizable(true)
        .show(ui.ctx(), |ui| {
            if power.building {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "Calculating... {}% (results update when done)",
                        power.progress
                    ));
                });
            }
            ui.horizontal(|ui| {
                for (i, (label, width)) in COLS.iter().enumerate() {
                    let marker = if power.sort_col == i {
                        if power.sort_desc { " ▼" } else { " ▲" }
                    } else {
                        ""
                    };
                    if ui
                        .add_sized(
                            [*width, 18.0],
                            egui::Button::new(format!("{label}{marker}")).small(),
                        )
                        .clicked()
                    {
                        new_sort = Some(i);
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("power_report_scroll")
                .show_rows(ui, 18.0, power.report.len(), |ui, range| {
                    for row in &power.report[range] {
                        ui.horizontal(|ui| {
                            let name_color = if row.allocated {
                                egui::Color32::from_rgb(120, 220, 120)
                            } else {
                                egui::Color32::WHITE
                            };
                            let clickable = row.id != 0;
                            let name = egui::RichText::new(&row.name).color(name_color).size(12.0);
                            let label = egui::Label::new(name).truncate();
                            let resp = if clickable {
                                let resp = ui.add_sized(
                                    [COLS[0].1, 16.0],
                                    label.sense(egui::Sense::click()),
                                );
                                resp.clone().on_hover_text("Click to show on the tree");
                                resp
                            } else {
                                ui.add_sized([COLS[0].1, 16.0], label)
                            };
                            if clickable && resp.clicked() {
                                pan = Some((row.x as f32, row.y as f32));
                            }
                            ui.add_sized(
                                [COLS[1].1, 16.0],
                                egui::Label::new(super::theme::pob_layout_job(
                                    &row.power_str,
                                    12.0,
                                    egui::Color32::WHITE,
                                )),
                            );
                            ui.add_sized(
                                [COLS[2].1, 16.0],
                                egui::Label::new(super::theme::pob_layout_job(
                                    &row.path_power_str,
                                    12.0,
                                    egui::Color32::WHITE,
                                )),
                            );
                            ui.add_sized(
                                [COLS[3].1, 16.0],
                                egui::Label::new(
                                    egui::RichText::new(row.path_dist.to_string()).size(12.0),
                                ),
                            );
                        });
                    }
                });
        });

    if let Some(col) = new_sort {
        if power.sort_col == col {
            power.sort_desc = !power.sort_desc;
        } else {
            power.sort_col = col;
            // Name sorts ascending by default, numbers descending
            power.sort_desc = col != 0;
        }
        sort_power_report(&mut power.report, power.sort_col, power.sort_desc);
    }
    power.report_open = open;
    pan
}

fn spec_label(spec: &SpecInfo) -> String {
    if spec.is_latest_version {
        spec.title.clone()
    } else {
        format!("[{}] {}", spec.tree_version.replace('_', "."), spec.title)
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

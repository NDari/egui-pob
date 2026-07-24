//! Passive tree renderer: draws nodes and connections using egui's Painter API.
//! When a sprite atlas is loaded, nodes are rendered with actual game textures.
//! Falls back to colored circles when sprites are unavailable.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use pob_egui::data::tree::{ArcInfo, GroupBackground, NodeType, TreeData, TreeNode};
use pob_egui::data::tree_sprites::{SpriteRegion, TreeSpriteAtlas};

/// Colors for different node states and types.
struct Palette;

impl Palette {
    const NORMAL: egui::Color32 = egui::Color32::from_rgb(120, 120, 120);
    const NOTABLE: egui::Color32 = egui::Color32::from_rgb(200, 180, 100);
    const KEYSTONE: egui::Color32 = egui::Color32::from_rgb(220, 160, 60);
    const SOCKET: egui::Color32 = egui::Color32::from_rgb(100, 180, 220);
    const MASTERY: egui::Color32 = egui::Color32::from_rgb(180, 140, 200);
    const CLASS_START: egui::Color32 = egui::Color32::from_rgb(200, 200, 200);
    const ALLOCATED: egui::Color32 = egui::Color32::from_rgb(255, 200, 50);
    const CONNECTION: egui::Color32 = egui::Color32::from_rgb(80, 80, 80);
    const CONNECTION_ALLOCATED: egui::Color32 = egui::Color32::from_rgb(200, 170, 50);
    const ASCENDANCY: egui::Color32 = egui::Color32::from_rgb(140, 100, 160);
    const HOVER_OUTLINE: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
    const SEARCH_HIGHLIGHT: egui::Color32 = egui::Color32::from_rgb(255, 50, 50);
    /// Connectors/rings along the shortest path to the hovered node.
    const PATH_PREVIEW: egui::Color32 = egui::Color32::from_rgb(140, 190, 80);
    /// Nodes/connectors that would disconnect if the hovered node were removed.
    const DEPENDENT: egui::Color32 = egui::Color32::from_rgb(220, 60, 50);
    /// Red tint applied to sprites of dependent nodes.
    const DEPENDENT_TINT: egui::Color32 = egui::Color32::from_rgb(255, 90, 80);
    /// Compare mode: allocated in the compare spec, not the current one.
    const COMPARE_ADD: egui::Color32 = egui::Color32::from_rgb(80, 230, 80);
    /// Compare mode: allocated in the current spec, not the compare one.
    const COMPARE_REMOVE: egui::Color32 = egui::Color32::from_rgb(235, 70, 60);
    /// Compare mode: mastery allocated in both with a different effect.
    const COMPARE_MASTERY: egui::Color32 = egui::Color32::from_rgb(100, 150, 255);
}

/// A click on a tree node.
pub struct NodeClick {
    pub node_id: u32,
    /// True for a right (secondary) click.
    pub is_right: bool,
}

/// Result of drawing the tree for one frame.
#[derive(Default)]
pub struct TreeViewResponse {
    pub clicked: Option<NodeClick>,
    /// Node currently under the cursor.
    pub hovered: Option<u32>,
}

/// Per-frame interaction overlays.
pub struct TreeOverlays<'a> {
    pub search_matches: &'a HashSet<u32>,
    /// The node the path/depends sets below belong to. They are only applied
    /// while this node is the hovered one (the caller's cache can lag a frame).
    pub hover_node: Option<u32>,
    /// Nodes on the shortest path to `hover_node`.
    pub hover_path: &'a HashSet<u32>,
    /// Nodes that would disconnect if `hover_node` were deallocated.
    pub hover_depends: &'a HashSet<u32>,
    /// Stat difference lines for `hover_node` (PoB color-coded), appended to
    /// its tooltip.
    pub hover_diff: &'a [String],
    /// Item tooltip of the jewel socketed in `hover_node` (empty when the
    /// hovered node is not a filled socket). Replaces the node tooltip.
    pub hover_jewel: &'a [pob_egui::data::items::TooltipLine],
    /// Spec comparison diff (None when compare mode is off).
    pub compare: Option<&'a pob_egui::data::tree_specs::CompareDiff>,
    /// Jewel radius definitions for the active tree version.
    pub jewel_radii: &'a [pob_egui::data::jewels::RadiusDef],
    /// Jewel sockets with socketed-jewel info.
    pub jewel_sockets: &'a [pob_egui::data::jewels::SocketInfo],
    /// Node power heatmap tints per node id (None = heatmap off). Applied to
    /// unallocated nodes.
    pub heatmap: Option<&'a std::collections::HashMap<u32, egui::Color32>>,
}

/// Border color for passive tooltips (upstream default: rgb(128, 77, 0)).
const TOOLTIP_BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(128, 77, 0);
/// Node name color in tooltips: slightly dimmer than the allocated-path
/// yellow (`Palette::CONNECTION_ALLOCATED`, rgb(200, 170, 50)).
const TOOLTIP_NAME_COLOR: egui::Color32 = egui::Color32::from_rgb(178, 150, 44);
const TOOLTIP_BORDER_WIDTH: f32 = 3.0;
const TOOLTIP_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 0, 0, 217);

/// A 3-part header image (left cap, tiled middle, right cap) for tooltip decoration.
struct HeaderStrip {
    left: egui::TextureHandle,
    middle: egui::TextureHandle,
    right: egui::TextureHandle,
    height: f32,
    side_width: f32,
    middle_width: f32,
}

/// Loaded tooltip header textures for each passive node type.
pub struct TooltipHeaders {
    passive: Option<HeaderStrip>,
    notable: Option<HeaderStrip>,
    keystone: Option<HeaderStrip>,
    jewel: Option<HeaderStrip>,
    ascendancy: Option<HeaderStrip>,
    mastery: Option<HeaderStrip>,
    mastery_alloc: Option<HeaderStrip>,
    /// Oil icon textures keyed by full name (e.g. "GoldenOil").
    oil_icons: HashMap<String, egui::TextureHandle>,
}

impl TooltipHeaders {
    /// Load tooltip header images from the upstream Assets directory
    /// and oil icons from the TreeData directory.
    pub fn load(ctx: &egui::Context, assets_dir: &Path, tree_data_dir: Option<&Path>) -> Self {
        let load_png = |path: &Path, name: &str| -> Option<egui::TextureHandle> {
            let img = image::open(path)
                .map_err(|e| log::warn!("Failed to load {}: {e}", path.display()))
                .ok()?;
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
        };

        let load_header = |prefix: &str, label: &str| -> Option<HeaderStrip> {
            let left = load_png(
                &assets_dir.join(format!("{prefix}left.png")),
                &format!("tooltip_{label}_left"),
            )?;
            let middle = load_png(
                &assets_dir.join(format!("{prefix}middle.png")),
                &format!("tooltip_{label}_middle"),
            )?;
            let right = load_png(
                &assets_dir.join(format!("{prefix}right.png")),
                &format!("tooltip_{label}_right"),
            )?;

            Some(HeaderStrip {
                height: left.size()[1] as f32,
                side_width: left.size()[0] as f32,
                middle_width: middle.size()[0] as f32,
                left,
                middle,
                right,
            })
        };

        // Load oil icons from TreeData/
        let oil_names = [
            "AmberOil",
            "AzureOil",
            "BlackOil",
            "ClearOil",
            "CrimsonOil",
            "GoldenOil",
            "IndigoOil",
            "OpalescentOil",
            "PrismaticOil",
            "SepiaOil",
            "SilverOil",
            "TealOil",
            "VerdantOil",
            "VioletOil",
        ];
        let mut oil_icons = HashMap::new();
        if let Some(td) = tree_data_dir {
            // Oil icons live in the TreeData root, not the versioned subdirectory
            let oil_dir = td.parent().unwrap_or(td);
            for name in &oil_names {
                if let Some(tex) =
                    load_png(&oil_dir.join(format!("{name}.png")), &format!("oil_{name}"))
                {
                    oil_icons.insert((*name).to_string(), tex);
                }
            }
        }

        Self {
            passive: load_header("normalpassiveheader", "passive"),
            notable: load_header("notablepassiveheader", "notable"),
            keystone: load_header("keystonepassiveheader", "keystone"),
            jewel: load_header("jewelpassiveheader", "jewel"),
            ascendancy: load_header("ascendancypassiveheader", "ascendancy"),
            mastery: load_header("masteryheaderunallocated", "mastery"),
            mastery_alloc: load_header("masteryheaderallocated", "mastery_alloc"),
            oil_icons,
        }
    }

    fn get(&self, node: &TreeNode) -> Option<&HeaderStrip> {
        match node.node_type {
            NodeType::Normal => {
                if node.ascendancy_name.is_some() {
                    self.ascendancy.as_ref()
                } else {
                    self.passive.as_ref()
                }
            }
            NodeType::Notable => self.notable.as_ref(),
            NodeType::Keystone => self.keystone.as_ref(),
            NodeType::Socket => self.jewel.as_ref(),
            NodeType::Mastery => {
                if node.is_allocated {
                    self.mastery_alloc.as_ref()
                } else {
                    self.mastery.as_ref()
                }
            }
            NodeType::ClassStart | NodeType::AscendClassStart => self.passive.as_ref(),
        }
    }
}

/// Camera state for pan/zoom.
pub struct TreeCamera {
    pub center_x: f32,
    pub center_y: f32,
    pub zoom: f32,
}

impl TreeCamera {
    pub fn new(tree: &TreeData) -> Self {
        let (cx, cy) = tree.bounds.center();
        let size = tree.bounds.size();
        Self {
            center_x: cx,
            center_y: cy,
            zoom: 0.03_f32.max(1.0 / (size / 800.0)),
        }
    }

    fn tree_to_screen(&self, tree_x: f32, tree_y: f32, rect: &egui::Rect) -> egui::Pos2 {
        let screen_cx = rect.center().x;
        let screen_cy = rect.center().y;
        egui::pos2(
            screen_cx + (tree_x - self.center_x) * self.zoom,
            screen_cy + (tree_y - self.center_y) * self.zoom,
        )
    }

    fn screen_to_tree(&self, screen_pos: egui::Pos2, rect: &egui::Rect) -> (f32, f32) {
        let screen_cx = rect.center().x;
        let screen_cy = rect.center().y;
        (
            (screen_pos.x - screen_cx) / self.zoom + self.center_x,
            (screen_pos.y - screen_cy) / self.zoom + self.center_y,
        )
    }
}

/// Draw the passive tree and handle pan/zoom/hover interactions.
pub fn draw_tree(
    ui: &mut egui::Ui,
    tree: &TreeData,
    camera: &mut TreeCamera,
    atlas: Option<&TreeSpriteAtlas>,
    tooltip_headers: Option<&TooltipHeaders>,
    overlays: &TreeOverlays,
) -> TreeViewResponse {
    ui.ctx().style_mut(|s| {
        s.interaction.tooltip_delay = 0.05;
        s.interaction.tooltip_grace_time = 0.05;
    });
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let rect = response.rect;

    // Handle pan (drag)
    if response.dragged() {
        let delta = response.drag_delta();
        camera.center_x -= delta.x / camera.zoom;
        camera.center_y -= delta.y / camera.zoom;
    }

    // Handle zoom (scroll) - only while the pointer is over the tree itself
    // (not the sidebar or a window on top of it)
    if response.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            let zoom_factor = 1.0 + scroll_delta * 0.002;
            let old_zoom = camera.zoom;
            camera.zoom = (camera.zoom * zoom_factor).clamp(0.01, 2.0);

            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos())
                && rect.contains(mouse_pos)
            {
                let (tree_x, tree_y) = camera.screen_to_tree(mouse_pos, &rect);
                camera.center_x += (tree_x - camera.center_x) * (1.0 - old_zoom / camera.zoom);
                camera.center_y += (tree_y - camera.center_y) * (1.0 - old_zoom / camera.zoom);
            }
        }
    }

    // Visible area for culling
    let visible_margin = 50.0;
    let visible_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x - visible_margin, rect.min.y - visible_margin),
        egui::pos2(rect.max.x + visible_margin, rect.max.y + visible_margin),
    );

    // Fill background to match upstream PoB's dark blue-gray tree background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(8, 12, 17));

    // Draw backgrounds (behind everything else)
    if let Some(atlas) = atlas {
        draw_class_start_background(&painter, tree, camera, &rect, &visible_rect, atlas);
        draw_group_backgrounds(&painter, tree, camera, &rect, &visible_rect, atlas);
    }

    // Find hovered node (before connections so they can react to it)
    let mouse_pos = ui.input(|i| i.pointer.hover_pos());
    let mut hovered_node: Option<&TreeNode> = None;
    let mut hovered_dist_sq = f32::MAX;

    if let Some(mouse) = mouse_pos
        && rect.contains(mouse)
    {
        for node in tree.nodes.values() {
            let screen_pos = camera.tree_to_screen(node.x, node.y, &rect);
            let dx = mouse.x - screen_pos.x;
            let dy = mouse.y - screen_pos.y;
            let dist_sq = dx * dx + dy * dy;
            let hit_radius = node.node_type.radius() * camera.zoom + 4.0;
            if dist_sq < hit_radius * hit_radius && dist_sq < hovered_dist_sq {
                hovered_node = Some(node);
                hovered_dist_sq = dist_sq;
            }
        }
    }

    // Only apply path/depends overlays if they belong to the node actually
    // being hovered (the caller's cache can lag a frame behind).
    let hovered_id = hovered_node.map(|n| n.id);
    let (hover_path, hover_depends): (&HashSet<u32>, &HashSet<u32>) =
        if overlays.hover_node.is_some() && overlays.hover_node == hovered_id {
            (overlays.hover_path, overlays.hover_depends)
        } else {
            static EMPTY: std::sync::LazyLock<HashSet<u32>> =
                std::sync::LazyLock::new(HashSet::new);
            (&EMPTY, &EMPTY)
        };

    // Draw connections
    for conn in &tree.connections {
        let (Some(from_node), Some(to_node)) =
            (tree.nodes.get(&conn.from_id), tree.nodes.get(&conn.to_id))
        else {
            continue;
        };

        let from_screen = camera.tree_to_screen(from_node.x, from_node.y, &rect);
        let to_screen = camera.tree_to_screen(to_node.x, to_node.y, &rect);

        if !line_intersects_rect(from_screen, to_screen, &visible_rect) {
            continue;
        }

        // Connector state, mirroring upstream: dependent (red) wins, then
        // active (both allocated), then hover-path preview, then normal.
        let both_allocated = from_node.is_allocated && to_node.is_allocated;
        let in_path =
            |n: &TreeNode| n.is_allocated || Some(n.id) == hovered_id || hover_path.contains(&n.id);
        let (color, width): (egui::Color32, f32) =
            if hover_depends.contains(&from_node.id) && hover_depends.contains(&to_node.id) {
                (Palette::DEPENDENT, 2.0)
            } else if both_allocated {
                (Palette::CONNECTION_ALLOCATED, 2.0)
            } else if !hover_path.is_empty() && in_path(from_node) && in_path(to_node) {
                (Palette::PATH_PREVIEW, 2.0)
            } else {
                (Palette::CONNECTION, 1.0)
            };
        let stroke = egui::Stroke::new(width, color);

        if let Some(arc) = &conn.arc {
            draw_arc(&painter, arc, from_node, to_node, camera, &rect, stroke);
        } else {
            painter.line_segment([from_screen, to_screen], stroke);
        }
    }

    // Draw nodes
    for node in tree.nodes.values() {
        let screen_pos = camera.tree_to_screen(node.x, node.y, &rect);

        if !visible_rect.contains(screen_pos) {
            continue;
        }

        let radius = (node.node_type.radius() * camera.zoom).max(1.5);
        let is_hovered = hovered_node.is_some_and(|h| h.id == node.id);
        let is_dependent = hover_depends.contains(&node.id) && Some(node.id) != hovered_id;

        // Node power heatmap tint (unallocated nodes only, like upstream)
        let heat_tint = if node.is_allocated {
            None
        } else {
            overlays.heatmap.and_then(|hm| hm.get(&node.id)).copied()
        };

        // Dependent nodes get a red tint: they would disconnect if the
        // hovered node were deallocated.
        let tint = if is_dependent {
            Palette::DEPENDENT_TINT
        } else if let Some(heat) = heat_tint {
            heat
        } else {
            egui::Color32::WHITE
        };

        // Socketed jewels change the socket's art (crimson/viridian/cluster...)
        let socket_art: Option<&str> = if node.node_type == NodeType::Socket {
            overlays
                .jewel_sockets
                .iter()
                .find(|s| s.node_id == node.id)
                .and_then(|s| s.active_art.as_deref())
        } else {
            None
        };

        let drew_sprite = if let Some(atlas) = atlas {
            draw_node_sprite(&painter, node, screen_pos, radius, atlas, tint, socket_art)
        } else {
            false
        };

        if !drew_sprite {
            // Fallback: colored circle
            let color = if is_dependent {
                Palette::DEPENDENT
            } else if let Some(heat) = heat_tint {
                heat
            } else if node.is_allocated {
                Palette::ALLOCATED
            } else if node.ascendancy_name.is_some() {
                Palette::ASCENDANCY
            } else {
                node_type_color(node.node_type)
            };
            painter.circle_filled(screen_pos, radius, color);
        }

        // Path preview ring: unallocated nodes on the shortest path to the
        // hovered node (stand-in for upstream's highlighted frame).
        if !node.is_allocated && Some(node.id) != hovered_id && hover_path.contains(&node.id) {
            painter.circle_stroke(
                screen_pos,
                radius + 2.0,
                egui::Stroke::new(2.0_f32, Palette::PATH_PREVIEW),
            );
        }

        // Hover outline
        if is_hovered {
            painter.circle_stroke(
                screen_pos,
                radius + 2.0,
                egui::Stroke::new(2.0_f32, Palette::HOVER_OUTLINE),
            );
        }

        // Search match highlight ring. The zoom^0.4 divisor (from upstream)
        // keeps rings visible when zoomed out.
        if overlays.search_matches.contains(&node.id) {
            let ring_radius = (43.75 * camera.zoom / camera.zoom.powf(0.4)).max(radius + 4.0);
            painter.circle_stroke(
                screen_pos,
                ring_radius,
                egui::Stroke::new(2.5_f32, Palette::SEARCH_HIGHLIGHT),
            );
        }

        // Spec comparison rings: green = would allocate (in compare spec
        // only), red = would deallocate (in current only), blue = mastery
        // with a different effect selected.
        if let Some(diff) = overlays.compare {
            let ring_color = if diff.mastery_diff.contains(&node.id) {
                Some(Palette::COMPARE_MASTERY)
            } else if diff.to_allocate.contains(&node.id) {
                Some(Palette::COMPARE_ADD)
            } else if diff.to_deallocate.contains(&node.id) {
                Some(Palette::COMPARE_REMOVE)
            } else {
                None
            };
            if let Some(color) = ring_color {
                painter.circle_stroke(screen_pos, radius + 3.0, egui::Stroke::new(3.0_f32, color));
            }
        }
    }

    // Jewel radius rings, mirroring upstream's ring overlay pass:
    // - hovering any socket previews the standard radii (or the annuli for
    //   Thread of Hope-like jewels)
    // - allocated sockets with a radius jewel show that jewel's radius;
    //   Impossible Escape draws on its keystones instead
    for socket in overlays.jewel_sockets {
        let Some(node) = tree.nodes.get(&socket.node_id) else {
            continue;
        };
        let screen_pos = camera.tree_to_screen(node.x, node.y, &rect);
        let is_hovered = hovered_node.is_some_and(|n| n.id == socket.node_id);
        if is_hovered {
            for def in overlays.jewel_radii {
                let is_annulus = def.inner > 0.0;
                if is_annulus != socket.is_variable {
                    continue;
                }
                let stroke = egui::Stroke::new(2.0_f32, def.color);
                painter.circle_stroke(screen_pos, def.outer * camera.zoom, stroke);
                if is_annulus {
                    painter.circle_stroke(screen_pos, def.inner * camera.zoom, stroke);
                }
            }
        } else if socket.allocated
            && let Some(def) = socket
                .radius_index
                .and_then(|i| overlays.jewel_radii.get(i))
        {
            let stroke = egui::Stroke::new(
                2.0_f32,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180),
            );
            if socket.keystone_positions.is_empty() {
                painter.circle_stroke(screen_pos, def.outer * camera.zoom, stroke);
                if def.inner > 0.0 {
                    painter.circle_stroke(screen_pos, def.inner * camera.zoom, stroke);
                }
            } else {
                // Impossible Escape: ring around each affected keystone
                for &(kx, ky) in &socket.keystone_positions {
                    let key_pos = camera.tree_to_screen(kx, ky, &rect);
                    painter.circle_stroke(key_pos, def.outer * camera.zoom, stroke);
                }
            }
        }
    }

    // Tooltip - temporarily override popup frame to be transparent
    if let Some(node) = hovered_node {
        let saved = ui.ctx().style().visuals.clone();
        ui.ctx().style_mut(|s| {
            s.visuals.window_fill = egui::Color32::TRANSPARENT;
            s.visuals.window_stroke = egui::Stroke::NONE;
            s.visuals.window_shadow = egui::epaint::Shadow::NONE;
            s.visuals.popup_shadow = egui::epaint::Shadow::NONE;
            s.visuals.window_corner_radius = egui::CornerRadius::ZERO;
        });
        // Diff/jewel lines belong to the node the caller's hover cache is
        // for; it can lag a frame behind the node hovered right now
        let (diff, jewel): (&[String], &[pob_egui::data::items::TooltipLine]) =
            if overlays.hover_node == Some(node.id) {
                (overlays.hover_diff, overlays.hover_jewel)
            } else {
                (&[], &[])
            };
        response.clone().on_hover_ui_at_pointer(|ui| {
            if !jewel.is_empty() {
                // A socket with a jewel shows the jewel's item tooltip
                show_jewel_tooltip(ui, jewel);
            } else {
                show_node_tooltip(ui, node, tooltip_headers, diff);
            }
        });
        ui.ctx().style_mut(|s| s.visuals = saved);
    }

    // Handle click (left or right)
    let clicked = hovered_node.and_then(|node| {
        if response.clicked() {
            Some(NodeClick {
                node_id: node.id,
                is_right: false,
            })
        } else if response.secondary_clicked() {
            Some(NodeClick {
                node_id: node.id,
                is_right: true,
            })
        } else {
            None
        }
    });

    TreeViewResponse {
        clicked,
        hovered: hovered_id,
    }
}

/// Try to draw a node using sprite textures. Returns true if successful.
fn draw_node_sprite(
    painter: &egui::Painter,
    node: &TreeNode,
    screen_pos: egui::Pos2,
    radius: f32,
    atlas: &TreeSpriteAtlas,
    tint: egui::Color32,
    socket_art: Option<&str>,
) -> bool {
    // Jewel sockets: base frame first, then the filled-socket art on top
    // (upstream draws the active-jewel art as an overlay; the frame graphic
    // has an opaque center and would hide it the other way around)
    if node.node_type == NodeType::Socket {
        let mut drew = false;
        let half = radius * 1.2;
        let rect = egui::Rect::from_center_size(screen_pos, egui::vec2(half * 2.0, half * 2.0));
        if let Some(frame) = get_frame_region(&atlas.frames, node)
            && let Some(frame_tex) = atlas.texture_id(frame.sheet_index)
        {
            let frame_uv = egui::Rect::from_min_max(
                egui::pos2(frame.u_min, frame.v_min),
                egui::pos2(frame.u_max, frame.v_max),
            );
            painter.image(frame_tex, rect, frame_uv, tint);
            drew = true;
        }
        if node.is_allocated
            && let Some(region) = socket_art.and_then(|name| atlas.jewel_art.get(name))
            && let Some(tex) = atlas.texture_id(region.sheet_index)
        {
            let uv = egui::Rect::from_min_max(
                egui::pos2(region.u_min, region.v_min),
                egui::pos2(region.u_max, region.v_max),
            );
            painter.image(tex, rect, uv, tint);
            drew = true;
        }
        return drew;
    }

    // ClassStart nodes use dedicated art instead of normal icon+frame
    if node.node_type == NodeType::ClassStart {
        let art_name = if node.is_allocated {
            node.start_art.as_deref()
        } else {
            Some("PSStartNodeBackgroundInactive")
        };
        if let Some(name) = art_name
            && let Some(bg) = atlas.class_start_art.get(name)
            && let Some(tex) = atlas.texture_id(bg.sheet_index)
        {
            let w = bg.width * 1.33 * radius / node.node_type.radius();
            let h = bg.height * 1.33 * radius / node.node_type.radius();
            let img_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(w * 2.0, h * 2.0));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex, img_rect, uv, tint);
            return true;
        }
        return false;
    }

    // Look up the icon sprite
    // For masteries, use inactiveIcon/activeIcon paths instead of the generic icon
    let icon_region = if node.node_type == NodeType::Mastery {
        // Masteries have dedicated icons per state, each on different spritesheets
        if node.is_allocated {
            // Try activeIcon path with masteryActiveSelected sprite
            node.active_icon
                .as_deref()
                .and_then(|icon| atlas.node_sprites.get(icon))
                .and_then(|ns| ns.mastery_active.as_ref())
                // Fall back to generic icon's mastery sprite
                .or_else(|| {
                    atlas
                        .node_sprites
                        .get(&node.icon)
                        .and_then(|ns| ns.mastery.as_ref())
                })
        } else {
            // Try inactiveIcon path with masteryInactive sprite
            node.inactive_icon
                .as_deref()
                .and_then(|icon| atlas.node_sprites.get(icon))
                .and_then(|ns| {
                    ns.mastery_inactive
                        .as_ref()
                        .or(ns.mastery_connected.as_ref())
                })
                // Fall back to generic icon's mastery sprite
                .or_else(|| {
                    atlas
                        .node_sprites
                        .get(&node.icon)
                        .and_then(|ns| ns.mastery.as_ref())
                })
        }
    } else {
        let node_sprites = atlas.node_sprites.get(&node.icon);
        node_sprites.and_then(|ns| {
            if node.is_allocated {
                match node.node_type {
                    NodeType::Normal => ns.normal_active.as_ref(),
                    NodeType::Notable => ns.notable_active.as_ref(),
                    NodeType::Keystone => ns.keystone_active.as_ref(),
                    _ => ns.normal_active.as_ref(),
                }
            } else {
                match node.node_type {
                    NodeType::Normal => ns.normal_inactive.as_ref(),
                    NodeType::Notable => ns.notable_inactive.as_ref(),
                    NodeType::Keystone => ns.keystone_inactive.as_ref(),
                    _ => ns.normal_inactive.as_ref(),
                }
            }
        })
    };

    let Some(region) = icon_region else {
        return false;
    };

    let Some(texture_id) = atlas.texture_id(region.sheet_index) else {
        return false;
    };

    let half = radius;

    // Draw mastery active effect behind the icon (decorative background pattern)
    if node.is_allocated
        && node.node_type == NodeType::Mastery
        && let Some(effect_region) = node
            .active_effect_image
            .as_deref()
            .and_then(|img| atlas.node_sprites.get(img))
            .and_then(|ns| ns.mastery_effect.as_ref())
        && let Some(effect_tex) = atlas.texture_id(effect_region.sheet_index)
    {
        // Effect is larger than the icon — scale relative to the icon
        let effect_scale = 3.5;
        let effect_half = half * effect_scale;
        let effect_rect = egui::Rect::from_center_size(
            screen_pos,
            egui::vec2(effect_half * 2.0, effect_half * 2.0),
        );
        let effect_uv = egui::Rect::from_min_max(
            egui::pos2(effect_region.u_min, effect_region.v_min),
            egui::pos2(effect_region.u_max, effect_region.v_max),
        );
        painter.image(effect_tex, effect_rect, effect_uv, tint);
    }

    // Draw the icon sprite — scale down slightly so square JPEG corners
    // are hidden behind the circular frame overlay
    let icon_half = half * 0.85;
    let icon_rect =
        egui::Rect::from_center_size(screen_pos, egui::vec2(icon_half * 2.0, icon_half * 2.0));
    let uv = egui::Rect::from_min_max(
        egui::pos2(region.u_min, region.v_min),
        egui::pos2(region.u_max, region.v_max),
    );
    painter.image(texture_id, icon_rect, uv, tint);

    // Draw frame overlay
    let frame_region = get_frame_region(&atlas.frames, node);
    if let Some(frame) = frame_region
        && let Some(frame_tex) = atlas.texture_id(frame.sheet_index)
    {
        // Frame is slightly larger than the icon; masteries get a larger ring
        let frame_scale = if node.node_type == NodeType::Mastery {
            1.5
        } else {
            1.3
        };
        let frame_half = half * frame_scale;
        let frame_rect = egui::Rect::from_center_size(
            screen_pos,
            egui::vec2(frame_half * 2.0, frame_half * 2.0),
        );
        let frame_uv = egui::Rect::from_min_max(
            egui::pos2(frame.u_min, frame.v_min),
            egui::pos2(frame.u_max, frame.v_max),
        );
        painter.image(frame_tex, frame_rect, frame_uv, tint);
    }

    true
}

fn get_frame_region<'a>(
    frames: &'a pob_egui::data::tree_sprites::FrameSprites,
    node: &TreeNode,
) -> Option<&'a SpriteRegion> {
    match node.node_type {
        NodeType::Normal | NodeType::ClassStart | NodeType::AscendClassStart => {
            if node.is_allocated {
                frames.normal_allocated.as_ref()
            } else {
                frames.normal_unallocated.as_ref()
            }
        }
        NodeType::Notable => {
            if node.is_allocated {
                frames.notable_allocated.as_ref()
            } else {
                frames.notable_unallocated.as_ref()
            }
        }
        NodeType::Keystone => {
            if node.is_allocated {
                frames.keystone_allocated.as_ref()
            } else {
                frames.keystone_unallocated.as_ref()
            }
        }
        NodeType::Socket => {
            if node.is_allocated {
                frames.jewel_allocated.as_ref()
            } else {
                frames.jewel_unallocated.as_ref()
            }
        }
        NodeType::Mastery => {
            if node.is_allocated {
                frames.mastery_allocated.as_ref()
            } else {
                frames.mastery_unallocated.as_ref()
            }
        }
    }
}

fn node_type_color(node_type: NodeType) -> egui::Color32 {
    match node_type {
        NodeType::Normal => Palette::NORMAL,
        NodeType::Notable => Palette::NOTABLE,
        NodeType::Keystone => Palette::KEYSTONE,
        NodeType::Socket => Palette::SOCKET,
        NodeType::Mastery => Palette::MASTERY,
        NodeType::ClassStart | NodeType::AscendClassStart => Palette::CLASS_START,
    }
}

/// Item tooltip for the jewel socketed in a hovered tree socket, framed like
/// the node tooltips.
fn show_jewel_tooltip(ui: &mut egui::Ui, lines: &[pob_egui::data::items::TooltipLine]) {
    let frame = egui::Frame::NONE
        .fill(TOOLTIP_BG)
        .stroke(egui::Stroke::new(
            TOOLTIP_BORDER_WIDTH,
            TOOLTIP_BORDER_COLOR,
        ))
        .inner_margin(egui::Margin::same(6));
    frame.show(ui, |ui| {
        ui.set_max_width(340.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        for line in lines {
            if line.is_separator {
                ui.separator();
            } else if line.text.is_empty() {
                ui.add_space(line.size * 0.4);
            } else {
                let size = (line.size * 0.75).clamp(9.0, 16.0);
                ui.label(super::theme::pob_layout_job(
                    &line.text,
                    size,
                    egui::Color32::WHITE,
                ));
            }
        }
    });
}

/// Rich tooltip for a passive tree node, styled to match upstream PoB.
fn show_node_tooltip(
    ui: &mut egui::Ui,
    node: &TreeNode,
    headers: Option<&TooltipHeaders>,
    diff: &[String],
) {
    let frame = egui::Frame::NONE
        .fill(TOOLTIP_BG)
        .stroke(egui::Stroke::new(
            TOOLTIP_BORDER_WIDTH,
            TOOLTIP_BORDER_COLOR,
        ))
        .inner_margin(egui::Margin::same(6));

    frame.show(ui, |ui| {
        // Oil (anoint) vials + names are drawn right of the node name in the
        // header
        const OIL_ICON_SIZE: f32 = 15.0;
        const OIL_ICON_GAP: f32 = 2.0;
        const OIL_SPACING: f32 = 6.0;
        let oil_color = egui::Color32::from_rgb(248, 230, 202);
        let oil_items: Vec<(&egui::TextureHandle, std::sync::Arc<egui::Galley>)> = node
            .recipe
            .iter()
            .filter_map(|oil| {
                headers
                    .and_then(|h| h.oil_icons.get(oil.as_str()))
                    .map(|tex| {
                        let short = oil.strip_suffix("Oil").unwrap_or(oil).trim();
                        let galley = ui.fonts(|f| {
                            f.layout_no_wrap(
                                short.to_string(),
                                egui::FontId::proportional(11.0),
                                oil_color,
                            )
                        });
                        (tex, galley)
                    })
            })
            .collect();
        let oils_w: f32 = if oil_items.is_empty() {
            0.0
        } else {
            oil_items
                .iter()
                .map(|(_, g)| OIL_ICON_SIZE + OIL_ICON_GAP + g.size().x)
                .sum::<f32>()
                + (oil_items.len() - 1) as f32 * OIL_SPACING
        };

        // Widen the tooltip when the centered name + oils to its right need
        // more room (the name stays truly centered, so the oil block's width
        // must fit on both sides)
        let name_w = ui.fonts(|f| {
            f.layout_no_wrap(
                node.name.clone(),
                egui::FontId::proportional(17.0),
                egui::Color32::WHITE,
            )
            .size()
            .x
        });
        // Horizontal breathing room on each side of the centered title group
        const TITLE_SIDE_PAD: f32 = 40.0;
        let needed = if oil_items.is_empty() {
            name_w + 2.0 * TITLE_SIDE_PAD
        } else {
            name_w + 8.0 + oils_w + 2.0 * TITLE_SIDE_PAD
        };
        ui.set_max_width(300.0_f32.max(needed));

        // Draw the header image strip behind the title text
        let header_strip = headers.and_then(|h| h.get(node));
        if let Some(strip) = header_strip {
            // Scale the header down so it doesn't dominate the tooltip
            let scale = 0.55;
            let h = strip.height * scale;
            let side_w = strip.side_width * scale;
            let mid_w = strip.middle_width * scale;

            let available_w = ui.available_width();
            let (header_rect, _) =
                ui.allocate_exact_size(egui::vec2(available_w, h), egui::Sense::hover());

            let painter = ui.painter();
            let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

            // Left cap
            let left_rect = egui::Rect::from_min_size(header_rect.min, egui::vec2(side_w, h));
            painter.image(strip.left.id(), left_rect, full_uv, egui::Color32::WHITE);

            // Tiled middle
            let middle_start = header_rect.min.x + side_w;
            let middle_end = header_rect.max.x - side_w;
            let mut x = middle_start;
            while x < middle_end {
                let w = (middle_end - x).min(mid_w);
                let u_max = w / mid_w;
                let tile_rect =
                    egui::Rect::from_min_size(egui::pos2(x, header_rect.min.y), egui::vec2(w, h));
                painter.image(
                    strip.middle.id(),
                    tile_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(u_max, 1.0)),
                    egui::Color32::WHITE,
                );
                x += mid_w;
            }

            // Right cap
            let right_rect = egui::Rect::from_min_size(
                egui::pos2(header_rect.max.x - side_w, header_rect.min.y),
                egui::vec2(side_w, h),
            );
            painter.image(strip.right.id(), right_rect, full_uv, egui::Color32::WHITE);

            // Draw the node name + oil vials centered as one group over the
            // header. Paint the name twice with 1px offset to simulate bold
            let name_galley = ui.painter().layout_no_wrap(
                node.name.clone(),
                egui::FontId::proportional(17.0),
                TOOLTIP_NAME_COLOR,
            );
            let center = header_rect.center();
            let group_w = if oil_items.is_empty() {
                name_galley.size().x
            } else {
                name_galley.size().x + 8.0 + oils_w
            };
            // Optical centering: box-centered text reads ~8% of the font size
            // too low because the (mostly empty) descender zone counts toward
            // the box height; raise it by that much
            let optical_raise = 17.0 * 0.08;
            let text_pos = egui::pos2(
                center.x - group_w / 2.0,
                center.y - name_galley.size().y / 2.0 - optical_raise,
            );
            painter.galley(text_pos, name_galley.clone(), TOOLTIP_NAME_COLOR);
            painter.galley(
                text_pos + egui::vec2(1.0, 0.0),
                name_galley.clone(),
                TOOLTIP_NAME_COLOR,
            );
            let mut x = text_pos.x + name_galley.size().x + 8.0;
            for (tex, galley) in &oil_items {
                let icon_rect = egui::Rect::from_min_size(
                    egui::pos2(x, center.y - OIL_ICON_SIZE / 2.0),
                    egui::vec2(OIL_ICON_SIZE, OIL_ICON_SIZE),
                );
                painter.image(tex.id(), icon_rect, full_uv, egui::Color32::WHITE);
                x += OIL_ICON_SIZE + OIL_ICON_GAP;
                painter.galley(
                    egui::pos2(x, center.y - galley.size().y / 2.0 - 11.0 * 0.08),
                    galley.clone(),
                    oil_color,
                );
                x += galley.size().x + OIL_SPACING;
            }
        } else {
            // Fallback: plain text header
            let type_label = match node.node_type {
                NodeType::Notable => "Notable",
                NodeType::Keystone => "Keystone",
                NodeType::Mastery => "Mastery",
                NodeType::Socket => "Jewel Socket",
                NodeType::ClassStart | NodeType::AscendClassStart => "Class Start",
                NodeType::Normal => {
                    if node.ascendancy_name.is_some() {
                        "Ascendancy"
                    } else {
                        "Passive"
                    }
                }
            };
            let type_color = match node.node_type {
                NodeType::Notable => egui::Color32::from_rgb(210, 180, 100),
                NodeType::Keystone => egui::Color32::from_rgb(220, 160, 60),
                NodeType::Mastery => egui::Color32::from_rgb(180, 140, 200),
                NodeType::Socket => egui::Color32::from_rgb(120, 200, 120),
                _ => egui::Color32::from_rgb(170, 170, 170),
            };
            ui.label(egui::RichText::new(type_label).small().color(type_color));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = OIL_ICON_GAP;
                ui.label(
                    egui::RichText::new(&node.name)
                        .strong()
                        .size(17.0)
                        .color(TOOLTIP_NAME_COLOR),
                );
                for (tex, galley) in &oil_items {
                    ui.image(egui::load::SizedTexture::new(
                        tex.id(),
                        egui::vec2(OIL_ICON_SIZE, OIL_ICON_SIZE),
                    ));
                    ui.label(
                        egui::RichText::new(galley.text())
                            .size(11.0)
                            .color(oil_color),
                    );
                }
            });
        }

        // Oil recipe text fallback: only when the icon textures are missing
        if !node.recipe.is_empty() && oil_items.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, oil_name) in node.recipe.iter().enumerate() {
                    if i > 0 {
                        ui.label(egui::RichText::new("+").size(11.0).color(oil_color));
                    }
                    let short = oil_name.strip_suffix("Oil").unwrap_or(oil_name);
                    ui.label(egui::RichText::new(short).size(11.0).color(oil_color));
                }
            });
        }

        // Stats
        if !node.stats.is_empty() {
            ui.separator();
            for stat in &node.stats {
                ui.label(
                    egui::RichText::new(stat)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(136, 136, 255)),
                );
            }
        }

        // Reminder text
        if !node.reminder_text.is_empty() {
            ui.separator();
            for line in &node.reminder_text {
                ui.label(
                    egui::RichText::new(line)
                        .size(11.0)
                        .italics()
                        .color(egui::Color32::from_rgb(160, 160, 128)),
                );
            }
        }

        // Flavour text
        if !node.flavour_text.is_empty() {
            ui.separator();
            for line in &node.flavour_text {
                ui.label(
                    egui::RichText::new(line)
                        .size(12.0)
                        .italics()
                        .color(egui::Color32::from_rgb(175, 96, 37)),
                );
            }
        }

        // Stat difference preview ("Allocating this node will give you: ...")
        if !diff.is_empty() {
            ui.separator();
            for line in diff {
                ui.label(super::theme::pob_layout_job(
                    line,
                    10.0,
                    egui::Color32::WHITE,
                ));
            }
            ui.label(
                egui::RichText::new("Tip: Ctrl+D toggles stat differences")
                    .size(9.0)
                    .color(egui::Color32::from_rgb(120, 120, 120)),
            );
        }

        // Allocation status
        if node.is_allocated {
            ui.separator();
            ui.label(
                egui::RichText::new("Allocated")
                    .small()
                    .color(Palette::ALLOCATED),
            );
        }
    });
}

/// Draw the class start background art for the current class.
/// Positions are hardcoded to match upstream PoB (the tree data doesn't include them).
fn draw_class_start_background(
    painter: &egui::Painter,
    tree: &TreeData,
    camera: &TreeCamera,
    viewport: &egui::Rect,
    visible_rect: &egui::Rect,
    atlas: &TreeSpriteAtlas,
) {
    // class_id -> (asset suffix, tree x, tree y)
    let (suffix, tx, ty) = match tree.class_id {
        1 => ("Str", -2750.0_f32, 1600.0_f32), // Marauder
        2 => ("Dex", 2550.0, 1600.0),          // Ranger
        3 => ("Int", -250.0, -2200.0),         // Witch
        4 => ("StrDex", -150.0, 2350.0),       // Duelist
        5 => ("StrInt", -2100.0, -1500.0),     // Templar
        6 => ("DexInt", 2350.0, -1950.0),      // Shadow
        _ => return,                           // Scion (0) or unknown — no background
    };

    let Some(bg) = atlas.class_backgrounds.get(suffix) else {
        return;
    };
    let screen_pos = camera.tree_to_screen(tx, ty, viewport);
    let w = bg.width * 1.33 * camera.zoom;
    let h = bg.height * 1.33 * camera.zoom;
    let img_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(w * 2.0, h * 2.0));
    if !img_rect.intersects(*visible_rect) {
        return;
    }
    let Some(tex) = atlas.texture_id(bg.sheet_index) else {
        return;
    };
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(tex, img_rect, uv, egui::Color32::WHITE);
}

/// Draw group backgrounds behind all nodes.
fn draw_group_backgrounds(
    painter: &egui::Painter,
    tree: &TreeData,
    camera: &TreeCamera,
    viewport: &egui::Rect,
    visible_rect: &egui::Rect,
    atlas: &TreeSpriteAtlas,
) {
    for group in &tree.groups {
        // Draw ascendancy class background art
        if group.is_ascendancy_start {
            if let Some(name) = &group.ascendancy_name {
                // Fall back to Ascendant art for regular ascendancies without their own image;
                // bloodlines have no background art.
                let bg = atlas.ascendancy_backgrounds.get(name.as_str()).or_else(|| {
                    if group.is_bloodline {
                        None
                    } else {
                        atlas.ascendancy_backgrounds.get("Ascendant")
                    }
                });
                if let Some(bg) = bg {
                    let screen_pos = camera.tree_to_screen(group.x, group.y, viewport);
                    let w = bg.width * 1.33 * camera.zoom;
                    let h = bg.height * 1.33 * camera.zoom;
                    let img_rect =
                        egui::Rect::from_center_size(screen_pos, egui::vec2(w * 2.0, h * 2.0));
                    if img_rect.intersects(*visible_rect)
                        && let Some(tex) = atlas.texture_id(bg.sheet_index)
                    {
                        let uv =
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        // Dim non-selected ascendancies (upstream uses alpha 0.25)
                        let is_selected = tree.ascendancy_name.as_deref() == Some(name.as_str());
                        let tint = if is_selected {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 64)
                        };
                        painter.image(tex, img_rect, uv, tint);
                    }
                }
            }
            continue;
        }

        // Skip all ascendancy groups from regular group background rendering
        if group.is_ascendancy {
            continue;
        }

        // Only draw backgrounds for groups that have them in the tree data
        let Some(bg_type) = group.background else {
            continue;
        };

        let screen_pos = camera.tree_to_screen(group.x, group.y, viewport);

        // Quick visibility cull
        let max_size = 400.0 * camera.zoom;
        let group_visible =
            egui::Rect::from_center_size(screen_pos, egui::vec2(max_size, max_size));
        if !group_visible.intersects(*visible_rect) {
            continue;
        }

        let (bg_region, is_half) = match bg_type {
            GroupBackground::Large => (atlas.frames.group_background_large.as_ref(), true),
            GroupBackground::Medium => (atlas.frames.group_background_medium.as_ref(), false),
            GroupBackground::Small => (atlas.frames.group_background_small.as_ref(), false),
        };

        let Some(bg_region) = bg_region else {
            continue;
        };
        let Some(bg_tex) = atlas.texture_id(bg_region.sheet_index) else {
            continue;
        };

        // Scale: sprite dimensions * 1.33 (same as upstream DrawAsset)
        let bg_w = bg_region.width * 1.33 * camera.zoom;
        let bg_h = bg_region.height * 1.33 * camera.zoom;

        if is_half {
            // Large background is a half-circle — draw it twice (normal + vertically flipped)
            // Top half
            let top_rect = egui::Rect::from_min_size(
                egui::pos2(screen_pos.x - bg_w, screen_pos.y - bg_h * 2.0),
                egui::vec2(bg_w * 2.0, bg_h * 2.0),
            );
            let top_uv = egui::Rect::from_min_max(
                egui::pos2(bg_region.u_min, bg_region.v_min),
                egui::pos2(bg_region.u_max, bg_region.v_max),
            );
            painter.image(bg_tex, top_rect, top_uv, egui::Color32::WHITE);

            // Bottom half (vertically flipped)
            let bottom_rect = egui::Rect::from_min_size(
                egui::pos2(screen_pos.x - bg_w, screen_pos.y),
                egui::vec2(bg_w * 2.0, bg_h * 2.0),
            );
            let bottom_uv = egui::Rect::from_min_max(
                egui::pos2(bg_region.u_min, bg_region.v_max),
                egui::pos2(bg_region.u_max, bg_region.v_min),
            );
            painter.image(bg_tex, bottom_rect, bottom_uv, egui::Color32::WHITE);
        } else {
            let bg_rect =
                egui::Rect::from_center_size(screen_pos, egui::vec2(bg_w * 2.0, bg_h * 2.0));
            let bg_uv = egui::Rect::from_min_max(
                egui::pos2(bg_region.u_min, bg_region.v_min),
                egui::pos2(bg_region.u_max, bg_region.v_max),
            );
            painter.image(bg_tex, bg_rect, bg_uv, egui::Color32::WHITE);
        }
    }
}

fn draw_arc(
    painter: &egui::Painter,
    arc: &ArcInfo,
    from_node: &TreeNode,
    to_node: &TreeNode,
    camera: &TreeCamera,
    viewport: &egui::Rect,
    stroke: egui::Stroke,
) {
    // Calculate angles of both nodes relative to the arc center
    let angle1 = (from_node.y - arc.center_y).atan2(from_node.x - arc.center_x);
    let angle2 = (to_node.y - arc.center_y).atan2(to_node.x - arc.center_x);

    // Determine the shorter arc direction
    let mut start = angle1;
    let mut end = angle2;
    let mut diff = end - start;
    if diff > std::f32::consts::PI {
        diff -= 2.0 * std::f32::consts::PI;
    } else if diff < -std::f32::consts::PI {
        diff += 2.0 * std::f32::consts::PI;
    }
    if diff < 0.0 {
        std::mem::swap(&mut start, &mut end);
        diff = -diff;
    }

    // Number of line segments to approximate the arc
    let segments = ((diff * arc.radius * camera.zoom / 10.0).ceil() as usize).clamp(4, 32);

    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let angle = start + diff * t;
        let tree_x = arc.center_x + arc.radius * angle.cos();
        let tree_y = arc.center_y + arc.radius * angle.sin();
        points.push(camera.tree_to_screen(tree_x, tree_y, viewport));
    }

    // Draw the arc as connected line segments
    for window in points.windows(2) {
        painter.line_segment([window[0], window[1]], stroke);
    }
}

fn line_intersects_rect(a: egui::Pos2, b: egui::Pos2, rect: &egui::Rect) -> bool {
    if rect.contains(a) || rect.contains(b) {
        return true;
    }
    let line_min_x = a.x.min(b.x);
    let line_max_x = a.x.max(b.x);
    let line_min_y = a.y.min(b.y);
    let line_max_y = a.y.max(b.y);
    !(line_max_x < rect.min.x
        || line_min_x > rect.max.x
        || line_max_y < rect.min.y
        || line_min_y > rect.max.y)
}

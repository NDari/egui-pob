//! Passive tree renderer: draws nodes and connections using egui's Painter API.
//! When a sprite atlas is loaded, nodes are rendered with actual game textures.
//! Falls back to colored circles when sprites are unavailable.

use pob_egui::data::tree::{ArcInfo, NodeType, TreeData, TreeNode};
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
/// Returns the ID of a clicked node, if any.
pub fn draw_tree(
    ui: &mut egui::Ui,
    tree: &TreeData,
    camera: &mut TreeCamera,
    atlas: Option<&TreeSpriteAtlas>,
) -> Option<u32> {
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let rect = response.rect;

    // Handle pan (drag)
    if response.dragged() {
        let delta = response.drag_delta();
        camera.center_x -= delta.x / camera.zoom;
        camera.center_y -= delta.y / camera.zoom;
    }

    // Handle zoom (scroll)
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

    // Visible area for culling
    let visible_margin = 50.0;
    let visible_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x - visible_margin, rect.min.y - visible_margin),
        egui::pos2(rect.max.x + visible_margin, rect.max.y + visible_margin),
    );

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

        let both_allocated = from_node.is_allocated && to_node.is_allocated;
        let color = if both_allocated {
            Palette::CONNECTION_ALLOCATED
        } else {
            Palette::CONNECTION
        };
        let width = if both_allocated { 2.0 } else { 1.0 };
        let stroke = egui::Stroke::new(width, color);

        if let Some(arc) = &conn.arc {
            draw_arc(&painter, arc, from_node, to_node, camera, &rect, stroke);
        } else {
            painter.line_segment([from_screen, to_screen], stroke);
        }
    }

    // Find hovered node
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

    // Draw nodes
    for node in tree.nodes.values() {
        let screen_pos = camera.tree_to_screen(node.x, node.y, &rect);

        if !visible_rect.contains(screen_pos) {
            continue;
        }

        let radius = (node.node_type.radius() * camera.zoom).max(1.5);
        let is_hovered = hovered_node.is_some_and(|h| h.id == node.id);

        let drew_sprite = if let Some(atlas) = atlas {
            draw_node_sprite(&painter, node, screen_pos, radius, atlas)
        } else {
            false
        };

        if !drew_sprite {
            // Fallback: colored circle
            let color = if node.is_allocated {
                Palette::ALLOCATED
            } else if node.ascendancy_name.is_some() {
                Palette::ASCENDANCY
            } else {
                node_type_color(node.node_type)
            };
            painter.circle_filled(screen_pos, radius, color);
        }

        // Hover outline
        if is_hovered {
            painter.circle_stroke(
                screen_pos,
                radius + 2.0,
                egui::Stroke::new(2.0, Palette::HOVER_OUTLINE),
            );
        }

        // Draw name for notable/keystone when zoomed in
        if camera.zoom > 0.08 && matches!(node.node_type, NodeType::Keystone | NodeType::Notable) {
            let font = egui::FontId::proportional(10.0);
            painter.text(
                egui::pos2(screen_pos.x, screen_pos.y + radius + 4.0),
                egui::Align2::CENTER_TOP,
                &node.name,
                font,
                egui::Color32::from_rgb(200, 200, 200),
            );
        }
    }

    // Tooltip
    if let Some(node) = hovered_node {
        response.clone().on_hover_ui_at_pointer(|ui| {
            ui.strong(&node.name);
            ui.label(format!("{:?}", node.node_type));
            for stat in &node.stats {
                ui.label(stat);
            }
            if node.is_allocated {
                ui.colored_label(Palette::ALLOCATED, "Allocated");
            }
        });
    }

    // Handle click
    let mut clicked_node_id = None;
    if response.clicked()
        && let Some(node) = hovered_node
    {
        clicked_node_id = Some(node.id);
    }

    clicked_node_id
}

/// Try to draw a node using sprite textures. Returns true if successful.
fn draw_node_sprite(
    painter: &egui::Painter,
    node: &TreeNode,
    screen_pos: egui::Pos2,
    radius: f32,
    atlas: &TreeSpriteAtlas,
) -> bool {
    // Look up the icon sprite
    // For masteries, use inactiveIcon/activeIcon paths instead of the generic icon
    let icon_region = if node.node_type == NodeType::Mastery {
        // Masteries have dedicated inactive/active icons in the mastery spritesheet
        let mastery_icon = if node.is_allocated {
            node.active_icon.as_deref()
        } else {
            node.inactive_icon.as_deref()
        };
        mastery_icon
            .and_then(|icon| atlas.node_sprites.get(icon))
            .and_then(|ns| {
                ns.mastery
                    .as_ref()
                    .or(ns.normal_active.as_ref())
                    .or(ns.normal_inactive.as_ref())
            })
            // Fall back to the generic icon's mastery sprite
            .or_else(|| {
                atlas
                    .node_sprites
                    .get(&node.icon)
                    .and_then(|ns| ns.mastery.as_ref())
            })
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

    // Draw the icon sprite
    let half = radius;
    let icon_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(half * 2.0, half * 2.0));
    let uv = egui::Rect::from_min_max(
        egui::pos2(region.u_min, region.v_min),
        egui::pos2(region.u_max, region.v_max),
    );
    painter.image(texture_id, icon_rect, uv, egui::Color32::WHITE);

    // Draw frame overlay
    let frame_region = get_frame_region(&atlas.frames, node);
    if let Some(frame) = frame_region
        && let Some(frame_tex) = atlas.texture_id(frame.sheet_index)
    {
        // Frame is slightly larger than the icon
        let frame_scale = 1.3;
        let frame_half = half * frame_scale;
        let frame_rect = egui::Rect::from_center_size(
            screen_pos,
            egui::vec2(frame_half * 2.0, frame_half * 2.0),
        );
        let frame_uv = egui::Rect::from_min_max(
            egui::pos2(frame.u_min, frame.v_min),
            egui::pos2(frame.u_max, frame.v_max),
        );
        painter.image(frame_tex, frame_rect, frame_uv, egui::Color32::WHITE);
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
        NodeType::Mastery => None, // Masteries don't have standard frames
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

/// Draw an arc connection between two nodes on the same orbit.
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

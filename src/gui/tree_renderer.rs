//! Passive tree renderer: draws nodes and connections using egui's Painter API.

use pob_egui::data::tree::{NodeType, TreeData, TreeNode};

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
    /// Center of view in tree coordinates.
    pub center_x: f32,
    pub center_y: f32,
    /// Zoom level (pixels per tree unit).
    pub zoom: f32,
}

impl TreeCamera {
    pub fn new(tree: &TreeData) -> Self {
        let (cx, cy) = tree.bounds.center();
        let size = tree.bounds.size();
        // Start zoomed out to fit the whole tree
        Self {
            center_x: cx,
            center_y: cy,
            zoom: 0.03_f32.max(1.0 / (size / 800.0)),
        }
    }

    /// Convert tree coordinates to screen position.
    fn tree_to_screen(&self, tree_x: f32, tree_y: f32, rect: &egui::Rect) -> egui::Pos2 {
        let screen_cx = rect.center().x;
        let screen_cy = rect.center().y;
        egui::pos2(
            screen_cx + (tree_x - self.center_x) * self.zoom,
            screen_cy + (tree_y - self.center_y) * self.zoom,
        )
    }

    /// Convert screen position to tree coordinates.
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
pub fn draw_tree(ui: &mut egui::Ui, tree: &TreeData, camera: &mut TreeCamera) -> Option<u32> {
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

        // Zoom toward mouse position
        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos())
            && rect.contains(mouse_pos)
        {
            let (tree_x, tree_y) = camera.screen_to_tree(mouse_pos, &rect);
            camera.center_x += (tree_x - camera.center_x) * (1.0 - old_zoom / camera.zoom);
            camera.center_y += (tree_y - camera.center_y) * (1.0 - old_zoom / camera.zoom);
        }
    }

    // Determine visible area for culling
    let visible_margin = 50.0;
    let visible_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x - visible_margin, rect.min.y - visible_margin),
        egui::pos2(rect.max.x + visible_margin, rect.max.y + visible_margin),
    );

    // Draw connections first (behind nodes)
    for &(from_id, to_id) in &tree.connections {
        let (Some(from_node), Some(to_node)) = (tree.nodes.get(&from_id), tree.nodes.get(&to_id))
        else {
            continue;
        };

        let from_screen = camera.tree_to_screen(from_node.x, from_node.y, &rect);
        let to_screen = camera.tree_to_screen(to_node.x, to_node.y, &rect);

        // Cull connections entirely outside the visible area
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

        painter.line_segment([from_screen, to_screen], egui::Stroke::new(width, color));
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

        // Cull nodes outside visible area
        if !visible_rect.contains(screen_pos) {
            continue;
        }

        let radius = (node.node_type.radius() * camera.zoom).max(1.5);
        let is_hovered = hovered_node.is_some_and(|h| h.id == node.id);

        let color = if node.is_allocated {
            Palette::ALLOCATED
        } else if node.ascendancy_name.is_some() {
            Palette::ASCENDANCY
        } else {
            node_type_color(node.node_type)
        };

        painter.circle_filled(screen_pos, radius, color);

        // Outline for hovered or keystone/notable (when zoomed in enough)
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

    // Show tooltip for hovered node
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

/// Quick check if a line segment might intersect a rectangle.
fn line_intersects_rect(a: egui::Pos2, b: egui::Pos2, rect: &egui::Rect) -> bool {
    // If either endpoint is inside, it intersects
    if rect.contains(a) || rect.contains(b) {
        return true;
    }
    // Quick bounding box check
    let line_min_x = a.x.min(b.x);
    let line_max_x = a.x.max(b.x);
    let line_min_y = a.y.min(b.y);
    let line_max_y = a.y.max(b.y);
    !(line_max_x < rect.min.x
        || line_min_x > rect.max.x
        || line_max_y < rect.min.y
        || line_min_y > rect.max.y)
}

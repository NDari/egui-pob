//! Passive tree renderer: draws nodes and connections using egui's Painter API.
//! When a sprite atlas is loaded, nodes are rendered with actual game textures.
//! Falls back to colored circles when sprites are unavailable.

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

    // Fill background to match upstream PoB's dark blue-gray tree background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(8, 12, 17));

    // Draw backgrounds (behind everything else)
    if let Some(atlas) = atlas {
        draw_class_start_background(&painter, tree, camera, &rect, &visible_rect, atlas);
        draw_group_backgrounds(&painter, tree, camera, &rect, &visible_rect, atlas);
    }

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

    }

    // Tooltip
    if let Some(node) = hovered_node {
        response.clone().on_hover_ui_at_pointer(|ui| {
            show_node_tooltip(ui, node);
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
            let img_rect =
                egui::Rect::from_center_size(screen_pos, egui::vec2(w * 2.0, h * 2.0));
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(tex, img_rect, uv, egui::Color32::WHITE);
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
        painter.image(effect_tex, effect_rect, effect_uv, egui::Color32::WHITE);
    }

    // Draw the icon sprite — scale down slightly so square JPEG corners
    // are hidden behind the circular frame overlay
    let icon_half = half * 0.85;
    let icon_rect = egui::Rect::from_center_size(screen_pos, egui::vec2(icon_half * 2.0, icon_half * 2.0));
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

/// Rich tooltip for a passive tree node, styled to match upstream PoB.
fn show_node_tooltip(ui: &mut egui::Ui, node: &TreeNode) {
    ui.set_max_width(400.0);

    // Header: node type label + name
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
    ui.label(egui::RichText::new(&node.name).strong().size(16.0));

    // Oil recipe (for notables)
    if !node.recipe.is_empty() {
        ui.horizontal(|ui| {
            let oils: Vec<&str> = node
                .recipe
                .iter()
                .map(|r| r.strip_suffix("Oil").unwrap_or(r))
                .collect();
            ui.label(
                egui::RichText::new(format!("Anoint: {}", oils.join(" + ")))
                    .small()
                    .color(egui::Color32::from_rgb(200, 180, 100)),
            );
        });
    }

    // Stats
    if !node.stats.is_empty() {
        ui.separator();
        for stat in &node.stats {
            ui.label(
                egui::RichText::new(stat).color(egui::Color32::from_rgb(136, 136, 255)),
            );
        }
    }

    // Reminder text
    if !node.reminder_text.is_empty() {
        ui.separator();
        for line in &node.reminder_text {
            ui.label(
                egui::RichText::new(line)
                    .small()
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
                    .italics()
                    .color(egui::Color32::from_rgb(175, 96, 37)),
            );
        }
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
        1 => ("Str", -2750.0_f32, 1600.0_f32),      // Marauder
        2 => ("Dex", 2550.0, 1600.0),                // Ranger
        3 => ("Int", -250.0, -2200.0),                // Witch
        4 => ("StrDex", -150.0, 2350.0),              // Duelist
        5 => ("StrInt", -2100.0, -1500.0),            // Templar
        6 => ("DexInt", 2350.0, -1950.0),             // Shadow
        _ => return, // Scion (0) or unknown — no background
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
                if let Some(bg) = atlas.ascendancy_backgrounds.get(name.as_str()) {
                    let screen_pos = camera.tree_to_screen(group.x, group.y, viewport);
                    let w = bg.width * 1.33 * camera.zoom;
                    let h = bg.height * 1.33 * camera.zoom;
                    let img_rect =
                        egui::Rect::from_center_size(screen_pos, egui::vec2(w * 2.0, h * 2.0));
                    if img_rect.intersects(*visible_rect) {
                        if let Some(tex) = atlas.texture_id(bg.sheet_index) {
                            let uv =
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                            painter.image(tex, img_rect, uv, egui::Color32::WHITE);
                        }
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

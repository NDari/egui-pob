//! Tree sprite atlas: loads spritesheets and provides UV coordinates for node icons.

use std::collections::HashMap;
use std::path::Path;

use mlua::prelude::*;

/// Sprite region within an atlas (UV coordinates are normalized 0-1).
#[derive(Debug, Clone, Copy)]
pub struct SpriteRegion {
    pub u_min: f32,
    pub v_min: f32,
    pub u_max: f32,
    pub v_max: f32,
    pub width: f32,
    pub height: f32,
    pub sheet_index: usize,
}

/// Pre-resolved sprite data for a node: which atlas region to use for each state.
#[derive(Debug, Clone, Default)]
pub struct NodeSprites {
    pub normal_active: Option<SpriteRegion>,
    pub normal_inactive: Option<SpriteRegion>,
    pub notable_active: Option<SpriteRegion>,
    pub notable_inactive: Option<SpriteRegion>,
    pub keystone_active: Option<SpriteRegion>,
    pub keystone_inactive: Option<SpriteRegion>,
    pub mastery: Option<SpriteRegion>,
    pub mastery_inactive: Option<SpriteRegion>,
    pub mastery_active: Option<SpriteRegion>,
    pub mastery_connected: Option<SpriteRegion>,
    pub mastery_effect: Option<SpriteRegion>,
}

/// Frame overlay sprite for each node type/state.
#[derive(Debug, Clone, Default)]
pub struct FrameSprites {
    pub normal_unallocated: Option<SpriteRegion>,
    pub normal_allocated: Option<SpriteRegion>,
    pub normal_can_allocate: Option<SpriteRegion>,
    pub notable_unallocated: Option<SpriteRegion>,
    pub notable_allocated: Option<SpriteRegion>,
    pub notable_can_allocate: Option<SpriteRegion>,
    pub keystone_unallocated: Option<SpriteRegion>,
    pub keystone_allocated: Option<SpriteRegion>,
    pub keystone_can_allocate: Option<SpriteRegion>,
    pub jewel_unallocated: Option<SpriteRegion>,
    pub jewel_allocated: Option<SpriteRegion>,
    pub jewel_can_allocate: Option<SpriteRegion>,
    pub mastery_unallocated: Option<SpriteRegion>,
    pub mastery_allocated: Option<SpriteRegion>,
    pub mastery_can_allocate: Option<SpriteRegion>,
    pub group_background_small: Option<SpriteRegion>,
    pub group_background_medium: Option<SpriteRegion>,
    pub group_background_large: Option<SpriteRegion>,
}

/// All loaded sprite atlas data.
pub struct TreeSpriteAtlas {
    /// Loaded spritesheet images as raw RGBA data, ready to upload to egui.
    pub sheets: Vec<SpriteSheet>,
    /// Per-node sprite data, keyed by icon path.
    pub node_sprites: HashMap<String, NodeSprites>,
    /// Frame overlay sprites.
    pub frames: FrameSprites,
}

pub struct SpriteSheet {
    pub image: egui::ColorImage,
    pub texture: Option<egui::TextureHandle>,
}

impl TreeSpriteAtlas {
    /// Load sprite atlas from the tree data directory.
    /// Reads the spritesheet images and parses the sprite coordinates from Lua.
    pub fn load(lua: &Lua, tree_data_dir: &Path) -> Result<Self, mlua::Error> {
        let mut sheets = Vec::new();
        let mut sheet_map: HashMap<String, usize> = HashMap::new();

        // Load spritesheets
        let skills_path = tree_data_dir.join("skills-3.jpg");
        let frame_path = tree_data_dir.join("frame-3.png");
        let mastery_path = tree_data_dir.join("mastery-3.png");
        let mastery_connected_path = tree_data_dir.join("mastery-connected-3.png");
        let mastery_disabled_path = tree_data_dir.join("mastery-disabled-3.png");
        let mastery_active_path = tree_data_dir.join("mastery-active-selected-3.png");
        let mastery_effect_path = tree_data_dir.join("mastery-active-effect-3.png");
        let ascendancy_path = tree_data_dir.join("ascendancy-3.webp");
        let group_bg_path = tree_data_dir.join("group-background-3.png");

        let skills_index = load_sheet(&mut sheets, &skills_path);
        let frame_index = load_sheet(&mut sheets, &frame_path);
        let mastery_index = load_sheet(&mut sheets, &mastery_path);
        let mastery_connected_index = load_sheet(&mut sheets, &mastery_connected_path);
        let mastery_disabled_index = load_sheet(&mut sheets, &mastery_disabled_path);
        let mastery_active_index = load_sheet(&mut sheets, &mastery_active_path);
        let mastery_effect_index = load_sheet(&mut sheets, &mastery_effect_path);
        let ascendancy_index = load_sheet(&mut sheets, &ascendancy_path);
        let group_bg_index = load_sheet(&mut sheets, &group_bg_path);

        // Map filenames to sheet indices
        if let Some(idx) = skills_index {
            sheet_map.insert("skills-3.jpg".to_string(), idx);
        }
        if let Some(idx) = frame_index {
            sheet_map.insert("frame-3.png".to_string(), idx);
        }
        if let Some(idx) = mastery_index {
            sheet_map.insert("mastery-3.png".to_string(), idx);
        }
        if let Some(idx) = mastery_connected_index {
            sheet_map.insert("mastery-connected-3.png".to_string(), idx);
        }
        if let Some(idx) = mastery_disabled_index {
            sheet_map.insert("mastery-disabled-3.png".to_string(), idx);
        }
        if let Some(idx) = mastery_active_index {
            sheet_map.insert("mastery-active-selected-3.png".to_string(), idx);
        }
        if let Some(idx) = mastery_effect_index {
            sheet_map.insert("mastery-active-effect-3.png".to_string(), idx);
        }
        if let Some(idx) = ascendancy_index {
            sheet_map.insert("ascendancy-3.webp".to_string(), idx);
        }
        if let Some(idx) = group_bg_index {
            sheet_map.insert("group-background-3.png".to_string(), idx);
        }

        // Parse sprite coordinates from the processed spriteMap in Lua
        let node_sprites = extract_node_sprites(lua, &sheets, &sheet_map)?;
        let frames = extract_frame_sprites(lua, &sheets, &sheet_map)?;

        log::info!(
            "Loaded {} spritesheets, {} node sprite entries",
            sheets.len(),
            node_sprites.len()
        );

        Ok(Self {
            sheets,
            node_sprites,
            frames,
        })
    }

    /// Upload textures to the egui context. Call once after creating the atlas.
    pub fn upload_textures(&mut self, ctx: &egui::Context) {
        for (i, sheet) in self.sheets.iter_mut().enumerate() {
            if sheet.texture.is_none() {
                sheet.texture = Some(ctx.load_texture(
                    format!("tree_sheet_{i}"),
                    sheet.image.clone(),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
    }

    /// Get the texture ID for a sheet.
    pub fn texture_id(&self, sheet_index: usize) -> Option<egui::TextureId> {
        self.sheets
            .get(sheet_index)
            .and_then(|s| s.texture.as_ref())
            .map(|t| t.id())
    }
}

fn load_sheet(sheets: &mut Vec<SpriteSheet>, path: &Path) -> Option<usize> {
    let img = image::open(path)
        .map_err(|e| log::warn!("Failed to load spritesheet {}: {e}", path.display()))
        .ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
    let index = sheets.len();
    sheets.push(SpriteSheet {
        image: color_image,
        texture: None,
    });
    Some(index)
}

/// Extract node icon sprites from Lua's processed spriteMap.
fn extract_node_sprites(
    lua: &Lua,
    sheets: &[SpriteSheet],
    sheet_map: &HashMap<String, usize>,
) -> Result<HashMap<String, NodeSprites>, mlua::Error> {
    let sprites_data: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local tree = build.spec.tree
            if not tree or not tree.spriteMap then
                return {}
            end
            local result = {}
            for iconName, spriteSet in pairs(tree.spriteMap) do
                result[iconName] = {}
                for spriteName, sprite in pairs(spriteSet) do
                    if type(sprite) == "table" and sprite[1] then
                        result[iconName][spriteName] = {
                            u0 = sprite[1],
                            v0 = sprite[2],
                            u1 = sprite[3],
                            v1 = sprite[4],
                            w = sprite.width,
                            h = sprite.height,
                        }
                    end
                end
            end
            return result
        "#,
        )
        .eval()?;

    // Sheet indices and dimensions for UV normalization
    // (Lua's ImageSize() stub returns 1,1 so spriteMap coords are in pixels)
    let skills_idx = sheet_map.get("skills-3.jpg").copied();
    let mastery_idx = sheet_map.get("mastery-3.png").copied();
    let mastery_connected_idx = sheet_map.get("mastery-connected-3.png").copied();
    let mastery_disabled_idx = sheet_map.get("mastery-disabled-3.png").copied();
    let mastery_active_idx = sheet_map.get("mastery-active-selected-3.png").copied();
    let mastery_effect_idx = sheet_map.get("mastery-active-effect-3.png").copied();

    let sheet_dims = |idx: Option<usize>| -> (f32, f32) {
        idx.and_then(|i| sheets.get(i))
            .map(|s| (s.image.width() as f32, s.image.height() as f32))
            .unwrap_or((1.0, 1.0))
    };

    let mut node_sprites = HashMap::new();
    for pair in sprites_data.pairs::<String, LuaTable>() {
        let (icon_name, sprite_set) = pair?;
        let mut ns = NodeSprites::default();

        for entry in sprite_set.pairs::<String, LuaTable>() {
            let (sprite_type, coords) = entry?;

            // Each sprite type uses its own spritesheet
            let sheet_info = match sprite_type.as_str() {
                "mastery" => mastery_idx.map(|i| (i, sheet_dims(Some(i)))),
                "masteryConnected" => mastery_connected_idx.map(|i| (i, sheet_dims(Some(i)))),
                "masteryInactive" => mastery_disabled_idx.map(|i| (i, sheet_dims(Some(i)))),
                "masteryActiveSelected" => mastery_active_idx.map(|i| (i, sheet_dims(Some(i)))),
                "masteryActiveEffect" => mastery_effect_idx.map(|i| (i, sheet_dims(Some(i)))),
                _ => skills_idx.map(|i| (i, sheet_dims(Some(i)))),
            };
            let Some((sheet_index, (sw, sh))) = sheet_info else {
                continue;
            };

            let mut region = parse_sprite_region(&coords, sheet_index)?;
            // Normalize pixel coordinates to 0-1 UV range
            region.u_min /= sw;
            region.v_min /= sh;
            region.u_max /= sw;
            region.v_max /= sh;
            match sprite_type.as_str() {
                "normalActive" => ns.normal_active = Some(region),
                "normalInactive" => ns.normal_inactive = Some(region),
                "notableActive" => ns.notable_active = Some(region),
                "notableInactive" => ns.notable_inactive = Some(region),
                "keystoneActive" => ns.keystone_active = Some(region),
                "keystoneInactive" => ns.keystone_inactive = Some(region),
                "mastery" => ns.mastery = ns.mastery.or(Some(region)),
                "masteryInactive" => ns.mastery_inactive = Some(region),
                "masteryActiveSelected" => ns.mastery_active = Some(region),
                "masteryConnected" => ns.mastery_connected = Some(region),
                "masteryActiveEffect" => ns.mastery_effect = Some(region),
                _ => {}
            }
        }

        node_sprites.insert(icon_name, ns);
    }

    Ok(node_sprites)
}

/// Extract frame overlay sprites.
fn extract_frame_sprites(
    _lua: &Lua,
    sheets: &[SpriteSheet],
    sheet_map: &HashMap<String, usize>,
) -> Result<FrameSprites, mlua::Error> {
    let frame_idx = sheet_map.get("frame-3.png").copied();
    let mut frames = FrameSprites::default();

    // Hard-code frame coordinates from sprites.lua since they're in a separate
    // sprite category not indexed by spriteMap
    if let Some(idx) = frame_idx {
        let Some(sheet) = sheets.get(idx) else {
            return Ok(frames);
        };
        let sw = sheet.image.width() as f32;
        let sh = sheet.image.height() as f32;

        // Normal frames (PSSkillFrame* in sprites.lua)
        frames.normal_unallocated = Some(region_from_px(39, 295, 39, 39, sw, sh, idx));
        frames.normal_allocated = Some(region_from_px(0, 295, 39, 39, sw, sh, idx));
        frames.normal_can_allocate = Some(region_from_px(325, 232, 39, 39, sw, sh, idx));

        // Notable frames
        frames.notable_unallocated = Some(region_from_px(0, 237, 58, 58, sw, sh, idx));
        frames.notable_allocated = Some(region_from_px(116, 237, 58, 58, sw, sh, idx));
        frames.notable_can_allocate = Some(region_from_px(58, 237, 58, 58, sw, sh, idx));

        // Keystone frames
        frames.keystone_unallocated = Some(region_from_px(0, 0, 83, 85, sw, sh, idx));
        frames.keystone_allocated = Some(region_from_px(166, 0, 83, 85, sw, sh, idx));
        frames.keystone_can_allocate = Some(region_from_px(83, 0, 83, 85, sw, sh, idx));

        // Jewel frames
        frames.jewel_unallocated = Some(region_from_px(174, 237, 58, 58, sw, sh, idx));
        frames.jewel_allocated = Some(region_from_px(325, 0, 58, 58, sw, sh, idx));
        frames.jewel_can_allocate = Some(region_from_px(232, 237, 58, 58, sw, sh, idx));
    }

    // Mastery frames are in the ascendancy spritesheet
    if let Some(idx) = sheet_map.get("ascendancy-3.webp").copied()
        && let Some(sheet) = sheets.get(idx)
    {
        let sw = sheet.image.width() as f32;
        let sh = sheet.image.height() as f32;

        // AscendancyFrameLarge* coords from sprites.lua
        frames.mastery_unallocated = Some(region_from_px(1672, 1494, 58, 58, sw, sh, idx));
        frames.mastery_can_allocate = Some(region_from_px(1730, 1494, 58, 58, sw, sh, idx));
        frames.mastery_allocated = Some(region_from_px(1788, 1494, 58, 58, sw, sh, idx));
    }

    // Group backgrounds from group-background-3.png
    if let Some(idx) = sheet_map.get("group-background-3.png").copied()
        && let Some(sheet) = sheets.get(idx)
    {
        let sw = sheet.image.width() as f32;
        let sh = sheet.image.height() as f32;

        frames.group_background_small = Some(region_from_px(443, 444, 138, 138, sw, sh, idx));
        frames.group_background_medium = Some(region_from_px(723, 286, 178, 178, sw, sh, idx));
        frames.group_background_large = Some(region_from_px(723, 0, 283, 143, sw, sh, idx));
    }

    Ok(frames)
}

fn parse_sprite_region(coords: &LuaTable, sheet_index: usize) -> Result<SpriteRegion, mlua::Error> {
    Ok(SpriteRegion {
        u_min: coords.get("u0")?,
        v_min: coords.get("v0")?,
        u_max: coords.get("u1")?,
        v_max: coords.get("v1")?,
        width: coords.get("w")?,
        height: coords.get("h")?,
        sheet_index,
    })
}

fn region_from_px(x: u32, y: u32, w: u32, h: u32, sw: f32, sh: f32, idx: usize) -> SpriteRegion {
    SpriteRegion {
        u_min: x as f32 / sw,
        v_min: y as f32 / sh,
        u_max: (x + w) as f32 / sw,
        v_max: (y + h) as f32 / sh,
        width: w as f32,
        height: h as f32,
        sheet_index: idx,
    }
}

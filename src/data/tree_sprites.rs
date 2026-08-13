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

/// A background image (ascendancy class art or class start art). Most are a
/// whole file, but the newer ascendancy emblems only ship as a region of a
/// spritesheet, so the UV rect is carried alongside.
pub struct BackgroundImage {
    pub sheet_index: usize,
    pub width: f32,
    pub height: f32,
    /// Normalized region of `sheet_index` to draw; the full image for
    /// standalone files.
    pub uv: egui::Rect,
}

/// UV rect covering a whole image.
fn full_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// All loaded sprite atlas data.
pub struct TreeSpriteAtlas {
    /// Loaded spritesheet images as raw RGBA data, ready to upload to egui.
    pub sheets: Vec<SpriteSheet>,
    /// Per-node sprite data, keyed by icon path.
    pub node_sprites: HashMap<String, NodeSprites>,
    /// Frame overlay sprites.
    pub frames: FrameSprites,
    /// Socketed-jewel socket art (JewelSocketActiveBlue etc.), keyed by
    /// upstream asset name, from the jewel-3.png sheet.
    pub jewel_art: HashMap<String, SpriteRegion>,
    /// Ascendancy class background images, keyed by ascendancy name (e.g. "Berserker").
    pub ascendancy_backgrounds: HashMap<String, BackgroundImage>,
    /// Class start background images, keyed by asset name (e.g. "Str" for BackgroundStr).
    pub class_backgrounds: HashMap<String, BackgroundImage>,
    /// Class start node art, keyed by asset name (e.g. "centertemplar", "PSStartNodeBackgroundInactive").
    pub class_start_art: HashMap<String, BackgroundImage>,
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

        let jewel_path = tree_data_dir.join("jewel-3.png");
        let skills_index = load_sheet(&mut sheets, &skills_path);
        let jewel_index = load_sheet(&mut sheets, &jewel_path);
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
        if let Some(idx) = jewel_index {
            sheet_map.insert("jewel-3.png".to_string(), idx);
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
        let jewel_art = extract_jewel_art(lua, &sheets, &sheet_map)?;

        // Load standalone background images from the parent TreeData/ directory
        let tree_data_root = tree_data_dir.parent();
        let mut ascendancy_backgrounds =
            load_prefixed_backgrounds(&mut sheets, tree_data_root, "Classes");
        let class_backgrounds =
            load_prefixed_backgrounds(&mut sheets, tree_data_root, "Background");

        // Emblems that only exist inside a spritesheet (the 3.29 bloodlines,
        // Reliquarian, Luminary) - added on top of the loose PNGs above.
        load_sprite_backgrounds(
            lua,
            &mut sheets,
            &mut sheet_map,
            tree_data_dir,
            &mut ascendancy_backgrounds,
        )?;

        // Load class start node art — keyed by full asset name to match node.startArt
        let mut class_start_art = HashMap::new();
        if let Some(root) = tree_data_root {
            let class_start_files = [
                "centerscion",
                "centermarauder",
                "centerranger",
                "centerwitch",
                "centerduelist",
                "centertemplar",
                "centershadow",
                "PSStartNodeBackgroundInactive",
            ];
            for name in class_start_files {
                let path = root.join(format!("{name}.png"));
                if let Some(idx) = load_sheet(&mut sheets, &path) {
                    let w = sheets[idx].image.width() as f32;
                    let h = sheets[idx].image.height() as f32;
                    class_start_art.insert(
                        name.to_string(),
                        BackgroundImage {
                            sheet_index: idx,
                            width: w,
                            height: h,
                            uv: full_uv(),
                        },
                    );
                }
            }
        }

        log::info!(
            "Loaded {} spritesheets, {} node sprite entries, {} ascendancy backgrounds, {} class backgrounds, {} class start art",
            sheets.len(),
            node_sprites.len(),
            ascendancy_backgrounds.len(),
            class_backgrounds.len(),
            class_start_art.len(),
        );

        Ok(Self {
            sheets,
            node_sprites,
            frames,
            ascendancy_backgrounds,
            class_backgrounds,
            class_start_art,
            jewel_art,
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

/// Load PNG images with a given filename prefix from the TreeData/ directory.
/// Returns a map from the suffix (prefix stripped) to the background info.
/// E.g., prefix "Classes" loads "ClassesBerserker.png" keyed as "Berserker".
fn load_prefixed_backgrounds(
    sheets: &mut Vec<SpriteSheet>,
    tree_data_root: Option<&Path>,
    prefix: &str,
) -> HashMap<String, BackgroundImage> {
    let mut backgrounds = HashMap::new();

    let Some(root) = tree_data_root else {
        return backgrounds;
    };

    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Failed to read TreeData directory: {e}");
            return backgrounds;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };
        if !filename.starts_with(prefix) || !filename.ends_with(".png") {
            continue;
        }
        let name = &filename[prefix.len()..filename.len() - ".png".len()];
        if name.is_empty() {
            continue;
        }

        if let Some(idx) = load_sheet(sheets, &path) {
            let w = sheets[idx].image.width() as f32;
            let h = sheets[idx].image.height() as f32;
            backgrounds.insert(
                name.to_string(),
                BackgroundImage {
                    sheet_index: idx,
                    width: w,
                    height: h,
                    uv: full_uv(),
                },
            );
        }
    }

    backgrounds
}

/// One `Classes*` emblem as it appears in a spritesheet, straight from Lua.
struct SpriteBackground {
    /// Ascendancy name with the "Classes" prefix stripped (e.g. "Trialmaster").
    name: String,
    /// Spritesheet file this region lives in (e.g. "bloodline-3.webp").
    file: String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    height: f32,
}

/// Add ascendancy backgrounds that only exist as spritesheet regions.
///
/// The 3.29 bloodline emblems (and Reliquarian / Luminary) ship inside
/// `bloodline-3.webp` / `ascendancy-3.webp` rather than as loose `Classes*.png`
/// files, which is why they had no art here. Upstream pulls them out of its
/// spriteMap in `PassiveTree.lua:349-365`, keeping the loose PNG whenever one
/// exists; entries already loaded by [`load_prefixed_backgrounds`] are likewise
/// left alone, which covers upstream's Primalist / Warlock / Warden carve-out.
fn load_sprite_backgrounds(
    lua: &Lua,
    sheets: &mut Vec<SpriteSheet>,
    sheet_map: &mut HashMap<String, usize>,
    tree_data_dir: &Path,
    backgrounds: &mut HashMap<String, BackgroundImage>,
) -> Result<(), mlua::Error> {
    let entries: LuaTable = lua
        .load(
            r#"
            local tree = mainObject_ref.main.modes['BUILD'].spec.tree
            local result = {}
            if not (tree and tree.spriteMap and tree.skillSprites) then
                return result
            end
            for name, spriteSet in pairs(tree.spriteMap) do
                if name:match("^Classes") then
                    -- One sheet per emblem; upstream takes the first entry too
                    local spriteType, sprite = next(spriteSet)
                    local sheet = spriteType and tree.skillSprites[spriteType]
                    if sprite and sprite[1] and sheet and sheet.filename then
                        -- "https://.../bloodline-3.webp?c89491a1" -> "bloodline-3.webp"
                        local file = sheet.filename:gsub("%?%x+$", ""):gsub(".*/", "")
                        table.insert(result, {
                            name = name:sub(#"Classes" + 1),
                            file = file,
                            x0 = sprite[1], y0 = sprite[2],
                            x1 = sprite[3], y1 = sprite[4],
                            w = sprite.width, h = sprite.height,
                        })
                    end
                end
            end
            return result
        "#,
        )
        .eval()?;

    let mut parsed = Vec::new();
    for entry in entries.sequence_values::<LuaTable>() {
        let e = entry?;
        parsed.push(SpriteBackground {
            name: e.get("name")?,
            file: e.get("file")?,
            x0: e.get("x0")?,
            y0: e.get("y0")?,
            x1: e.get("x1")?,
            y1: e.get("y1")?,
            width: e.get("w")?,
            height: e.get("h")?,
        });
    }
    // Deterministic order so the log line and any sheet loading are stable
    parsed.sort_by(|a, b| a.name.cmp(&b.name));

    let mut added = 0;
    for bg in parsed {
        if bg.name.is_empty() || backgrounds.contains_key(&bg.name) {
            continue;
        }
        let sheet_index = match sheet_map.get(&bg.file).copied() {
            Some(idx) => idx,
            None => {
                let path = tree_data_dir.join(&bg.file);
                let Some(idx) = load_sheet(sheets, &path) else {
                    continue;
                };
                sheet_map.insert(bg.file.clone(), idx);
                idx
            }
        };
        let Some(sheet) = sheets.get(sheet_index) else {
            continue;
        };
        // Lua's coords stay in pixels because our ImageSize() stub reports 1x1
        let sw = sheet.image.width() as f32;
        let sh = sheet.image.height() as f32;
        let (u_min, v_min, u_max, v_max) = normalize_uv(bg.x0, bg.y0, bg.x1, bg.y1, sw, sh);
        backgrounds.insert(
            bg.name,
            BackgroundImage {
                sheet_index,
                width: bg.width,
                height: bg.height,
                uv: egui::Rect::from_min_max(egui::pos2(u_min, v_min), egui::pos2(u_max, v_max)),
            },
        );
        added += 1;
    }
    log::info!("Loaded {added} ascendancy backgrounds from spritesheets");
    Ok(())
}

/// Extract jewel socket art (the "jewel" sprite section: JewelSocketActive*
/// variants shown when a jewel is socketed) from Lua's spriteMap.
fn extract_jewel_art(
    lua: &Lua,
    sheets: &[SpriteSheet],
    sheet_map: &HashMap<String, usize>,
) -> Result<HashMap<String, SpriteRegion>, mlua::Error> {
    let mut jewel_art = HashMap::new();
    let Some(jewel_idx) = sheet_map.get("jewel-3.png").copied() else {
        return Ok(jewel_art);
    };
    let (sw, sh) = sheets
        .get(jewel_idx)
        .map(|s| (s.image.width() as f32, s.image.height() as f32))
        .unwrap_or((1.0, 1.0));

    let entries: LuaTable = lua
        .load(
            r#"
            local tree = mainObject_ref.main.modes['BUILD'].spec.tree
            local result = {}
            if tree and tree.spriteMap then
                for name, spriteSet in pairs(tree.spriteMap) do
                    local sprite = spriteSet.jewel
                    if type(sprite) == "table" and sprite[1] then
                        result[name] = {
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

    for pair in entries.pairs::<String, LuaTable>() {
        let (name, coords) = pair?;
        let mut region = parse_sprite_region(&coords, jewel_idx)?;
        (region.u_min, region.v_min, region.u_max, region.v_max) = normalize_uv(
            region.u_min,
            region.v_min,
            region.u_max,
            region.v_max,
            sw,
            sh,
        );
        jewel_art.insert(name, region);
    }
    Ok(jewel_art)
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
            (region.u_min, region.v_min, region.u_max, region.v_max) = normalize_uv(
                region.u_min,
                region.v_min,
                region.u_max,
                region.v_max,
                sw,
                sh,
            );
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
    let (u_min, v_min, u_max, v_max) =
        normalize_uv(x as f32, y as f32, (x + w) as f32, (y + h) as f32, sw, sh);
    SpriteRegion {
        u_min,
        v_min,
        u_max,
        v_max,
        width: w as f32,
        height: h as f32,
        sheet_index: idx,
    }
}

/// Convert a pixel rect in a spritesheet to UV coordinates, inset by half a
/// texel on every side.
///
/// Sprites are packed edge to edge, and the sheets are uploaded with linear
/// filtering, so a UV rect landing exactly on the texel boundary lets bilinear
/// sampling reach into the neighbouring sprite along that edge. That is what
/// drew a faint golden line above every medium group background: the row above
/// `PSGroupBackground2` in group-background-3.png is the opaque bottom edge of
/// `GroupBackgroundLargeHalfAlt`. Sampling texel centres instead keeps every
/// sprite inside its own rect. Rects thinner than one texel collapse to their
/// centre rather than inverting.
fn normalize_uv(x0: f32, y0: f32, x1: f32, y1: f32, sw: f32, sh: f32) -> (f32, f32, f32, f32) {
    let inset = |min: f32, max: f32, size: f32| -> (f32, f32) {
        if max - min <= 1.0 {
            let mid = (min + max) * 0.5 / size;
            return (mid, mid);
        }
        ((min + 0.5) / size, (max - 0.5) / size)
    };
    let (u_min, u_max) = inset(x0, x1, sw);
    let (v_min, v_max) = inset(y0, y1, sh);
    (u_min, v_min, u_max, v_max)
}

#[cfg(test)]
mod tests {
    use super::normalize_uv;

    #[test]
    fn uv_is_inset_by_half_a_texel() {
        // PSGroupBackground2 in group-background-3.png (1006x666). Row 285,
        // just above it, is the opaque bottom edge of GroupBackgroundLargeHalfAlt.
        let (u_min, v_min, u_max, v_max) = normalize_uv(723.0, 286.0, 901.0, 464.0, 1006.0, 666.0);
        assert!(
            v_min * 666.0 > 286.0,
            "top edge must sit inside the sprite, got row {}",
            v_min * 666.0
        );
        assert!(
            v_max * 666.0 < 464.0,
            "bottom edge must sit inside the sprite, got row {}",
            v_max * 666.0
        );
        assert!((u_min * 1006.0 - 723.5).abs() < 1e-3);
        assert!((u_max * 1006.0 - 900.5).abs() < 1e-3);
        // ...while still covering all but half a texel of the sprite
        assert!((v_max - v_min) * 666.0 > 176.0);
    }

    #[test]
    fn thin_regions_collapse_instead_of_inverting() {
        let (u_min, _, u_max, _) = normalize_uv(10.0, 0.0, 11.0, 4.0, 100.0, 100.0);
        assert_eq!(u_min, u_max, "a one-texel column samples its centre");
        assert!((u_min * 100.0 - 10.5).abs() < 1e-3);
    }
}

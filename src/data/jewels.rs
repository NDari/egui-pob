//! Jewel socket data for the tree view: radius definitions and per-socket
//! jewel info (radius index, Thread of Hope annuli, Impossible Escape
//! keystone targets), mirroring upstream's PassiveTreeView ring overlays.

use mlua::prelude::*;

/// One entry of upstream's data.jewelRadius for the active tree version.
#[derive(Debug, Clone)]
pub struct RadiusDef {
    /// Inner radius in tree units (0 for plain circles).
    pub inner: f32,
    /// Outer radius in tree units.
    pub outer: f32,
    pub color: egui::Color32,
    /// "Small" / "Medium" / "Large" / "Variable" (Thread of Hope annuli).
    pub label: String,
}

/// A jewel socket on the tree and what is socketed in it.
#[derive(Debug, Clone)]
pub struct SocketInfo {
    pub node_id: u32,
    pub allocated: bool,
    pub has_jewel: bool,
    pub jewel_title: String,
    /// 0-based index into the radius defs, when the jewel has a radius.
    pub radius_index: Option<usize>,
    /// True for Thread of Hope-like jewels (annular radius).
    pub is_variable: bool,
    /// Tree positions to draw the ring at instead of the socket
    /// (Impossible Escape draws on its keystones).
    pub keystone_positions: Vec<(f32, f32)>,
}

/// Parse "^xRRGGBB" into a colour (white on failure).
fn parse_color_code(code: &str) -> egui::Color32 {
    let hex = code.trim_start_matches("^x");
    if hex.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        )
    {
        return egui::Color32::from_rgb(r, g, b);
    }
    egui::Color32::WHITE
}

/// The radius table for the active tree version (upstream data.jewelRadius).
pub fn radius_defs(lua: &Lua) -> Result<Vec<RadiusDef>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local defs = {}
            for _, info in ipairs(data.jewelRadius) do
                table.insert(defs, {
                    inner = info.inner or 0,
                    outer = info.outer or 0,
                    col = info.col or "^xFFFFFF",
                    label = info.label or "",
                })
            end
            return defs
        "#,
        )
        .eval()?;

    let mut defs = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        defs.push(RadiusDef {
            inner: entry.get("inner").unwrap_or(0.0),
            outer: entry.get("outer").unwrap_or(0.0),
            color: parse_color_code(&entry.get::<String>("col").unwrap_or_default()),
            label: entry.get("label").unwrap_or_default(),
        });
    }
    Ok(defs)
}

/// All jewel sockets on the active tree with their socketed jewels.
/// Excludes charm sockets and cluster expansion sub-sockets, like upstream's
/// ring overlay pass.
pub fn socket_jewels(lua: &Lua) -> Result<Vec<SocketInfo>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.spec
            local tree = spec.tree
            local result = {}
            for nodeId in pairs(tree.sockets) do
                local node = spec.nodes[nodeId]
                if node and node.name ~= "Charm Socket"
                   and (not node.expansionJewel or node.expansionJewel.size == 2) then
                    local socket, jewel = build.itemsTab:GetSocketAndJewelForNodeID(nodeId)
                    local entry = {
                        nodeId = nodeId,
                        allocated = node.alloc == true,
                        hasJewel = jewel ~= nil,
                        title = jewel and (jewel.title or "") or "",
                        radiusIndex = jewel and jewel.jewelRadiusIndex or nil,
                        isVariable = jewel ~= nil and jewel.jewelRadiusLabel == "Variable",
                        keystones = {},
                    }
                    if jewel and jewel.title and jewel.title:match("Impossible Escape")
                       and jewel.jewelData and jewel.jewelData.impossibleEscapeKeystones then
                        for keystoneName in pairs(jewel.jewelData.impossibleEscapeKeystones) do
                            local keystone = tree.keystoneMap[keystoneName]
                            if keystone and keystone.x and keystone.y then
                                table.insert(entry.keystones, { x = keystone.x, y = keystone.y })
                            end
                        end
                    end
                    table.insert(result, entry)
                end
            end
            return result
        "#,
        )
        .eval()?;

    let mut sockets = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        let mut keystone_positions = Vec::new();
        if let Ok(keystones) = entry.get::<LuaTable>("keystones") {
            for pos in keystones.sequence_values::<LuaTable>() {
                let pos = pos?;
                keystone_positions.push((
                    pos.get("x").unwrap_or(0.0f32),
                    pos.get("y").unwrap_or(0.0f32),
                ));
            }
        }
        sockets.push(SocketInfo {
            node_id: entry.get("nodeId")?,
            allocated: entry.get("allocated").unwrap_or(false),
            has_jewel: entry.get("hasJewel").unwrap_or(false),
            jewel_title: entry.get("title").unwrap_or_default(),
            // Lua's jewelRadiusIndex is 1-based
            radius_index: entry
                .get::<Option<usize>>("radiusIndex")
                .ok()
                .flatten()
                .and_then(|i| i.checked_sub(1)),
            is_variable: entry.get("isVariable").unwrap_or(false),
            keystone_positions,
        });
    }
    Ok(sockets)
}

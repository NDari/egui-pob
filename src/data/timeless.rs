//! Timeless jewel search: brute-force seed evaluation over upstream's legion
//! lookup tables (`data.readLUT`), a port of the search core of upstream's
//! Find Timeless Jewel popup. Single-socket search; Militant Faith devotion
//! totals and protected nodes are not modelled yet.

use mlua::prelude::*;

/// (id, display label, internal name prefix) per jewel type.
pub const TIMELESS_JEWEL_TYPES: [(i64, &str, &str); 6] = [
    (1, "Glorious Vanity", "vaal"),
    (2, "Lethal Pride", "karui"),
    (3, "Brutal Restraint", "maraketh"),
    (4, "Militant Faith", "templar"),
    (5, "Elegant Hubris", "eternal"),
    (6, "Heroic Tragedy", "kalguur"),
];

/// Conqueror labels per jewel type (upstream's lists minus "Any").
pub const CONQUERORS: [[&str; 3]; 6] = [
    [
        "Doryani (Corrupted Soul)",
        "Xibaqua (Divine Flesh)",
        "Ahuana (Immortal Ambition)",
    ],
    [
        "Kaom (Strength of Blood)",
        "Rakiata (Tempered by War)",
        "Akoya (Chainbreaker)",
    ],
    [
        "Asenath (Dance with Death)",
        "Nasima (Second Sight)",
        "Balbala (The Traitor)",
    ],
    [
        "Avarius (Power of Purpose)",
        "Dominus (Inner Conviction)",
        "Maxarius (Transcendence)",
    ],
    [
        "Cadiro (Supreme Decadence)",
        "Victario (Supreme Grandstanding)",
        "Caspiro (Supreme Ostentation)",
    ],
    [
        "Vorana (Black Scythe Training)",
        "Uhtred (Celestial Mathematics)",
        "Medved (The Unbreaking Circle)",
    ],
];

/// Jewel line format per type; `{}` takes the seed, then the conqueror.
const JEWEL_LINES: [&str; 6] = [
    "Bathed in the blood of {seed} sacrificed in the name of {conq}",
    "Commanded leadership over {seed} warriors under {conq}",
    "Denoted service of {seed} dekhara in the akhara of {conq}",
    "Carved to glorify {seed} new faithful converted by High Templar {conq}",
    "Commissioned {seed} coins to commemorate {conq}",
    "Struck to commemorate {seed} valorous exiles who spurned {conq}",
];

/// A jewel socket eligible for timeless search.
#[derive(Debug, Clone)]
pub struct TimelessSocket {
    pub node_id: i64,
    pub label: String,
    pub allocated: bool,
}

/// A searchable legion stat (replacement notable or small-node addition).
#[derive(Debug, Clone)]
pub struct TimelessStat {
    pub id: String,
    pub name: String,
    /// True for notable replacements, false for small-node additions.
    pub is_notable: bool,
}

/// One seed search result.
#[derive(Debug, Clone)]
pub struct SeedResult {
    pub seed: i64,
    pub weight: f64,
    /// "3x Name" summaries of the matched desired stats.
    pub matches: Vec<String>,
}

/// Jewel sockets on the tree, labelled by their nearest keystone (upstream's
/// labelling incl. the special-cased ids).
pub fn timeless_sockets(lua: &Lua) -> Result<Vec<TimelessSocket>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local build = mainObject_ref.main.modes['BUILD']
        local treeData = build.spec.tree
        local out = {}
        for socketId, socketData in pairs(build.spec.nodes) do
            if socketData.isJewelSocket and socketData.name ~= "Charm Socket" then
                local keystone = "Unknown"
                if socketId == 26725 then
                    keystone = "Marauder"
                elseif socketId == 54127 then
                    keystone = "Duelist"
                elseif socketId == 7960 then
                    keystone = "Templar/Witch"
                elseif treeData.nodes[socketId] and treeData.nodes[socketId].nodesInRadius then
                    local minDistance = math.huge
                    for _, nodeInRadius in pairs(treeData.nodes[socketId].nodesInRadius[3]) do
                        if nodeInRadius.isKeystone then
                            local distance = math.sqrt((nodeInRadius.x - socketData.x) ^ 2
                                + (nodeInRadius.y - socketData.y) ^ 2)
                            if distance < minDistance then
                                keystone = nodeInRadius.name
                                minDistance = distance
                            end
                        end
                    end
                end
                table.insert(out, {
                    id = socketId,
                    label = keystone .. ": " .. socketId,
                    allocated = build.spec.allocNodes[socketId] ~= nil,
                })
            end
        end
        table.sort(out, function(a, b) return a.label < b.label end)
        return out
    "#,
        )
        .eval()?;

    let mut sockets = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        sockets.push(TimelessSocket {
            node_id: entry.get("id").unwrap_or(0),
            label: entry.get("label").unwrap_or_default(),
            allocated: entry.get("allocated").unwrap_or(false),
        });
    }
    Ok(sockets)
}

/// Searchable legion stats for a jewel type (filtered by id prefix).
pub fn timeless_stats(lua: &Lua, type_name: &str) -> Result<Vec<TimelessStat>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local typeName = ...
        local treeData = mainObject_ref.main.modes['BUILD'].spec.tree
        local out = {}
        local prefix = "^" .. typeName .. "_"
        for _, node in ipairs(treeData.legion.nodes) do
            if node.id:match(prefix) then
                table.insert(out, { id = node.id, name = node.dn, notable = true })
            end
        end
        for _, addition in ipairs(treeData.legion.additions) do
            if addition.id:match(prefix) then
                table.insert(out, { id = addition.id, name = addition.dn, notable = false })
            end
        end
        table.sort(out, function(a, b)
            if a.notable ~= b.notable then
                return a.notable
            end
            return a.name < b.name
        end)
        return out
    "#,
        )
        .call(type_name)?;

    let mut stats = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        stats.push(TimelessStat {
            id: entry.get("id").unwrap_or_default(),
            name: entry.get("name").unwrap_or_default(),
            is_notable: entry.get("notable").unwrap_or(false),
        });
    }
    Ok(stats)
}

/// Brute-force seed search around one socket: score every seed by the
/// weighted desired stats it produces in radius. Port of the popup's search
/// loop (Glorious Vanity's value-weighted replacements/additions and plain
/// presence-weighting for the other types).
pub fn find_timeless_seeds(
    lua: &Lua,
    jewel_type_id: i64,
    socket_node_id: i64,
    desired: &[(String, f64, f64)],
    limit: usize,
) -> Result<Vec<SeedResult>, mlua::Error> {
    let desired_table = lua.create_table()?;
    for (i, (id, w1, w2)) in desired.iter().enumerate() {
        let entry = lua.create_table()?;
        entry.set("id", id.as_str())?;
        entry.set("w", *w1)?;
        entry.set("w2", *w2)?;
        desired_table.set(i + 1, entry)?;
    }
    let list: LuaTable = lua
        .load(
            r#"
        local jewelTypeId, socketId, desired, limit = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeData = build.spec.tree
        local legionNodes = treeData.legion.nodes
        local legionAdditions = treeData.legion.additions
        local out = {}
        local socketNode = treeData.nodes[socketId]
        if not socketNode or not socketNode.nodesInRadius then
            return out
        end
        local desiredNodes = {}
        for _, d in ipairs(desired) do
            desiredNodes[d.id] = { nodeWeight = d.w, nodeWeight2 = d.w2 or 0 }
        end
        local rootNodes = {}
        for _, class in pairs(treeData.classes) do
            rootNodes[class.startNodeId] = true
        end
        local targetNodes = {}
        for nodeId in pairs(socketNode.nodesInRadius[3]) do
            local node = treeData.nodes[nodeId]
            if node and not rootNodes[nodeId] and not node.isJewelSocket
               and not node.isKeystone
               and (node.isNotable or jewelTypeId == 1) then
                targetNodes[nodeId] = true
            end
        end
        local seedMultiplier = jewelTypeId == 5 and 20 or 1
        local seedMin = data.timelessJewelSeedMin[jewelTypeId] * seedMultiplier
        local seedMax = data.timelessJewelSeedMax[jewelTypeId] * seedMultiplier
        local results = {}
        for curSeed = seedMin, seedMax, seedMultiplier do
            local weight = 0
            local matches = {}
            for targetNode in pairs(targetNodes) do
                local tbl = data.readLUT(curSeed, targetNode, jewelTypeId)
                if next(tbl) then
                    local curNode
                    if tbl[1] >= data.timelessJewelAdditions then
                        curNode = legionNodes[tbl[1] + 1 - data.timelessJewelAdditions]
                    else
                        curNode = legionAdditions[tbl[1] + 1]
                    end
                    local curNodeId = curNode and curNode.id or nil
                    if jewelTypeId == 1 then
                        local headerSize = #tbl
                        if headerSize == 2 or headerSize == 3 then
                            local d = curNodeId and desiredNodes[curNodeId]
                            if d then
                                local statMod1 = curNode.stats[curNode.sortedStats[1]]
                                local w = d.nodeWeight * tbl[statMod1.index + 1]
                                local statMod2 = curNode.stats[curNode.sortedStats[2]]
                                if statMod2 then
                                    w = w + d.nodeWeight2 * tbl[statMod2.index + 1]
                                end
                                weight = weight + w
                                matches[curNode.dn] = (matches[curNode.dn] or 0) + 1
                            end
                        elseif headerSize == 6 or headerSize == 8 then
                            for i = 1, headerSize / 2 do
                                local addNode = legionAdditions[tbl[i] + 1]
                                local addId = addNode and addNode.id
                                local d = addId and desiredNodes[addId]
                                if d then
                                    weight = weight + d.nodeWeight * tbl[i + headerSize / 2]
                                    matches[addNode.dn] = (matches[addNode.dn] or 0) + 1
                                end
                            end
                        end
                    else
                        local d = curNodeId and desiredNodes[curNodeId]
                        if d then
                            weight = weight + d.nodeWeight
                            matches[curNode.dn] = (matches[curNode.dn] or 0) + 1
                        end
                    end
                end
            end
            if weight > 0 then
                table.insert(results, { seed = curSeed, weight = weight, matches = matches })
            end
        end
        table.sort(results, function(a, b) return a.weight > b.weight end)
        for i = 1, math.min(#results, limit) do
            local r = results[i]
            local matchList = {}
            for name, count in pairs(r.matches) do
                table.insert(matchList, count .. "x " .. name)
            end
            table.sort(matchList)
            table.insert(out, { seed = r.seed, weight = r.weight, matches = matchList })
        end
        return out
    "#,
        )
        .call((jewel_type_id, socket_node_id, desired_table, limit))?;

    let mut results = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let matches: LuaTable = entry.get("matches")?;
        results.push(SeedResult {
            seed: entry.get("seed").unwrap_or(0),
            weight: entry.get("weight").unwrap_or(0.0),
            matches: matches.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(results)
}

/// Create a timeless jewel item for a search result and add it to the build.
pub fn create_timeless_jewel(
    lua: &Lua,
    jewel_type_id: i64,
    conqueror_index: usize,
    seed: i64,
) -> Result<Option<String>, mlua::Error> {
    let type_entry = TIMELESS_JEWEL_TYPES
        .iter()
        .find(|(id, _, _)| *id == jewel_type_id);
    let Some((_, label, _)) = type_entry else {
        return Ok(Some("Unknown jewel type".to_string()));
    };
    let conqueror_full = CONQUERORS[(jewel_type_id - 1) as usize]
        .get(conqueror_index)
        .copied()
        .unwrap_or(CONQUERORS[(jewel_type_id - 1) as usize][0]);
    // "Doryani (Corrupted Soul)" -> "Doryani"
    let conqueror = conqueror_full
        .split(" (")
        .next()
        .unwrap_or(conqueror_full)
        .to_string();
    let line = JEWEL_LINES[(jewel_type_id - 1) as usize]
        .replace("{seed}", &seed.to_string())
        .replace("{conq}", &conqueror);
    let raw = format!("Rarity: UNIQUE\n{label}\nTimeless Jewel\nLimited to: 1 Historic\n{line}");
    super::items::add_item_from_raw(lua, &raw)
}

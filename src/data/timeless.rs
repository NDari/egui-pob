//! Timeless jewel search: brute-force seed evaluation over upstream's legion
//! lookup tables (`data.readLUT`), a port of the search core of upstream's
//! Find Timeless Jewel popup. Supports single-socket and all-socket search,
//! the "Total Strength/Dexterity/Devotion" pseudo-stats, and generated
//! fallback weights. Protected nodes and the socket allocation filter are
//! not modelled yet.

use mlua::prelude::*;

/// Sentinel socket id for the multi-socket "All Sockets" search entry.
pub const ALL_SOCKETS_ID: i64 = -1;

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
    /// Socket the result was found at (set by the "All Sockets" search).
    pub socket_id: Option<i64>,
}

/// A generated fallback weight row (legion stat id, display name, weights).
#[derive(Debug, Clone)]
pub struct FallbackWeight {
    pub id: String,
    pub name: String,
    pub weight1: f64,
    pub weight2: f64,
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

    let mut sockets = vec![TimelessSocket {
        node_id: ALL_SOCKETS_ID,
        label: "All Sockets".to_string(),
        allocated: false,
    }];
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

/// Searchable legion stats for a jewel type. Follows upstream's mod list:
/// replacement notables and additions (sorted together), small replacements
/// at the end, a "Total <attribute>" pseudo-stat at the top for Lethal
/// Pride/Brutal Restraint/Militant Faith, and upstream's ignored-mod filter
/// (stats already covered by the totals, keystones, vaal additions).
pub fn timeless_stats(lua: &Lua, type_name: &str) -> Result<Vec<TimelessStat>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local typeName = ...
        local treeData = mainObject_ref.main.modes['BUILD'].spec.tree
        local ignored = {
            ["Might of the Vaal"] = true, ["Legacy of the Vaal"] = true,
            ["Strength"] = true, ["Add Strength"] = true, ["Dex"] = true,
            ["Add Dexterity"] = true, ["Devotion"] = true,
            ["Price of Glory"] = true, ["Ward"] = true,
        }
        local out = {}
        local smalls = {}
        local prefix = "^" .. typeName .. "_"
        for _, node in ipairs(treeData.legion.nodes) do
            -- v2.67: abyss ascendancy notables are excluded from the stat list
            if node.id:match(prefix)
               and not node.id:match("^abyss_special_ascendancy_notable_")
               and not ignored[node.dn] and not node.ks then
                if node["not"] then
                    table.insert(out, { id = node.id, name = node.dn, notable = true })
                else
                    table.insert(smalls, { id = node.id, name = node.dn, notable = false })
                end
            end
        end
        if typeName ~= "vaal" then
            for _, addition in ipairs(treeData.legion.additions) do
                if addition.id:match(prefix) and not ignored[addition.dn] then
                    table.insert(out, { id = addition.id, name = addition.dn, notable = true })
                end
            end
        end
        table.sort(out, function(a, b) return a.name < b.name end)
        table.sort(smalls, function(a, b) return a.name < b.name end)
        local totalNames = { karui = "Strength", maraketh = "Dexterity", templar = "Devotion" }
        if totalNames[typeName] then
            table.insert(out, 1, {
                id = "total_" .. totalNames[typeName]:lower(),
                name = "Total " .. totalNames[typeName],
                notable = true,
            })
        end
        for _, s in ipairs(smalls) do
            table.insert(out, s)
        end
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

/// Brute-force seed search: score every seed by the weighted desired stats
/// it produces in radius. Port of the popup's search loop (Glorious Vanity's
/// value-weighted replacements/additions, plain presence-weighting for the
/// other types, and the "Total <attribute>" pseudo-stat with upstream's
/// small-node bonus formulas). Pass [`ALL_SOCKETS_ID`] to search every jewel
/// socket; results then carry the socket they were found at. `fallback` rows
/// are merged in for ids not already in `desired` (upstream's fallback list).
pub fn find_timeless_seeds(
    lua: &Lua,
    jewel_type_id: i64,
    socket_node_id: i64,
    desired: &[(String, f64, f64)],
    fallback: &[(String, f64, f64)],
    limit: usize,
) -> Result<Vec<SeedResult>, mlua::Error> {
    let desired_table = lua.create_table()?;
    let mut idx = 0;
    for (id, w1, w2) in desired.iter().chain(
        fallback
            .iter()
            .filter(|(id, _, _)| !desired.iter().any(|(d, _, _)| d == id)),
    ) {
        let entry = lua.create_table()?;
        entry.set("id", id.as_str())?;
        entry.set("w", *w1)?;
        entry.set("w2", *w2)?;
        idx += 1;
        desired_table.set(idx, entry)?;
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

        local totalMods = { [2] = "Strength", [3] = "Dexterity", [4] = "Devotion" }
        local reverseTotalModIDs = {
            karui_notable_add_strength = true,
            karui_attribute_strength = true,
            karui_small_strength = true,
            maraketh_notable_add_dexterity = true,
            maraketh_attribute_dex = true,
            maraketh_small_dex = true,
            templar_notable_devotion = true,
            templar_devotion_node = true,
            templar_small_devotion = true,
        }
        local totalStatName = totalMods[jewelTypeId]

        local desiredNodes = {}
        for _, d in ipairs(desired) do
            local key = d.id
            if totalStatName and d.id == "total_" .. totalStatName:lower() then
                key = "totalStat"
            end
            desiredNodes[key] = { nodeWeight = d.w, nodeWeight2 = d.w2 or 0 }
        end
        local rootNodes = {}
        for _, class in pairs(treeData.classes) do
            rootNodes[class.startNodeId] = true
        end

        local function searchSocket(socketNodeId)
            local socketNode = treeData.nodes[socketNodeId]
            if not socketNode or not socketNode.nodesInRadius then
                return {}
            end
            local targetNodes = {}
            local attributeSmalls, otherSmalls = 0, 0
            for nodeId in pairs(socketNode.nodesInRadius[3]) do
                local node = treeData.nodes[nodeId]
                if node and not rootNodes[nodeId] and not node.isJewelSocket
                   and not node.isKeystone then
                    if node.isNotable or jewelTypeId == 1 then
                        targetNodes[nodeId] = true
                    elseif desiredNodes["totalStat"] then
                        if node.dn == "Strength" or node.dn == "Intelligence"
                           or node.dn == "Dexterity" then
                            attributeSmalls = attributeSmalls + 1
                        else
                            otherSmalls = otherSmalls + 1
                        end
                    end
                end
            end
            local seedMultiplier = jewelTypeId == 5 and 20 or 1
            local seedMin = data.timelessJewelSeedMin[jewelTypeId] * seedMultiplier
            local seedMax = data.timelessJewelSeedMax[jewelTypeId] * seedMultiplier
            local results = {}
            for curSeed = seedMin, seedMax, seedMultiplier do
                local weight = 0
                local totalStatWeight = 0
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
                        if desiredNodes["totalStat"] and curNodeId
                           and reverseTotalModIDs[curNodeId] then
                            curNodeId = "totalStat"
                        end
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
                        elseif curNodeId == "totalStat" then
                            totalStatWeight = totalStatWeight
                                + desiredNodes["totalStat"].nodeWeight
                            local name = "Total " .. totalStatName
                            matches[name] = (matches[name] or 0) + 1
                        else
                            local d = curNodeId and desiredNodes[curNodeId]
                            if d then
                                weight = weight + d.nodeWeight
                                matches[curNode.dn] = (matches[curNode.dn] or 0) + 1
                            end
                        end
                    end
                end
                if desiredNodes["totalStat"] then
                    -- Upstream's small-node bonus: smalls in radius always
                    -- contribute to the total, matched notables count 5x
                    -- (Militant Faith) or 20x (attribute totals).
                    local d = desiredNodes["totalStat"]
                    local added
                    if jewelTypeId == 4 then
                        added = d.nodeWeight * (5 * otherSmalls + 10 * attributeSmalls)
                            + totalStatWeight * 4
                    else
                        added = d.nodeWeight * (4 * otherSmalls + 2 * attributeSmalls)
                            + totalStatWeight * 19
                    end
                    weight = weight + totalStatWeight + added
                end
                if weight > 0 then
                    table.insert(results, {
                        seed = curSeed, weight = weight, matches = matches,
                    })
                end
            end
            return results
        end

        local merged
        if socketId == -1 then
            merged = {}
            for sid, socketData in pairs(build.spec.nodes) do
                if socketData.isJewelSocket and socketData.name ~= "Charm Socket" then
                    for _, r in ipairs(searchSocket(sid)) do
                        r.socketId = sid
                        table.insert(merged, r)
                    end
                end
            end
        else
            merged = searchSocket(socketId)
        end
        table.sort(merged, function(a, b) return a.weight > b.weight end)
        for i = 1, math.min(#merged, limit) do
            local r = merged[i]
            local matchList = {}
            for name, count in pairs(r.matches) do
                table.insert(matchList, count .. "x " .. name)
            end
            table.sort(matchList)
            table.insert(out, {
                seed = r.seed, weight = r.weight, matches = matchList,
                socketId = r.socketId,
            })
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
            socket_id: entry.get::<Option<i64>>("socketId").unwrap_or(None),
        });
    }
    Ok(results)
}

/// Power stats usable for fallback weight generation (upstream's filter:
/// `data.powerStatList` entries not flagged `ignoreForItems`, minus "Name").
/// `index` is 1-based into the unfiltered upstream list.
pub fn list_fallback_stats(lua: &Lua) -> Result<Vec<super::node_power::PowerStat>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local out = {}
        for i, stat in ipairs(data.powerStatList) do
            if not stat.ignoreForItems and stat.label ~= "Name" then
                table.insert(out, { index = i, label = stat.label })
            end
        end
        return out
    "#,
        )
        .eval()?;

    let mut stats = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        stats.push(super::node_power::PowerStat {
            index: entry.get("index").unwrap_or(0),
            label: entry.get("label").unwrap_or_default(),
        });
    }
    Ok(stats)
}

/// Generate fallback weights for the given legion stat ids: one throwaway
/// calc pass per stat (via upstream's `GetMiscCalculator` with `addNodes`),
/// weighted by the relative change in the selected power stat. Port of the
/// popup's `setupFallbackWeights`/`generateFallbackWeights`. `stat_index` is
/// 1-based into `data.powerStatList`. Rows with zero weight are dropped.
pub fn generate_fallback_weights(
    lua: &Lua,
    ids: &[String],
    stat_index: usize,
) -> Result<Vec<FallbackWeight>, mlua::Error> {
    let ids_table = lua.create_table()?;
    for (i, id) in ids.iter().enumerate() {
        ids_table.set(i + 1, id.as_str())?;
    }
    let list: LuaTable = lua
        .load(
            r#"
        local ids, statIndex = ...
        local build = mainObject_ref.main.modes['BUILD']
        local treeData = build.spec.tree
        local legionNodes = treeData.legion.nodes
        local legionAdditions = treeData.legion.additions
        local selection = data.powerStatList[statIndex]
        local out = {}
        if not selection then
            return out
        end

        local totalModIDs = {
            total_strength = {
                karui_notable_add_strength = true,
                karui_attribute_strength = true,
                karui_small_strength = true,
            },
            total_dexterity = {
                maraketh_notable_add_dexterity = true,
                maraketh_attribute_dex = true,
                maraketh_small_dex = true,
            },
            total_devotion = {
                templar_notable_devotion = true,
                templar_devotion_node = true,
                templar_small_devotion = true,
            },
        }

        -- Upstream's replaceHelperFunc: substitute a stat value into the
        -- (min-max) range of a legion stat description line.
        local function replaceHelper(statToFix, statKey, statMod, value)
            if statMod.fmt == "g" then
                if statKey:find("per_minute") then
                    value = math.floor(value / 60 * 10 + 0.5) / 10
                elseif statKey:find("permyriad") then
                    value = value / 100
                elseif statKey:find("_ms") then
                    value = value / 1000
                end
            end
            if statMod.min ~= statMod.max then
                return statToFix:gsub("%(" .. statMod.min .. "%-" .. statMod.max .. "%)", value)
            elseif statMod.min ~= value then
                return statToFix:gsub(statMod.min, value)
            end
            return statToFix
        end

        -- Upstream v2.67's buildStatModLists: give each stat its own mod list
        -- even when several stats share a single display line, by describing
        -- one stat at a time at a value of 100.
        local function buildStatModLists(legionPassive)
            local modLists = {}
            for statIndex, statKey in ipairs(legionPassive.sortedStats) do
                local statValues = {}
                for key in pairs(legionPassive.stats) do
                    statValues[key] = key == statKey and 100 or 0
                end
                local line = data.describeStats(statValues, "stat_descriptions")[1]
                modLists[statIndex] = { modList = modLib.parseMod(line), divisor = 100 }
            end
            return modLists
        end

        local nodes = {}
        for _, wantedId in ipairs(ids) do
            local newNode = nil
            local isVaal = wantedId:match("^vaal_") ~= nil
            for _, legionNode in ipairs(legionNodes) do
                if legionNode.id == wantedId
                   or (totalModIDs[wantedId] and totalModIDs[wantedId][legionNode.id]) then
                    newNode = { id = wantedId, name = legionNode.dn }
                    if legionNode.id:match("^vaal_") then
                        -- v2.67: split on the number of stats rather than the
                        -- number of display lines, since several stats can
                        -- share one line and one stat can span several
                        if #legionNode.sortedStats > 1 then
                            newNode.calcMultiple = true
                            if legionNode.modListGenerated then
                                newNode.node = copyTable(legionNode.modListGenerated)
                            else
                                local modLists = buildStatModLists(legionNode)
                                legionNode.modListGenerated = copyTable(modLists)
                                newNode.node = copyTable(modLists)
                            end
                            for _, node in ipairs(newNode.node) do
                                node.id = legionNode.id
                            end
                        else
                            local originalLine = legionNode.sd[1]
                            local line = replaceHelper(
                                originalLine, legionNode.sortedStats[1],
                                legionNode.stats[legionNode.sortedStats[1]], 100)
                            if line == originalLine and #legionNode.sd > 1 then
                                -- Some fixed game stats span several display
                                -- lines; score the whole effect together.
                                newNode.modList = legionNode.modList
                            elseif legionNode.modListGenerated then
                                newNode.modList = copyTable(legionNode.modListGenerated)
                            else
                                local modList = modLib.parseMod(line)
                                legionNode.modListGenerated = modList
                                newNode.modList = modList
                            end
                            -- Only a substituted line was scaled by 100
                            newNode.divisor = line ~= originalLine and 100 or 1
                        end
                    else
                        newNode.modList = legionNode.modList
                        if totalModIDs[wantedId] then
                            newNode.name = wantedId:gsub("^total_(%a+)", function(s)
                                return "Total " .. s:gsub("^%l", string.upper)
                            end)
                            newNode.divisor = legionNode.modList[1].value
                        end
                    end
                    break
                end
            end
            if not newNode then
                for _, legionAddition in ipairs(legionAdditions) do
                    if legionAddition.id == wantedId
                       or (totalModIDs[wantedId] and totalModIDs[wantedId][legionAddition.id]) then
                        newNode = { id = wantedId, name = legionAddition.dn }
                        -- v2.67: multi-stat additions score each stat separately
                        if isVaal and #legionAddition.sortedStats > 1 then
                            newNode.calcMultiple = true
                            if legionAddition.modListGenerated then
                                newNode.node = copyTable(legionAddition.modListGenerated)
                            else
                                local modLists = buildStatModLists(legionAddition)
                                legionAddition.modListGenerated = copyTable(modLists)
                                newNode.node = copyTable(modLists)
                            end
                            for _, node in ipairs(newNode.node) do
                                node.id = legionAddition.id
                            end
                        elseif legionAddition.modList then
                            newNode.modList = legionAddition.modList
                        elseif legionAddition.modListGenerated then
                            newNode.modList = legionAddition.modListGenerated
                        else
                            local originalLine = legionAddition.sd[1]
                            local line = originalLine
                            if isVaal then
                                for key, stat in pairs(legionAddition.stats) do
                                    line = replaceHelper(line, key, stat, 100)
                                end
                            end
                            local modList = modLib.parseMod(line)
                            legionAddition.modListGenerated = modList
                            newNode.modList = modList
                            -- v2.67: only a substituted line was scaled by 100
                            newNode.divisor = line ~= originalLine and 100 or 1
                        end
                        if not isVaal and totalModIDs[wantedId] then
                            newNode.divisor = newNode.modList[1].value
                        end
                        break
                    end
                end
            end
            if newNode then
                table.insert(nodes, newNode)
            end
        end

        -- v2.66+: stat values resolve through powerStatList.GetFromOutput
        -- (handles Minion-prefixed stats and combined damage), and weights
        -- are the gain normalized by the absolute base power
        local calcFunc, calcBase = build.calcsTab:GetMiscCalculator(build)
        local basePower = data.powerStatList.GetFromOutput(calcBase, selection)

        local function statGain(addNode)
            if basePower == 0 then
                return 0
            end
            local nodeOutput = calcFunc({ addNodes = { [addNode] = true } })
            local nodePower = data.powerStatList.GetFromOutput(nodeOutput, selection)
            return (nodePower - basePower) / math.abs(basePower)
        end

        local weightScalar = 100
        local function roundW(v)
            return math.floor(v * weightScalar * 1000 + 0.5) / 1000
        end
        for _, newNode in ipairs(nodes) do
            -- v2.67: a multi-line node's individual lines may carry their own
            -- divisor, which takes precedence over the entry's
            -- (`node.divisor or newNode.divisor or 1`)
            local divisor = newNode.divisor or 1
            local weight1, weight2
            if newNode.calcMultiple then
                weight1 = statGain(newNode.node[1]) / (newNode.node[1].divisor or divisor)
                weight2 = statGain(newNode.node[2]) / (newNode.node[2].divisor or divisor)
            else
                weight1 = statGain(newNode) / divisor
            end
            if weight1 ~= 0 or (weight2 and weight2 ~= 0) then
                table.insert(out, {
                    id = newNode.id,
                    name = newNode.name,
                    w1 = roundW(weight1),
                    w2 = roundW(weight2 or 0),
                })
            end
        end
        return out
    "#,
        )
        .call((ids_table, stat_index))?;

    let mut weights = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        weights.push(FallbackWeight {
            id: entry.get("id").unwrap_or_default(),
            name: entry.get("name").unwrap_or_default(),
            weight1: entry.get("w1").unwrap_or(0.0),
            weight2: entry.get("w2").unwrap_or(0.0),
        });
    }
    Ok(weights)
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

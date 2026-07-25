//! Tattoos (and runegrafts): node modifier replacement via upstream's
//! hashOverrides + ReplaceNode, mirroring TreeTab:ModifyNodePopup.

use mlua::prelude::*;

/// A tattoo that can be applied to the node.
#[derive(Debug, Clone)]
pub struct TattooOption {
    /// Key into `tree.tattoo.nodes`.
    pub id: String,
    pub name: String,
    pub descriptions: Vec<String>,
}

/// Tattoos applicable to a node (empty = node cannot be modified). Mirrors
/// upstream's buildMods filter: target type/value matching, minimum linked
/// nodes, and the legacy toggle.
pub fn tattoo_options(
    lua: &Lua,
    node_id: u32,
    show_legacy: bool,
) -> Result<Vec<TattooOption>, mlua::Error> {
    let list: LuaTable = lua
        .load(
            r#"
        local nodeId, showLegacy = ...
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local treeNodes = spec.tree.nodes
        local out = {}
        local selected = treeNodes[nodeId]
        if not selected or not spec.tree.tattoo then
            return out
        end
        local nodeName = selected.dn
        local nodeValue = selected.sd and selected.sd[1] or ""
        local numLinkedNodes = selected.linkedId and #selected.linkedId or 0
        for id, node in pairs(spec.tree.tattoo.nodes) do
            if (nodeName:match(node.targetType:gsub("^Small ", ""))
                or (node.targetValue ~= "" and nodeValue:match(node.targetValue))
                or (node.targetType == "Small Attribute"
                    and (nodeName == "Intelligence" or nodeName == "Strength" or nodeName == "Dexterity"))
                or (node.targetType == "Keystone" and selected.type == node.targetType))
               and node.MinimumConnected <= numLinkedNodes
               and ((node.legacy == nil or node.legacy == false) or node.legacy == showLegacy) then
                local descriptions = {}
                for _, line in ipairs(node.sd or {}) do
                    table.insert(descriptions, line)
                end
                if node.reminderText and node.reminderText[1] then
                    table.insert(descriptions, node.reminderText[1])
                end
                table.insert(out, {
                    id = id,
                    name = node.dn,
                    descriptions = descriptions,
                })
            end
        end
        table.sort(out, function(a, b) return a.name < b.name end)
        return out
    "#,
        )
        .call((node_id, show_legacy))?;

    let mut options = Vec::new();
    for entry in list.sequence_values::<LuaTable>() {
        let entry = entry?;
        let descriptions: LuaTable = entry.get("descriptions")?;
        options.push(TattooOption {
            id: entry.get("id").unwrap_or_default(),
            name: entry.get("name").unwrap_or_default(),
            descriptions: descriptions.sequence_values::<String>().flatten().collect(),
        });
    }
    Ok(options)
}

/// Replace the node's modifiers with a tattoo (upstream's addModifier).
pub fn apply_tattoo(lua: &Lua, node_id: u32, tattoo_id: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local nodeId, tattooId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local tattoo = spec.tree.tattoo.nodes[tattooId]
        local node = spec.nodes[nodeId]
        if not tattoo or not node then
            return
        end
        local newNode = copyTable(tattoo, true)
        newNode.id = nodeId
        spec.hashOverrides[nodeId] = newNode
        spec:ReplaceNode(node, newNode)
        spec:BuildAllDependsAndPaths()
        spec:AddUndoState()
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call((node_id, tattoo_id))
}

/// Restore a tattooed node to its original modifiers.
pub fn remove_tattoo(lua: &Lua, node_id: u32) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local nodeId = ...
        local build = mainObject_ref.main.modes['BUILD']
        local spec = build.spec
        local node = spec.nodes[nodeId]
        if not node then
            return
        end
        spec.tree.nodes[nodeId].isTattoo = false
        spec.hashOverrides[nodeId] = nil
        spec:ReplaceNode(node, spec.tree.nodes[nodeId])
        spec:BuildAllDependsAndPaths()
        spec:AddUndoState()
        build.modFlag = true
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .call(node_id)
}

/// True when the node currently has an override (tattoo/runegraft).
pub fn is_tattooed(lua: &Lua, node_id: u32) -> Result<bool, mlua::Error> {
    lua.load(
        r#"
        local nodeId = ...
        local spec = mainObject_ref.main.modes['BUILD'].spec
        return spec.hashOverrides[nodeId] ~= nil
    "#,
    )
    .call(node_id)
}

/// Count of applied tattoos (runegrafts excluded), max 50 in game.
pub fn tattoo_count(lua: &Lua) -> Result<i64, mlua::Error> {
    lua.load(
        r#"
        local spec = mainObject_ref.main.modes['BUILD'].spec
        local count = 0
        for _, node in pairs(spec.hashOverrides) do
            if node.isTattoo and not node.dn:find("Runegraft") then
                count = count + 1
            end
        end
        return count
    "#,
    )
    .eval()
}

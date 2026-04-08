//! Skills data: socket groups and gems extracted from Lua's build.skillsTab.

use mlua::prelude::*;

/// A socket group containing linked gems.
#[derive(Debug, Clone)]
pub struct SocketGroup {
    pub index: usize,
    pub label: String,
    pub slot: Option<String>,
    pub enabled: bool,
    pub is_main: bool,
    pub gems: Vec<GemInfo>,
}

/// A gem within a socket group.
#[derive(Debug, Clone)]
pub struct GemInfo {
    pub name: String,
    pub level: i64,
    pub quality: i64,
    pub enabled: bool,
    pub is_support: bool,
}

/// Extract all socket groups and the main skill index from the loaded build.
pub fn extract_skills(lua: &Lua) -> Result<Vec<SocketGroup>, mlua::Error> {
    let result: LuaTable = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local skillsTab = build.skillsTab
            local mainGroup = build.mainSocketGroup or 1
            local result = {}
            for i, group in ipairs(skillsTab.socketGroupList) do
                local entry = {
                    index = i,
                    label = group.label or "",
                    slot = group.slot,
                    enabled = group.enabled ~= false,
                    isMain = (i == mainGroup),
                    gems = {}
                }
                if group.gemList then
                    for _, gem in ipairs(group.gemList) do
                        local gemEntry = {
                            name = gem.nameSpec or "",
                            level = gem.level or 1,
                            quality = gem.quality or 0,
                            enabled = gem.enabled ~= false,
                            isSupport = false
                        }
                        if gem.gemData and gem.gemData.tags then
                            gemEntry.isSupport = gem.gemData.tags.support == true
                        end
                        table.insert(entry.gems, gemEntry)
                    end
                end
                table.insert(result, entry)
            end
            return result
        "#,
        )
        .eval()?;

    let mut groups = Vec::new();
    for pair in result.sequence_values::<LuaTable>() {
        let entry = pair?;
        let gems_table: LuaTable = entry.get("gems")?;
        let mut gems = Vec::new();
        for gem_pair in gems_table.sequence_values::<LuaTable>() {
            let gem = gem_pair?;
            gems.push(GemInfo {
                name: gem.get("name").unwrap_or_default(),
                level: gem.get("level").unwrap_or(1),
                quality: gem.get("quality").unwrap_or(0),
                enabled: gem.get("enabled").unwrap_or(true),
                is_support: gem.get("isSupport").unwrap_or(false),
            });
        }
        groups.push(SocketGroup {
            index: entry.get("index")?,
            label: entry.get("label").unwrap_or_default(),
            slot: entry.get("slot").ok(),
            enabled: entry.get("enabled").unwrap_or(true),
            is_main: entry.get("isMain").unwrap_or(false),
            gems,
        });
    }

    Ok(groups)
}

/// Set the main socket group index in Lua and trigger recalc.
pub fn set_main_socket_group(lua: &Lua, index: usize) -> Result<(), mlua::Error> {
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.mainSocketGroup = {index}
        build.buildFlag = true
        _runCallback('OnFrame')
    "#
    ))
    .exec()
}

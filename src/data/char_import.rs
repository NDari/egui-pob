//! Character import from a Path of Exile account, using the official
//! character-window API. HTTP happens in Rust; the JSON responses are handed
//! to upstream's ImportTab (ImportPassiveTreeAndJewels / ImportItemsAndSkills)
//! so all parsing and build mutation stays upstream.

use mlua::prelude::*;

/// A realm (platform) the account can be on.
pub struct Realm {
    pub label: &'static str,
    pub code: &'static str,
}

/// Realm list matching upstream's ImportTab.
pub const REALMS: &[Realm] = &[
    Realm {
        label: "PC",
        code: "pc",
    },
    Realm {
        label: "Xbox",
        code: "xbox",
    },
    Realm {
        label: "PS4",
        code: "sony",
    },
];

const HOST: &str = "https://www.pathofexile.com/";
const USER_AGENT: &str = "PathOfBuildingCommunity (egui-pob)";

/// A character from the account's character list.
#[derive(Debug, Clone)]
pub struct CharacterInfo {
    pub name: String,
    pub league: String,
    pub class: String,
    pub level: i64,
}

/// Percent-encode a query-string value.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// GET a character-window API endpoint. `sessid` is the POESESSID cookie for
/// private profiles; pass an empty string for public profiles.
fn fetch(path_and_query: &str, sessid: &str) -> anyhow::Result<String> {
    let url = format!("{HOST}{path_and_query}");
    log::info!("Fetching {url}");
    let mut request = reqwest::blocking::Client::new()
        .get(&url)
        .header("User-Agent", USER_AGENT);
    if !sessid.is_empty() {
        request = request.header("Cookie", format!("POESESSID={}", sessid.trim()));
    }
    let response = request
        .send()
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?;

    let status = response.status();
    let body = response
        .text()
        .map_err(|e| anyhow::anyhow!("Failed to read response: {e}"))?;

    if !status.is_success() {
        if status.as_u16() == 403 {
            anyhow::bail!(
                "HTTP 403 - profile is private. Set a POESESSID, or make the \
                 profile's characters public in privacy settings."
            );
        }
        if status.as_u16() == 404 {
            anyhow::bail!("HTTP 404 - account not found. Use the full name, e.g. Name#1234.");
        }
        anyhow::bail!("HTTP {status} from {url}");
    }
    Ok(body)
}

/// Download the account's character list.
pub fn fetch_character_list(
    account: &str,
    realm_code: &str,
    sessid: &str,
) -> anyhow::Result<String> {
    fetch(
        &format!(
            "character-window/get-characters?accountName={}&realm={}",
            url_encode(account.trim()),
            realm_code
        ),
        sessid,
    )
}

/// Download a character's passive tree JSON.
pub fn fetch_passive_tree(
    account: &str,
    character: &str,
    realm_code: &str,
    sessid: &str,
) -> anyhow::Result<String> {
    fetch(
        &format!(
            "character-window/get-passive-skills?accountName={}&character={}&realm={}",
            url_encode(account.trim()),
            url_encode(character),
            realm_code
        ),
        sessid,
    )
}

/// Download a character's items JSON.
pub fn fetch_items(
    account: &str,
    character: &str,
    realm_code: &str,
    sessid: &str,
) -> anyhow::Result<String> {
    fetch(
        &format!(
            "character-window/get-items?accountName={}&character={}&realm={}",
            url_encode(account.trim()),
            url_encode(character),
            realm_code
        ),
        sessid,
    )
}

/// Parse a get-characters response with upstream's JSON handling.
/// Returns Err with the API's error message when the response is an error
/// object (private profile, bad account name).
pub fn parse_character_list(lua: &Lua, json: &str) -> anyhow::Result<Vec<CharacterInfo>> {
    let result: LuaTable = lua
        .load(
            r#"
            local json = ...
            local build = mainObject_ref.main.modes['BUILD']
            local data, errMsg = build.importTab:ProcessJSON(json)
            if not data then
                return { err = "Invalid response: " .. tostring(errMsg) }
            end
            if data.error then
                local msg = data.error
                if type(msg) == "table" then
                    msg = msg.message or "unknown error"
                end
                return { err = tostring(msg) }
            end
            local list = {}
            for _, char in ipairs(data) do
                table.insert(list, {
                    name = char.name or "",
                    league = char.league or "",
                    class = char.class or "",
                    level = char.level or 1,
                })
            end
            return { chars = list }
        "#,
        )
        .call(json)
        .map_err(|e| anyhow::anyhow!("Failed to parse character list: {e}"))?;

    if let Ok(err) = result.get::<String>("err") {
        anyhow::bail!("{err}");
    }

    let chars_table: LuaTable = result
        .get("chars")
        .map_err(|e| anyhow::anyhow!("Bad character list: {e}"))?;
    let mut chars = Vec::new();
    for pair in chars_table.sequence_values::<LuaTable>() {
        let c = pair.map_err(|e| anyhow::anyhow!("Bad character entry: {e}"))?;
        chars.push(CharacterInfo {
            name: c.get("name").unwrap_or_default(),
            league: c.get("league").unwrap_or_default(),
            class: c.get("class").unwrap_or_default(),
            level: c.get("level").unwrap_or(1),
        });
    }
    Ok(chars)
}

/// Import a character's passive tree and jewels via upstream's
/// ImportPassiveTreeAndJewels. Returns the (colour-coded) status message.
pub fn import_passive_tree_and_jewels(
    lua: &Lua,
    json: &str,
    character: &CharacterInfo,
    clear_jewels: bool,
) -> Result<String, mlua::Error> {
    lua.load(
        r#"
        local json, name, league, class, level, clearJewels = ...
        local build = mainObject_ref.main.modes['BUILD']
        local importTab = build.importTab
        importTab.controls.charImportTreeClearJewels.state = clearJewels
        local charData = { name = name, league = league, class = class, level = level }
        importTab:ImportPassiveTreeAndJewels(json, charData)
        build.buildFlag = true
        _runCallback('OnFrame')
        return importTab.charImportStatus or ""
    "#,
    )
    .call((
        json,
        character.name.as_str(),
        character.league.as_str(),
        character.class.as_str(),
        character.level,
        clear_jewels,
    ))
}

/// Import a character's items and skills via upstream's ImportItemsAndSkills.
/// Returns the (colour-coded) status message.
pub fn import_items_and_skills(
    lua: &Lua,
    json: &str,
    clear_items: bool,
    clear_skills: bool,
    ignore_weapon_swap: bool,
) -> Result<String, mlua::Error> {
    lua.load(
        r#"
        local json, clearItems, clearSkills, ignoreSwap = ...
        local build = mainObject_ref.main.modes['BUILD']
        local importTab = build.importTab
        importTab.controls.charImportItemsClearItems.state = clearItems
        importTab.controls.charImportItemsClearSkills.state = clearSkills
        importTab.controls.charImportItemsIgnoreWeaponSwap.state = ignoreSwap
        importTab:ImportItemsAndSkills(json)
        build.buildFlag = true
        _runCallback('OnFrame')
        return importTab.charImportStatus or ""
    "#,
    )
    .call((json, clear_items, clear_skills, ignore_weapon_swap))
}

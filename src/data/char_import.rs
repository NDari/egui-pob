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
            -- v2.66+ removed ImportTab:ProcessJSON; decode directly like
            -- upstream now does
            local dkjson = require "dkjson"
            local data, _, errMsg = dkjson.decode(json)
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
        -- v2.66+: ImportPassiveTreeAndJewels takes a decoded charData table
        -- shaped like the OAuth API response (upstream's legacy-site path)
        local dkjson = require "dkjson"
        local responseLua, _, err = dkjson.decode(json)
        if not responseLua then
            return "^1Error parsing character data: " .. tostring(err)
        end
        local charData = { name = name, league = league, class = class, level = level }
        charData.passives = responseLua
        charData.jewels = responseLua.items
        -- v2.66+ no longer sets a status message; report the outcome ourselves
        local ok, err = pcall(function()
            importTab:ImportPassiveTreeAndJewels(charData, clearJewels)
        end)
        if not ok then
            return "^1Error importing passive tree: " .. tostring(err)
        end
        build.buildFlag = true
        _runCallback('OnFrame')
        return "Passive tree imported successfully"
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
        -- v2.66+: ImportItemsAndSkills takes a decoded charData table with
        -- an `equipment` list plus the option flags as parameters
        local dkjson = require "dkjson"
        local responseLua, _, err = dkjson.decode(json)
        if not responseLua then
            return "^1Error parsing character data: " .. tostring(err)
        end
        -- The legacy site path copies the char-list entry (name/league/
        -- class/level at top level); lift them from the response's character
        -- object, keeping the current level as a fallback
        local charInfo = responseLua.character or { }
        local charData = {
            name = charInfo.name,
            league = charInfo.league,
            class = charInfo.class,
            level = charInfo.level or build.characterLevel,
        }
        charData.character = responseLua.character
        charData.equipment = responseLua.items or { }
        -- v2.66+ no longer sets a status message; report the outcome ourselves
        local ok, err = pcall(function()
            importTab:ImportItemsAndSkills(charData, clearItems, clearSkills, ignoreSwap)
        end)
        if not ok then
            return "^1Error importing items: " .. tostring(err)
        end
        build.buildFlag = true
        _runCallback('OnFrame')
        return "Items and skills imported successfully"
    "#,
    )
    .call((json, clear_items, clear_skills, ignore_weapon_swap))
}

/// Path of the account-history file (app data dir, like recent builds).
fn account_history_file() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pob-egui")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("account_history.txt"))
}

/// Load past account names, sorted case-insensitively like upstream's
/// history dropdown.
pub fn load_account_history() -> Vec<String> {
    let Some(file) = account_history_file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    let mut list: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    list.sort_by_key(|a| a.to_lowercase());
    list.dedup();
    list
}

fn save_account_history(list: &[String]) {
    let Some(file) = account_history_file() else {
        return;
    };
    let text: String = list.iter().map(|n| format!("{n}\n")).collect();
    if let Err(e) = std::fs::write(&file, text) {
        log::warn!("Failed to write account history: {e}");
    }
}

/// Record an account name after a successful character-list fetch
/// (upstream SaveAccountHistory: dedupe + sorted insert).
pub fn add_account_history(name: &str) -> Vec<String> {
    let mut list = load_account_history();
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
        list.sort_by_key(|a| a.to_lowercase());
        save_account_history(&list);
    }
    list
}

/// Remove an account name from the history (upstream's X button).
pub fn remove_account_history(name: &str) -> Vec<String> {
    let mut list = load_account_history();
    list.retain(|n| n != name);
    save_account_history(&list);
    list
}

/// The build's remembered import league (upstream importTab.lastLeague,
/// persisted in the build XML by upstream's saver).
pub fn last_league(lua: &Lua) -> Option<String> {
    lua.load("return mainObject_ref.main.modes['BUILD'].importTab.lastLeague")
        .eval::<Option<String>>()
        .ok()
        .flatten()
        .filter(|l| !l.is_empty())
}

/// Remember the league a character was imported from, if none is remembered
/// yet (upstream fills lastLeague only when unset on the legacy site path).
pub fn set_last_league_if_unset(lua: &Lua, league: &str) -> Result<(), mlua::Error> {
    lua.load(
        r#"
        local league = ...
        local importTab = mainObject_ref.main.modes['BUILD'].importTab
        if not importTab.lastLeague then
            importTab.lastLeague = league
        end
    "#,
    )
    .call(league)
}

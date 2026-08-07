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

/// The current PoE 1 challenge league. Used as the default character-list
/// filter when the build has no remembered league of its own. Bump this each
/// league (see the version stamp in plans/parity-plan.md).
pub const CURRENT_LEAGUE: &str = "Allflame";

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
        -- v2.67: account-name imports omit the quest choices, so keep whatever
        -- the build already has rather than clearing them
        responseLua.bandit_choice = responseLua.bandit_choice or build.configTab.input.bandit
        responseLua.pantheon_major = responseLua.pantheon_major or build.configTab.input.pantheonMajorGod
        responseLua.pantheon_minor = responseLua.pantheon_minor or build.configTab.input.pantheonMinorGod
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
        -- v2.67: the guardian block travels with the equipment
        charData.guardian = responseLua.guardian
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

/// Remove an account name from the history (upstream's X button). Also drops
/// it as the remembered last account so it stops being prefilled.
pub fn remove_account_history(name: &str) -> Vec<String> {
    let mut list = load_account_history();
    list.retain(|n| n != name);
    save_account_history(&list);
    if load_last_account().is_some_and(|last| last == name) {
        set_last_account("");
    }
    list
}

/// Path of the last-used-account file (upstream's `main.lastAccountName`,
/// which it stores in Settings.xml alongside the history).
fn last_account_file() -> Option<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pob-egui")?;
    let dir = dirs.data_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("last_account.txt"))
}

/// The most recently used account name, if one has been recorded.
fn load_last_account() -> Option<String> {
    let file = last_account_file()?;
    let text = std::fs::read_to_string(&file).ok()?;
    let name = text.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Record the account used by a successful character-list fetch. Pass an
/// empty string to clear it. Upstream assigns `main.lastAccountName` at the
/// same point.
pub fn set_last_account(name: &str) {
    let Some(file) = last_account_file() else {
        return;
    };
    if let Err(e) = std::fs::write(&file, name) {
        log::warn!("Failed to write last account: {e}");
    }
}

/// Account name to prefill the character-import field with, matching
/// upstream's `main.lastAccountName or ""` initialiser.
///
/// Prefers the most recently used account. Histories written before last-use
/// tracking existed have no recorded account, so a sole history entry is used
/// as the fallback; with several and no recorded use there is nothing to pick.
pub fn initial_account_name() -> String {
    pick_initial_account(&load_account_history(), load_last_account().as_deref())
}

/// Decision half of [`initial_account_name`], split out so it can be tested
/// without touching the app data dir.
fn pick_initial_account(history: &[String], last: Option<&str>) -> String {
    if let Some(last) = last
        && history.iter().any(|n| n == last)
    {
        return last.to_string();
    }
    match history {
        [only] => only.clone(),
        _ => String::new(),
    }
}

/// Index for the character-list league filter: 0 selects "All", otherwise a
/// 1-based index into `leagues`.
///
/// The build's remembered league (upstream `importTab.lastLeague`) wins when
/// it is present in the account's leagues; otherwise [`CURRENT_LEAGUE`] is
/// preferred over showing every league at once. Falls back to "All" when
/// neither is among the fetched characters.
pub fn pick_league_index(leagues: &[String], remembered: Option<&str>) -> usize {
    let preferred = remembered.unwrap_or(CURRENT_LEAGUE);
    leagues
        .iter()
        .position(|l| l == preferred)
        .map_or(0, |pos| pos + 1)
}

#[cfg(test)]
mod league_filter_tests {
    use super::{CURRENT_LEAGUE, pick_league_index};

    fn leagues(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn current_league_is_the_default_when_the_build_remembers_none() {
        let list = leagues(&["Standard", CURRENT_LEAGUE, "Hardcore"]);
        assert_eq!(pick_league_index(&list, None), 2);
    }

    #[test]
    fn remembered_league_beats_the_current_league() {
        let list = leagues(&["Standard", CURRENT_LEAGUE]);
        assert_eq!(pick_league_index(&list, Some("Standard")), 1);
    }

    #[test]
    fn falls_back_to_all_when_the_current_league_has_no_characters() {
        let list = leagues(&["Standard", "Hardcore"]);
        assert_eq!(pick_league_index(&list, None), 0);
    }

    #[test]
    fn falls_back_to_all_when_the_remembered_league_has_no_characters() {
        let list = leagues(&["Standard", CURRENT_LEAGUE]);
        assert_eq!(pick_league_index(&list, Some("Settlers")), 0);
    }

    #[test]
    fn empty_league_list_selects_all() {
        assert_eq!(pick_league_index(&[], None), 0);
    }
}

#[cfg(test)]
mod account_prefill_tests {
    use super::pick_initial_account;

    fn hist(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sole_history_entry_is_prefilled_without_a_recorded_last_use() {
        assert_eq!(
            pick_initial_account(&hist(&["Alice#1234"]), None),
            "Alice#1234"
        );
    }

    #[test]
    fn most_recently_used_account_wins_over_history_order() {
        // History is stored alphabetically, so the last-used account is
        // generally not the first entry.
        let history = hist(&["Alice#1234", "Bob#5678", "Carol#9012"]);
        assert_eq!(pick_initial_account(&history, Some("Bob#5678")), "Bob#5678");
    }

    #[test]
    fn several_accounts_and_no_recorded_last_use_prefills_nothing() {
        let history = hist(&["Alice#1234", "Bob#5678"]);
        assert_eq!(pick_initial_account(&history, None), "");
    }

    #[test]
    fn last_used_account_removed_from_history_is_not_prefilled() {
        let history = hist(&["Alice#1234", "Bob#5678"]);
        assert_eq!(pick_initial_account(&history, Some("Carol#9012")), "");
    }

    #[test]
    fn stale_last_use_still_falls_back_to_a_sole_history_entry() {
        assert_eq!(
            pick_initial_account(&hist(&["Alice#1234"]), Some("Carol#9012")),
            "Alice#1234"
        );
    }

    #[test]
    fn empty_history_prefills_nothing() {
        assert_eq!(pick_initial_account(&[], None), "");
    }
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

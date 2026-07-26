# Feature Parity Plan: egui-pob vs Upstream PoB

This document tracks every feature needed to reach parity with upstream Path of Building Community. Items are grouped by area rather than priority — ordering and phasing will be decided separately.

**Parity validated against upstream v2.66.1 (PoE 3.29).** Update this stamp on every submodule pin bump (see docs/upstream-upgrade.md).

Status key: `[x]` done, `[~]` partial, `[ ]` not started

---

## 1. Build Management

### Build List
- [x] Scan and display builds from build directory
- [x] Folder navigation (enter, go up)
- [x] Refresh build list
- [x] Open build by clicking
- [x] New build creation
- [x] Delete build (right-click menu, confirmation popup; folders too, recursive)
- [x] Rename build (right-click menu; folders too)
- [x] Move build to folder (right-click "Move to" submenu: subfolders + parent)
- [x] Create new folder
- [x] Sort builds (by name, date modified)
- [x] Build search/filter by name
- [x] Recent builds list (last 10 opened, shown above the list; persisted in the app data dir)
- [x] Build preview tooltip on hover (class, level, DPS/Life/EHP parsed from the XML's PlayerStat elements)

### Save System
- [x] Save build to disk (Save)
- [x] Save As (new name, for new builds)
- [x] Save As with folder browser (subfolder list + breadcrumb navigation, New Folder, overwrite guard, dbFileSubPath tracking like upstream's OpenSaveAsPopup)
- [x] Save confirmation popup on close/switch with unsaved changes
- [~] Auto-save / dirty-state tracking (dirty tracking via upstream build.unsaved done; auto-save not planned)

### Loadout System
- [x] Multiple loadouts per build (linked tree/items/skills/config sets, matched by set title or {tag}, via upstream SyncLoadouts)
- [x] Loadout dropdown in top bar (shows the currently matched loadout; activating switches all four sets)
- [x] Create/delete/rename loadouts (create via + button; rename/delete happen through the underlying sets, same as upstream)
- [x] Sync between loadout tabs (list + selection refresh on every set change)

---

## 2. Character Header (Top Bar)

- [x] Back button to build list
- [x] Build name display
- [x] Class dropdown
- [x] Ascendancy dropdown
- [x] Secondary ascendancy (bloodline) dropdown
- [x] Character level edit field (1-100)
- [x] Level scaling mode toggle (Auto/Manual)
- [x] Passive points used display (N / M)
- [x] Ascendancy points used display
- ~~Bandit reward selection~~ (covered by Config tab)
- ~~Pantheon major god selection~~ (covered by Config tab)
- ~~Pantheon minor god selection~~ (covered by Config tab)
- [x] Experience multiplier tooltip on level hover

---

## 3. Passive Tree

### Rendering
- [x] Node rendering with sprites (normal, notable, keystone, mastery, socket, class start)
- [x] Frame overlays per node type/state
- [x] Connection lines (straight and curved arcs)
- [x] Group backgrounds (small, medium, large)
- [x] Class start backgrounds
- [x] Ascendancy backgrounds (with dimming for non-selected)
- [x] Mastery active effect overlay
- [x] Pan and zoom with mouse
- [x] Zoom-to-cursor
- [x] Visibility culling
- [x] Connector state coloring (path preview, intermediate, dependent)
- [x] Red highlight for dependent nodes (nodes that would disconnect)
- [x] Path preview line (shortest path to unallocated node on hover)

### Interaction
- [x] Click to allocate/deallocate node
- [x] Hover tooltip with stats, name, type, recipe, flavour text
- [x] Stat difference preview in hover tooltip ("Allocating this node will give you: ..." via upstream AddStatComparesToTooltip; Ctrl+D toggle)
- [x] Path stat difference in hover tooltip ("Allocating this node and all nodes leading to it will give you: ..." with per-point values; "unallocating ... and all nodes depending on it" for allocated nodes)
- [x] Right-click context menu (jump to items for jewel sockets, modify node for tattoos)
- [x] Mastery popup (select mastery effect on click; right-click allocated mastery to change effect)
- [x] Shift+drag path tracing mode (upstream traceMode: Shift starts a trace, hover extends node-by-node, click on the trace end allocates the whole path as one undo step)
- [x] Ascendancy node click → class/ascendancy switching with confirmation (same-class and bloodline switches immediate; cross-class switches confirm with Continue/Connect Path/Cancel like upstream; clicks route through upstream's path-gated allocation)

### Search
- [x] Tree search bar (text search across node names, stats, and type)
- [x] Highlighted search results (colored rings on matching nodes)
- [x] Lua pattern matching support (search delegated to upstream's matcher: terms are Lua patterns with (a|b) or-groups, matched against name, stat lines, parsed mod names, and type; invalid patterns match nothing)
- [x] "oil:" prefix for anoint recipe search
- [x] Multi-term search (all terms must match, like upstream; quoted phrases supported)
- [x] Ctrl+F to focus search

### Node Power
- [x] Show node power heatmap toggle
- [x] Power stat selection dropdown (DPS, Defense, etc.; upstream powerStatList)
- [x] Node power max depth controls (All/5/10/15; no custom depth input yet)
- [x] Power report list (sortable table of node values)
- [x] Click node in power report to pan to it
- [x] Color-coded power visualization (red=offense, blue=defense; RED/BLUE theme only)

### Jewel Sockets
- [x] Jewel radius display (ring overlay on allocated sockets; hover previews all radii)
- [x] Thread of Hope annular display (inner + outer ring)
- [x] Impossible Escape keystone radius display
- [~] Named jewel overlays (radius ring shown; themed rotating art not implemented)
- [x] Cluster jewel subgraph rendering (subgraph nodes/connections flow through spec.nodes into the renderer; tree data re-extracts when item changes rebuild subgraphs)
- [x] Right-click jewel socket → jump to items tab

### Comparison
- [x] Compare checkbox to enable comparison mode
- [x] Compare spec dropdown to select comparison tree
- [x] Green/red node diff coloring (allocate/deallocate indicators)
- [x] Blue mastery diff (different effect selected)

### Tree Specs
- [x] Multiple tree specs per build (dropdown in tree toolbar)
- [x] Spec management popup (create, copy, delete, rename, reorder via up/down buttons; active index follows the moved spec like upstream)
- [x] Import tree from URL (PoE official, PoePlanner; poeurl shortlinks expanded via HTTP)
- [x] Export tree as URL
- [x] Copy tree URL to clipboard
- [~] PoEURL shortlink generation (Shrink with PoEURL button, faithful port over https; live verification blocked - poeurl.com was down, ignored network test covers it)

### Tree Version
- [x] Tree version dropdown
- [x] Convert to latest version button
- [x] Convert all trees button
- [x] Version mismatch warning banner
- [x] Conversion confirmation popup (Convert / Copy + Convert / Cancel)

### Tattoos
- [x] Modify node popup (right-click an eligible node; upstream target-type filter)
- [x] Tattoo modifier dropdown (with stat descriptions)
- [x] Legacy tattoo toggle
- [x] Tattoo count tracking (max 50, red when over)
- [x] Remove tattoo from node ("Reset node")

### Timeless Jewels
- [x] Find Timeless Jewel dialog (seed search over upstream readLUT; creates the jewel from a result)
- [x] Jewel type selection (6 types)
- [x] Conqueror variant selection
- [x] Socket selection (specific socket or "All Sockets" multi-search; results tagged with the socket found at)
- [x] Devotion modifier selection (Total Strength/Dexterity/Devotion pseudo-stats with upstream's small-node bonus formulas; the upstream devotion-variant dropdowns only feed trade URLs, which we don't have)
- [x] Node search and weighting system (weights; GV secondary-stat weight)
- [x] Fallback weight mode (power-stat dropdown + Generate: one calc pass per legion stat via GetMiscCalculator; merged into the search below explicit weights)

### Undo/Redo
- [x] Ctrl+Z to undo tree changes
- [x] Ctrl+Y to redo
- [x] Undo state snapshots (handled by Lua's UndoHandler; AddUndoState called on every change)

---

## 4. Skills Tab

### Socket Group Management
- [x] Display socket groups with gems
- [x] Main skill selection ("Set Main" button)
- [x] Create new socket group
- [x] Delete socket group (with confirmation if gems exist; item-granted groups protected)
- [x] Delete all socket groups (button + confirmation; item-granted groups kept)
- [x] Reorder socket groups (drag handle on each group; main group and calcs skill number follow the move like upstream OnOrderChange)
- [x] Copy/paste socket groups (upstream text format; per-group Copy button + Paste button; Ctrl+C copies the main group, Ctrl+V pastes)
- [x] Enable/disable socket group (checkbox)
- [x] Include/exclude from FullDPS (checkbox per group; upstream's Ctrl+Right-Click shortcut not implemented)
- [x] Socket group label editing
- [x] Socket group slot assignment (socketed in dropdown)
- [x] Socket group count multiplier (item-granted groups, like upstream)

### Gem Management
- [x] Display gem name, level, quality
- [x] Add gem to socket group (text entry, fuzzy-matched by Lua's FindSkillGem; e.g. "CtF")
- [x] Remove gem from socket group
- [x] Gem search/autocomplete (via upstream GemSelectControl instance)
  - [x] Name search, tag search (`:tag`), exclusion (`-tag`)
  - [x] Sort by DPS impact
  - [x] Color-coded gem types (Str/Dex/Int)
  - [x] Support relationship indicators (check mark)
- [x] Edit gem level
- [x] Edit gem quality
- [x] Quality variant selection (Default, Anomalous, Divergent, Phantasmal; dropdown shown when the gem has alt-quality stats)
- [x] Enable/disable individual gem
- [x] Gem count (for totems, minions, traps, mines; shown for gems granting active effects)
- [x] Vaal gem global effect toggles (per-gem Enable <skill> checkboxes for vaal gems, upstream enableGlobal1/2)

### Gem Options
- [x] Sort gems by DPS toggle with stat selector (all upstream sort stats)
- [x] Default gem level dropdown (Normal/Corrupted/Awakened Max, Match Character Level, Level 1; applied on gem add via upstream ProcessGemLevel)
- [x] Default gem quality input
- [x] Show support gem type filter (All / Non-Exceptional / Exceptional)
- ~~Show quality variants toggle~~ (alternate gem qualities removed from the game and upstream in v2.66)
- [x] Show legacy gems toggle

### Skill Sets
- [x] Multiple skill sets per build (upstream skillSets; XML round-trip via upstream save/load)
- [x] Skill set management popup (new/copy/rename/delete with confirmation)
- [x] Switch between skill sets (dropdown in the skills toolbar)

---

## 5. Items Tab

### Equipment Display
- [x] Display equipped items by slot (slot list with equip dropdowns)
- [x] Full item tooltip (upstream's AddItemTooltip rendered via Tooltip lines, incl. stat diff)
- [~] Item rarity styling (colored names + tooltip color codes; no borders/separator art)
- [~] Socket and link display (text line in tooltip; no dedicated socket widget)
- [ ] Influence icons display
- [~] Flask display with charges/duration (via tooltip)
- [x] Weapon DPS breakdown in tooltip (Physical, Elemental, Chaos, Total)
- [x] Armor stats breakdown (Armour, Evasion, Energy Shield, Ward)

### Item Management
- [x] Item list panel (all owned items)
- [x] Equip item to slot (dropdown per slot)
- [x] Unequip item from slot
- [x] Delete item (with confirmation)
- [x] Sort item list (upstream SortItemList: by slot, equipped first)
- [x] Drag items between slots (drag handle on occupied slots; swap with the displaced item; validity via upstream IsItemValidForSlot; item list entries drag onto slots to equip)

### Item Editing
- [x] Edit item text (raw editor popup with live validation; also creates custom items)
- [x] Variant selection dropdown (for multi-variant uniques; in the edit dialog, rebuilds raw text via BuildAndParseRaw)
- [x] Alt variant dropdowns (up to 5)
- [x] Socket color selection (R/G/B/W per socket; "Sockets & catalyst" dialog, add-socket to base cap)
- [x] Link toggles between sockets (group shifting like upstream)
- [x] Quality edit (in the edit dialog)
- [x] Influence selection (2 dropdowns: Shaper, Elder, Warlord, etc.; in the edit dialog)
- [x] Catalyst type and quality (amulets/rings/belts; re-crafts affixes for the catalyst scalar)

### Item Creation
- [x] Craft item popup (select base type, rarity; rare gets a name; opens the affix editor)
- [x] Affix selection (prefix/suffix dropdowns; tiers listed flat with ilvl, eligibility via upstream GetModSpawnWeight)
- [x] Affix range sliders (roll position within the tier, re-crafts via Item:Craft)
- [x] Custom modifier popup (Add Modifier dialog: Crafting Bench, Essence, Veiled, Beastcraft, Necropolis, Delve, and Prefix/Suffix catalogs per upstream eligibility, plus free-text custom with a bench-craft flag; ported buildMods registered in ports.toml)
- [x] Paste item from clipboard (button + Ctrl+V, parsed by upstream Item:ParseRaw)

### Enchanting & Anointing
- [x] Apply enchantment popup (helmet/gloves/boots/flasks; per-skill catalog for helmets with used-skills filter, source grouping, apply/remove)
- [x] Apply anoint popup (notable search + oil recipe; searchable list with stats and oils, apply/replace/remove)
- [x] Multiple anoint slots (up to 4 via canHaveTwo/Three/FourEnchants; slots open as previous ones fill, per-slot removal)
- [x] Stat comparison preview for anoints (upstream AppendAnointTooltip: repItem calc diff, already-allocated/anointed detection)

### Corruption & Influence
- [x] Corrupt item popup (two group-exclusive implicit dropdowns; corrupting with none marks Corrupted only)
- [x] Add implicit popup (Exarch/Eater when influenced, Delve, Custom; tiered groups; eldritch replaces in place. Synthesis omitted like upstream; Scourge not implemented)
- Crucible modifier popup - deferred, see §16

### Cluster Jewels
- [x] Cluster jewel skill dropdown (in the craft editor; upstream skill list minus unavailable attrs)
- [x] Node count slider (min-max per jewel size; rebuilds enchant lines like CraftClusterJewel)
- [x] Craft cluster jewel mods (affix editor works on cluster jewels; skill tag feeds spawn weights)

### Item Comparison
- [x] Stat diff tooltip when hovering unequipped items (in slot dropdowns, via upstream tooltip)
- [ ] Side-by-side comparison view

### Item Database
- [x] Unique item database browser (window from Items tab; add to build, full tooltips)
- [x] Rare template database browser (same window, second tab)
- [x] Search and filter in databases (search with Anywhere/Names/Modifiers modes + base type filter; league/requirement/obtainable filters not yet)

### Item Sets
- [x] Multiple item sets per build (upstream itemSets; slot selections swap via SetActiveItemSet)
- [x] Item set dropdown
- [x] Item set management (new/copy/rename/delete popup with confirmation)
- [x] Weapon swap support (per-set checkbox; empty swap slots become visible/equippable while enabled)

### Undo/Redo
- [x] Ctrl+Z / Ctrl+Y for item changes (upstream ItemsTab UndoHandler; all item mutations add undo states)

---

## 6. Calcs Tab

- [x] Calcs tab (full calculation breakdown display, driven by upstream's CalcSections data)
- [x] Socket group / active skill / skill part selectors (calcs-mode selection via mainActiveSkillCalcs/skillPartCalcs, independent of the sidebar like upstream)
- [x] Calculation mode dropdown (Unbuffed, Buffed, In Combat, Effective DPS)
- [x] Expandable stat sections (Offense, Defense, etc.)
- [x] Click stat to show detailed breakdown
- [x] Breakdown panel (right side) with formula/steps, tables, and mod lists
- [x] Pin breakdown to keep visible (breakdown stays open until closed)
- [x] Search bar for stat filtering (Ctrl+F)
- [x] Buff/debuff lists (auras, combat buffs, curses - via the View Skill Details section)
- [x] Show minion stats toggle
- [x] Minion selection and skill dropdowns (calcs-tab minion + minion-skill dropdowns via upstream's Calcs-suffix srcInstance fields)

---

## 7. Config Tab

- [x] Display all config option types (checkbox, count, list, text)
- [x] Change config values and trigger recalc
- [x] Config search/filter bar
- [x] Show/hide ineligible configurations toggle
- [x] Section headers with collapsible groups
- [x] Conditional option visibility (ifNode, ifOption, ifCond dependencies)
- [x] Tooltips for config option explanations
- [x] Config sets (multiple independent configs per build; dropdown in the config toolbar)
- [x] Config set management popup (new/copy/rename/delete with confirmation)
- [x] Reset to defaults (button + confirmation; restores upstream varList defaults)
- [x] Undo/redo for config changes (upstream ConfigTab UndoHandler; Ctrl+Z/Y in the config tab)

---

## 8. Notes Tab

- [x] Notes tab (large multiline text editor)
- [x] Color code support (PoB color tags: `^7`, `^xHEXCODE`)
- [x] Color code buttons (Normal, Magic, Rare, Unique, Fire, Cold, etc.)
- [x] Show/hide color codes toggle
- [ ] Ctrl+Z/Y undo/redo within editor
- [x] Zoom support (Ctrl+scroll over the editor, 8-40pt)

---

## 9. Party Tab

Deferred - see §16.

---

## 10. Import/Export

### Build Codes
- [x] Generate export code (deflate + base64)
- [x] Copy code to clipboard
- [x] Import from raw build code
- [x] Auto-detect URL vs code

### URL Import
- [x] Import from pobb.in
- [x] Import from poe.ninja
- [x] Import from pastebin.com
- [x] Import from maxroll.gg
- [x] Import from rentry.co
- [x] Import from poedb.tw
- [ ] Import from YouTube/Google redirects (follow redirects)

### Build Sharing
- [ ] Website selection dropdown for export target
- [ ] Share button (upload to website API)
- [ ] Support character export toggle

### Character Import (from PoE Account)
- [x] Account name input with realm selection (PC, Xbox, PS4, etc.)
- [x] POESESSID input for private profiles
- [x] Download character list from PoE API (HTTP in Rust, parsed by upstream ProcessJSON)
- [x] League filter dropdown
- [x] Character selection dropdown
- [x] Import passive tree and jewels (upstream ImportPassiveTreeAndJewels, jewel clearing option)
- [x] Import items and skills (upstream ImportItemsAndSkills; delete items/skills, ignore swap options)
- [ ] Account history tracking
- [x] Privacy settings link

### Import Modes
- [x] Import to current build vs. create new build toggle (radio in the import section; new-build path resets the view to an unsaved "Imported Build")

---

## 11. Main Skill Selection (Sidebar)

- [x] Stat sidebar with key stats
- [x] Main socket group dropdown
- [x] Main active skill dropdown
- [x] Skill part dropdown (for multi-part skills)
- [x] Skill stage count input
- [x] Active mines count input
- [x] Minion type dropdown
- [~] Manage Spectres button (button shown; library popup not implemented)
- [x] Minion skill dropdown

---

## 12. Stat Display & Warnings

- [x] Key stats in sidebar (DPS, Life, ES, Mana, Resistances, etc.)
- [x] Number formatting (commas, percentages, decimals)
- [x] Color-coded stats
- [x] Full stat list (reads upstream's pre-built statBox list from RefreshStatList, so all display stats, formatting, and colors match upstream)
- [x] Conditional stat display (condFunc filtering runs in Lua via RefreshStatList)
- [x] Warning messages panel (collapsible bar above stats; lines come from upstream's warnings control):
  - [x] Too many passive/ascendancy points
  - [x] Missing item requirements (upstream shows these as red text in item tooltips, which we render; not a warnings-bar item upstream either)
  - [x] Skill cost vs. pool warnings
  - [x] Gem socket limit warnings
  - [x] Jewel limit warnings
  - [x] Aspect skill conflicts
- [ ] Clickable warnings (jump to relevant tab)
- [x] Minion stat display toggle (minion/player sections appear automatically when a minion exists, like upstream)
- [x] DPS breakdown by source/trigger in stat list (SkillDPS entries with source/trigger annotation lines)

---

## 13. Keyboard Shortcuts

- [x] Ctrl+S: Save build (opens Save As when the build has no file)
- [x] Ctrl+W: Close build (with save prompt)
- [x] Ctrl+Z: Undo (tree, items, config)
- [x] Ctrl+Y: Redo (tree, items, config)
- [x] Ctrl+F: Focus search (tree, calcs, config)
- [x] Ctrl+I: Open Import/Export
- [x] Ctrl+1-7: Switch tabs (Tree, Skills, Items, Calcs, Config, Notes, Import/Export; no Party tab yet)
- [x] Ctrl+V: Paste item (in items tab)
- [x] Ctrl+C: Copy (skills tab copies the main socket group; items tab copies the hovered item's text with CRLF endings like upstream's item-list copy)
- [x] E: Edit hovered item (upstream binds plain "e" over a slot, not Ctrl+E; ours opens the text editor for any hovered slot or list item)
- [x] Ctrl+D: Toggle stat differences (in node tooltips, on the tree tab)
- [x] F1: Open wiki for hovered item/gem (via upstream itemLib.wiki; items and skills tabs)
- [x] Mouse4: Close build

---

## 14. UI Polish & UX

- [~] Global undo/redo system (per-tab UndoHandler like upstream: tree, items, config wired; skills/notes not wired)
- [ ] Tooltip positioning (avoid screen edges)
- [ ] DPI scaling / HiDPI support
- [x] Window title with build name and class ("Name (Ascendancy [+ Secondary]) - Path of Building")
- [ ] Confirmation popups for destructive actions
- [ ] Status bar / toast notifications
- [ ] Loading indicators for async operations
- [x] Drag-and-drop support (items between slots and list-to-slot, gems within groups, socket group reorder)
- [ ] Copy/paste support throughout
- [ ] Consistent theme/styling matching upstream
- [ ] Responsive layout for different window sizes
- [x] Wiki integration (F1 opens poewiki.net for hovered items/gems via upstream itemLib.wiki)
- [ ] Similar builds popup (from PoB Archives)

---

## 15. Data & Infrastructure

- [x] Full item text parsing (via Lua's Item:ParseRaw, per the recommendation below)
- [x] Modifier evaluation and spawn weight calculation (upstream GetModSpawnWeight/CheckIfModIsDelve drive the affix lists)
- [x] Item modifier list building (upstream Item:BuildModList/Craft; invoked from all item mutations)
- [ ] Gem data access (tags, requirements, stats, descriptions)
- [x] Build XML round-trip fidelity (load → save → load: structural fixed point + stats/counts preserved; hash-ordered sections compared as sets)
- [ ] Sub-script system (LaunchSubScript for background tasks)
- [x] Power calculation coroutine (upstream PowerBuilder driven via per-frame stepping with progress display)
- [ ] Config condition evaluation (mainEnv tracking)
- [ ] Asset extraction pipeline (Rust tool to extract from PoE GGPK/bundles)

---

## 16. Deferred

Items parked deliberately - revisit when the core parity work is done.

- [ ] Party tab for configuring party member effects
- [ ] Party aura/buff configuration
- [ ] Enemy modifier list from party
- [ ] Crucible modifier popup (5-node tree selection; Crucible is a past league, upstream keeps it for legacy builds)
- [ ] Hover shortcuts in the item-DB browser (F1 wiki and Ctrl+C copy work on hovered slot/list items but not in the unique/rare-template DB window; upstream's ItemDBControl supports both)

---

## Effort Estimates & Dependencies

Effort key: **S** = a few hours, **M** = 1-2 days, **L** = 3-5 days, **XL** = 1-2 weeks

### Section Effort Summary

| Section | Total Effort | Blocker? |
|---------|-------------|----------|
| 1. Build Management | M (basics), XL (loadouts) | Loadouts blocked by §3/§4/§5/§7 sets |
| 2. Character Header | M | None — all quick wins |
| 3. Passive Tree | XL overall | Node power blocked by §15 coroutine; cluster jewels blocked by Lua subgraph; comparison blocked by multi-specs |
| 4. Skills Tab | L (editing), XL (gem search) | Gem search blocked by §15 gem data; drag-reorder needs §14 drag-and-drop |
| 5. Items Tab | XL overall | Almost everything blocked by §15 item text parsing; crafting blocked by §15 modifier evaluation |
| 6. Calcs Tab | L | None — reads from existing Lua calcs |
| 7. Config Tab | M (UI), L (sets) | Conditional visibility needs §15 config condition eval |
| 8. Notes Tab | M | None — fully standalone |
| 9. Party Tab | L | None |
| 10. Import/Export | S (sharing), XL (character import) | Character import blocked by §15 item text parsing |
| 11. Sidebar Skill Selection | M | §15 gem data for minion/spectre features |
| 12. Stat Display & Warnings | L | Warnings need data from §3/§4/§5 |
| 13. Keyboard Shortcuts | M | Features they trigger must exist first |
| 14. UI Polish | L (undo, drag-drop), S-M (rest) | Undo/drag-drop are foundational — design early |
| 15. Data & Infrastructure | XL (item parsing), M-L (rest) | Item text parsing is the single biggest blocker |

### Dependency Graph

```
§15 Item Text Parsing ──> §5 Items Tab (all editing/creation)
                      ──> §10 Character Import
                      ──> §5 Item Comparison

§15 Gem Data Access ───> §4 Gem Search/Autocomplete
                      ──> §11 Sidebar Skill Selection

§15 Modifier Evaluation > §5 Crafting / Affix Selection

§14 Global Undo/Redo ──> §3 Tree Undo, §5 Item Undo, §7 Config Undo

§14 Drag-and-Drop ─────> §4 Reorder Groups, §5 Drag Items

§3 Multiple Tree Specs ─> §3 Comparison, §3 Version Switching

§3, §4, §5, §7 ────────> §1 Loadout System (needs all set systems)

§3, §4, §5 ────────────> §12 Warnings (needs data from all tabs)
```

### Key Decision: Item Text Parsing Strategy

Item text parsing (§15) is the single biggest blocker — it gates the entire items tab,
character import, and clipboard paste. Two approaches:

1. **Call Lua's existing `Item:ParseRaw()`** — faster to ship, leverages upstream's battle-tested 1800-line parser, stays in sync with upstream updates automatically.
2. **Reimplement in Rust** — better long-term performance, no Lua round-trip overhead, but massive effort and ongoing maintenance burden to stay in sync.

Recommendation: Use Lua's parser via mlua calls. Reimplement in Rust only if profiling shows it's a bottleneck.

---

## Upstream Delta: v2.64.0 - v2.66.1 (3.29 Allflame, July 2026)

New upstream features to track for parity, from the changelog and diff at the
v2.66.1 pin. Calc-engine and game-data changes (3.29 trees, gems, uniques,
runegrafts, bloodline nodes, spectres) come free through the Lua VM and are
not listed.

### Build Comparison Tab (new upstream tab, ~5k lines)
- [ ] Compare tab: side-by-side build comparison (CompareTab.lua; subsumes the old "Side-by-side comparison view" item)
- [ ] Compare calcs with "only show differences" filter
- [ ] Compare power report
- [ ] Abyss sockets in comparison
- [ ] "Buy similar" trade integration (trade-dependent)

### Import/Export
- [ ] PoB2 OAuth API import for character and trade (PoEAPI.lua; our legacy session-id import still works)
- [ ] pob.codes build export/import
- [ ] Preserve skill selection on character re-import
- [ ] Remember league for imported characters

### Skills/Gems
- [ ] Progressive gem sort results while typing (we drive the new DPSBuilder to completion synchronously instead)
- [ ] Imbued Supports (new GemSelectControl imbued mode)
- [x] Gem tooltips (GemTooltip.AddGemTooltip called headless, shown on skills-tab gem-name hover)
- [x] Gem color indicators on socket group labels (R/G/B/W letters per gem on player group headers)
- [ ] Sort gem suggestions by minion-specific stats (data side done via powerStatList; UI dropdown lists them already)

### Items/Crafting
- [x] Sorting in add-modifier, enchant, corrupt, and implicit popups (power-stat dropdown in all four, one calc pass per option/group/line/entry on first use, values shown next to each option)
- [x] Sinistral and Dextral catalysts (12-entry list synced to upstream's index order; drift-guarded in ports.toml)
- [ ] Foulborn modifier toggles on uniques (per-mod mutate checkbox + magnitude slider via Item:MutateMod)
- [ ] Volatile Vaal Orb corruption (per-explicit-mod roll-range sliders, corruptedRange 0.78-1.22 persisted on mod lines)
- [x] Increased-magnitude mods (Kane of Kulemak, Helical Ring, Heist enchants) (Item:ParseRaw/data path we call; free)
- [ ] Advanced item copy/paste format
- [x] Warning for eligible items missing an anoint (flows through upstream's warning list we already display; asserted in tests)

### Tree/Power Report
- [x] Masteries in the power report (flows through BuildPowerReportList, which we call; asserted in tests)
- [ ] Node description tooltips in the power report
- [x] Intuitive-Leap-aware power report distances (calc-side in PowerBuilder, which we drive; free)
- [ ] Timeless jewel trade QoL (copy trade URL, open link; trade-dependent)
- [ ] Ascendancy flavour text only at high zoom
- [ ] Allocate ascendancy nodes through custom modifiers

### UI/Options
- [ ] Pinnable calc panes as overlay windows on other tabs
- [ ] Sidebar stat suffixes and compact value formatting toggle
- [x] Staged skills default to maximum stages (calc-engine change in called code; free)
- [x] Crafted cluster jewels default to minimum passives (default synced in cluster_craft_info)
- [ ] Option to disable scroll wheel on controls (may not apply to egui)

### Removed upstream (parity items now obsolete)
- Alternate gem qualities (quality variant dropdowns, Show quality variants toggle) - removed in v2.66
- ImportTab:ProcessJSON - replaced by direct dkjson decoding
- Import status messages - import functions no longer report status text

---

## Implementation Phases

### Phase 1 — Quick wins, high daily-use value
*Mostly S/M effort, no blockers. Makes the app feel more complete immediately.*

- §2 Character Header: level field, level scaling toggle, points display, bandits, pantheon
- §11 Sidebar Skill Selection: socket group/skill/part dropdowns, stage/mine counts, minion selection
- §8 Notes Tab: multiline editor, color code support
- §7 Config Tab improvements: search/filter, section headers, collapsible groups, conditional visibility, tooltips

### Phase 2 — Core build planning
*M/L effort. Makes the app usable for real build creation and iteration.*

- §3 Tree: mastery popup, search + highlighting, path preview, dependent node highlighting, undo/redo
- §4 Skills: create/delete socket groups, add/remove gems, edit gem level/quality, enable/disable, label editing
- §6 Calcs Tab: full breakdown display, skill/mode selectors, expandable sections, stat breakdown panel
- §12 Stat Display: full 203+ stat list, conditional display, warning messages panel

### Phase 3 — Infrastructure that unblocks heavy features
*Foundational work. Must land before Phase 4 can proceed.*

- §15 Item text parsing (call Lua's `Item:ParseRaw()` via mlua)
- §15 Gem data access (tags, requirements, stats, descriptions)
- §14 Global undo/redo system design
- §14 Drag-and-drop infrastructure

### Phase 4 — Items and advanced skill features
*L/XL effort. Depends on §15 infrastructure from Phase 3.*

- §5 Items: full tooltips, item list panel, equip/unequip, edit item text, variant selection, socket/link editing
- §4 Gem search/autocomplete with DPS sorting
- §5 Crafting: craft item popup, affix selection, range sliders, custom modifiers
- §5 Enchanting, anointing, corruption, implicits
- §10 Character import from PoE account (API, JSON parsing, item/skill import)

### Phase 5 — Power user features and polish
*Advanced features, set systems, and final polish.*

- §3 Tree: node power heatmap + report, tree comparison, jewel radius/overlays, cluster jewel subgraphs, timeless jewels, tattoos
- §3 Tree specs + version switching
- §5 Item database browser, item sets, weapon swap, item comparison
- §4 Skill sets
- §7 Config sets
- §9 Party Tab
- §1 Loadout system (coordinates tree/item/skill/config sets)
- §10 Build sharing (upload to website APIs)
- §13 Keyboard shortcuts (added incrementally as features land)
- §14 Remaining UI polish: consistent theming, responsive layout, wiki integration, similar builds popup
- §15 Asset extraction pipeline (standalone Rust tool — long-term)

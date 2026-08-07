# Feature Parity Plan: egui-pob vs Upstream PoB

This document tracks every feature needed to reach parity with upstream Path of Building Community. Items are grouped by area rather than priority — ordering and phasing will be decided separately.

**Parity validated against upstream v2.67.0 (PoE 3.29).** Update this stamp on every submodule pin bump (see docs/upstream-upgrade.md).

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
- [~] Named jewel overlays (radius ring shown; themed rotating art deferred to §16, needs jewel art assets)
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
- [~] Item rarity styling (colored names + tooltip color codes; upstream's header/separator art deferred to §16, needs asset art)
- [x] Socket and link display (inline colored socket/link dots on equipped slots + the Sockets & Catalyst dialog; text line in tooltips)
- [x] Influence icons display (upstream Assets pngs loaded as textures; shown on equipped slots and the item list)
- [x] Flask display with charges/duration (via the item tooltip; presentation choice, no dedicated flask widget)
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
- ~~Side-by-side comparison view~~ (subsumed by the Compare tab, see the v2.66 delta section)

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
- [x] Ctrl+Z/Y undo/redo within editor (egui TextEdit's built-in undo; nothing consumes the keys while the editor has focus)
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
- [x] Import from YouTube/Google redirects (textual q= unwrap like upstream, plus reqwest follows HTTP redirects on download)

### Build Sharing
- [x] Website selection dropdown for export target (Maxroll, pob.codes, pobb.in, PoeNinja, poedb.tw; upstream's export-capable buildSites set)
- [~] Share button (POST postFields+code per upstream UploadBuild, share URL replaces the code box; live upload not yet verified against the real site APIs)
- [x] Support character export toggle (Export Support checkbox; persists as exportParty via upstream's saver, calc effect awaits the Party tab)

### Character Import (from PoE Account)
- [x] Account name input with realm selection (PC, Xbox, PS4, etc.)
- [x] POESESSID input for private profiles
- [x] Download character list from PoE API (HTTP in Rust, parsed by upstream ProcessJSON)
- [x] League filter dropdown
- [x] Character selection dropdown
- [x] Import passive tree and jewels (upstream ImportPassiveTreeAndJewels, jewel clearing option)
- [x] Import items and skills (upstream ImportItemsAndSkills; delete items/skills, ignore swap options)
- [x] Account history tracking (persisted in the app data dir; history dropdown with per-entry removal, saved on successful fetch like upstream)
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
- [x] Manage Spectres button (Spectre Library popup: staged in-build list, name/skill-searchable available list, upstream minion tooltips; commits build.spectreList on Save)
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
- [x] Clickable warnings (resolved: upstream has no click-to-jump either, its warnings are a hover-only tooltip; ours render as a collapsible list, strictly more visible)
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

- [x] Global undo/redo system (per-tab UndoHandler like upstream: tree, items, config, and skills wired to Ctrl+Z/Y; notes uses the editor's native undo)
- [x] Tooltip positioning (egui positions hover tooltips within the screen natively)
- [x] DPI scaling / HiDPI support (native scale factor via eframe/winit; egui's built-in Ctrl+plus/minus/0 zoom)
- [x] Window title with build name and class ("Name (Ascendancy [+ Secondary]) - Path of Building")
- [x] Confirmation popups for destructive actions (audited: builds, folders, items, socket groups, all set types, config reset, unsaved close, class switches, tree conversion; tree-spec delete confirmation added)
- [x] Status bar / toast notifications (transient save toast in the build view; per-section status messages in import/export and character import)
- [x] Loading indicators for async operations (progress bars for the incremental power reports; network buttons are labeled and report outcomes via status messages - blocking calls are a recorded consequence of the no-sub-script divergence)
- [x] Drag-and-drop support (items between slots and list-to-slot, gems within groups, socket group reorder)
- [x] Copy/paste support throughout (items copy/paste incl. advanced format, socket groups, tree URLs, build codes, notes via native editor)
- [x] Consistent theme/styling (deliberate divergence: our own egui theme with PoB color codes honoured; recorded in DIVERGENCES.md)
- [x] Responsive layout (egui layouts, scroll areas, and resizable panels throughout)
- [x] Wiki integration (F1 opens poewiki.net for hovered items/gems via upstream itemLib.wiki)
- Similar builds popup - deferred, see §16

---

## 15. Data & Infrastructure

- [x] Full item text parsing (via Lua's Item:ParseRaw, per the recommendation below)
- [x] Modifier evaluation and spawn weight calculation (upstream GetModSpawnWeight/CheckIfModIsDelve drive the affix lists)
- [x] Item modifier list building (upstream Item:BuildModList/Craft; invoked from all item mutations)
- [x] Gem data access (all through the Lua VM: FindSkillGem matching, GemSelectControl search/sort, GemTooltip rendering, data.gems lookups for imbued supports; no Rust-side gem database needed)
- [x] Build XML round-trip fidelity (load → save → load: structural fixed point + stats/counts preserved; hash-ordered sections compared as sets)
- [x] Sub-script system (deliberate divergence: Rust owns HTTP and background work, LaunchSubScript stays stubbed; recorded in DIVERGENCES.md)
- [x] Power calculation coroutine (upstream PowerBuilder driven via per-frame stepping with progress display)
- [x] Config condition evaluation (upstream ConfigVisibility predicates drive both the config tab's conditional visibility and the compare config view; mainEnv *Used tables read live from Lua)
- [ ] Asset extraction pipeline (Rust tool to extract from PoE GGPK/bundles)

---

## 16. Deferred

Items parked deliberately - revisit when the core parity work is done.

- [ ] Party tab for configuring party member effects
- [ ] Party aura/buff configuration
- [ ] Enemy modifier list from party
- [ ] Crucible modifier popup (5-node tree selection; Crucible is a past league, upstream keeps it for legacy builds)
- [ ] Hover shortcuts in the item-DB browser (F1 wiki and Ctrl+C copy work on hovered slot/list items but not in the unique/rare-template DB window; upstream's ItemDBControl supports both)
- [ ] Similar builds popup (PoB Archives integration; external service)
- [ ] PoB2 OAuth API import for character and trade (needs OAuth app registration; the legacy session-id import covers the use case)
- [ ] Named jewel themed art, item header/separator art (needs asset art beyond the submodule's icons)
- [ ] Ascendancy flavour text at high zoom (we do not render tree flavour text at all yet)
- [ ] Timeless jewel trade QoL and "Buy similar" trade integration (trade-site dependent)

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
- [x] Compare tab: side-by-side build comparison (upstream CompareEntry gives the comparison build a full calc environment in the same VM; import from the builds folder or share codes, entry management, and sub-views: Summary stat table (ported DrawStatList), Tree (named node/mastery diff lists + copy spec; graphical overlay on the tree renderer not wired), Items (slot union with match/missing/extra/different statuses + copy/equip), Skills (ported Jaccard group pairing with common/additional/missing gems incl. imbued), Config (ported section grouping, diffs first, copy config))
- [x] Compare calcs with "only show differences" filter (ported DrawCalcs filter + row-match/subsection-match; two-sided cells, filter defaults on like upstream)
- [x] Compare power report (upstream ComparePowerBuilder driven per-frame with progress bar; metric dropdown + five category toggles; primary panels refresh after a run since the builder mutates the primary temporarily)
- [x] Abyss sockets in comparison (item rows include the abyss-socket union; the power builder's abyss-jewel swap workaround comes from upstream)
- "Buy similar" trade integration - deferred, see §16 (trade-dependent)

### Import/Export
- PoB2 OAuth API import - deferred, see §16 (our legacy session-id import works)
- [x] pob.codes build export/import (import via api.pob.codes raw URLs; export in the share dropdown)
- [x] Preserve skill selection on character re-import (free: the snapshot/restore runs inside upstream ImportItemsAndSkills, which we call)
- [x] Remember league for imported characters (build-level lastLeague preselects the filter and is set on import; persisted by upstream's ImportTab saver)

### Skills/Gems
- [x] Progressive gem sort results while typing (upstream's DPSBuilder resumed one ~50ms slice per frame like upstream's Draw; suggestions appear immediately, DPS values fill in with a progress indicator and the list re-sorts as they land)
- [x] Imbued Supports (per-slot imbued dropdown on socketed groups via upstream's GemSelectControl imbued mode; level-1 extra support wired through RebuildImbuedSupportBySlot, calc pickup and XML persistence from upstream)
- [x] Gem tooltips (GemTooltip.AddGemTooltip called headless, shown on skills-tab gem-name hover)
- [x] Gem color indicators on socket group labels (R/G/B/W letters per gem on player group headers)
- [x] Sort gem suggestions by minion-specific stats (automatic: GemSelectControl's DPS sort uses the minion actor for minion skills; our sort-field dropdown matches upstream's sortGemTypeList exactly)

### Items/Crafting
- [x] Sorting in add-modifier, enchant, corrupt, and implicit popups (power-stat dropdown in all four, one calc pass per option/group/line/entry on first use, values shown next to each option)
- [x] Sinistral and Dextral catalysts (12-entry list synced to upstream's index order; drift-guarded in ports.toml)
- [x] Foulborn modifier toggles on uniques (Modifier Ranges dialog via right-click: per-line roll sliders + mutate checkboxes calling Item:MutateMod; MUTATED tint, title prefix and {mutated} raw round-trip come from upstream)
- [x] Volatile Vaal Orb corruption (Roll Ranges mode in the corrupt popup for uniques/relics: per-explicit sliders 0.78-1.22 with live applyRange preview; corruptedRange persisted on mod lines by upstream BuildRaw)
- [x] Increased-magnitude mods (Kane of Kulemak, Helical Ring, Heist enchants) (Item:ParseRaw/data path we call; free)
- [x] Advanced item copy/paste format (free via upstream Item:ParseRaw's advancedCopy handling; covered by test_advanced_copy_paste_format)
- [x] Warning for eligible items missing an anoint (flows through upstream's warning list we already display; asserted in tests)

### Tree/Power Report
- [x] Masteries in the power report (flows through BuildPowerReportList, which we call; asserted in tests)
- [x] Node description tooltips in the power report (node.sd lines from the report entries, shown on row hover)
- [x] Intuitive-Leap-aware power report distances (calc-side in PowerBuilder, which we drive; free)
- Timeless jewel trade QoL - deferred, see §16 (trade-dependent)
- Ascendancy flavour text - deferred, see §16 (we do not render ascendancy flavour text at any zoom yet)
- [x] Allocate ascendancy nodes through custom modifiers (calc-side in upstream CalcSetup's GrantedPassive lookup, which we call; the tree renderer does not yet highlight granted passives as allocated)

### UI/Options
- [x] Pinnable calc panes as overlay windows on other tabs (pin button on the breakdown panel; pinned breakdowns float as windows on every tab and refresh with the calcs)
- [x] Sidebar stat suffixes and compact value formatting toggle (suffixes are always-on upstream metadata we already display; Compact checkbox sets main.useCompactValues, consumed by upstream FormatStat)
- [x] Staged skills default to maximum stages (calc-engine change in called code; free)
- [x] Crafted cluster jewels default to minimum passives (default synced in cluster_craft_info)
- [x] Option to disable scroll wheel on controls (not applicable: egui controls do not capture scroll while a pane scrolls; recorded in DIVERGENCES.md)

### Removed upstream (parity items now obsolete)
- Alternate gem qualities (quality variant dropdowns, Show quality variants toggle) - removed in v2.66
- ImportTab:ProcessJSON - replaced by direct dkjson decoding
- Import status messages - import functions no longer report status text

---

## Upstream Delta: v2.66.1 - v2.67.0 (3.29 Allflame, August 2026)

74 upstream commits, ~4k changed lines across 45 files in `src/Classes/` and
`src/Modules/`. Everything below is new work unless marked otherwise.
Calc-engine and game-data changes (3.29 uniques, Pacts calcs, tree 3.29.1,
skill fixes) come free through the Lua VM and are collected at the end.

### Abyss Timeless Jewels (largest new feature)

Upstream added five timeless jewel types that work fundamentally differently
from Legion jewels: they conquer **allocated** nodes rather than nodes in a
radius, driven by a new lookup-table pipeline.

- [ ] Five new jewel types in the Find Timeless Jewel dialog (ids 7-11):
  Festering Vengeance (`abyss_murderous`), Extinguishing Grasp
  (`abyss_searching`), Baleful Dominion (`abyss_hypnotic`), Destructive
  Aspiration (`abyss_ghastly`), Reclaimed Malevolence (`abyss_special`)
- [ ] Their conqueror variants: Tecrod, Ulaman, Kurgal, Amanamu, Zorath
- [ ] New LUT pipeline: `Modules/DataAbyssJewelLookUpTableHelper` exports
  `readAbyssJewelLUT` / `resolveAbyssJewelComponent` /
  `getAbyssJewelComponentRoll`; `Modules/DataJewelFileLoader` and
  `Data/TimelessJewelData/AbyssNotableNames` are new. Our timeless search
  reads `readLUT` directly, so the abyss path needs separate wiring
- [ ] Allocated-node conquest model in `PassiveSpec:BuildAllDependsAndPaths`:
  abyss conquests are read *before* the node reset, and radius-based
  `conqueredBy` is suppressed for jewel types >= 7
- [ ] Zorath (type 11) needs `GetShortestPathToClassStart(socketId)` fed into
  the LUT read
- [ ] Zorath ascendancy-notable dropdown in the search dialog (built from
  `abyss_special_ascendancy_notable_*` nodes, alphabetical, with "Any");
  those nodes are excluded from the normal notable list
- [ ] "Protect allocated nodes" now applies to Zorath as well as Eternal
- [ ] Item text generation for all five new jewel types
  (`TimelessJewelListControl`, one template per type)
- [ ] Abyss tree art: connectors between two abyss-conquered nodes use
  `Abyss`-prefixed atlas assets with bounds-mapped UV quads; abyss
  notables/keystones/ascendancy notables get `Abyss*Frame*` overlays; abyss
  jewels draw no radius ring
- [ ] `PassiveSpec` root check relaxed (no longer requires `connectedToStart`)

### Config Tab (custom modifiers reworked, stat previews added)

- [x] Custom modifier **groups** replace the single custom-mods text box: each
  group has a title, enable checkbox, mod text, and delete button, with an
  "Add Mod Group" button. Mods are sourced as `Custom:<title>` (upstream's
  `customModsList` model and `BuildModList`; our editor also colours each line
  by `modLib.parseMod` the way upstream's `inactiveText` does. Ours commits on
  blur rather than per keystroke - see DIVERGENCES.md. The box is not
  drag-resizable; it grows with its content)
- [x] `<CustomModifierBlock title enabled>` XML persistence, plus migration of
  the legacy `customMods` input on load (`input.customMods` is then cleared) -
  entirely upstream's `ConfigTab:Load`/`Save`, which we call
- [x] Config undo state changed shape: `{ input, customModsList }` instead of
  a bare input copy (old states still restored via a fallback branch) -
  upstream's `CreateUndoState`/`RestoreUndoState`, which we call
- [ ] "Add Mod" popup per group: catalog built from `masterMods`, `itemMods`,
  `veiledMods` and `beastCraft`, collapsed by numeric template, sorted
  ignoring values, with fuzzy multi-word search scoring
- [ ] Stat-difference tooltips on config options: hovering a checkbox or a
  dropdown entry shows "Toggling this option will give you:" /
  "Selecting this option will give you:", cached per `outputRevision`.
  Number inputs deliberately excluded. `Build.lua` now refreshes
  `configTab.calcFunc` / `calcBase` on every rebuild
- [ ] Stat-difference tooltip on each custom-mod group's enable checkbox
  ("Enabling/Disabling this group will give you:")
- [ ] `countAllowZero` inputs accept negative and zero values; Enemy Fire /
  Cold / Lightning / Chaos Resistance moved from `integer` to
  `countAllowZero` (previously they could not be set to 0)
- [ ] New "Pact calc mode" list option (Average / Max Hit), gated on Pact skills
- [ ] "# of Brands attached to enemy (if not maximum)" is now gated by
  `ifSkillFlag = "brand"` instead of an enemy multiplier
- [ ] New "# of Permanent Minions (if not maximum)" count option
- Note: the "Add Mod" popup above is the one piece of the custom-modifier
  rework still outstanding; groups themselves are editable as free text.

### Items Tab

- [ ] Toggle individual mod lines by clicking them in the item tooltip:
  hover highlight box over the line, `disabled` line flag, struck-through
  rendering in `DISABLED` colour, round-tripped through the raw text.
  Requires per-line hit boxes (`Tooltip:AddLine` now takes `modLine` and
  `background`, and stores `bounds` per drawn line)
- [ ] Variant groups: new `Version` / `Selected Version` /
  `Selected Variant Group` / `Allow Duplicate Variants` specs, plus
  `{group:N}` and `{version:N}` mod-line tags. Items using them get a version
  dropdown and one dropdown per variant group (with duplicate-selection
  exclusion) instead of the fixed alt-variant dropdowns. Our variant code
  drives `variantAlt1-5` directly (`src/data/items.rs`, `src/data/crafting.rs`)
  and needs the group path added alongside the legacy one
- [ ] Persistent "Modifier sorting:" power-stat dropdown in the craft panel,
  replacing upstream's per-popup sorting. Hidden for cluster jewels. We
  shipped per-popup sorting in the v2.66 delta, so this is a consolidation
- [ ] Abyssal sockets rendered as real item slots per equipment slot
  (including weapon swap), not just tooltip text
- [ ] Item list toolbar reworked: loadout filter dropdown (Any Loadout /
  Current Loadout / Unused Items / per-loadout entries), Sort button moved
  into the list, Delete / Del All / Del Unused renamed and reordered
- [ ] Second enchantment button ("Change Enchantment 2...")
- [ ] Rare-like uniques (`data.rareLikeUniques`): Subsume the Source, The
  Crimson Storm, and Dread Captain's Cutlass craft like rares with their own
  affix pools, prefix/suffix limits, `ignoreModType`, and a restricted set of
  custom-modifier sources
- [ ] Fractured affixes round-trip through `Prefix`/`Suffix` specs (a
  `{fractured}` marker) and per-value roll lists (`{range:a,b,c}`);
  `itemLib.applyRange` now accepts a table of per-value ranges, which our
  range-slider previews call directly
- [ ] Advanced copy/paste fixes: `current(base)` fixed-value form, enum
  ranges, independent per-value rolls, min/max swap, exact-vs-fallback affix
  matching, and stat-order sorting of unique explicit lines. Our
  `test_advanced_copy_paste_format` conformance test should be re-checked
- [ ] Vestigial items: `vestigial` line flag, `Vestigial ` base-name prefix,
  `VESTIGIAL` colour code, plus `Intangibility` and `Memory Strands` specs
  and their tooltip icons
- [ ] Attribute requirements on socketed items derived from base + local mods
  rather than the imported total
- [ ] Crucible passive lines auto-tagged `{crucible}` on parse
- [ ] Flask in-game state lines ("Lasts N Seconds", "Consumes N of N Charges
  on use", "Currently has N Charges") no longer parsed as modifiers
- [ ] Legacy Talisman bases excluded from anointing (`isAnointable`)
- [ ] Enchantments no longer copied when editing an item
- [ ] Tooltip lines can carry a background image (gem mod lines, desecrated mods)
- [ ] Add Implicit crash fix (affects the popup we ported)

### Skills Tab

- [ ] "Item sockets:" label plus an "Optimise Sockets" button on the socket
  group editor: rewrites the equipped item's socket colours and link groups
  to match the group's gems, preserving abyssal socket count
- [ ] Gem quality from a matching socket colour
  (`data.MatchingSocketQualityBonus = 10`); `CalcSetup` now tracks quality by
  source and the gem tooltip breaks it down (Item / Support / Global
  Modifiers / Socket Colour) instead of one combined line
- [ ] "Light Radius" added to the stat sort list
- [ ] Gem dropdown fixes: case-insensitive selection sync, re-sort whenever
  the list is rebuilt (`SortCurrentList` / `SyncSelection`)
- [ ] Socket groups reprocessed after build load so item-granted groups
  resolve regardless of load order

### Build List

- [ ] Recursive search: the build tree is indexed once into `buildIndex`, and
  filtering searches nested folders, showing each hit with its relative
  subpath prefix
- [ ] `class:<name>` search term (placeholder `(e.g. class:assassin myfilename)`)
- [ ] Build headers read from the first 2KB and pattern-matched instead of
  parsing the whole file as XML
- [ ] Guard against copying or moving a folder into itself, in both the
  drag-drop and the copy/move paths
- [ ] Duplicate-name resolution fixed to produce `name[1].xml` rather than a
  bare `name[1]`; rename/move failures surface an error popup
- [ ] Selection tracked by full file name (`SelByFullFileName`) rather than
  bare file name, so nested results select correctly

### Compare Tab

- [ ] Comparison build picker inherits the recursive search, `class:` filter,
  and full-path selection

### Calcs Tab

- [ ] New "Pacts" section: Empowered Spells table (uptime, count,
  projectiles, beam chains, cascades) for Pact of Beidat / Ghorr / K'Tash /
  Lycia. Flows through `CalcSections`, which we read, so this may be free
  once the data is present
- [ ] Cost sections restructured around per-resource cost/efficiency stat
  groups (mana, life, ES, rage)

### Import/Export

- [ ] Animate Guardian items import into a dedicated "Animate Guardian" item
  set, auto-assigned to the AG gem's `skillMinionItemSet`. `charData.guardian`
  is populated inside `DownloadPassiveTree` and `DownloadItems`, both of which
  we ported, so this needs the port re-sync below plus the new item-set logic
- [ ] Character re-import keeps bandit and pantheon choices when the API omits
  them. Also inside `DownloadPassiveTree`, so it arrives with that port
  re-sync rather than for free
- [ ] "Importing..." button state distinguished from "Fetching..."
- [ ] Reimport of skills no longer shows stale gem data. Upstream's fix clears
  `skillsTab.controls.groupList` selection inside `ImportItemsAndSkills`,
  which is UI-control state we deliberately do not depend on. Not applicable
  as written, but our own skills-tab selection should be verified to reset
  after a re-import
- OAuth login window 30s -> 60s and URL copied to clipboard - deferred, see
  §16 (we do not implement OAuth import)

### Search & Controls

- [ ] `SearchHost` gains `ignoreOrder`: multi-word matches no longer require
  left-to-right order, and overlapping highlight ranges are merged. Used by
  dropdown search
- [ ] Dropdown highlight offsets computed from escape-stripped labels (colour
  codes no longer skew highlight positions)
- [ ] `EditControl` draws a placeholder string when empty
- [ ] `ListControl` click handlers can veto selection by returning `false`
- [ ] New engine global `GetDrawColor()` (the only one added this release).
  Used at `Tooltip.lua:609` in `Tooltip:Draw` to save the draw colour before
  painting a mod-line background. We read `tooltip.lines` rather than calling
  `Draw`, so it should not be reached, but `src/lua_bridge/stubs.rs` needs it
  to avoid a nil-call if any path ever does.

### Free through the Lua VM (no Rust work)

Pacts support, new 3.29 uniques, Vestigial and Intangibility mod parsing, the
3.29.1 tree update, Staff Life and Mana Mastery, automatic Brand-count and
Wintertide-debuff DPS, and the calc/behaviour fixes: Soulwrest Phantasmal
Might, Mana-Infused Staff, Communion Support, Cleave and Vaal Cleave radius,
Herald area scaling, Howlcrack life cost, Drillneck, shield block chance,
Inquisitor Fanaticism, Raise Spider attack speed, Devastator corpse explode,
Spellslinger trigger selection, Reap of Butchery radius, the cost-efficiency
revert, and Cane of Kulemak Catarina veiled mods.

### Deferred - trade integration (see §16)

- Currency Exchange API replacing poe.ninja in the trader
- Talisman enchants and item quality on traded items; trader explicit-parsing fix
- Pseudo-stat and word-order-insensitive stat search in `TradeQueryGenerator`

### Registered ports needing re-sync

Verified by extracting each `ports.toml` anchor at both pins. These seven
anchored upstream functions changed and will fail `cargo test --test
ports_sync` after the pin moves:

| Port | Upstream file |
|------|---------------|
| `timeless-seed-search` | `src/Classes/TreeTab.lua` |
| `timeless-fallback-weights` | `src/Classes/TreeTab.lua` |
| `timeless-fallback-node-building` | `src/Classes/TreeTab.lua` |
| `timeless-stat-list` | `src/Classes/TreeTab.lua` |
| `char-import-tree-shaping` | `src/Classes/ImportTab.lua` |
| `char-import-items-shaping` | `src/Classes/ImportTab.lua` |
| `add-modifier-popup` | `src/Classes/ItemsTab.lua` |

Verified causes: the four `timeless-*` ports changed for the abyss jewel
types; `char-import-tree-shaping` for `charData.guardian` plus the
bandit/pantheon preservation, and `char-import-items-shaping` for
`charData.guardian`; `add-modifier-popup` for the rare-like unique
`supportsCustomModifiers` filter and a nil guard on `listMod` (the Add
Implicit crash fix). No anchor went missing.

---

## Upstream Delta: v2.67.0 - v2.67.2 (3.29 Allflame, August 2026)

Two patch releases, 25 upstream commits, 13 files / ~121 insertions across
`src/Classes/` and `src/Modules/`. Almost entirely fixes; the data side is a
regenerated `ClusterJewels.lua` plus small `ModCache` / skill / stat-description
touch-ups and a 3.29.2 export. Reviewed against the still-current v2.67.0 pin.

**Ports:** no registered `ports.toml` anchor changed. Only `ItemsTab.lua` is
touched among our port sources, and neither `add-modifier-popup` nor
`check-line-for-allocates` falls in the changed region, so `ports_sync` stays
green through this bump.

**Stubs:** no new engine-level globals in the diff.

### New work

- [ ] Trade query generator supports pseudo stats in its **weights**
  ([\#10085](https://github.com/PathOfBuildingCommunity/PathOfBuilding/pull/10085)).
  Extends the already-tracked "Pseudo-stat and word-order-insensitive stat
  search in `TradeQueryGenerator`" in the v2.67.0 delta's deferred-trade block
  above; both land together whenever trade integration is taken on (§16). No
  separate work item.

Nothing else in these two releases is new work for us.

### Free through the Lua VM (no work)

- Calc fixes: Scornful Herald not counting buffs as affecting you (#10158),
  The Unblinking Eye evasion not applying to Arcane Might attacks (#10155),
  Chip Away not using a global limit and Foulgrasp not counting toward the
  Brand limit (#10141), block-chance rounding with Mana-Infused Staff (#10142).
- Crash when equipping a Quiver or Shield that grants a skill (#10144):
  `CalcActiveSkill.lua` plus skill data.
- `itemLib.applyRange` gains the `locations_to_metres` value format, so a 0.1m
  weapon-range Harvest enchant no longer rounds to zero (#10133). We call
  `applyRange` directly (`crafting.rs`), so this arrives with the pin.

### Not applicable to us

- **Double-click-then-drag fixes** in `ItemDBControl` / `ItemListControl`
  (#10134, #10149) and the `ItemDBClass:GetRowValue` nil guard. These clear
  `selDragging` inside upstream's own SimpleGraphic list controls, where a
  double click could leave an item stuck to the cursor. Our item lists are
  egui and hold no drag state across a double click.
- **Missing modifier controls on imported items** (#10138). Upstream gates its
  `displayItemSectionAffix` on `displayItem.crafted` and re-anchors the custom
  modifier section so it still lays out when the affix section is hidden - a
  fix to their single display-item editor. Ours is a rarity-gated
  "Add modifier..." context-menu entry (`items_tab.rs`, MAGIC/RARE) that never
  depended on `crafted`, so imported items already get it.
- **Build list sort by relative path** (#10131) and **recursion stopping on a
  file error** (#10130). Ours is a per-directory Rust scan
  (`build_list::scan_builds`), not upstream's recursive `BuildListHelpers`: one
  listing's entries all share a `subPath`, so prefixing it cannot reorder them,
  and an unreadable entry or unparseable header is skipped rather than aborting
  the scan. Both bugs are structurally absent.
- **"Buy Similar" searches for punctuated unique names** (#10139): trade
  integration, deferred (§16).
- **`BreachIcon.png` renamed to `breachicon.png`** for case-sensitive
  filesystems, fixing the Foulborn icon on Linux (#10160). We reference no
  asset by that name.

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

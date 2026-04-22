# Feature Parity Plan: egui-pob vs Upstream PoB

This document tracks every feature needed to reach parity with upstream Path of Building Community. Items are grouped by area rather than priority — ordering and phasing will be decided separately.

Status key: `[x]` done, `[~]` partial, `[ ]` not started

---

## 1. Build Management

### Build List
- [x] Scan and display builds from build directory
- [x] Folder navigation (enter, go up)
- [x] Refresh build list
- [x] Open build by clicking
- [x] New build creation
- [ ] Delete build
- [ ] Rename build
- [ ] Move build to folder
- [ ] Create new folder
- [ ] Sort builds (by name, date modified)
- [ ] Build search/filter by name
- [ ] Recent builds list
- [ ] Build preview tooltip on hover (class, level, DPS summary)

### Save System
- [x] Save build to disk (Save)
- [x] Save As (new name, for new builds)
- [ ] Save As with folder browser (like upstream's folder list + new folder)
- [ ] Save confirmation popup on close/switch with unsaved changes
- [ ] Auto-save / dirty-state tracking

### Loadout System
- [ ] Multiple loadouts per build (linked tree/items/skills/config sets)
- [ ] Loadout dropdown in top bar
- [ ] Create/delete/rename loadouts
- [ ] Sync between loadout tabs

---

## 2. Character Header (Top Bar)

- [x] Back button to build list
- [x] Build name display
- [x] Class dropdown
- [x] Ascendancy dropdown
- [ ] Secondary ascendancy (bloodline) dropdown
- [x] Character level edit field (1-100)
- [x] Level scaling mode toggle (Auto/Manual)
- [x] Passive points used display (N / M)
- [x] Ascendancy points used display
- ~~Bandit reward selection~~ (covered by Config tab)
- ~~Pantheon major god selection~~ (covered by Config tab)
- ~~Pantheon minor god selection~~ (covered by Config tab)
- [ ] Experience multiplier tooltip on level hover

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
- [ ] Connector state coloring (path preview, intermediate, dependent)
- [ ] Red highlight for dependent nodes (nodes that would disconnect)
- [ ] Path preview line (shortest path to unallocated node on hover)

### Interaction
- [x] Click to allocate/deallocate node
- [x] Hover tooltip with stats, name, type, recipe, flavour text
- [ ] Right-click context menu (jump to items for jewel sockets, modify node for tattoos)
- [ ] Mastery popup (select mastery effect on click)
- [ ] Shift+drag path tracing mode
- [ ] Ascendancy node click → class/ascendancy switching with confirmation

### Search
- [ ] Tree search bar (text search across node names and stats)
- [ ] Highlighted search results (colored rings on matching nodes)
- [ ] Lua pattern matching support
- [ ] "oil:" prefix for anoint recipe search
- [ ] Multi-term search (OR matching)
- [ ] Ctrl+F to focus search

### Node Power
- [ ] Show node power heatmap toggle
- [ ] Power stat selection dropdown (DPS, Defense, etc.)
- [ ] Node power max depth controls
- [ ] Power report list (sortable table of node values)
- [ ] Click node in power report to pan to it
- [ ] Color-coded power visualization (red=offense, blue=defense)

### Jewel Sockets
- [ ] Jewel radius display (ring overlay on allocated sockets)
- [ ] Thread of Hope annular display (inner + outer ring)
- [ ] Impossible Escape keystone radius display
- [ ] Named jewel overlays (Brutal Restraint, Elegant Hubris, etc.)
- [ ] Cluster jewel subgraph rendering
- [ ] Right-click jewel socket → jump to items tab

### Comparison
- [ ] Compare checkbox to enable comparison mode
- [ ] Compare spec dropdown to select comparison tree
- [ ] Green/red node diff coloring (allocate/deallocate indicators)
- [ ] Blue mastery diff (different effect selected)

### Tree Specs
- [ ] Multiple tree specs per build
- [ ] Spec management popup (create, delete, rename, reorder)
- [ ] Import tree from URL (PoE official, PoePlanner, PoESkillTree, PoEURL)
- [ ] Export tree as URL
- [ ] Copy tree URL to clipboard
- [ ] PoEURL shortlink generation

### Tree Version
- [ ] Tree version dropdown
- [ ] Convert to latest version button
- [ ] Convert all trees button
- [ ] Version mismatch warning banner
- [ ] Conversion confirmation popup

### Tattoos
- [ ] Modify node popup (tattoo selection)
- [ ] Tattoo modifier dropdown
- [ ] Legacy tattoo toggle
- [ ] Tattoo count tracking (max 50)
- [ ] Remove tattoo from node

### Timeless Jewels
- [ ] Find Timeless Jewel dialog
- [ ] Jewel type selection (6 types)
- [ ] Conqueror variant selection
- [ ] Socket selection (multi or specific)
- [ ] Devotion modifier selection
- [ ] Node search and weighting system
- [ ] Fallback weight mode

### Undo/Redo
- [ ] Ctrl+Z to undo tree changes
- [ ] Ctrl+Y to redo
- [ ] Undo state snapshots (nodes, masteries, jewels, overrides)

---

## 4. Skills Tab

### Socket Group Management
- [~] Display socket groups with gems (read-only)
- [x] Main skill selection ("Set Main" button)
- [ ] Create new socket group
- [ ] Delete socket group (with confirmation if gems exist)
- [ ] Delete all socket groups
- [ ] Reorder socket groups (drag and drop)
- [ ] Copy/paste socket groups (Ctrl+C/V)
- [ ] Enable/disable socket group (Ctrl+Click)
- [ ] Include/exclude from FullDPS (Ctrl+Right-Click)
- [ ] Socket group label editing
- [ ] Socket group slot assignment (socketed in dropdown)
- [ ] Socket group count multiplier

### Gem Management
- [~] Display gem name, level, quality (read-only)
- [ ] Add gem to socket group
- [ ] Remove gem from socket group
- [ ] Gem search/autocomplete (GemSelectControl)
  - [ ] Name search, tag search (`:tag`), exclusion (`-tag`)
  - [ ] Sort by DPS impact
  - [ ] Color-coded gem types (Str/Dex/Int)
  - [ ] Support relationship indicators (check mark)
- [ ] Edit gem level
- [ ] Edit gem quality
- [ ] Quality variant selection (Default, Anomalous, Divergent, Phantasmal)
- [ ] Enable/disable individual gem
- [ ] Gem count (for totems, minions, traps, mines)
- [ ] Vaal gem global effect toggles

### Gem Options
- [ ] Sort gems by DPS toggle with stat selector
- [ ] Default gem level dropdown (Normal Max, Corrupted Max, etc.)
- [ ] Default gem quality input
- [ ] Show support gem type filter
- [ ] Show quality variants toggle
- [ ] Show legacy gems toggle

### Skill Sets
- [ ] Multiple skill sets per build
- [ ] Skill set management popup
- [ ] Switch between skill sets

---

## 5. Items Tab

### Equipment Display
- [~] Display equipped items by slot (read-only, basic info)
- [ ] Full item tooltip (matching upstream format with rarity headers, DPS, armor stats)
- [ ] Item rarity styling (borders, headers matching upstream)
- [ ] Socket and link display
- [ ] Influence icons display
- [ ] Flask display with charges/duration
- [ ] Weapon DPS breakdown in tooltip (Physical, Elemental, Chaos, Total)
- [ ] Armor stats breakdown (Armour, Evasion, Energy Shield, Ward)

### Item Management
- [ ] Item list panel (all owned items)
- [ ] Equip item to slot
- [ ] Unequip item from slot
- [ ] Delete item
- [ ] Sort item list
- [ ] Drag items between slots

### Item Editing
- [ ] Edit item text (raw text editor)
- [ ] Variant selection dropdown (for multi-variant uniques)
- [ ] Alt variant dropdowns (up to 5)
- [ ] Socket color selection (R/G/B/W per socket)
- [ ] Link toggles between sockets
- [ ] Quality edit
- [ ] Influence selection (2 dropdowns: Shaper, Elder, Warlord, etc.)
- [ ] Catalyst type and quality

### Item Creation
- [ ] Craft item popup (select base type, rarity)
- [ ] Affix selection (prefix/suffix dropdowns with tier selection)
- [ ] Affix range sliders
- [ ] Custom modifier popup (Crafting Bench, Essence, Veiled, Beastcraft, etc.)
- [ ] Paste item from clipboard (Ctrl+V)

### Enchanting & Anointing
- [ ] Apply enchantment popup (helmet/gloves/boots)
- [ ] Apply anoint popup (notable search + oil recipe)
- [ ] Multiple anoint slots (up to 4)
- [ ] Stat comparison preview for anoints

### Corruption & Influence
- [ ] Corrupt item popup (implicit mod selection)
- [ ] Add implicit popup (Exarch, Eater, Delve, Synthesis, Custom)
- [ ] Crucible modifier popup (5-node tree selection)

### Cluster Jewels
- [ ] Cluster jewel skill dropdown
- [ ] Node count slider
- [ ] Craft cluster jewel mods

### Item Comparison
- [ ] Stat diff tooltip when hovering unequipped items
- [ ] Side-by-side comparison view

### Item Database
- [ ] Unique item database browser
- [ ] Rare template database browser
- [ ] Search and filter in databases

### Item Sets
- [ ] Multiple item sets per build
- [ ] Item set dropdown
- [ ] Item set management
- [ ] Weapon swap support

### Undo/Redo
- [ ] Ctrl+Z / Ctrl+Y for item changes

---

## 6. Calcs Tab

- [ ] Calcs tab (full calculation breakdown display)
- [ ] Socket group / active skill / skill part selectors
- [ ] Calculation mode dropdown (Unbuffed, Buffed, In Combat, Effective DPS)
- [ ] Expandable stat sections (Offense, Defense, etc.)
- [ ] Click stat to show detailed breakdown
- [ ] Breakdown panel (right side) with formula/steps
- [ ] Pin breakdown to keep visible
- [ ] Search bar for stat filtering (Ctrl+F)
- [ ] Buff/debuff lists (auras, combat buffs, curses)
- [ ] Show minion stats toggle
- [ ] Minion selection and skill dropdowns

---

## 7. Config Tab

- [x] Display all config option types (checkbox, count, list, text)
- [x] Change config values and trigger recalc
- [ ] Config search/filter bar
- [ ] Show/hide ineligible configurations toggle
- [ ] Section headers with collapsible groups
- [ ] Conditional option visibility (ifNode, ifOption, ifCond dependencies)
- [ ] Tooltips for config option explanations
- [ ] Config sets (multiple independent configs per build)
- [ ] Config set management popup
- [ ] Reset to defaults
- [ ] Undo/redo for config changes

---

## 8. Notes Tab

- [ ] Notes tab (large multiline text editor)
- [ ] Color code support (PoB color tags: `^7`, `^xHEXCODE`)
- [ ] Color code buttons (Normal, Magic, Rare, Unique, Fire, Cold, etc.)
- [ ] Show/hide color codes toggle
- [ ] Ctrl+Z/Y undo/redo within editor
- [ ] Zoom support (Ctrl+scroll)

---

## 9. Party Tab

- [ ] Party tab for configuring party member effects
- [ ] Party aura/buff configuration
- [ ] Enemy modifier list from party

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
- [ ] Account name input with realm selection (PC, Xbox, PS4, etc.)
- [ ] POESESSID input for private profiles
- [ ] Download character list from PoE API
- [ ] League filter dropdown
- [ ] Character selection dropdown
- [ ] Import passive tree and jewels (with optional jewel clearing)
- [ ] Import items and skills (with options to delete existing)
- [ ] Account history tracking
- [ ] Privacy settings link

### Import Modes
- [ ] Import to current build vs. create new build toggle

---

## 11. Main Skill Selection (Sidebar)

- [x] Stat sidebar with key stats
- [x] Main socket group dropdown
- [x] Main active skill dropdown
- [x] Skill part dropdown (for multi-part skills)
- [x] Skill stage count input
- [x] Active mines count input
- [ ] Minion type dropdown
- [ ] Manage Spectres button + spectre library popup
- [ ] Minion skill dropdown

---

## 12. Stat Display & Warnings

- [x] Key stats in sidebar (DPS, Life, ES, Mana, Resistances, etc.)
- [x] Number formatting (commas, percentages, decimals)
- [x] Color-coded stats
- [ ] Full stat list (all 203+ stats from BuildDisplayStats.lua)
- [ ] Conditional stat display (condFunc filtering)
- [ ] Warning messages panel:
  - [ ] Too many passive/ascendancy points
  - [ ] Missing item requirements
  - [ ] Skill cost vs. pool warnings
  - [ ] Gem socket limit warnings
  - [ ] Jewel limit warnings
  - [ ] Aspect skill conflicts
- [ ] Clickable warnings (jump to relevant tab)
- [ ] Minion stat display toggle
- [ ] DPS breakdown by source/trigger in stat list

---

## 13. Keyboard Shortcuts

- [ ] Ctrl+S: Save build
- [ ] Ctrl+W: Close build (with save prompt)
- [ ] Ctrl+Z: Undo (context-dependent: tree, items, config)
- [ ] Ctrl+Y: Redo
- [ ] Ctrl+F: Focus search (tree, calcs, config)
- [ ] Ctrl+I: Open Import/Export
- [ ] Ctrl+1-7: Switch tabs (Tree, Skills, Items, Calcs, Config, Notes, Party)
- [ ] Ctrl+V: Paste item (in items tab)
- [ ] Ctrl+C: Copy (context-dependent)
- [ ] Ctrl+E: Edit equipped item
- [ ] Ctrl+D: Toggle stat differences
- [ ] F1: Open wiki for hovered item/gem
- [ ] Mouse4: Close build

---

## 14. UI Polish & UX

- [ ] Global undo/redo system
- [ ] Tooltip positioning (avoid screen edges)
- [ ] DPI scaling / HiDPI support
- [ ] Window title with build name and class
- [ ] Confirmation popups for destructive actions
- [ ] Status bar / toast notifications
- [ ] Loading indicators for async operations
- [ ] Drag-and-drop support (items, gems, socket groups)
- [ ] Copy/paste support throughout
- [ ] Consistent theme/styling matching upstream
- [ ] Responsive layout for different window sizes
- [ ] Wiki integration (F1 to open wiki for items/gems)
- [ ] Similar builds popup (from PoB Archives)

---

## 15. Data & Infrastructure

- [ ] Full item text parsing (Item.lua equivalent in Rust or via Lua)
- [ ] Modifier evaluation and spawn weight calculation
- [ ] Item modifier list building (local mods, quality scaling, DPS calc)
- [ ] Gem data access (tags, requirements, stats, descriptions)
- [ ] Build XML round-trip fidelity (load → save → load produces same build)
- [ ] Sub-script system (LaunchSubScript for background tasks)
- [ ] Power calculation coroutine (async node power evaluation)
- [ ] Config condition evaluation (mainEnv tracking)
- [ ] Asset extraction pipeline (Rust tool to extract from PoE GGPK/bundles)

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

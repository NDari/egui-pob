# Plan: pob-egui — Rust + egui Frontend with LuaJIT Backend

## What This Document Is

Implementation plan for a Path of Building (PoB) frontend using **Rust + egui** for the GUI and **embedded LuaJIT** (via mlua) for the calc engine/data model. Upstream PoB is a read-only git submodule. The Rust app loads upstream Lua code headless (rendering stubbed), then renders everything with egui. Zero upstream modifications, zero merge conflicts.

**Upstream repo:** `https://github.com/PathOfBuildingCommunity/PathOfBuilding.git` (pinned to v2.63.0)

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│              Rust + egui (you own)                │
│  src/gui/*.rs — egui panels, widgets, tree view  │
│  Reads from / writes to Lua via mlua             │
└───────────────────┬──────────────────────────────┘
                    │  mlua FFI bridge
                    │  read: lua.globals().get("build")...
                    │  write: build_table.set("characterLevel", 95)
                    │  call: run_callback("OnFrame")
┌───────────────────┴──────────────────────────────┐
│          LuaJIT VM (embedded via mlua)            │
│  Upstream code runs headless                      │
│  System functions implemented in Rust             │
│  Rendering/input stubbed as Lua no-ops            │
│  Classes/ manage data model + game logic          │
│  Modules/Calc*.lua produce numbers                │
│  Data/ provides game constants                    │
└──────────────────────────────────────────────────┘
```

Upstream's `HeadlessWrapper.lua` already proves this works — it loads the entire app with stubbed rendering, creates builds, runs calcs, and exposes `build.itemsTab`, `build.skillsTab`, `build.spec`, etc. We do the same but with a real GUI on top.

### Why Rust + egui

- **egui is immediate mode** — perfect for PoB's data-viewer UI (tables, trees, config panels, tooltips). All built-in.
- **Single binary distribution** — no runtime dependencies, no shared libs
- **Native performance** for tree rendering (thousands of nodes with pan/zoom)
- **Modern Rust ecosystem**: reqwest (HTTP), flate2 (compression), arboard (clipboard), image (decoding)
- **Long-term optionality** — can progressively move hot paths from Lua to Rust
- **Upstream sync is trivial** — update the submodule pointer, done. Upstream UI refactors are invisible.

### What We Reuse from Upstream (via Headless LuaJIT)

- **Calc engine** (`Modules/Calc*.lua`): ~18K lines of pure computation
- **Data model classes** (`Classes/`): Build state management, item parsing, tree allocation, skill socketing
- **Static data** (`Data/`): Gems, uniques, bases, mods, tree data (~3.5M lines)
- **Core infrastructure** (`Modules/Common.lua`, `ModParser.lua`, `ModTools.lua`, `ItemTools.lua`, `CalcTools.lua`)
- **Build I/O**: XML save/load, import/export, build codes

### What Rust Provides

- **GUI**: egui (via eframe) — all rendering, layout, widgets, input
- **System functions**: Rust implements `GetTime`, `Deflate`, `Copy`, `MakeDir`, etc. and registers them as Lua globals via mlua
- **HTTP**: reqwest (upstream's `require("lcurl.safe")` returns nil — HTTP handled from Rust side)
- **Compression**: flate2 (`Deflate`/`Inflate` globals)
- **Clipboard**: arboard (`Copy`/`Paste` globals)
- **File I/O**: std::fs
- **Sub-scripts**: std::thread + separate Lua VMs (TODO)

---

## Current State (What's Already Implemented)

### Working

- Rust project compiles and runs
- LuaJIT VM boots with upstream PoB v2.63.0 loaded headless
- `mainObject_ref` exists (boot verification passes)
- egui window opens (placeholder UI)
- All ~50 SimpleGraphic API functions stubbed (rendering, input, images, file search, sub-scripts)
- Working system functions: time, paths, clipboard, compression, module loading, file ops, callbacks, console output
- `require("lcurl.safe")` stubbed to return nil (same as HeadlessWrapper)
- Path detection works for both dev mode (cargo run) and distribution

### Repository Structure

```
egui-pob/
├── upstream/                        # git submodule @ v2.63.0 (READ-ONLY)
│   ├── src/                         # All upstream Lua source
│   ├── runtime/                     # Runtime utilities
│   └── manifest.xml
├── src/                             # Rust source
│   ├── main.rs                      # Entry point: path detection, init Lua, launch eframe
│   ├── lua_bridge/                  # Lua ↔ Rust interface
│   │   ├── mod.rs                   # LuaBridge struct: VM setup, headless bootstrap, run_frame
│   │   ├── stubs.rs                 # SimpleGraphic API stubs (rendering/input no-ops)
│   │   └── system.rs               # Working system functions (time, paths, clipboard, etc.)
│   ├── gui/
│   │   └── mod.rs                   # PobApp: eframe::App impl (placeholder UI)
│   └── data/
│       └── mod.rs                   # Placeholder for marshaled data types
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── .gitmodules
└── PLAN.md                          # This file
```

### Key Dependencies (Cargo.toml)

```toml
eframe = "0.31"                      # egui + windowing + rendering
egui = "0.31"
egui_extras = { version = "0.31", features = ["image"] }
mlua = { version = "0.10", features = ["luajit", "vendored", "serialize", "send"] }
reqwest = { version = "0.12", features = ["blocking"] }
flate2 = "1"
arboard = "3"                        # Clipboard
image = "0.25"                       # Image loading (tree assets)
directories = "6"                    # XDG/AppData user paths
walkdir = "2"                        # Directory traversal
glob = "0.3"                         # Glob pattern matching
open = "5"                           # Open URLs in browser
anyhow = "1"
log = "0.4"
env_logger = "0.11"
```

- **`mlua` with `luajit` + `vendored` + `send`**: Embeds LuaJIT directly, compiles from source. The `send` feature makes mlua::Error compatible with anyhow.
- **`eframe`**: Standard egui integration — window, event loop, rendering (wgpu/glow).
- **No Lua libraries needed**: all system functions (HTTP, compression, clipboard, file I/O) are implemented in Rust.

---

## The Lua ↔ Rust Data Interface

### Data Marshaling Strategy: Hybrid

**Hot data (marshaled after each calc, used every frame):**
```rust
/// Extracted from env.player.output after each recalc
struct CalcOutput {
    stats: HashMap<String, f64>,      // "TotalDPS" → 1234567.8
    flags: HashMap<String, bool>,     // "attack" → true, "spell" → false
}
```

Read from Lua after each recalc:
```rust
fn extract_calc_output(lua: &Lua) -> CalcOutput {
    let build: Table = lua.globals().get("build")?;
    let calcs_tab: Table = build.get("calcsTab")?;
    // calcsTab stores the last calc environment
    // navigate to env.player.output
    let mut stats = HashMap::new();
    for pair in output.pairs::<String, f64>() {
        let (k, v) = pair?;
        stats.insert(k, v);
    }
    CalcOutput { stats, flags }
}
```

**Cold data (read lazily via mlua handles, only when needed):**
- Item tooltips (only on hover) — read item mods/affixes from Lua table on demand
- Skill gem details (only when skills tab is open)
- Tree node descriptions (only on node hover)
- Config option definitions (read once at build load, re-read on upstream update)

**Write-back (when user interacts):**
```rust
// User allocates a tree node
fn alloc_node(lua: &Lua, node_id: u32) {
    let build: Table = lua.globals().get("build")?;
    let spec: Table = build.get("spec")?;
    spec.call_method::<()>("AllocNode", node_id)?;
    build.set("buildFlag", true)?;
    run_callback(lua, "OnFrame")?;
}
```

### Display Stats Interface

Upstream's `BuildDisplayStats.lua` defines ~203 stat entries with format strings, labels, and conditional display functions:

```rust
struct DisplayStat {
    stat: String,        // "TotalDPS"
    label: String,       // "Hit DPS"
    fmt: String,         // ".1f"
    lower_is_better: bool,
}
```

For conditional display (`condFunc`), call back into Lua:
```rust
fn should_show_stat(lua: &Lua, stat_def: &LuaTable, value: f64, output: &LuaTable) -> bool {
    if let Ok(cond_func) = stat_def.get::<Function>("condFunc") {
        cond_func.call::<bool>((value, output)).unwrap_or(true)
    } else {
        true
    }
}
```

### Config Options Interface

Config options are defined in `Modules/ConfigOptions.lua` as a Lua table with types `check`, `count`, `list`:

```rust
enum ConfigOption {
    Check { var: String, label: String, value: bool },
    Count { var: String, label: String, value: i64 },
    List { var: String, label: String, options: Vec<(String, String)>, selected: usize },
}
```

When user changes a value:
```rust
fn set_config(lua: &Lua, var: &str, value: mlua::Value) {
    let build: Table = lua.globals().get("build")?;
    let config_tab: Table = build.get("configTab")?;
    let input: Table = config_tab.get("input")?;
    input.set(var, value)?;
    build.set("buildFlag", true)?;
    run_callback(lua, "OnFrame")?;
}
```

### Passive Tree Interface

Tree data is read once when a build loads:

```rust
struct TreeNode {
    id: u32,
    x: f32, y: f32,
    name: String,
    node_type: String,      // "Normal", "Notable", "Keystone", etc.
    is_allocated: bool,
    mods: Vec<String>,
}
```

Read from `build.spec.nodes`. Re-read allocation state after each recalc. Tree structure (positions, connections) is static per tree version — cache it. Render using egui's `Painter` API with pan/zoom transform.

---

## The Data Model Interface (Lua Side)

### What the calc engine reads from `build`:

```lua
build.data                              -- static game data tables
build.characterLevel                    -- number
build.configTab.input                   -- config option values
build.configTab.placeholder             -- config placeholder values
build.configTab.modList                  -- modifier list from config
build.configTab.enemyModList             -- enemy modifier list
build.configTab.enemyLevel               -- number
build.calcsTab.input                     -- calc tab input
build.spec                               -- PassiveSpec: allocNodes, tree, jewels
build.spec.treeVersion                   -- string like "3_28"
build.itemsTab.orderedSlots              -- item slot list
build.itemsTab.items                     -- item table (id → item object)
build.itemsTab.activeItemSet             -- active item set
build.itemsTab.slots                     -- slot table (name → slot object)
build.skillsTab.socketGroupList          -- socket group list
build.mainSocketGroup                    -- index of main skill
build.partyTab.enemyModList              -- party tab enemy mods
```

### What the calc engine produces:

```lua
env.player.output                        -- all offensive/defensive stats (~270 fields)
env.minion.output                        -- minion stats (if applicable)
```

### How to trigger recalculation:

```lua
build.buildFlag = true
_runCallback("OnFrame")
```

### How to load/save builds:

```lua
mainObject.main:SetMode("BUILD", false, buildName, xmlText)
_runCallback("OnFrame")
build = mainObject.main.modes["BUILD"]
```

---

## Next Steps: Build the GUI Incrementally

### Phase A — Stat Display (done)

- [x] Load a build XML (from `test_builds/`)
- [x] Call `mainObject.main:SetMode("BUILD", ...)` via mlua
- [x] Extract calc output from `build.calcsTab.mainOutput` into `CalcOutput` struct
- [x] Display stats in an egui table/grid (key stats + collapsible full list)
- [x] Verify numbers match upstream's output exactly (95/95 stats, integration test)
- [x] Fixes: `lua-utf8` shim, `arg` global, nested stat flattening (`MainHand.Accuracy`)

This proves the full pipeline: Lua boot → build load → calc → Rust display.

### Phase B — Build List + Config Panel (done)

- [x] Scan user build directory (`GetUserPath()`), list builds in egui with folder navigation
- [x] Open builds via upstream's SetMode (double-click to open, back button to return)
- [x] Config panel: read `ConfigOptions.lua` definitions, render as egui widgets (checkboxes, dropdowns, text inputs)
- [x] Changing config triggers recalc, stats update live in sidebar
- [x] Implemented `NewFileSearch` (glob-based file search with Lua userdata handle)
- [x] Build view layout: stat sidebar (left) + tabbed content (right)

### Phase C — Passive Tree View (done)

- [x] Custom egui painter for tree rendering (colored circles by node type, connection lines)
- [x] Pan/zoom with mouse drag/scroll (zoom toward cursor)
- [x] Node hover → tooltip with name, type, stats, allocation status
- [x] Click to allocate/deallocate → triggers recalc, stats update live
- [x] Filtered cross-tree connections (ascendancy links) and mastery connections for clean display
- [x] Visibility culling for performance
- [ ] Load tree node images as egui textures (deferred — functional without sprites)

### Phase D — Items + Skills (done)

- [x] Items panel: list equipped items by slot with rarity colors, mod display (implicit + explicit)
- [x] Skills panel: socket groups with gem list (name, level, quality, support/active coloring)
- [x] Main skill selection: "Set Main" button triggers recalc, stats update live
- [ ] Item comparison tooltips (deferred — items display but no diff view yet)

### Phase E — Import/Export + Polish (done)

- [x] Build code export: generate shareable code (deflate + base64 + URL-safe encoding)
- [x] Build code import: decode + inflate + load as build, refreshes all panels
- [x] Build saving to disk via upstream's SaveDBFile
- [x] Copy to clipboard button for export codes
- [ ] Keyboard shortcuts (deferred)
- [ ] Additional polish (deferred)

### Phase F — URL Import (done)

- [x] URL-based build import via reqwest (pobb.in, pastebin, poe.ninja, maxroll, rentry, poedb)
- [x] Auto-detects URL vs raw build code in import field
- [x] `NewFileSearch` already implemented in Phase B
- [~] `LaunchSubScript` remains stubbed — not needed (upstream update system replaced by git submodule workflow)

---

## Files To Create Next

| File | Purpose |
|---|---|
| `src/data/stats.rs` | `CalcOutput` struct + extraction from Lua tables |
| `src/data/config.rs` | `ConfigOption` enum + read/write via mlua |
| `src/data/tree.rs` | `TreeData`, `TreeNode` structs + extraction |
| `src/gui/build_list.rs` | Build list panel |
| `src/gui/build_view.rs` | Build editor tab container |
| `src/gui/calcs_tab.rs` | Stat display (egui Grid) |
| `src/gui/config_tab.rs` | Config panel (checkboxes, dropdowns, sliders) |
| `src/gui/tree_tab.rs` | Tree tab + custom painter |
| `src/gui/tree_renderer.rs` | Passive tree rendering (egui::Painter) |
| `src/gui/items_tab.rs` | Items display |
| `src/gui/skills_tab.rs` | Skills/gems display |
| `src/gui/import_tab.rs` | Import/export |
| `src/lua_bridge/filesearch.rs` | `NewFileSearch` via walkdir + glob |
| `src/lua_bridge/subscript.rs` | `LaunchSubScript` via std::thread + separate Lua VMs |
| `scripts/sync-upstream.sh` | Submodule update + drift detection |
| `.github/workflows/build.yml` | CI: cargo build + test for Linux/Windows/macOS |

---

## Upstream Sync Procedure

```bash
cd upstream
git fetch origin
git checkout v2.XX.0
cd ..
git add upstream
git commit -m "Update upstream to v2.XX.0"
cargo build  # verify it still compiles and boots
```

When upstream adds new features:
1. **Calc-only** (new skills, damage formulas): Free — numbers appear in output automatically
2. **New config options**: Read the new option definitions, add egui widget rendering
3. **New UI features** (toast notifications, DPI scaling): Irrelevant — we have our own UI

---

## Distribution

Single binary + data directory:

```
dist/PathOfBuilding/
├── pob-egui                # Single binary (Linux) or pob-egui.exe (Windows)
├── src/                    # Upstream Lua source (flat copy from submodule)
├── runtime/                # Upstream runtime utilities (flat copy)
└── manifest.xml
```

CI builds with `cargo build --release` for each target. Cross-compilation via `cross` or GitHub Actions matrix.

---

## Verification

### Phase 0 (done — headless bootstrap):
- [x] `cargo run` starts, Lua VM initializes, upstream code loads
- [x] No Lua errors in console
- [x] `mainObject_ref` exists (boot verified)
- [x] egui window opens

### Phase A (done — stat display):
- [x] Load a known build XML
- [x] Extract calc output to Rust
- [x] Compare DPS / life / ES numbers with upstream — **95/95 stats match exactly**

### Phase B (done — build list + config):
- [x] Build list renders, folder navigation works
- [x] Config panel renders all option types without panic
- [x] Config changes trigger recalc, stats update live

### Phase C (done — passive tree):
- [x] Tree renders without panic, pan/zoom/click works
- [x] Node allocation triggers recalc, stats update live

### Phase D (done — items + skills):
- [x] Items + skills panels render without panic
- [x] Main skill selection triggers recalc

### Phase E (done — import/export):
- [x] Build export generates shareable code
- [x] Build import loads from code
- [x] Build save to disk works

# PoE 2 Integration Plan

Plan for adding Path of Exile 2 passive-tree support to egui-pob alongside the existing PoE 1 implementation.

**Scope (this plan):** tree tab only — node rendering, frame overlays, connections, ascendancy regions, dual-ascendancy support, class backgrounds. Items / skills / calcs / import-export for PoE 2 are deliberately out of scope and left for follow-up plans once the tree is landed.

**Effort estimate:** XL (1-2 weeks) for tree-only. Dominated by second Lua VM plumbing (~20%) and BC7/zstd atlas pipeline (~40%). Remaining 40% is schema additions, renderer adjustments, UX polish.

---

## 1. Where we are today (PoE 1)

| Area | Size | Notes |
|---|---|---|
| `src/data/tree.rs` | 457 LoC | schema parser: classes, nodes, groups, connections, ascendancies |
| `src/data/tree_sprites.rs` | 481 LoC | atlas loader — **hardcodes PoE 1 filenames** (`skills-3.jpg`, `frame-3.png`, `mastery-3.png`, `group-background-3.png`) |
| `src/gui/tree_renderer.rs` | 943 LoC | node/frame/arc rendering, pan/zoom, ascendancy regions, tooltips |
| `src/lua_bridge/mod.rs` | 184 LoC | LuaJIT embedding, SimpleGraphic stubs, bootstrap |
| Build list + tabs + state extraction | ~4600 LoC | config, items, skills, notes, import-export, build view |

Total Rust: ~6,700 LoC. Our tree renderer's geometry (orbit radii, orbit angles, arc connections, camera) is PoE-version-agnostic. Everything else that's PoE-1-specific lives in the sprite loader and a few hardcoded frame offsets.

---

## 2. How PoE 2 differs (what actually matters)

### Shape-compatible (good news)

- Same Lua module layout (`src/Classes/`, `src/Modules/`, `src/Data/`, `src/TreeData/`).
- Same SimpleGraphic API surface. Our stubs work unchanged. (One unused stub removed upstream; no new ones needed.)
- Same `main.modes['BUILD']` / `mainObject_ref` shape — our build-state extraction transfers.
- Same node/group geometry: seven orbits, `connections[].orbit`, `ascendancyName`, `group/orbit/orbitIndex`, `orbit_radii`, `skillsPerOrbit`. Our renderer math reuses.
- Calcs are a fork of PoE 1's engine, not a ground-up rewrite — same architecture, ~4k lines of CalcOffence divergence. Not relevant to tree-only scope.
- Dual ascendancy (PoE 2's native mechanism): `alternate_ascendancies` + `curSecondaryAscendClassId` — **the same code path we just wired up for PoE 1's Azmeri bloodlines**. UX layer is already done.

### Real divergences

1. **No `sprites.lua`.** All sprite metadata moved inside `tree.lua` under top-level tables: `assets`, `ddsCoords`, `nodeOverlay`, `connectionArt`.

2. **Sprite sheets are `*.dds.zst`** — zstd-compressed BC1/BC7 DDS atlases. Examples:
   - `skills_64_64_BC1.dds.zst`
   - `ascendancy-background_1500_1500_BC7.dds.zst`
   - `group-background_104_104_BC7.dds.zst`

   The `image` crate cannot read these. This invalidates our entire hardcoded-filename atlas loader.

3. **Ascendancy class backgrounds live inside `ascendancy-background_1500_1500_BC7.dds.zst`**, keyed by sprite name (`ClassesDeadeye`, `ClassesAmazon`, `ClassesWitchHunter`, etc.) — not as standalone `ClassesXXX.png` files under `TreeData/`.

4. **Frame art is data-driven** via `nodeOverlay[type].{alloc, path, unalloc}` (and per-ascendancy overrides on individual nodes). Our current renderer hardcodes frame sprite coords; this needs to flip to data lookup.

5. **Connector art is data-driven** via `connectionArt = {ascendancy="CharacterAscendancy", default="Character"}` — PoE 2 uses per-orbit connector PNGs instead of a single `line-3.png`.

6. **Forked upstream.** `PathOfBuildingCommunity/PathOfBuilding-PoE2` is a separate repo. Data layouts diverge (new `ModCharm`, `ModRunes`; dropped enchants/crucible/beastcraft). Merging into one Lua VM would fight `LoadModule` / `newClass` / globals. Two VMs is the realistic architecture.

7. **Class IDs differ.** PoE 2 classes have `integerId`; our current `classId` mapping in `tree.rs` needs a small remap path.

8. **Version model.** PoE 2 is at `liveTargetVersion = "0_4"` with `treeVersionList = {"0_1".."0_4"}`. Single-active-tree for now — no multi-version tree-switching UI needed initially.

---

## 3. Architecture: dual-game, minimal coupling

### Submodule layout

```
upstream/          # PoE 1 (existing — read-only submodule)
upstream-poe2/     # PoE 2 (new submodule)
```

Pin each to a specific upstream release tag. Don't try to share `upstream/` between games.

### Lua bridge

Introduce a game variant enum and parameterize `LuaBridge`:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum GameVariant {
    PoE1,
    PoE2,
}

impl LuaBridge {
    pub fn new(variant: GameVariant, src_path: &Path, base_dir: &Path) -> Result<Self> { ... }
    pub fn variant(&self) -> GameVariant { self.variant }
}
```

One Lua VM per `LuaBridge`. The app holds one `LuaBridge` per game the user has active builds in — probably both at once. Switching between a PoE 1 and PoE 2 build in the UI means switching the active bridge. The bridges are cheap to keep around; boot cost is a few hundred ms each.

**Don't** try to reuse one VM with namespace-juggling. The globals (`main`, `build`, `colorCodes`, `data`) collide in ways that fight `LoadModule` and `newClass`.

### Build entry / build list

```rust
pub struct BuildEntry {
    pub game: GameVariant,
    pub full_path: PathBuf,
    ...
}
```

Two options for the build-list UI:
- **Tabbed**: "PoE 1" / "PoE 2" tabs at the top of the build list, each scanning its own `buildPath`.
- **Mixed**: single list with a small game badge per entry.

Recommend **tabbed** — matches how users think about their builds and avoids accidentally opening a PoE 2 build into a PoE 1 VM.

### Per-game build directories

Each upstream has its own default `buildPath`. Don't share. Read it from the respective Lua VM on boot.

### Sprite atlas abstraction

```rust
pub trait SpriteAtlas {
    fn texture_id(&self, sheet: &str) -> Option<egui::TextureId>;
    fn sprite(&self, sheet: &str, name: &str) -> Option<SpriteRect>;
    fn class_background(&self, class: &str) -> Option<(egui::TextureId, egui::Rect)>;
    fn node_frame(&self, node_type: NodeType, state: NodeState) -> Option<(egui::TextureId, egui::Rect)>;
}
```

Two implementations:
- `PoE1SpriteAtlas` — current logic, refactored behind the trait.
- `PoE2SpriteAtlas` — new, BC7/zstd-based, data-driven from `tree.lua`'s `ddsCoords`/`nodeOverlay`.

`tree_renderer.rs` becomes generic over `&dyn SpriteAtlas`.

### Tree schema

Keep `tree.rs` structs as a **superset**. Populate only what each game provides:

```rust
pub struct TreeData {
    pub classes: Vec<Class>,
    pub nodes: HashMap<NodeId, Node>,
    pub groups: Vec<Group>,
    pub orbit_radii: [f32; 7],
    pub skills_per_orbit: [u32; 7],
    // PoE 2 additions (None on PoE 1):
    pub node_overlay: Option<NodeOverlayMap>,
    pub connection_art: Option<ConnectionArt>,
    pub alternate_ascendancies: Vec<AltAscendancy>,
}
```

Same for `Node` (add `is_switchable: bool`, `options: Option<Vec<NodeOption>>`).

---

## 4. New dependencies

| Crate | Purpose | Notes |
|---|---|---|
| `zstd` | Decompress `.dds.zst` payloads | already widely used |
| `image_dds` | DDS parser + BC1/BC7 decode to RGBA | preferred — pure Rust, simpler API |
| **fallback:** `ddsfile` + `texpresso` | DDS parse + BC decode | if `image_dds` has perf/quality issues |

Skip Oodle — PoE 2 tree data is plain zstd; Oodle is only relevant if we later go down the GGPK extraction path (see `docs/asset-extraction.md`).

---

## 5. Work breakdown

### Phase A — Dual-VM plumbing (M, ~1.5 days)

1. Add `upstream-poe2/` submodule pinned to a specific PoE 2 release tag.
2. Introduce `GameVariant` enum in `lua_bridge/mod.rs`.
3. Parameterize `LuaBridge::new` with variant. Adjust `package.path`, `chdir`, bootstrap.
4. Verify both VMs boot cleanly and stay independent (no global collision).
5. Verify `disableDevAutoSave` hook works in PoE 2 too.
6. Thread `game: GameVariant` through `BuildEntry` and `BuildView`.
7. Add per-game build-list tabs (or badges).
8. Update `main.rs` to initialize both bridges at startup (handle missing submodule gracefully — warn, don't crash).

**Done when:** can switch between a PoE 1 build and a PoE 2 build without restarting the app.

### Phase B — BC7/zstd atlas pipeline (M, ~1-2 days)

1. Add `zstd` + `image_dds` deps.
2. Write a sprite-sheet loader: `fn load_dds_zst(path: &Path) -> Result<RgbaImage>`.
3. Verify round-trip on a couple of PoE 2 sheets (`skills_64_64_BC1`, `ascendancy-background_1500_1500_BC7`).
4. Benchmark BC7 decode time — if >~500ms for all sheets combined, consider caching decoded textures to disk or switching to `texpresso`.
5. Wire into egui's texture system (same pattern as current atlas — upload as `TextureHandle`).

**Done when:** can load one PoE 2 `.dds.zst` atlas, decode it, and render it on a blank egui canvas.

### Phase C — `SpriteAtlas` trait + PoE 2 implementation (M, ~1-2 days)

1. Extract the `SpriteAtlas` trait from current `tree_sprites.rs` usage patterns.
2. Refactor existing code into `PoE1SpriteAtlas` behind the trait. Verify PoE 1 tree still renders identically.
3. Write `PoE2SpriteAtlas`:
   - Enumerate `tree.assets` + `tree.ddsCoords` from PoE 2's `tree.lua`.
   - Load each `.dds.zst` sheet via Phase B.
   - Index sprites by `ddsCoords[filename][name] = index`.
   - Resolve frame lookups via `nodeOverlay[type].{alloc, path, unalloc}`.
   - Resolve class backgrounds from the ascendancy-background atlas.

**Done when:** PoE 2 sprite atlas can return a `TextureId` + `Rect` for every sprite the renderer asks for.

### Phase D — Schema additions in `tree.rs` (S, ~½ day)

1. Extend `TreeData`, `Node`, `Class` structs with PoE 2 fields.
2. Extend the Lua-side extractor to read `nodeOverlay`, `connectionArt`, `alternate_ascendancies`, `isOnlyImage`, class `integerId`.
3. Keep PoE 1 fields `None`/defaulted on the PoE 2 side and vice versa.
4. Remap class IDs for PoE 2 (small translation layer).

**Done when:** PoE 2's `tree.lua` parses into `TreeData` without errors.

### Phase E — Renderer adjustments (S-M, ~1 day)

1. Flip frame lookup to go through `nodeOverlay[node.type]` when `tree_data.node_overlay.is_some()`.
2. Draw ascendancy class backgrounds from the atlas sprite instead of standalone PNG.
3. Connection rendering: if `connectionArt` is present, use the per-orbit connector sprite; else keep our current curve rendering.
4. Dual-ascendancy regions: render both ascendancy backgrounds when applicable, with the non-selected one dimmed (same pattern we already have for a single ascendancy).
5. Drop any PoE-1-specific hardcoded sprite coords from `tree_renderer.rs`.

**Done when:** a PoE 2 build's passive tree renders with correct nodes, frames, connections, and ascendancy regions.

### Phase F — UX polish (S, ~½ day)

1. Build-list tab labels, per-game badges.
2. Window title reflects game (e.g. "PoE 2 — My Build").
3. Ensure the secondary ascendancy dropdown (already working) correctly drives PoE 2's alt-ascendancy regions.
4. Test with 2-3 known-good PoE 2 build XMLs (find via pobb.in or construct manually).

**Done when:** feels like a first-class dual-game app, not a hack.

---

## 6. Risks and open questions

- **BC7 decode perf.** BC7 is slower than BC1. If load time becomes unacceptable, mitigations: (a) cache decoded RGBA atlases to disk keyed by sprite-sheet hash, (b) decode lazily per sheet, (c) switch decoder crate. Budget half a day for this if it bites.
- **Class ID remapping edge cases.** PoE 2 class `integerId`s may not be contiguous; validate against PoE 2's class table before committing to a remap strategy.
- **Dual-ascendancy path validation.** When both ascendancies are allocated, path-dependence checks need to consider both start nodes. Existing path logic may need touch-up.
- **Build format compatibility.** PoE 2 build XMLs use different `Tree` schema (version "0_4" root). Our build loader calls `mainObject_ref.main:SetMode("BUILD", false, name, xml)` — verify this hits the PoE 2 variant's parser, not the PoE 1 parser by accident (it won't if the VMs are separate, but worth confirming).
- **Upstream churn.** PoE 2 is pre-1.0 (`liveTargetVersion = "0_4"`). Schemas and file layouts may still shift. Pin the submodule tightly; treat each upstream bump as a deliberate integration pass.

---

## 7. Out of scope (follow-up plans)

Once the tree lands, the next natural chunks (separate plans, not this one):

- **PoE 2 items tab** — new mod types (ModCharm, ModRunes), corrupted-only mods, quality tiers.
- **PoE 2 skills tab** — different socket/support model, spirit cost, weapon-set linking.
- **PoE 2 calcs** — re-sync CalcOutput extraction against PoE 2's forked CalcOffence.
- **PoE 2 import/export** — pobb.in supports PoE 2 URLs; add `game` to auto-detect.
- **Unified build list** — eventually merge PoE 1 and PoE 2 tabs with a clear game selector.

---

## 8. Key references

- PoE 2 upstream: `https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2`
- Canonical PoE 2 tree schema: `upstream-poe2/src/TreeData/0_4/tree.lua` — sections: `assets` (line ~2), `connectionArt` (~751), `constants` (~755), `ddsCoords` (~1345), `nodeOverlay` (~21960), `nodes` (~21982).
- Dual-ascendancy mechanism: `upstream-poe2/src/Classes/PassiveSpec.lua:664-684` (`alternate_ascendancies`, `curSecondaryAscendClassId`).
- Our existing dual-ascendancy UI: `src/gui/build_view.rs` — `SecondaryAscendEntry`, `load_secondary_ascendancies`, `select_secondary_ascendancy`.
- Files that will change most:
  - `src/lua_bridge/mod.rs` — add `GameVariant`, dual VM.
  - `src/data/tree.rs` — add PoE 2 schema fields.
  - `src/data/tree_sprites.rs` — split into `SpriteAtlas` trait + `PoE1SpriteAtlas` + `PoE2SpriteAtlas`.
  - `src/gui/tree_renderer.rs` — data-driven frame/connector lookups.
  - `src/data/build_list.rs`, `src/gui/build_list.rs`, `src/gui/build_view.rs` — per-game tabs and routing.

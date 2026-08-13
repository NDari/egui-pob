# Path of Exile Asset Extraction

This document describes how art assets (ascendancy backgrounds, tree sprites, icons, etc.) are stored in the Path of Exile game files and how to extract them. The long-term goal is to have a self-contained extraction pipeline so we don't rely on upstream's bundled assets.

## Game Asset Storage

Since patch 3.11.2, PoE stores all game data in **compressed bundles** rather than loose files.

- **Standalone client**: Bundles are packed inside a single `Content.ggpk` file using the existing `PDIR`/`FILE` container structure.
- **Steam client**: Uses tens of thousands of individual `.bundle.bin` files in the install directory.

### Bundle format

Each bundle has a fixed header specifying uncompressed/compressed payload sizes, followed by compressed data blocks:

- Compression: **Oodle** (Kraken, Leviathan, or Mermaid variants)
- Block size: Each block decompresses to **256 KiB** (except the final block)
- Note: Oodle decompressors may write up to 64 bytes past the end of the output buffer -- allocate extra space

### Index file

All bundle and file metadata is in `_.index.bin` (itself a compressed bundle), containing:

- Bundle names and payload sizes
- File path hashes with byte offsets and sizes into bundles
- Directory path representation data

File path hashing changed over time:

- Pre-3.21.2: FNV1a hash of the lowercase full path with a `++` suffix
- 3.21.2+: MurmurHash64A with seed `0x1337b33f`

## Art Asset Paths

Ascendancy background images are at paths like:

```
Art/2DArt/UIImages/InGame/PassiveSkillScreen/ClassesBerserker.png
Art/2DArt/UIImages/InGame/PassiveSkillScreen/ClassesChieftain.png
```

In the upstream PoB repo, these are stored as `src/TreeData/ClassesXXX.png`.

Tree spritesheets (node icons, frames, mastery art) are in versioned subdirectories:

```
src/TreeData/<version>/skills-3.jpg
src/TreeData/<version>/frame-3.png
src/TreeData/<version>/mastery-3.png
```

Tooltip header images are in `src/Assets/` (e.g. `notablepassiveheaderleft.png`).

Oil icons are in `src/TreeData/` (e.g. `GoldenOil.png`).

## Extraction Toolchain

### Prerequisites

- A Path of Exile installation (standalone or Steam)
- [zao/ooz](https://github.com/zao/ooz) -- Oodle decompressor (build from source)
- [Visual Studio Community](https://visualstudio.microsoft.com/vs/community/) + [CMake](https://cmake.org) (for building ooz)

### Building the decompressor

```sh
git clone --recurse-submodules -b master https://github.com/zao/ooz
cd ooz
# Configure and build with CMake via Visual Studio
```

This produces: `bun_extract_file.exe`, `libbun.dll`, `libooz.dll`

### Using PoB's built-in exporter (current upstream method)

1. Copy `bun_extract_file.exe`, `libbun.dll`, `libooz.dll` to `upstream/src/Export/ggpk/`
2. Create a shortcut to `upstream/runtime/Path of Building.exe` with `upstream/src/Export/Launch.lua` as the first argument
3. Run it -- the "Dat View" UI appears
4. Click `Edit Sources...` > `New`, point "Source from GGPK/Steam PoE path" to:
   - Standalone: `C:\Path of Exile\Content.ggpk`
   - Steam: `C:\Program Files (x86)\Steam\steamapps\common\Path of Exile`
5. Click `Scripts >>` and run the relevant export scripts from `upstream/src/Export/Scripts/`

### Alternative: direct bundle extraction

For extracting individual art assets without the full PoB exporter:

- [poe-tool-dev/ggpk.discussion](https://github.com/poe-tool-dev/ggpk.discussion/wiki) -- community documentation on the GGPK/bundle format
- [poe-tool-dev implementations wiki](https://github.com/poe-tool-dev/ggpk.discussion/wiki/Implementations) -- list of all known tools across languages
- [poedb.tw/us/Bundle_schema](https://poedb.tw/us/Bundle_schema) -- detailed bundle format specification

The general approach:

1. Parse `_.index.bin` to build a file path hash -> bundle/offset mapping
2. Hash the desired asset path (e.g. `art/2dart/uiimages/ingame/passiveskillscreen/classesberserker.png` lowercase, using the appropriate hash function for the game version)
3. Find the containing bundle, decompress with Oodle, extract at the offset

## Existing Tools and Libraries

### Rust

| Crate / Tool | Description | Status |
|---|---|---|
| [`oozextract`](https://crates.io/crates/oozextract) ([GitHub](https://github.com/lvlvllvlvllvlvl/oozextract)) | **Pure-Rust Oodle decompressor.** Supports Kraken / Mermaid / Selkie / Leviathan / LZNA / Bitknit. No `cc`, no `bindgen`, no `build.rs` — deps are just `bytemuck`, `bytes`, `wide` (SIMD). Has a WASM feature. MIT. | v0.5.1, Nov 2024. Eliminates the need for a C/C++ toolchain. |
| [`ggpk`](https://crates.io/crates/ggpk) ([GitHub](https://github.com/ex-nihil/ggpk)) | CLI + library for reading/extracting files from GGPK archives. Handles the legacy GGPK container format (file listing, regex filtering, extraction). **Does not handle bundle decompression.** LGPL-3.0. | v1.2.2, Nov 2022. Dormant. |
| [`poe_bundle`](https://lib.rs/crates/poe_bundle) | Library for extracting Oodle-compressed bundles. Wraps a C++ ooz fork via FFI. | v0.1.5, Nov 2022. Early stage. |
| [ggpk-explorer](https://github.com/juddisjudd/ggpk-explorer) | Full GUI explorer for both PoE 1 GGPK and PoE 2 bundle formats. GPL-3.0. Bundle + index parsing (`src/bundles/{bundle,cdn,index,path_enrichment}.rs`) is cleanly isolated from the GUI — vendorable. Its `src/ooz/sys.rs` is raw FFI to a vendored zao/ooz (16 C++ files via `cc`+`bindgen`); swapping that for `oozextract` collapses to ~one function. The `cdn.rs` module shows the bundle fallback endpoint (see below). | v1.1.3, Jan 2026. Actively maintained. |

**Recommended pipeline:** vendor `oozextract` + write a small `_.index.bin` parser with MurmurHash64A (seed `0x1337b33f`). No C toolchain, reproducible from any local PoE install. Roughly ~300 LoC plus the crate, excluding GGPK container reading (only needed for standalone — Steam installs are already loose bundle files).

### Other languages

| Tool | Language | Description |
|---|---|---|
| [libooz/libbun](https://github.com/zao/ooz) | C | Reference Oodle decompressor. What upstream PoB uses. Produces `bun_extract_file.exe`, `libbun.dll`, `libooz.dll`. |
| [LibGGPK3 / VisualGGPK3](https://github.com/aianlinb/LibGGPK3) | C# | Full read/write GGPK library with GUI. Most mature cross-language option. |
| [gooz](https://github.com/oriath-net/gooz) | Go | Go port of ooz. Decompresses Kraken/Mermaid/Selkie/Leviathan. |
| [PyPoE](https://github.com/OmegaK2/PyPoE) | Python | Developing bundle structure parsing support. |
| PoB Exporter | Lua | Built into upstream PoB (`src/Export/Launch.lua`). DAT viewer + custom export scripts. What upstream uses to extract game data. |

## PoE CDN URLs: not a shortcut

Upstream PoB hardcodes URLs like:

```
https://web.poecdn.com/gen/image/<base64>/<hash>/ClassesPrimalist.png
```

Decoding that base64 (e.g. `WzIyLCJlMzIwYTYwYmNiZTY4ZmQ5YTc2NmE1ZmY4MzhjMDMyNCIseyJ0IjoyNywic3AiOjAuMzgzNX1d`) yields a JSON tuple:

```json
[22, "e320a60bcbe68fd9a766a5ff838c0324", {"t": 27, "sp": 0.3835}]
```

- `22` = API version
- the hex string = MD5 content fingerprint of the source DDS/atlas
- `{t, sp}` = texture index + scale/pixel ratio
- the trailing `/3d68393250/` path segment is a short HMAC-style signature

**These URLs rotate every patch**: the md5 changes when the source asset changes, and the signature changes alongside. Upstream PoB re-patches these strings each release. Testing the URLs currently committed to upstream returns **404**. There is no public "latest version of this texture" endpoint, and simpler fallbacks (`/image/Art/...`, `/image/passive-skill/...`) also 404.

**What IS stable:** `https://patch.poecdn.com/{patch_version}/Bundles2/{bundle_name}` serves whole bundles by name (PoE 2: `patch-poe2.poecdn.com`). This means a fresh PoE install is not strictly required — you can pin a patch version, fetch `_.index.bin` + specific bundles by name over HTTP, and extract. See `ggpk-explorer/src/bundles/cdn.rs` for the pattern.

## Concrete example: extracting bloodline ascendancy backgrounds

The Azmeri / secondary-ascendancy backgrounds live at:

```
Art/2DArt/UIImages/InGame/PassiveSkillScreen/ClassesPrimalist.png
Art/2DArt/UIImages/InGame/PassiveSkillScreen/ClassesWarden.png
Art/2DArt/UIImages/InGame/PassiveSkillScreen/ClassesWarlock.png
```

Plus the Azmeri node frames: `AzmeriAscendancyFrameLargeNormal.png`, etc. (see `upstream/src/Classes/PassiveTree.lua` lines 202-222 for the full list).

Upstream already has these PNGs committed under `upstream/src/TreeData/` (pre-extracted), so they're available at runtime through the submodule today.

The 3.29 bloodline emblems (Trialmaster, Olroth, Catarina, ...) plus Reliquarian and Luminary are **not** loose PNGs: they ship as regions of `TreeData/3_29/bloodline-3.webp` and `ascendancy-3.webp`, indexed by `sprites.lua`. No extraction is needed for these either -- `tree_sprites::load_sprite_backgrounds` reads their UV rects out of upstream's `spriteMap`, the same place upstream gets them (`PassiveTree.lua:349-365`).

To re-extract them ourselves:

1. Parse `Bundles2/_.index.bin` (decompress with `oozextract`, then read the structured header: bundle list → file-hash table → path-rep table).
2. Lowercase the target path and hash it with MurmurHash64A, seed `0x1337b33f`.
3. Look up the containing bundle + byte offset in the index.
4. Fetch the bundle either from local `Bundles2/<name>.bundle.bin` or `patch.poecdn.com/{patch_ver}/Bundles2/<name>`.
5. Decompress with `oozextract`, slice at the offset, write the PNG.

## Current State

- Upstream PoB includes pre-extracted art files committed directly to the repo, as loose PNGs and as spritesheets (`skills-3.jpg`, `ascendancy-3.webp`, `bloodline-3.webp`, ...) with coordinates in `sprites.lua`
- New ascendancies may still arrive without art -- a regular ascendancy falls back to the Ascendant background, a bloodline draws none
- Our app (`egui-pob`) reads these files from the upstream submodule at runtime

## Future Work

Build a standalone Rust tool that can:

1. Decompress `_.index.bin` with `oozextract`, parse the index to build a path-hash → (bundle, offset, size) map
2. Fetch bundles either from a local PoE install or from `patch.poecdn.com/{ver}/Bundles2/{name}` (installer-free)
3. Extract a declarative list of art assets (class backgrounds, Azmeri frames, oil icons, tooltip headers) to `assets/`
4. Optionally re-run against a newer patch to update everything

Scope estimate with the `oozextract` + custom index parser approach: ~300 LoC of Rust plus the crate, no C/C++ toolchain required. `ggpk-explorer`'s `src/bundles/` directory is the cleanest reference implementation for the index parsing and CDN fallback logic; swap its `src/ooz/sys.rs` FFI for `oozextract::Oozextract::decompress()` and the tool is pure Rust.

For the legacy standalone GGPK container (Windows-only `Content.ggpk`), the `ggpk` crate (dormant but functional) wraps reading, or the container parsing in `ggpk-explorer/src/ggpk/` can be vendored. Steam installs skip this layer entirely — bundles are already loose files.

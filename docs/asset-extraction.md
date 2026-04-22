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
| [`ggpk`](https://crates.io/crates/ggpk) ([GitHub](https://github.com/ex-nihil/ggpk)) | CLI + library for reading/extracting files from GGPK archives. Handles the legacy GGPK container format (file listing, regex filtering, extraction). **Does not handle bundle decompression.** LGPL-3.0. | v1.2.2, last updated Nov 2022. Dormant. |
| [`poe_bundle`](https://lib.rs/crates/poe_bundle) | Library for extracting Oodle-compressed bundles. Wraps a C++ ooz fork via FFI for decompression. Depends on the `ggpk` crate for GGPK container reading. | v0.1.5, last updated Nov 2022. Early stage. |
| [ggpk-explorer](https://github.com/juddisjudd/ggpk-explorer) | Full GUI explorer for **both** PoE 1 GGPK and PoE 2 bundle formats. Oodle decompression, DAT schema viewing, DDS texture viewer, search, CDN fallback for missing bundles. Most feature-complete Rust option. GPL-3.0. | v1.1.3, Jan 2026. Actively maintained. |

The `ggpk` + `poe_bundle` combo is the most relevant as library dependencies since we're already Rust. `ggpk-explorer` is the most complete and actively maintained -- useful as a reference implementation or for extracting its bundle/decompression code.

### Other languages

| Tool | Language | Description |
|---|---|---|
| [libooz/libbun](https://github.com/zao/ooz) | C | Reference Oodle decompressor. What upstream PoB uses. Produces `bun_extract_file.exe`, `libbun.dll`, `libooz.dll`. |
| [LibGGPK3 / VisualGGPK3](https://github.com/aianlinb/LibGGPK3) | C# | Full read/write GGPK library with GUI. Most mature cross-language option. |
| [gooz](https://github.com/oriath-net/gooz) | Go | Go port of ooz. Decompresses Kraken/Mermaid/Selkie/Leviathan. |
| [PyPoE](https://github.com/OmegaK2/PyPoE) | Python | Developing bundle structure parsing support. |
| PoB Exporter | Lua | Built into upstream PoB (`src/Export/Launch.lua`). DAT viewer + custom export scripts. What upstream uses to extract game data. |

## Current State

- Upstream PoB includes pre-extracted art files committed directly to the repo
- New ascendancies (e.g. Scavenger) may not have art extracted yet -- the game itself falls back to the Ascendant background for these
- Our app (`egui-pob`) reads these files from the upstream submodule at runtime

## Future Work

Build a standalone Rust tool that can:

1. Parse `_.index.bin` from a PoE installation
2. Decompress Oodle bundles (via FFI to libooz, or using `poe_bundle` crate, or referencing `ggpk-explorer`'s implementation)
3. Extract all needed art assets directly, independent of upstream's bundled files
4. Automate tree data + art updates when new PoE patches ship

The `ggpk` + `poe_bundle` crates could serve as a starting point, though they haven't been updated since 2022. The `ggpk-explorer` project is actively maintained and handles both PoE 1 and PoE 2 formats -- its decompression and index parsing code would be the best reference for a custom extraction tool.

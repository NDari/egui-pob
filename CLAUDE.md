# CLAUDE.md — egui-pob

## Project Overview

Rust + egui frontend for Path of Building (PoB) with an embedded LuaJIT backend. Upstream PoB is a read-only git submodule. The Rust app loads upstream Lua code headless (rendering stubbed), then renders everything with egui.

## Architecture

```
Rust + egui (GUI) ↔ mlua FFI bridge ↔ Embedded LuaJIT (calc engine)
```

- **Rust** owns: GUI rendering, system functions, HTTP, compression, clipboard, file I/O
- **Lua** owns: calc engine, data model, game data, build I/O
- **upstream/** is a read-only git submodule pinned to a PoB release tag — never modify it

## Prerequisites

- Rust stable (1.94+)
- [just](https://github.com/casey/just) task runner (`pacman -S just` on Arch)
- Git submodule initialized: `git submodule update --init`

## Commands

| Command | Description |
|---------|-------------|
| `just run` | Release build + run (`RUST_LOG=info`) |
| `just debug` | Debug build + run (`RUST_LOG=debug`) |
| `just build` | Release build |
| `just build-debug` | Debug build |
| `just test` | Run all tests (`cargo test`) |
| `just clippy` | Run clippy lints |
| `just fmt` | Format code |
| `just check` | Type-check without building |
| `just sync` | Update upstream submodule |

## Tooling

- **rust-analyzer LSP** plugin is available — use it for go-to-definition, type lookups, and diagnostics when needed
- **lua-lsp** plugin is available — use it when working with upstream Lua code for navigation and understanding

## Code Style

- Standard `rustfmt` and `clippy` defaults, no overrides
- Use typed errors where it aids debugging and code correctness; `anyhow::Result` for top-level plumbing
- `unwrap()` only when the call is logically infallible
- Organize files however makes sense for the module — no rigid rule
- Cross-platform code only — no Linux-specific assumptions. Must build on Linux, Windows, and macOS

## Git Rules

**Claude must NEVER perform git operations with side effects.** No commits, no branches, no merges, no pushes, no rebases, no resets. Read-only operations only (status, log, diff, blame, show).

The user handles all git workflow manually.

## Key Constraints

- **Never modify files under `upstream/`** — it is a read-only submodule
- **The Lua VM is the source of truth** for game data and calculations — Rust reads from it, never reimplements calc logic
- **Dependencies** — add new crates as needed, no special approval process
- **Testing** — primarily integration tests (boot Lua VM, verify calc output against known builds) and manual verification. Unit tests where they make sense.

## Project Structure

```
test_builds/             # Known-good build XMLs for calc verification

src/
├── main.rs              # Entry point, path detection, init
├── lib.rs               # Library crate (exposes data + lua_bridge for tests)
├── lua_bridge/          # Lua ↔ Rust FFI interface
│   ├── mod.rs           # LuaBridge: VM setup, bootstrap, build loading
│   ├── filesearch.rs    # NewFileSearch: glob-based file search (Lua userdata)
│   ├── stubs.rs         # Rendering/input no-ops (~50 SimpleGraphic API stubs)
│   └── system.rs        # Working system functions (time, paths, clipboard, etc.)
├── gui/
│   ├── mod.rs           # PobApp: screen routing (build list ↔ build view)
│   ├── build_list.rs    # Build list panel with folder navigation
│   ├── build_view.rs    # Build view: stat sidebar + tabbed content
│   └── config_tab.rs    # Config panel: checkboxes, dropdowns, text inputs
└── data/
    ├── mod.rs           # CalcOutput: stat extraction from Lua
    ├── build_list.rs    # BuildEntry/BuildInfo: build directory scanning
    └── config.rs        # ConfigOption: config definitions + read/write

upstream/                # Git submodule — READ-ONLY
├── src/                 # All upstream Lua source
│   ├── Modules/         # Calc engine (Calc*.lua), Common.lua, ModParser.lua
│   ├── Classes/         # Data model (Build, Item, Skill, PassiveSpec)
│   ├── Data/            # Game constants (~58M)
│   ├── TreeData/        # Passive tree definitions
│   └── Launch.lua       # Entry point
└── runtime/
```

## Plans

`plans/` contains all planning documents for the project:

- **`initial-plan.md`** — Original implementation plan covering architecture, data interface design, phased build-out (A through F, all complete), and distribution strategy.
- **`parity-plan.md`** — Comprehensive feature parity tracker (~200+ items) comparing current state against upstream PoB. Organized by area: build management, tree, skills, items, calcs, config, import/export, UI polish, and infrastructure.

## Documentation

`docs/` contains reference documentation for longer-term research and future work:

- **`asset-extraction.md`** — How PoE game assets (ascendancy art, tree sprites, icons) are stored in compressed bundles, how the upstream PoB project extracts them using the ooz/GGPK toolchain, and a roadmap for building our own standalone extraction pipeline in Rust.

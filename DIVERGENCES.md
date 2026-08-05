# Intentional divergences from upstream PoB

Every place this app deliberately behaves differently from upstream Path of
Building. Purpose: during submodule upgrades, "is our difference intentional
or drift?" must be answerable instantly. Add an entry whenever you diverge on
purpose; remove it if we later re-converge. Accidental drift is a bug (see
`ports.toml` and `cargo test --test ports_sync`).

Rules of thumb (see CLAUDE.md "Upstream usage policy"): interchange formats
and calc semantics never diverge; interaction semantics may, deliberately and
listed here; presentation is fully ours and needs no entries.

## Behavioral divergences

- **Slot-to-slot item drag swaps.** Dragging an equipped item onto an
  occupied compatible slot moves the displaced item back into the source
  slot when it fits (always true for paired slots like Ring 1/2). Upstream
  only supports dragging from the item list onto a slot (plain equip; no
  slot-to-slot drag at all). `items::move_item_between_slots`.

- **Gem reordering within a socket group** via drag handles. No upstream
  equivalent; upstream gem rows are fixed. `skills::move_gem`.

- **Ctrl+C in the skills tab copies the main socket group.** Upstream copies
  the list-selected group; our UI has no list selection, so the main group is
  the nearest equivalent. Per-group Copy buttons cover the rest.

- **PoEURL shrinking uses https.** Upstream requests `http://poeurl.com`;
  port 80 there is unreliable, https works. `tree_specs::shrink_tree_url`.

- **Character import status messages are ours.** Upstream v2.66 removed its
  status text entirely; our wrappers pcall the import and report
  success/failure themselves so the import panel still shows an outcome.
  `char_import::import_passive_tree_and_jewels` / `import_items_and_skills`.

- **Character list is fetched automatically for the remembered account.** On
  the first view of the Import/Export tab, if an account name was prefilled
  from history we fetch its character list without waiting for a button
  press. Upstream always requires clicking "Start". Fires once per build view
  and only when no characters are loaded yet; because our HTTP is blocking
  (see the sub-script entry below) it deliberately triggers on first view
  rather than at startup, so launches that never open the tab pay nothing.
  `ImportPanel::show` / `auto_fetch_attempted`.

- **We still support POESESSID; upstream does not.** Upstream removed session
  IDs wholesale in v2.66.0 and imports via OAuth. We keep the legacy
  session-id path because we have not implemented OAuth (tracked in
  `plans/parity-plan.md` §16). Note that upstream v2.67.0 still carries dead
  `gameAccounts[name].sessionID` save/load code in `Main.lua`, fed by an
  undeclared global at `ImportTab.lua:972` that is never assigned; it is not
  a behavior to match. We hold the session id in memory only and never write
  it to disk.

- **League filter defaults to the current league.** When a build has no
  remembered league (upstream `importTab.lastLeague`), the character-list
  filter preselects `char_import::CURRENT_LEAGUE` instead of "All", falling
  back to "All" when the account has no character there. The remembered
  league still wins when present. Bump the constant each league.
  `char_import::pick_league_index`.

- **Timeless jewel search omissions:** no protected-nodes list, no
  socket-allocation filter, no devotion-variant trade dropdowns (we have no
  trade integration). These are gaps rather than different behavior; tracked
  in `plans/parity-plan.md`, noted here because the seed-search port
  deliberately skips their branches.

## Mechanism differences (same semantics, different UI)

- Tree spec reorder via up/down buttons instead of list dragging (the index
  adjustment matches upstream's OnOrderChange).
- Socket group reorder via drag handles on collapsing headers instead of a
  draggable list row (same OnOrderChange index math).
- Undo/redo hotkeys only fire when no text widget has focus (egui text
  fields own Ctrl+Z internally).

- **No sub-script system (LaunchSubScript).** Upstream runs HTTP and other
  background work on Lua worker threads via LaunchSubScript; we do all
  networking in Rust (reqwest, blocking) and stub LaunchSubScript. Any
  upstream feature built on sub-scripts is reimplemented on the Rust side
  (share uploads, character import, URL imports).

- **Our own egui theme.** Colors, spacing, and widget styling are egui-native
  rather than a recreation of upstream's SimpleGraphic look. PoB color codes
  (`^7`, `^xRRGGBB`) in Lua-produced text are honoured via
  `theme::pob_layout_job`. Presentation is fully ours per policy; listed here
  only because the parity plan tracked "consistent theme/styling matching
  upstream" as an item.

- **No "disable scroll wheel on controls" option.** Upstream added an option
  to stop the scroll wheel changing dropdown/slider values while scrolling a
  pane. egui dropdowns and sliders do not capture scroll while a pane
  scrolls, so the problem this option solves does not exist here.

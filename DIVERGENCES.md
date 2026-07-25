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

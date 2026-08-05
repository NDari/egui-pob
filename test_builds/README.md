# Test build fixtures

Known-good build XMLs used as calc oracles. Each file's `<PlayerStat>`
elements were written by real Path of Building, so `cargo test --test
calc_verification` compares our headless engine against upstream's own output
rather than against ourselves.

| Fixture | Character | Tree | Verified against | Notes |
|---------|-----------|------|------------------|-------|
| `3.28-migrage/reliqlone.xml` | Scion / Reliquarian, lvl 95 | 3.28 | re-baselined at PoB 2.67.0 | The default fixture for the interaction tests; deliberately one tree version behind, so tree-version conversion stays covered. |
| `3.29-allflame/holyruuj.xml` | Templar / Hierophant, lvl 87 | 3.29 | PoB 2.67.0 (portable) | Socketed Lethal Pride (seed 17962, Rakiata), so the timeless-jewel LUT path is exercised. 96/96 stats matched on capture. |

## Rules

- **Fixtures are consumed by more than one test.** `tests/common` exposes
  `DEFAULT_FIXTURE`, `ALLFLAME_FIXTURE`, and `ALL_FIXTURES`. Never select a
  fixture by "first XML in the directory": iteration order is
  filesystem-dependent and silently changes which build is under test.

- **Fixtures go stale every league.** A fixture captured on the then-current
  tree becomes one version old at the next league. Tests needing
  latest-tree behavior must convert first (see `test_loadouts`,
  `test_tree_version_conversion`), which doubles as conversion coverage.

- **Re-baselining after an upstream bump.** When a pin move legitimately
  changes calc output, confirm the drift is explained by the changelog before
  overwriting any `value=` attribute, then rewrite only the drifted stats.
  Format numbers with Lua's `tostring` (`%.14g`), which is what upstream's
  saver uses (`Build.lua`), so the file stays byte-faithful to a real save.

## Adding a fixture

Save the build in real Path of Building (not in this app) so the
`<PlayerStat>` block is upstream's, drop the XML under a
`<patch>-<league>/` directory, and add it to `ALL_FIXTURES` in
`tests/common/mod.rs`. Record the PoB version you verified against in the
table above: a fixture captured on a different version than the current
submodule pin is not a valid oracle.

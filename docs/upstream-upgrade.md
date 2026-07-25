# Upstream submodule upgrade playbook

How to move the `upstream/` pin to a new PoB release with minimal surprise.
Derived from the v2.63.0 -> v2.66.1 (3.29) upgrade. Git operations are done
by the maintainer; everything else can be driven by tooling/tests.

## Sequence

1. **Commit the working tree first.** Upgrade breakage should never tangle
   with in-flight feature work.

2. **Fetch, don't pin yet** (maintainer):

   ```
   git -C upstream fetch origin --tags
   ```

3. **Pre-pin review** (read-only, against `OLD..NEW`):
   - `just upgrade-review OLD NEW` prints the commit count and the changelog
     section for the new releases.
   - Diff the registered port sources: every `[[port]]` entry in `ports.toml`
     names its upstream file and anchor. (The automated check runs after the
     pin moves; before it moves, diff the anchored functions by hand for the
     headline changes.)
   - Grep the diff for new engine-level globals that may need stubs in
     `src/lua_bridge/stubs.rs` (boot failures also catch these loudly).

4. **Move the pin** (maintainer):

   ```
   git -C upstream checkout NEW
   git add upstream
   git commit -m "pin upstream NEW"
   ```

5. **Make it green:**
   - `cargo test --test ports_sync` - fails with a precise list of every
     ported upstream function that changed. For each: diff old..new, re-sync
     the port (or record an intentional divergence in `DIVERGENCES.md`), then
     update the hash.
   - `just test` - the full VM-booting suite. Historically this catches
     everything the review missed (removed methods, signature changes,
     coroutine-ified code paths).
   - Conformance tests (`test_socket_group_copy_paste`,
     `test_search_conforms_to_upstream_matcher`, `test_build_xml_roundtrip`)
     verify our behavior against upstream's own functions, not just against
     ourselves.

6. **Update version references** when the PoE patch changed:
   - `CLAUDE.md` header ("Current PoE 1 version").
   - The parity plan stamp ("Parity validated against ...") in
     `plans/parity-plan.md`.
   - Claude's project memory (it tracks the current patch).

7. **Parity delta:** extract the changelog entries between pins and add an
   "Upstream Delta" section to `plans/parity-plan.md` mapping new features to
   checkboxes, obsolete features to strikethroughs.

## Rules learned the hard way

- **Tests must not assume the fixture build is on the latest tree version.**
  Every league, `test_builds/` becomes one version old; tests that need
  latest-tree behavior must convert first (see `test_loadouts`,
  `test_tree_version_conversion`), which doubles as conversion coverage.

- **Known upstream save nondeterminism** (do not "fix" these, canonicalize):
  config `<Input>` elements, the Spec `nodes`/`masteryEffects` attributes,
  and the legacy `URL` element are all built from hash-table iteration and
  reorder freely between VMs. `test_build_xml_roundtrip` compares
  canonicalized structure for exactly this reason.

- **Self-consistency tests don't catch format drift.** Our copy -> our paste
  passed while both sides were wrong (v2.66 dropped gem qualityId). Formats
  must round-trip through upstream's own functions (tier 1 in CLAUDE.md).

- **Upstream deletes features.** Alternate gem qualities vanished wholesale
  in v2.66; the fix was deleting our mirror, not repairing it. Check the
  changelog for removals, not just additions.

- **UI-control state is not an API.** Never read or write
  `tab.controls.*` from Rust; depend only on the data model and callable
  functions. (The v2.66 import rework broke exactly the two places we poked
  control state; the fix removed the dependency.)

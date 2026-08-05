#![allow(dead_code)] // shared across test binaries; not all use every helper

use std::path::PathBuf;

/// Find the repo root by walking up from the test executable.
pub fn find_repo_root() -> PathBuf {
    let exe = std::env::current_exe().expect("could not get exe path");
    let mut candidate = exe.parent().unwrap().to_path_buf();
    for _ in 0..5 {
        if candidate.join("upstream").join("src").is_dir() {
            return candidate;
        }
        if !candidate.pop() {
            break;
        }
    }
    panic!(
        "Could not find repo root with upstream/src/. \
         Make sure the git submodule is initialized: git submodule update --init"
    );
}

/// Boot the Lua bridge and load the test build.
/// Returns the bridge with a build already loaded.
///
/// Named explicitly rather than "first XML found": `test_builds/` holds more
/// than one fixture, and directory iteration order is filesystem-dependent,
/// so picking implicitly would let the build under test change between
/// machines. Use [`boot_and_load_build`] to target a different fixture.
pub fn boot_and_load_test_build() -> pob_egui::lua_bridge::LuaBridge {
    boot_and_load_build(DEFAULT_FIXTURE)
}

/// The fixture the interaction tests are written against (Scion Reliquarian,
/// tree 3.28). Assertions here reference its specific gear and skills, so it
/// must stay the default even as newer fixtures are added.
pub const DEFAULT_FIXTURE: &str = "3.28-migrage/reliqlone.xml";

/// A second fixture: Templar Hierophant on tree 3.29 with a socketed Lethal
/// Pride, verified stat-for-stat against Path of Building v2.67.0.
pub const ALLFLAME_FIXTURE: &str = "3.29-allflame/holyruuj.xml";

/// Every build fixture, in a stable order.
pub const ALL_FIXTURES: &[&str] = &[DEFAULT_FIXTURE, ALLFLAME_FIXTURE];

/// Boot the Lua bridge and load a specific fixture, given as a path relative
/// to `test_builds/`.
pub fn boot_and_load_build(relative_path: &str) -> pob_egui::lua_bridge::LuaBridge {
    let repo_root = find_repo_root();
    let src_path = repo_root.join("upstream").join("src");

    let bridge = pob_egui::lua_bridge::LuaBridge::new(&src_path, &repo_root)
        .expect("failed to init Lua bridge");
    bridge.verify_boot().expect("boot verification failed");

    let xml_path = repo_root.join("test_builds").join(relative_path);
    let xml_text = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", xml_path.display()));

    bridge
        .load_build_from_xml(&xml_text, "Test Build", None)
        .expect("failed to load build");

    bridge
}

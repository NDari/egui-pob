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
pub fn boot_and_load_test_build() -> pob_egui::lua_bridge::LuaBridge {
    let repo_root = find_repo_root();
    let src_path = repo_root.join("upstream").join("src");

    let bridge = pob_egui::lua_bridge::LuaBridge::new(&src_path, &repo_root)
        .expect("failed to init Lua bridge");
    bridge.verify_boot().expect("boot verification failed");

    let test_builds = repo_root.join("test_builds");
    let xml_path = find_first_xml(&test_builds).expect("no XML files found in test_builds/");
    let xml_text = std::fs::read_to_string(&xml_path).expect("failed to read test build XML");

    bridge
        .load_build_from_xml(&xml_text, "Test Build")
        .expect("failed to load build");

    bridge
}

pub fn find_first_xml(dir: &std::path::Path) -> Option<PathBuf> {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().is_some_and(|ext| ext == "xml") {
            return Some(entry.path().to_path_buf());
        }
    }
    None
}

//! Integration test: verify the Lua VM boots successfully and key objects exist.

use std::path::PathBuf;

/// Find the repo root by walking up from the test executable.
fn find_repo_root() -> PathBuf {
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

#[test]
fn test_lua_vm_boots_and_main_object_exists() {
    let _ = env_logger::builder().is_test(true).try_init();

    let repo_root = find_repo_root();
    let src_path = repo_root.join("upstream").join("src");

    let bridge = pob_egui::lua_bridge::LuaBridge::new(&src_path, &repo_root)
        .expect("failed to init Lua bridge");

    bridge.verify_boot().expect("boot verification failed");

    // Verify main object is fully initialized (not just mainObject_ref)
    let main_type: String = bridge
        .lua()
        .load("return type(mainObject_ref.main)")
        .eval()
        .expect("failed to check mainObject_ref.main");
    assert_eq!(main_type, "table", "mainObject_ref.main should be a table");

    // Verify no startup errors
    let prompt_msg: Option<String> = bridge
        .lua()
        .load("return mainObject_ref.promptMsg")
        .eval()
        .unwrap_or(None);
    assert!(
        prompt_msg.is_none(),
        "unexpected startup error: {}",
        prompt_msg.unwrap_or_default()
    );
}

mod gui;

use std::path::PathBuf;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    env_logger::init();

    let (src_path, base_dir) = find_paths()?;
    log::info!("upstream/src: {}", src_path.display());
    log::info!("base dir: {}", base_dir.display());

    let bridge = pob_egui::lua_bridge::LuaBridge::new(&src_path, &base_dir);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Path of Building"),
        ..Default::default()
    };

    eframe::run_native(
        "pob-egui",
        options,
        Box::new(|cc| Ok(Box::new(gui::PobApp::new(bridge, cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

/// Find the upstream/src/ directory and the repo base directory.
///
/// In dev mode: the binary is at target/debug/pob-egui, so base_dir is the repo root.
/// In distribution: src/ is a sibling of the binary.
fn find_paths() -> Result<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().context("Could not determine executable path")?;
    let exe_dir = exe.parent().unwrap();

    // Try dev mode first: repo_root/upstream/src/
    // Walk up from exe to find a directory containing "upstream/src/"
    let mut candidate = exe_dir.to_path_buf();
    for _ in 0..5 {
        let upstream_src = candidate.join("upstream").join("src");
        if upstream_src.is_dir() {
            return Ok((upstream_src, candidate));
        }
        if !candidate.pop() {
            break;
        }
    }

    // Try distribution mode: src/ is a sibling of the binary
    let dist_src = exe_dir.join("src");
    if dist_src.is_dir() {
        return Ok((dist_src, exe_dir.to_path_buf()));
    }

    // Try current working directory
    let cwd = std::env::current_dir().context("Could not get cwd")?;
    let upstream_src = cwd.join("upstream").join("src");
    if upstream_src.is_dir() {
        return Ok((upstream_src, cwd));
    }

    anyhow::bail!(
        "Could not find upstream/src/ directory. \
         Looked relative to exe ({}) and cwd ({})",
        exe_dir.display(),
        cwd.display()
    )
}

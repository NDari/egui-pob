//! Lua bridge: embeds LuaJIT, loads upstream PoB headless, and provides
//! the interface between Rust/egui and the Lua calc engine + data model.

mod filesearch;
mod stubs;
mod system;

use std::path::Path;

use anyhow::Result;
use mlua::prelude::*;

/// Convert mlua::Error to anyhow::Error with context.
fn lua_err(msg: &str) -> impl FnOnce(mlua::Error) -> anyhow::Error + '_ {
    move |e| anyhow::anyhow!("{msg}: {e}")
}

pub struct LuaBridge {
    lua: Lua,
}

impl LuaBridge {
    /// Create a new LuaBridge, loading upstream PoB headless.
    ///
    /// - `src_path`: absolute path to upstream/src/
    /// - `base_dir`: absolute path to the repo root (parent of upstream/)
    pub fn new(src_path: &Path, base_dir: &Path) -> Result<Self> {
        let lua = unsafe { Lua::unsafe_new() };

        // Set up package paths
        let src = src_path.to_string_lossy();
        let runtime_lua = base_dir
            .join("upstream")
            .join("runtime")
            .join("lua")
            .to_string_lossy()
            .to_string();

        lua.load(format!(
            r#"package.path = "{src}/?.lua;{src}/?/init.lua;{runtime_lua}/?.lua;{runtime_lua}/?/init.lua;" .. package.path"#,
        ))
        .exec()
        .map_err(lua_err("Failed to set package.path"))?;

        // Register SimpleGraphic API stubs (rendering/input no-ops)
        stubs::register(&lua).map_err(lua_err("Failed to register stubs"))?;

        // Register working system functions (paths, time, clipboard, etc.)
        system::register(&lua, src_path, base_dir)
            .map_err(lua_err("Failed to register system functions"))?;

        // Register NewFileSearch (overwrites the stub from stubs.rs)
        filesearch::register(&lua).map_err(lua_err("Failed to register NewFileSearch"))?;

        // Change working directory to upstream/src/ so relative paths resolve
        std::env::set_current_dir(src_path)
            .map_err(|e| anyhow::anyhow!("Failed to chdir to upstream/src/: {e}"))?;

        // Load upstream's entry point
        lua.load("LoadModule('Launch')")
            .exec()
            .map_err(lua_err("Failed to load Launch.lua"))?;

        // Initialize: OnInit + one frame
        Self::run_callback_static(&lua, "OnInit")?;
        Self::run_callback_static(&lua, "OnFrame")?;

        log::info!("Lua bridge initialized successfully");

        Ok(Self { lua })
    }

    /// Run a named callback (mirrors upstream's callback system).
    fn run_callback_static(lua: &Lua, name: &str) -> Result<()> {
        lua.load(format!("_runCallback('{name}')"))
            .exec()
            .map_err(|e| anyhow::anyhow!("Callback '{name}' failed: {e}"))?;
        Ok(())
    }

    /// Run a frame (triggers recalculation if buildFlag is set).
    pub fn run_frame(&self) -> Result<()> {
        Self::run_callback_static(&self.lua, "OnFrame")
    }

    /// Get a reference to the Lua VM.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Load a build from XML text. This calls upstream's SetMode("BUILD", ...)
    /// and runs a frame to trigger the initial calculation.
    pub fn load_build_from_xml(&self, xml_text: &str, name: &str) -> Result<()> {
        let main_obj: LuaTable = self
            .lua
            .load("return mainObject_ref.main")
            .eval()
            .map_err(lua_err("Failed to get mainObject.main"))?;

        main_obj
            .call_method::<()>("SetMode", ("BUILD", false, name, xml_text))
            .map_err(lua_err("SetMode('BUILD') failed"))?;

        Self::run_callback_static(&self.lua, "OnFrame")?;
        Self::run_callback_static(&self.lua, "OnFrame")?;

        log::info!("Build loaded: {name}");
        Ok(())
    }

    /// Get the build directory path (where user saves builds).
    pub fn build_path(&self) -> Result<String> {
        self.lua
            .load("return mainObject_ref.main.buildPath")
            .eval()
            .map_err(lua_err("Failed to get buildPath"))
    }

    /// Switch to the build list mode.
    pub fn set_mode_list(&self) -> Result<()> {
        let main_obj: LuaTable = self
            .lua
            .load("return mainObject_ref.main")
            .eval()
            .map_err(lua_err("Failed to get mainObject.main"))?;

        main_obj
            .call_method::<()>("SetMode", ("LIST",))
            .map_err(lua_err("SetMode('LIST') failed"))?;

        Self::run_callback_static(&self.lua, "OnFrame")?;
        Ok(())
    }

    /// Create a dummy bridge (for error display when real init fails).
    pub fn new_dummy() -> Self {
        Self {
            lua: unsafe { Lua::unsafe_new() },
        }
    }

    /// Check if the Lua VM booted successfully by verifying key objects exist.
    pub fn verify_boot(&self) -> Result<()> {
        let main_obj: LuaValue = self
            .lua
            .load("return mainObject_ref")
            .eval()
            .map_err(lua_err("mainObject_ref not found"))?;

        if main_obj.is_nil() {
            anyhow::bail!("mainObject_ref is nil — upstream bootstrap failed");
        }

        log::info!("Boot verification passed: mainObject_ref exists");
        Ok(())
    }
}

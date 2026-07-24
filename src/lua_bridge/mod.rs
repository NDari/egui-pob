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

        // Disable upstream's dev-mode auto-save on Shutdown. When running from
        // source, upstream detects devMode=true and writes `~~temp~~.xml` on
        // every BUILD mode transition - we manage saves explicitly via Save /
        // Save As, so this would resurrect discarded new builds.
        lua.load("mainObject_ref.main.disableDevAutoSave = true")
            .exec()
            .map_err(lua_err("Failed to set disableDevAutoSave"))?;

        // Upstream's dev mode puts the build directory inside upstream/src/,
        // which is our read-only submodule. Point it at the user data
        // directory instead (~/.local/share/pathofbuilding/Builds/ on Linux).
        let builds_dir = system::user_builds_path();
        std::fs::create_dir_all(&builds_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create build dir {builds_dir}: {e}"))?;
        lua.load("mainObject_ref.main.buildPath = ...")
            .call::<()>(builds_dir.as_str())
            .map_err(lua_err("Failed to set buildPath"))?;
        log::info!("Build directory: {builds_dir}");

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
    ///
    /// `db_file_name` is the full path of the build's XML file on disk, if it
    /// has one - upstream stores it as build.dbFileName so Save writes back to
    /// the same file. Pass None for builds that don't exist on disk yet
    /// (imports, pasted codes).
    pub fn load_build_from_xml(
        &self,
        xml_text: &str,
        name: &str,
        db_file_name: Option<&str>,
    ) -> Result<()> {
        let main_obj: LuaTable = self
            .lua
            .load("return mainObject_ref.main")
            .eval()
            .map_err(lua_err("Failed to get mainObject.main"))?;

        main_obj
            .call_method::<()>("SetMode", ("BUILD", db_file_name, name, xml_text))
            .map_err(lua_err("SetMode('BUILD') failed"))?;

        Self::run_callback_static(&self.lua, "OnFrame")?;
        Self::run_callback_static(&self.lua, "OnFrame")?;

        // Upstream marks XML-text loads as modified (that path is normally
        // only used for imports). A build opened from its own file starts
        // clean, like upstream's LoadDBFile path.
        if db_file_name.is_some() {
            self.lua
                .load(
                    r#"
                    local build = mainObject_ref.main.modes['BUILD']
                    build.modFlag = false
                    build.unsaved = false
                "#,
                )
                .exec()
                .map_err(lua_err("Failed to clear modFlag"))?;
        }

        log::info!("Build loaded: {name}");
        Ok(())
    }

    /// Create a new empty build (default Scion, level 1, nothing allocated).
    pub fn create_new_build(&self) -> Result<()> {
        let main_obj: LuaTable = self
            .lua
            .load("return mainObject_ref.main")
            .eval()
            .map_err(lua_err("Failed to get mainObject.main"))?;

        main_obj
            .call_method::<()>("SetMode", ("BUILD", false, "New Build"))
            .map_err(lua_err("SetMode('BUILD') for new build failed"))?;

        Self::run_callback_static(&self.lua, "OnFrame")?;
        Self::run_callback_static(&self.lua, "OnFrame")?;

        log::info!("New build created");
        Ok(())
    }

    /// Get the build directory path (where user saves builds).
    pub fn build_path(&self) -> Result<String> {
        self.lua
            .load("return mainObject_ref.main.buildPath")
            .eval()
            .map_err(lua_err("Failed to get buildPath"))
    }

    /// Whether the build has unsaved changes (upstream's build.unsaved,
    /// recomputed from all tab modFlags every OnFrame).
    pub fn is_build_dirty(&self) -> bool {
        self.lua
            .load("return mainObject_ref.main.modes['BUILD'].unsaved == true")
            .eval()
            .unwrap_or(false)
    }

    /// The build's file path on disk, if it has been saved before.
    pub fn build_file_name(&self) -> Option<String> {
        self.lua
            .load(
                r#"
                local name = mainObject_ref.main.modes['BUILD'].dbFileName
                if type(name) == "string" and name ~= "" then
                    return name
                end
                return nil
            "#,
            )
            .eval::<Option<String>>()
            .ok()
            .flatten()
    }

    /// Save the current build to its existing file (build.dbFileName).
    /// Fails if the build has never been saved - use save_build_as first.
    pub fn save_build(&self) -> Result<()> {
        let err: Option<String> = self
            .lua
            .load(
                r#"
                local build = mainObject_ref.main.modes['BUILD']
                if not build.dbFileName or build.dbFileName == "" then
                    return "No filename set - use Save As first"
                end
                -- SaveDBFile returns true on failure (unwritable path, etc.)
                if build:SaveDBFile() then
                    return "Couldn't write " .. build.dbFileName
                end
                build.unsaved = false
                return nil
            "#,
            )
            .call(())
            .map_err(lua_err("Save failed"))?;

        if let Some(err) = err {
            anyhow::bail!("{err}");
        }
        Ok(())
    }

    /// Save the current build to disk with a given name (Save As).
    /// Mirrors upstream's OpenSaveAsPopup: the file goes to
    /// main.buildPath .. name .. ".xml" with illegal filename characters
    /// replaced, and build.dbFileName/buildName are updated so subsequent
    /// saves write to the same file.
    pub fn save_build_as(&self, name: &str) -> Result<()> {
        let err: Option<String> = self
            .lua
            .load(
                r#"
                local name = ...
                local main = mainObject_ref.main
                local build = main.modes['BUILD']
                name = name:gsub("[\\/:%*%?\"<>|%c]", "-")
                if not name:match("%S") then
                    return "Build name cannot be empty"
                end
                MakeDir(main.buildPath)
                build.dbFileName = main.buildPath .. name .. ".xml"
                build.dbFileSubPath = ""
                build.buildName = name
                -- SaveDBFile returns true on failure (unwritable path, etc.)
                if build:SaveDBFile() then
                    return "Couldn't write " .. build.dbFileName
                end
                build.unsaved = false
                return nil
            "#,
            )
            .call(name)
            .map_err(lua_err("Save As failed"))?;

        if let Some(err) = err {
            anyhow::bail!("{err}");
        }
        Ok(())
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

//! SimpleGraphic API stubs — rendering/input no-ops registered as Lua globals.
//! These allow upstream PoB code to run headless without errors.

use mlua::prelude::*;

/// Strip PoB color escapes (^0-^9 and ^xRRGGBB) from text.
pub fn strip_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'^' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next.is_ascii_digit() {
                // ^0 through ^9 — skip 2 chars
                i += 2;
                continue;
            } else if next == b'x' && i + 8 <= bytes.len() {
                // ^xRRGGBB — skip 8 chars
                if bytes[i + 2..i + 8].iter().all(|b| b.is_ascii_hexdigit()) {
                    i += 8;
                    continue;
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Register all rendering/input/window stubs as Lua globals.
pub fn register(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();

    // -- Rendering no-ops --
    let noop = lua.create_function(|_, _args: LuaMultiValue| Ok(()))?;
    for name in [
        "RenderInit",
        "SetClearColor",
        "SetDrawLayer",
        "SetViewport",
        "SetDrawColor",
        "DrawImage",
        "DrawImageQuad",
        "DrawImageRotated",
        "DrawString",
        "SetProfiling",
        "TakeScreenshot",
    ] {
        g.set(name, noop.clone())?;
    }

    // DrawStringWidth — approximate width for layout calcs
    g.set(
        "DrawStringWidth",
        lua.create_function(|_, (height, _font, text): (f64, LuaValue, String)| {
            let stripped = strip_escapes(&text);
            Ok(stripped.len() as f64 * height * 0.6)
        })?,
    )?;

    // DrawStringCursorIndex
    g.set(
        "DrawStringCursorIndex",
        lua.create_function(|_, _args: LuaMultiValue| Ok(0i32))?,
    )?;

    // StripEscapes
    g.set(
        "StripEscapes",
        lua.create_function(|_, text: String| Ok(strip_escapes(&text)))?,
    )?;

    g.set("GetAsyncCount", lua.create_function(|_, ()| Ok(0i32))?)?;

    // -- Input stubs --
    g.set(
        "GetCursorPos",
        lua.create_function(|_, ()| Ok((0i32, 0i32)))?,
    )?;
    g.set(
        "SetCursorPos",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    g.set(
        "ShowCursor",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    g.set(
        "IsKeyDown",
        lua.create_function(|_, _name: LuaValue| Ok(false))?,
    )?;

    // -- Window stubs --
    g.set(
        "SetWindowTitle",
        lua.create_function(|_, _title: LuaValue| Ok(()))?,
    )?;
    g.set(
        "GetScreenSize",
        lua.create_function(|_, ()| Ok((1920i32, 1080i32)))?,
    )?;
    g.set(
        "GetVirtualScreenSize",
        lua.create_function(|_, ()| Ok((1920i32, 1080i32)))?,
    )?;
    g.set("GetScreenScale", lua.create_function(|_, ()| Ok(1.0f64))?)?;
    g.set(
        "GetDPIScaleOverridePercent",
        lua.create_function(|_, ()| Ok(1.0f64))?,
    )?;
    g.set(
        "SetDPIScaleOverridePercent",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;

    // -- Image handles (Lua-side metatable) --
    lua.load(
        r#"
        local mt = {}
        mt.__index = mt
        function mt:Load() self.valid = true end
        function mt:Unload() self.valid = false end
        function mt:IsValid() return self.valid end
        function mt:SetLoadingPriority() end
        function mt:ImageSize() return 1, 1 end
        function mt:IsLoading() return false end
        function NewImageHandle()
            return setmetatable({}, mt)
        end
    "#,
    )
    .exec()?;

    // -- File search stub --
    // Upstream needs NewFileSearch for build file enumeration.
    // We provide a minimal implementation here; a full version lives in filesearch.rs.
    // This stub is a fallback if the full version hasn't been registered yet.
    g.set(
        "NewFileSearch",
        lua.create_function(|_, _args: LuaMultiValue| Ok(LuaValue::Nil))?,
    )?;

    // -- Sub-script stubs (will be replaced by full impl in subscript.rs) --
    g.set(
        "LaunchSubScript",
        lua.create_function(|_, _args: LuaMultiValue| Ok(LuaValue::Nil))?,
    )?;
    g.set(
        "AbortSubScript",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    g.set(
        "IsSubScriptRunning",
        lua.create_function(|_, _args: LuaMultiValue| Ok(false))?,
    )?;

    // -- Misc stubs --
    g.set("Restart", lua.create_function(|_, ()| Ok(()))?)?;
    g.set("Exit", lua.create_function(|_, ()| Ok(()))?)?;
    g.set("ConExecute", lua.create_function(|_, _cmd: String| Ok(()))?)?;
    g.set("ConClear", lua.create_function(|_, ()| Ok(()))?)?;
    g.set(
        "SpawnProcess",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    g.set(
        "OpenURL",
        lua.create_function(|_, url: String| {
            let _ = open::that(&url);
            Ok(())
        })?,
    )?;
    g.set(
        "GetCloudProvider",
        lua.create_function(|_, _path: LuaValue| {
            Ok((LuaValue::Nil, LuaValue::Nil, LuaValue::Nil))
        })?,
    )?;

    Ok(())
}

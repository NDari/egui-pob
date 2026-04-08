//! Working system functions registered as Lua globals.
//! These are the functions upstream code actually NEEDS to operate:
//! paths, time, clipboard, compression, module loading, console, etc.

use std::path::Path;
use std::time::Instant;

use mlua::prelude::*;

/// Register all working system functions as Lua globals.
pub fn register(lua: &Lua, src_path: &Path, base_dir: &Path) -> LuaResult<()> {
    let g = lua.globals();

    // -- Time --
    let start = Instant::now();
    g.set(
        "GetTime",
        lua.create_function(move |_, ()| Ok(start.elapsed().as_millis() as f64))?,
    )?;

    // -- Paths --
    let script_path = src_path.to_string_lossy().to_string();
    let runtime_path = base_dir
        .join("upstream")
        .join("runtime")
        .to_string_lossy()
        .to_string();

    let user_path = get_user_path();

    let sp = script_path.clone();
    g.set(
        "GetScriptPath",
        lua.create_function(move |_, ()| Ok((sp.clone(), sp.clone())))?,
    )?;

    let rp = runtime_path.clone();
    g.set(
        "GetRuntimePath",
        lua.create_function(move |_, ()| Ok((rp.clone(), rp.clone())))?,
    )?;

    let up = user_path.clone();
    g.set(
        "GetUserPath",
        lua.create_function(move |_, ()| Ok(up.clone()))?,
    )?;

    g.set(
        "GetWorkDir",
        lua.create_function(|_, ()| {
            Ok(std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default())
        })?,
    )?;

    g.set(
        "SetWorkDir",
        lua.create_function(|_, path: String| {
            let _ = std::env::set_current_dir(&path);
            Ok(())
        })?,
    )?;

    // -- Clipboard --
    g.set(
        "Copy",
        lua.create_function(|_, text: String| {
            if let Ok(mut clip) = arboard::Clipboard::new() {
                let _ = clip.set_text(&text);
            }
            Ok(())
        })?,
    )?;

    g.set(
        "Paste",
        lua.create_function(|_, ()| {
            Ok(arboard::Clipboard::new()
                .and_then(|mut c| c.get_text())
                .ok())
        })?,
    )?;

    // -- Compression --
    g.set(
        "Deflate",
        lua.create_function(|lua, data: LuaString| {
            use flate2::Compression;
            use flate2::write::ZlibEncoder;
            use std::io::Write;
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&bytes).map_err(LuaError::external)?;
            let compressed = encoder.finish().map_err(LuaError::external)?;
            lua.create_string(&compressed)
        })?,
    )?;

    g.set(
        "Inflate",
        lua.create_function(|lua, data: LuaString| {
            use flate2::read::ZlibDecoder;
            use std::io::Read;
            let bytes: Vec<u8> = data.as_bytes().to_vec();
            let mut decoder = ZlibDecoder::new(bytes.as_slice());
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(LuaError::external)?;
            lua.create_string(&decompressed)
        })?,
    )?;

    // -- Module loading --
    g.set(
        "LoadModule",
        lua.create_function(|lua, args: LuaMultiValue| {
            let args_vec: Vec<LuaValue> = args.into_iter().collect();
            if args_vec.is_empty() {
                return Err::<LuaMultiValue, _>(LuaError::external("LoadModule: no arguments"));
            }
            let name: String = lua.unpack(args_vec[0].clone())?;
            let path = if name.ends_with(".lua") {
                name.clone()
            } else {
                format!("{name}.lua")
            };

            let code = std::fs::read_to_string(&path).map_err(|e| {
                LuaError::external(format!("LoadModule() error loading '{path}': {e}"))
            })?;

            let extra_args: LuaMultiValue = args_vec[1..].to_vec().into();
            lua.load(&code).set_name(&path).call(extra_args)
        })?,
    )?;

    g.set(
        "PLoadModule",
        lua.create_function(|lua, args: LuaMultiValue| {
            let args_vec: Vec<LuaValue> = args.into_iter().collect();
            if args_vec.is_empty() {
                return Err(LuaError::external("PLoadModule: no arguments"));
            }
            let name: String = lua.unpack(args_vec[0].clone())?;
            let path = if name.ends_with(".lua") {
                name.clone()
            } else {
                format!("{name}.lua")
            };

            let code = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    return Err(LuaError::external(format!(
                        "PLoadModule() error loading '{path}': {e}"
                    )));
                }
            };

            let extra_args: LuaMultiValue = args_vec[1..].to_vec().into();
            match lua
                .load(&code)
                .set_name(&path)
                .call::<LuaMultiValue>(extra_args)
            {
                Ok(results) => {
                    // PLoadModule returns (nil, results...) on success
                    let mut ret: Vec<LuaValue> = vec![LuaValue::Nil];
                    ret.extend(results);
                    Ok(LuaMultiValue::from_iter(ret))
                }
                Err(e) => {
                    // PLoadModule returns (errMsg) on error
                    Ok(LuaMultiValue::from_iter(vec![LuaValue::String(
                        lua.create_string(e.to_string())?,
                    )]))
                }
            }
        })?,
    )?;

    // PCall
    lua.load(
        r#"
        function PCall(func, ...)
            local ret = { pcall(func, ...) }
            if ret[1] then
                table.remove(ret, 1)
                return nil, unpack(ret)
            else
                return ret[2]
            end
        end
    "#,
    )
    .exec()?;

    // -- Console --
    // ConPrintf: use Lua's string.format for proper formatting, then log
    lua.load(
        r##"
        function ConPrintf(fmt, ...)
            if select("#", ...) > 0 then
                print(string.format(fmt, ...))
            else
                print(fmt)
            end
        end
        function ConPrintTable(tbl, noRecurse) end
    "##,
    )
    .exec()?;

    // -- File system --
    g.set(
        "MakeDir",
        lua.create_function(|_, path: String| {
            let _ = std::fs::create_dir_all(&path);
            Ok(())
        })?,
    )?;

    g.set(
        "RemoveDir",
        lua.create_function(|_, path: String| {
            let _ = std::fs::remove_dir_all(&path);
            Ok(())
        })?,
    )?;

    // -- Misc --
    g.set(
        "GetPlatform",
        lua.create_function(|_, ()| Ok(std::env::consts::OS.to_string()))?,
    )?;

    // Provide a pure-Lua utf8 shim and stub lcurl.safe
    lua.load(
        r#"
        -- Minimal lua-utf8 shim: delegates to string library for ASCII-safe operations.
        -- Upstream uses utf8.reverse, utf8.gsub, utf8.sub, utf8.find, utf8.match, utf8.next.
        -- For headless calc, ASCII-range behavior is sufficient.
        local utf8_shim = {}
        utf8_shim.reverse = string.reverse
        utf8_shim.gsub = string.gsub
        utf8_shim.sub = string.sub
        utf8_shim.find = string.find
        utf8_shim.match = string.match
        utf8_shim.len = string.len
        utf8_shim.byte = string.byte
        utf8_shim.char = string.char
        utf8_shim.gmatch = string.gmatch
        utf8_shim.rep = string.rep
        utf8_shim.lower = string.lower
        utf8_shim.upper = string.upper
        utf8_shim.format = string.format
        function utf8_shim.next(s, idx, offset)
            if not offset then offset = 1 end
            local new_idx = idx + offset
            if new_idx < 0 or new_idx > #s + 1 then return nil end
            return new_idx
        end
        package.preload['lua-utf8'] = function() return utf8_shim end

        local orig_require = require
        function require(name)
            if name == "lcurl.safe" then
                return nil
            end
            return orig_require(name)
        end
    "#,
    )
    .exec()?;

    // -- Globals expected by upstream --
    g.set("arg", lua.create_table()?)?;

    // -- Callbacks --
    lua.load(
        r#"
        local callbackTable = {}
        local mainObject = nil
        function SetCallback(name, func)
            callbackTable[name] = func
        end
        function GetCallback(name)
            return callbackTable[name]
        end
        function SetMainObject(obj)
            mainObject = obj
            mainObject_ref = obj
        end
        function _runCallback(name, ...)
            if callbackTable[name] then
                return callbackTable[name](...)
            elseif mainObject and mainObject[name] then
                return mainObject[name](mainObject, ...)
            end
        end
    "#,
    )
    .exec()?;

    Ok(())
}

/// Determine user data path (XDG on Linux, AppData on Windows).
fn get_user_path() -> String {
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "PathOfBuilding") {
        let data_dir = proj_dirs.data_dir();
        let _ = std::fs::create_dir_all(data_dir);
        let mut path = data_dir.to_string_lossy().to_string();
        if !path.ends_with('/') {
            path.push('/');
        }
        path
    } else {
        String::from("./")
    }
}

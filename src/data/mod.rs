//! Rust-side data structures for marshaled Lua data.

pub mod build_list;
pub mod config;
pub mod items;
pub mod skills;
pub mod tree;
pub mod tree_sprites;

use std::collections::HashMap;

use mlua::prelude::*;

/// Calc output extracted from `build.calcsTab.mainOutput` after each recalculation.
/// Contains all offensive/defensive stats the sidebar displays.
#[derive(Debug, Clone, Default)]
pub struct CalcOutput {
    /// Stat name → numeric value (e.g., "TotalDPS" → 4674080.18)
    pub stats: HashMap<String, f64>,
}

impl CalcOutput {
    /// Extract calc output from the Lua VM.
    /// Reads `build.calcsTab.mainOutput` which is set after each calc pass.
    pub fn extract(lua: &Lua) -> Result<Self, mlua::Error> {
        let build: LuaTable = lua
            .load("return mainObject_ref.main.modes['BUILD']")
            .eval()?;
        let calcs_tab: LuaTable = build.get("calcsTab")?;
        let main_output: LuaTable = calcs_tab.get("mainOutput")?;

        let mut stats = HashMap::new();
        for pair in main_output.pairs::<String, LuaValue>() {
            let (key, value) = pair?;
            match value {
                LuaValue::Number(n) => {
                    stats.insert(key, n);
                }
                LuaValue::Integer(n) => {
                    stats.insert(key, n as f64);
                }
                LuaValue::Table(sub_table) => {
                    // Handle nested tables like MainHand.Accuracy → MainHandAccuracy
                    for sub_pair in sub_table.pairs::<String, LuaValue>() {
                        let (sub_key, sub_value) = sub_pair?;
                        let flat_key = format!("{key}{sub_key}");
                        match sub_value {
                            LuaValue::Number(n) => {
                                stats.insert(flat_key, n);
                            }
                            LuaValue::Integer(n) => {
                                stats.insert(flat_key, n as f64);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    // Skip non-numeric values (functions, strings, booleans, etc.)
                }
            }
        }

        Ok(Self { stats })
    }
}

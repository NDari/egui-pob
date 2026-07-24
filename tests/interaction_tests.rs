//! Integration tests for interactive features: config changes, node allocation,
//! items extraction, and skills extraction.

mod common;

use mlua::prelude::*;
use pob_egui::data::tree::TreeData;
use pob_egui::data::tree_sprites::TreeSpriteAtlas;

// ---------------------------------------------------------------------------
// Config change triggers recalc
// ---------------------------------------------------------------------------

#[test]
fn test_config_change_triggers_recalc() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    // Get initial stats
    let before =
        pob_egui::data::CalcOutput::extract(bridge.lua()).expect("failed to extract stats before");
    let initial_life = before.stats.get("Life").copied().unwrap_or(0.0);
    assert!(initial_life > 0.0, "Life should be positive");

    // Change resistance penalty from Act 10 (-60) to None (0)
    pob_egui::data::config::set_config_value(
        bridge.lua(),
        "resistancePenalty",
        LuaValue::Number(0.0),
    )
    .expect("failed to set config value");

    let after =
        pob_egui::data::CalcOutput::extract(bridge.lua()).expect("failed to extract stats after");

    // Compare all stats — at least one should change when removing resistance penalty
    let mut any_changed = false;
    for (stat, before_val) in &before.stats {
        if let Some(after_val) = after.stats.get(stat)
            && (after_val - before_val).abs() > 0.001
        {
            println!("  {stat}: {before_val} -> {after_val}");
            any_changed = true;
        }
    }

    assert!(
        any_changed,
        "at least one stat should change when resistance penalty is removed"
    );
}

// ---------------------------------------------------------------------------
// Node allocation changes stats
// ---------------------------------------------------------------------------

#[test]
fn test_node_allocation_changes_stats() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let before =
        pob_egui::data::CalcOutput::extract(bridge.lua()).expect("failed to extract stats before");

    // Find an unallocated node adjacent to the allocated tree and allocate it.
    // AllocNode expects the node object (not just an ID) and requires node.path to be set.
    let node_id: u32 = bridge
        .lua()
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.spec
            -- BuildAllDependsAndPaths computes paths for all nodes
            spec:BuildAllDependsAndPaths()
            for id, node in pairs(spec.nodes) do
                if not spec.allocNodes[id]
                   and node.type == "Normal"
                   and not node.ascendancyName
                   and node.path and #node.path > 0 then
                    spec:AllocNode(node)
                    spec:AddUndoState()
                    build.buildFlag = true
                    _runCallback('OnFrame')
                    return id
                end
            end
            return 0
        "#,
        )
        .eval()
        .expect("failed to find and allocate node");

    assert!(node_id > 0, "should find at least one allocatable node");
    println!("Allocated node {node_id}");

    let after =
        pob_egui::data::CalcOutput::extract(bridge.lua()).expect("failed to extract stats after");

    // Stats should differ (the node grants some stat)
    // Compare all stats — at least one should change
    let mut any_changed = false;
    for (stat, before_val) in &before.stats {
        if let Some(after_val) = after.stats.get(stat)
            && (after_val - before_val).abs() > 0.001
        {
            println!("  {stat}: {before_val} -> {after_val}");
            any_changed = true;
        }
    }

    assert!(
        any_changed,
        "at least one stat should change after allocating a normal node"
    );
}

// ---------------------------------------------------------------------------
// Mastery effect selection
// ---------------------------------------------------------------------------

#[test]
fn test_mastery_effect_selection_roundtrip() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    // Find an unallocated, reachable mastery node with selectable effects
    let node_id: u32 = bridge
        .lua()
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            spec:BuildAllDependsAndPaths()
            for id, node in pairs(spec.nodes) do
                if not spec.allocNodes[id]
                   and node.type == "Mastery"
                   and node.masteryEffects
                   and node.path and #node.path > 0 then
                    return id
                end
            end
            return 0
        "#,
        )
        .eval()
        .expect("failed to find a mastery node");
    assert!(node_id > 0, "should find an unallocated mastery node");

    // Fetch its effects
    let list = pob_egui::data::tree::fetch_mastery_effects(bridge.lua(), node_id)
        .expect("failed to fetch mastery effects")
        .expect("mastery node should have selectable effects");
    assert!(!list.node_name.is_empty(), "mastery should have a name");
    assert!(!list.effects.is_empty(), "should have at least one effect");
    assert!(list.current.is_none(), "no effect selected yet");
    for effect in &list.effects {
        assert!(effect.id > 0, "effect id should be positive");
        assert!(!effect.label.is_empty(), "effect label should not be empty");
    }
    println!(
        "Mastery {node_id} ({}): {} effects",
        list.node_name,
        list.effects.len()
    );

    // Select the first effect - node should become allocated with that effect
    let effect_id = list.effects[0].id;
    pob_egui::data::tree::select_mastery_effect(bridge.lua(), node_id, effect_id)
        .expect("failed to select mastery effect");

    let (is_alloc, selected): (bool, u32) = bridge
        .lua()
        .load(format!(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            return spec.allocNodes[{node_id}] ~= nil, spec.masterySelections[{node_id}] or 0
        "#
        ))
        .eval()
        .expect("failed to read selection state");
    assert!(is_alloc, "mastery node should be allocated after selection");
    assert_eq!(selected, effect_id, "selected effect should be recorded");

    // Re-fetching should report the current selection
    let list2 = pob_egui::data::tree::fetch_mastery_effects(bridge.lua(), node_id)
        .expect("failed to re-fetch mastery effects")
        .expect("should still have effects");
    assert_eq!(list2.current, Some(effect_id));

    // Refreshing mastery stats should give the node the selected effect's stats
    let mut tree = TreeData::extract(bridge.lua()).expect("failed to extract tree");
    tree.refresh_mastery_stats(bridge.lua())
        .expect("failed to refresh mastery stats");
    let node = tree.nodes.get(&node_id).expect("node should exist");
    assert!(node.is_allocated, "extracted node should be allocated");
    assert_eq!(
        node.stats.join(" / "),
        list.effects[0].label,
        "node stats should match the selected effect"
    );

    // Deallocating should clear the mastery selection (Lua handles cleanup)
    pob_egui::data::tree::toggle_node(bridge.lua(), node_id).expect("failed to toggle node");
    let (is_alloc, selected): (bool, u32) = bridge
        .lua()
        .load(format!(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            return spec.allocNodes[{node_id}] ~= nil, spec.masterySelections[{node_id}] or 0
        "#
        ))
        .eval()
        .expect("failed to read selection state");
    assert!(!is_alloc, "mastery node should be deallocated");
    assert_eq!(
        selected, 0,
        "mastery selection should be cleared on dealloc"
    );
}

// ---------------------------------------------------------------------------
// Hover path/depends info and undo/redo
// ---------------------------------------------------------------------------

#[test]
fn test_hover_info_and_undo_redo() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    // Find an unallocated normal node reachable from the allocated tree.
    // Paths should already be current from the build load (no explicit
    // BuildAllDependsAndPaths) because the app relies on that too.
    let node_id: u32 = bridge
        .lua()
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            for id, node in pairs(spec.nodes) do
                if not spec.allocNodes[id]
                   and node.type == "Normal"
                   and not node.ascendancyName
                   and node.path and #node.path > 0 then
                    return id
                end
            end
            return 0
        "#,
        )
        .eval()
        .expect("failed to find an unallocated node");
    assert!(node_id > 0, "should find an unallocated reachable node");

    // Unallocated: path leads to it, nothing depends on it
    let info = pob_egui::data::tree::fetch_hover_info(bridge.lua(), node_id)
        .expect("failed to fetch hover info");
    assert!(
        info.path.contains(&node_id),
        "path should include the node itself"
    );
    assert!(
        info.depends.is_empty(),
        "unallocated node should have no dependents"
    );

    let alloc_count = |lua: &mlua::Lua| -> u32 {
        lua.load(
            r#"
            local count = 0
            for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do
                count = count + 1
            end
            return count
        "#,
        )
        .eval()
        .expect("failed to count allocated nodes")
    };

    // Allocate it: it should now (at least) depend on itself
    let before_count = alloc_count(bridge.lua());
    pob_egui::data::tree::toggle_node(bridge.lua(), node_id).expect("failed to allocate");
    let after_count = alloc_count(bridge.lua());
    assert!(after_count > before_count, "allocation should add node(s)");

    let info = pob_egui::data::tree::fetch_hover_info(bridge.lua(), node_id)
        .expect("failed to fetch hover info after alloc");
    assert!(
        info.depends.contains(&node_id),
        "allocated node should depend on itself"
    );

    // Undo restores the previous allocation state; redo reapplies it
    pob_egui::data::tree::undo(bridge.lua()).expect("undo failed");
    assert_eq!(
        alloc_count(bridge.lua()),
        before_count,
        "undo should restore allocation count"
    );
    pob_egui::data::tree::redo(bridge.lua()).expect("redo failed");
    assert_eq!(
        alloc_count(bridge.lua()),
        after_count,
        "redo should reapply the allocation"
    );
}

// ---------------------------------------------------------------------------
// Tree search
// ---------------------------------------------------------------------------

#[test]
fn test_tree_search_on_real_tree() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let tree = TreeData::extract(bridge.lua()).expect("failed to extract tree data");

    // Common stat text should match plenty of nodes
    let life_matches = tree.search_matches("life");
    assert!(
        life_matches.len() > 50,
        "'life' should match many nodes, got {}",
        life_matches.len()
    );

    // Multi-term search narrows results (AND semantics)
    let narrowed = tree.search_matches("life mana");
    assert!(
        !narrowed.is_empty() && narrowed.len() < life_matches.len(),
        "'life mana' should narrow the match set: {} vs {}",
        narrowed.len(),
        life_matches.len()
    );

    // oil: prefix matches nodes with anoint recipes (notables)
    let oil_matches = tree.search_matches("oil:");
    assert!(
        !oil_matches.is_empty(),
        "'oil:' should match nodes with anoint recipes"
    );
    for id in &oil_matches {
        assert!(
            !tree.nodes[id].recipe.is_empty(),
            "oil: matches should all have recipes"
        );
    }

    // Type search finds keystones
    let keystones = tree.search_matches("keystone");
    assert!(!keystones.is_empty(), "'keystone' should match keystones");

    // Empty and garbage queries
    assert!(tree.search_matches("").is_empty());
    assert!(tree.search_matches("xyzzy_no_such_stat").is_empty());
}

// ---------------------------------------------------------------------------
// Calcs tab sections and breakdowns
// ---------------------------------------------------------------------------

#[test]
fn test_calcs_sections_and_breakdown() {
    use pob_egui::data::calcs::{self, BreakdownSection};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let sections = calcs::extract_sections(lua).expect("failed to extract calc sections");
    assert!(
        sections.len() > 5,
        "should have many calc sections, got {}",
        sections.len()
    );

    // Both layout groups should be present (offence and defence)
    assert!(sections.iter().any(|s| s.group == 1), "group 1 missing");
    assert!(sections.iter().any(|s| s.group >= 2), "group 2+ missing");

    // Cells should have formatted, non-placeholder text
    let mut cell_count = 0;
    let mut breakdown_cell = None;
    for section in &sections {
        for sub in &section.subsections {
            assert!(!sub.label.is_empty(), "subsection should have a label");
            for row in &sub.rows {
                for cell in &row.cells {
                    if !cell.text.is_empty() {
                        cell_count += 1;
                        assert!(
                            !cell.text.contains("{output:"),
                            "cell text should be fully formatted: {}",
                            cell.text
                        );
                        assert_ne!(cell.text, "?", "cell formatting errored");
                    }
                    if cell.has_breakdown && breakdown_cell.is_none() && !cell.text.is_empty() {
                        breakdown_cell = Some((section.si, sub.ui, row.ri, cell.ci));
                    }
                }
            }
        }
    }
    assert!(cell_count > 50, "expected many cells, got {cell_count}");

    // Fetch a breakdown for the first clickable cell
    let (si, ui, ri, ci) = breakdown_cell.expect("should find a breakdown cell");
    let breakdown = calcs::fetch_breakdown(lua, si, ui, ri, ci).expect("failed to fetch breakdown");
    for section in &breakdown {
        match section {
            BreakdownSection::Text { lines } => {
                assert!(!lines.is_empty());
                for line in lines {
                    assert!(
                        !line.starts_with("Breakdown error"),
                        "breakdown errored: {line}"
                    );
                }
            }
            BreakdownSection::Table { columns, rows, .. } => {
                assert!(!columns.is_empty());
                for row in rows {
                    assert!(row.len() <= columns.len());
                }
            }
        }
    }

    // Breakdown of the Life stat specifically (find a Life cell)
    let mut life_breakdowns = 0;
    'outer: for section in &sections {
        for sub in &section.subsections {
            for row in &sub.rows {
                for cell in &row.cells {
                    if cell.has_breakdown {
                        let bd = calcs::fetch_breakdown(lua, section.si, sub.ui, row.ri, cell.ci)
                            .expect("breakdown fetch failed");
                        if !bd.is_empty() {
                            life_breakdowns += 1;
                            if life_breakdowns > 10 {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        life_breakdowns > 5,
        "several cells should produce non-empty breakdowns, got {life_breakdowns}"
    );

    // Input state and mode switching
    let input = calcs::get_input(lua).expect("failed to get input");
    assert_eq!(input.buff_mode, "EFFECTIVE");

    calcs::set_buff_mode(lua, "UNBUFFED").expect("failed to set mode");
    let input = calcs::get_input(lua).expect("failed to get input");
    assert_eq!(input.buff_mode, "UNBUFFED");

    // Sections should still extract fine after a mode change
    let sections2 = calcs::extract_sections(lua).expect("re-extract failed");
    assert!(!sections2.is_empty());
}

// ---------------------------------------------------------------------------
// Items extraction
// ---------------------------------------------------------------------------

#[test]
fn test_items_extraction() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let items = pob_egui::data::items::extract_equipped_items(bridge.lua())
        .expect("failed to extract items");

    assert!(!items.is_empty(), "should have equipment slots");

    // At least some slots should have items equipped
    let equipped_count = items.iter().filter(|s| s.item.is_some()).count();
    println!(
        "Equipment slots: {}, equipped: {equipped_count}",
        items.len()
    );
    assert!(
        equipped_count > 0,
        "test build should have at least one item equipped"
    );

    // Check that equipped items have valid data
    for slot in &items {
        if let Some(ref item) = slot.item {
            assert!(
                !item.name.is_empty(),
                "item name should not be empty in slot {}",
                slot.slot_name
            );
            assert!(
                !item.rarity.is_empty(),
                "item rarity should not be empty in slot {}",
                slot.slot_name
            );
            // Items should have at least implicit or explicit mods (most do)
            println!(
                "  {}: {} ({}) — {} implicit, {} explicit mods",
                slot.slot_name,
                item.name,
                item.rarity,
                item.implicit_mods.len(),
                item.explicit_mods.len()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Skills editing
// ---------------------------------------------------------------------------

#[test]
fn test_skills_editing_roundtrip() {
    use pob_egui::data::skills::{self, GemProperty};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let initial = skills::extract_skills(lua).expect("failed to extract skills");
    let initial_count = initial.len();

    // Create a new empty socket group
    skills::new_socket_group(lua).expect("failed to create group");
    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    assert_eq!(groups.len(), initial_count + 1, "group should be added");
    let group_index = groups.last().unwrap().index;
    assert!(groups.last().unwrap().gems.is_empty());
    assert!(!groups.last().unwrap().from_item);

    // Add a gem by exact name (resolved by Lua's FindSkillGem)
    let err = skills::add_gem(lua, group_index, "Fireball").expect("add_gem call failed");
    assert!(err.is_none(), "Fireball should resolve: {err:?}");

    // Unknown names are rejected and the gem is not added
    let err = skills::add_gem(lua, group_index, "Zzzzqqqx").expect("add_gem call failed");
    assert!(err.is_some(), "nonsense gem name should error");

    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    let group = groups.last().unwrap();
    assert_eq!(group.gems.len(), 1, "only the valid gem should be added");
    assert_eq!(group.gems[0].name, "Fireball");
    assert!(group.gems[0].level >= 1);

    // Edit level, quality, and enabled state
    skills::set_gem_property(lua, group_index, 1, GemProperty::Level(15))
        .expect("failed to set level");
    skills::set_gem_property(lua, group_index, 1, GemProperty::Quality(20))
        .expect("failed to set quality");
    skills::set_gem_property(lua, group_index, 1, GemProperty::Enabled(false))
        .expect("failed to disable gem");
    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    let gem = &groups.last().unwrap().gems[0];
    assert_eq!(gem.level, 15);
    assert_eq!(gem.quality, 20);
    assert!(!gem.enabled);

    // Label and group enabled state
    skills::set_group_label(lua, group_index, "Test Group").expect("failed to set label");
    skills::set_group_enabled(lua, group_index, false).expect("failed to disable group");
    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    let group = groups.last().unwrap();
    assert_eq!(group.label, "Test Group");
    assert!(!group.enabled);

    // Remove the gem
    skills::remove_gem(lua, group_index, 1).expect("failed to remove gem");
    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    assert!(groups.last().unwrap().gems.is_empty());

    // Delete the group; main socket group index must stay valid
    skills::delete_socket_group(lua, group_index).expect("failed to delete group");
    let groups = skills::extract_skills(lua).expect("failed to re-extract");
    assert_eq!(groups.len(), initial_count, "group should be deleted");
    let main_count = groups.iter().filter(|g| g.is_main).count();
    assert_eq!(main_count, 1, "main socket group should still be valid");
}

// ---------------------------------------------------------------------------
// Skills extraction
// ---------------------------------------------------------------------------

#[test]
fn test_skills_extraction() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let groups =
        pob_egui::data::skills::extract_skills(bridge.lua()).expect("failed to extract skills");

    assert!(!groups.is_empty(), "should have at least one socket group");

    // Exactly one group should be the main skill
    let main_count = groups.iter().filter(|g| g.is_main).count();
    assert_eq!(
        main_count, 1,
        "exactly one socket group should be the main skill"
    );

    // Check that groups have valid gem data
    let total_gems: usize = groups.iter().map(|g| g.gems.len()).sum();
    println!("Socket groups: {}, total gems: {total_gems}", groups.len());
    assert!(
        total_gems > 0,
        "should have at least one gem across all groups"
    );

    for group in &groups {
        let active: Vec<_> = group
            .gems
            .iter()
            .filter(|g| !g.is_support && g.enabled)
            .collect();
        let supports: Vec<_> = group
            .gems
            .iter()
            .filter(|g| g.is_support && g.enabled)
            .collect();
        let main_marker = if group.is_main { " [MAIN]" } else { "" };
        println!(
            "  Group {}{main_marker}: {} active, {} support gems",
            group.index,
            active.len(),
            supports.len()
        );

        for gem in &group.gems {
            assert!(!gem.name.is_empty(), "gem name should not be empty");
            assert!(gem.level >= 1, "gem level should be at least 1");
        }
    }
}

// ---------------------------------------------------------------------------
// Tree group backgrounds match tree data
// ---------------------------------------------------------------------------

#[test]
fn test_group_backgrounds_only_where_tree_data_defines_them() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let tree = TreeData::extract(bridge.lua()).expect("failed to extract tree data");

    // Ask Lua directly how many groups have a background field
    let (lua_with_bg, lua_total): (u32, u32) = bridge
        .lua()
        .load(
            r#"
            local tree = mainObject_ref.main.modes['BUILD'].spec.tree
            local with_bg = 0
            local total = 0
            for _, group in pairs(tree.groups) do
                if not group.isProxy then
                    total = total + 1
                    if group.background then
                        with_bg = with_bg + 1
                    end
                end
            end
            return with_bg, total
        "#,
        )
        .eval()
        .expect("failed to count Lua groups");

    let rust_with_bg = tree
        .groups
        .iter()
        .filter(|g| g.background.is_some())
        .count() as u32;
    let rust_total = tree.groups.len() as u32;

    println!("Lua:  {lua_with_bg}/{lua_total} groups have backgrounds");
    println!("Rust: {rust_with_bg}/{rust_total} groups have backgrounds");

    assert_eq!(
        rust_total, lua_total,
        "Rust should extract the same number of groups as Lua"
    );
    assert_eq!(
        rust_with_bg, lua_with_bg,
        "Rust should assign backgrounds to the same groups as Lua tree data"
    );

    // Sanity: not all groups should have backgrounds (the original bug)
    assert!(
        rust_with_bg < rust_total,
        "not every group should have a background — got {rust_with_bg}/{rust_total}"
    );
    // Sanity: at least some groups should have backgrounds
    assert!(
        rust_with_bg > 0,
        "at least some groups should have backgrounds"
    );
}

// ---------------------------------------------------------------------------
// Ascendancy start groups are extracted
// ---------------------------------------------------------------------------

#[test]
fn test_ascendancy_start_groups_extracted() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let tree = TreeData::extract(bridge.lua()).expect("failed to extract tree data");

    let ascendancy_groups: Vec<_> = tree.groups.iter().filter(|g| g.is_ascendancy).collect();
    let start_groups: Vec<_> = tree
        .groups
        .iter()
        .filter(|g| g.is_ascendancy_start)
        .collect();

    println!("Class ID: {}", tree.class_id);
    println!("Ascendancy groups: {}", ascendancy_groups.len());
    println!("Ascendancy start groups: {}", start_groups.len());
    for g in &start_groups {
        println!(
            "  Start group: {:?} at ({}, {})",
            g.ascendancy_name, g.x, g.y
        );
    }

    assert!(
        !ascendancy_groups.is_empty(),
        "should have ascendancy groups"
    );
    assert!(
        !start_groups.is_empty(),
        "should have ascendancy start groups"
    );
    assert!(
        start_groups.iter().all(|g| g.ascendancy_name.is_some()),
        "all start groups should have ascendancy names"
    );
}

// ---------------------------------------------------------------------------
// Sprite atlas loads ascendancy and class backgrounds
// ---------------------------------------------------------------------------

#[test]
fn test_sprite_atlas_loads_backgrounds() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let repo_root = common::find_repo_root();
    let version: String = bridge
        .lua()
        .load("return mainObject_ref.main.modes['BUILD'].spec.treeVersion")
        .eval()
        .expect("failed to get tree version");

    let tree_data_dir = repo_root
        .join("upstream")
        .join("src")
        .join("TreeData")
        .join(&version);
    assert!(
        tree_data_dir.is_dir(),
        "tree data dir should exist: {}",
        tree_data_dir.display()
    );

    let atlas =
        TreeSpriteAtlas::load(bridge.lua(), &tree_data_dir).expect("failed to load sprite atlas");

    println!(
        "Ascendancy backgrounds: {:?}",
        atlas.ascendancy_backgrounds.keys().collect::<Vec<_>>()
    );
    println!(
        "Class backgrounds: {:?}",
        atlas.class_backgrounds.keys().collect::<Vec<_>>()
    );

    assert!(
        !atlas.ascendancy_backgrounds.is_empty(),
        "should have ascendancy backgrounds"
    );
    assert!(
        atlas.ascendancy_backgrounds.contains_key("Berserker"),
        "should have Berserker background"
    );
    assert!(
        !atlas.class_backgrounds.is_empty(),
        "should have class backgrounds"
    );
    assert!(
        atlas.class_backgrounds.contains_key("Str"),
        "should have Str (Marauder) class background"
    );
}

// ---------------------------------------------------------------------------
// Item management: parse raw, add, equip, tooltip, delete
// ---------------------------------------------------------------------------

#[test]
fn test_item_management_roundtrip() {
    use pob_egui::data::items;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Item list extraction
    let initial_list = items::extract_item_list(lua).expect("failed to extract item list");
    assert!(!initial_list.is_empty(), "test build should have items");
    let initial_count = initial_list.len();
    for entry in &initial_list {
        assert!(entry.id > 0);
        assert!(!entry.name.is_empty());
    }

    // Equipped slots should expose valid-item choices including the equipped item
    let equipped = items::extract_equipped_items(lua).expect("failed to extract equipped");
    let slot = equipped
        .iter()
        .find(|s| s.sel_item_id > 0 && !s.valid_items.is_empty())
        .expect("an equipped slot with choices");
    assert!(
        slot.valid_items.iter().any(|c| c.id == slot.sel_item_id),
        "equipped item should be a valid choice for its own slot"
    );

    // Full upstream tooltip for an equipped item
    let tooltip = items::item_tooltip_lines(lua, slot.sel_item_id, Some(&slot.slot_name))
        .expect("tooltip generation failed");
    assert!(
        tooltip.iter().filter(|l| !l.text.is_empty()).count() >= 2,
        "tooltip should have several text lines, got {tooltip:?}"
    );

    // Nonsense text is rejected
    let err = items::add_item_from_raw(lua, "not an item at all").expect("call failed");
    assert!(err.is_some(), "nonsense item text should be rejected");
    let list = items::extract_item_list(lua).expect("re-extract failed");
    assert_eq!(list.len(), initial_count, "rejected item must not be added");

    // Add a rare ring via upstream's Item:ParseRaw
    let raw =
        "Rarity: RARE\nParity Test Ring\nRuby Ring\n+50 to maximum Life\n+30% to Fire Resistance";
    let err = items::add_item_from_raw(lua, raw).expect("call failed");
    assert!(err.is_none(), "valid item should parse: {err:?}");
    let list = items::extract_item_list(lua).expect("re-extract failed");
    assert_eq!(list.len(), initial_count + 1, "item should be added");
    let new_item = list
        .iter()
        .find(|e| e.name.contains("Parity Test Ring"))
        .expect("new item should be in the list");
    assert_eq!(new_item.rarity, "RARE");

    // Make the comparison deterministic: unequip the new ring from wherever
    // auto-equip put it, and empty Ring 1
    let equipped = items::extract_equipped_items(lua).expect("re-extract equipped failed");
    for slot in &equipped {
        if slot.sel_item_id == new_item.id {
            items::equip_item(lua, &slot.slot_name, 0).expect("clear auto-equip failed");
        }
    }
    items::equip_item(lua, "Ring 1", 0).expect("clear Ring 1 failed");

    // Equip it in Ring 1 and verify the stat change shows up
    let life_before = pob_egui::data::CalcOutput::extract(lua)
        .expect("calc extract failed")
        .stats
        .get("Life")
        .copied()
        .unwrap_or(0.0);
    items::equip_item(lua, "Ring 1", new_item.id).expect("equip failed");
    let equipped = items::extract_equipped_items(lua).expect("re-extract equipped failed");
    let ring1 = equipped
        .iter()
        .find(|s| s.slot_name == "Ring 1")
        .expect("Ring 1 slot");
    assert_eq!(ring1.sel_item_id, new_item.id, "ring should be equipped");
    let life_after = pob_egui::data::CalcOutput::extract(lua)
        .expect("calc extract failed")
        .stats
        .get("Life")
        .copied()
        .unwrap_or(0.0);
    assert!(
        life_after > life_before,
        "+50 life ring should raise Life ({life_before} -> {life_after})"
    );

    // Edit the item's raw text: raise the life roll. The id must be kept, so
    // the item stays equipped in Ring 1.
    let raw = items::get_item_raw(lua, new_item.id).expect("get raw failed");
    assert!(
        raw.contains("+50 to maximum Life"),
        "raw text should contain the life mod: {raw}"
    );
    assert!(
        items::validate_item_raw(lua, &raw).expect("validate call failed"),
        "BuildRaw output should validate"
    );
    assert!(
        !items::validate_item_raw(lua, "garbage").expect("validate call failed"),
        "garbage should not validate"
    );
    let edited = raw.replace("+50 to maximum Life", "+80 to maximum Life");
    let err = items::replace_item_from_raw(lua, new_item.id, &edited).expect("replace call failed");
    assert!(err.is_none(), "edited item should parse: {err:?}");
    let equipped = items::extract_equipped_items(lua).expect("re-extract equipped failed");
    let ring1 = equipped
        .iter()
        .find(|s| s.slot_name == "Ring 1")
        .expect("Ring 1 slot");
    assert_eq!(
        ring1.sel_item_id, new_item.id,
        "edited ring should stay equipped"
    );
    let life_edited = pob_egui::data::CalcOutput::extract(lua)
        .expect("calc extract failed")
        .stats
        .get("Life")
        .copied()
        .unwrap_or(0.0);
    assert!(
        life_edited > life_after,
        "raising the life roll should raise Life ({life_after} -> {life_edited})"
    );

    // Sorting keeps the same set of items
    items::sort_item_list(lua).expect("sort failed");
    let sorted = items::extract_item_list(lua).expect("re-extract failed");
    assert_eq!(sorted.len(), initial_count + 1, "sort must not drop items");
    let mut ids_before: Vec<i64> = list.iter().map(|e| e.id).collect();
    let mut ids_after: Vec<i64> = sorted.iter().map(|e| e.id).collect();
    ids_before.sort_unstable();
    ids_after.sort_unstable();
    assert_eq!(ids_before, ids_after, "sort must keep the same item ids");

    // Unequip
    items::equip_item(lua, "Ring 1", 0).expect("unequip failed");
    let equipped = items::extract_equipped_items(lua).expect("re-extract equipped failed");
    let ring1 = equipped
        .iter()
        .find(|s| s.slot_name == "Ring 1")
        .expect("Ring 1 slot");
    assert_eq!(ring1.sel_item_id, 0, "ring should be unequipped");

    // Delete removes it from the list
    items::delete_item(lua, new_item.id).expect("delete failed");
    let list = items::extract_item_list(lua).expect("re-extract failed");
    assert_eq!(list.len(), initial_count, "item should be deleted");
    assert!(
        !list.iter().any(|e| e.id == new_item.id),
        "deleted item id should be gone"
    );
}

// ---------------------------------------------------------------------------
// Save / Save As write to the build path
// ---------------------------------------------------------------------------

#[test]
fn test_save_as_writes_to_build_path() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Redirect buildPath to a scratch dir so the test doesn't touch real builds
    let tmp_dir = std::env::temp_dir().join(format!("egui-pob-save-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let mut build_path = tmp_dir.to_string_lossy().to_string();
    build_path.push('/');
    lua.load("mainObject_ref.main.buildPath = ...")
        .call::<()>(build_path.as_str())
        .expect("failed to redirect buildPath");

    // A build loaded without a file has no dbFileName - plain Save must fail
    let err = bridge.save_build();
    assert!(err.is_err(), "Save without a filename should fail");
    assert!(
        bridge.build_file_name().is_none(),
        "loaded-from-text build should have no file"
    );

    // A build loaded from XML text (import path) counts as unsaved, matching
    // upstream's semantics for imported builds
    assert!(
        bridge.is_build_dirty(),
        "text-loaded build counts as unsaved"
    );

    // Save As sanitises the name and writes <buildPath><name>.xml
    bridge
        .save_build_as("My: Save/Test?")
        .expect("Save As failed");
    assert!(!bridge.is_build_dirty(), "Save As should clear dirty state");

    // A change re-dirties; Save clears it again
    pob_egui::data::config::set_config_value(lua, "conditionEnemyShocked", LuaValue::Boolean(true))
        .expect("config change failed");
    assert!(bridge.is_build_dirty(), "config change should mark dirty");
    bridge.save_build().expect("Save failed");
    assert!(!bridge.is_build_dirty(), "Save should clear dirty state");
    let expected = tmp_dir.join("My- Save-Test-.xml");
    assert!(
        expected.is_file(),
        "expected {} to exist",
        expected.display()
    );
    let contents = std::fs::read_to_string(&expected).expect("failed to read saved file");
    assert!(
        contents.starts_with("<?xml"),
        "saved file should be XML, got: {}",
        &contents[..contents.len().min(40)]
    );

    // dbFileName is now set, so plain Save writes to the same file
    std::fs::remove_file(&expected).expect("failed to remove file");
    bridge.save_build().expect("Save failed");
    assert!(expected.is_file(), "Save should rewrite the same file");

    // The saved file must round-trip: load it back and check the build name
    let xml_text = std::fs::read_to_string(&expected).expect("failed to re-read");
    bridge
        .load_build_from_xml(&xml_text, "My- Save-Test-", expected.to_str())
        .expect("failed to reload saved build");
    let db_file_name: String = lua
        .load("return mainObject_ref.main.modes['BUILD'].dbFileName")
        .eval()
        .expect("failed to read dbFileName");
    assert_eq!(
        db_file_name,
        expected.to_string_lossy(),
        "reloaded build should keep its file path"
    );
    assert!(
        !bridge.is_build_dirty(),
        "build opened from its own file starts clean"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ---------------------------------------------------------------------------
// Character import plumbing (canned API responses, no network)
// ---------------------------------------------------------------------------

#[test]
fn test_char_import_plumbing() {
    use pob_egui::data::char_import::{self, CharacterInfo};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Character list parsing (valid response)
    let chars = char_import::parse_character_list(
        lua,
        r#"[{"name":"TestChar","league":"Standard","class":"Witch","level":93},
            {"name":"AltChar","league":"Settlers","class":"Duelist","level":12}]"#,
    )
    .expect("valid list should parse");
    assert_eq!(chars.len(), 2);
    assert_eq!(chars[0].name, "TestChar");
    assert_eq!(chars[0].league, "Standard");
    assert_eq!(chars[0].class, "Witch");
    assert_eq!(chars[0].level, 93);

    // API error object becomes an error
    let err = char_import::parse_character_list(
        lua,
        r#"{"error":{"code":1,"message":"Resource not found"}}"#,
    )
    .expect_err("error response should fail");
    assert!(
        err.to_string().contains("Resource not found"),
        "error message should surface: {err}"
    );

    // Import a synthetic empty passive tree: all points deallocated, level set
    let character = CharacterInfo {
        name: "TestChar".to_string(),
        league: "Standard".to_string(),
        class: "Scion".to_string(),
        level: 42,
    };
    let tree_json = r#"{"character":0,"ascendancy":0,"alternate_ascendancy":0,
        "hashes":[],"hashes_ex":[],"jewel_data":{},"items":[],"mastery_effects":{}}"#;
    let status = char_import::import_passive_tree_and_jewels(lua, tree_json, &character, true)
        .expect("tree import call failed");
    assert!(
        status.contains("successfully"),
        "tree import should succeed: {status}"
    );
    let (level, points): (i64, i64) = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local used = 0
            for _ in pairs(build.spec.allocNodes) do used = used + 1 end
            return build.characterLevel, used
        "#,
        )
        .eval()
        .expect("failed to read build state");
    assert_eq!(level, 42, "character level should be applied");
    assert!(
        points <= 1,
        "empty import should leave only the class start allocated, got {points}"
    );

    // Import synthetic empty items (keep existing skills/items)
    let items_json = r#"{"items":[],"character":{"name":"TestChar","level":55}}"#;
    let status = char_import::import_items_and_skills(lua, items_json, false, false, false)
        .expect("items import call failed");
    assert!(
        status.contains("successfully"),
        "items import should succeed: {status}"
    );
    let level: i64 = lua
        .load("return mainObject_ref.main.modes['BUILD'].characterLevel")
        .eval()
        .expect("failed to read level");
    assert_eq!(level, 55, "items import should update the level");
}

// ---------------------------------------------------------------------------
// Gem search (upstream GemSelectControl)
// ---------------------------------------------------------------------------

#[test]
fn test_gem_search() {
    use pob_egui::data::gems;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Exact name match ranks first
    let results = gems::search_gems(lua, 1, "Fireball", false, 12).expect("search failed");
    assert!(!results.is_empty(), "Fireball should match");
    assert_eq!(results[0].name, "Fireball");
    assert!(!results[0].is_support);

    // Abbreviation matching ("CtF" -> "Cold to Fire")
    let results = gems::search_gems(lua, 1, "CtF", false, 12).expect("search failed");
    assert!(
        results.iter().any(|g| g.name == "Cold to Fire"),
        "CtF should match Cold to Fire: {:?}",
        results.iter().map(|g| &g.name).collect::<Vec<_>>()
    );

    // Tag search: ":aura" returns only gems with the aura tag
    let results = gems::search_gems(lua, 1, ":aura", false, 50).expect("search failed");
    assert!(!results.is_empty(), "aura tag should match gems");
    assert!(
        results.iter().any(|g| g.name == "Anger"),
        "Anger is an aura"
    );

    // Tag exclusion: ":aura:-fire" excludes Anger
    let results = gems::search_gems(lua, 1, ":aura:-fire", false, 50).expect("search failed");
    assert!(!results.is_empty());
    assert!(
        !results.iter().any(|g| g.name == "Anger"),
        "Anger has the fire tag and must be excluded"
    );

    // Support-compatibility marks: search supports for the main socket group
    // (test build's group 1 has an active skill)
    let results = gems::search_gems(lua, 1, "Support", false, 50).expect("search failed");
    let supports: Vec<_> = results.iter().filter(|g| g.is_support).collect();
    assert!(!supports.is_empty(), "should find support gems");
    assert!(
        supports.iter().any(|g| g.can_support),
        "some support should be compatible with the main skill"
    );

    // DPS sorting produces DPS values and colors
    let results = gems::search_gems(lua, 1, "Support", true, 20).expect("dps search failed");
    assert!(
        results
            .iter()
            .any(|g| g.dps > 0.0 && !g.dps_color.is_empty()),
        "DPS sort should compute values"
    );
}

// ---------------------------------------------------------------------------
// Tree specs: create, copy, rename, switch, delete, URL round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_tree_specs_roundtrip() {
    use pob_egui::data::tree_specs;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 1, "test build has one spec");
    assert_eq!(active, 1);
    let original_alloc: i64 = lua
        .load("local n = 0; for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do n = n + 1 end; return n")
        .eval()
        .expect("count failed");
    assert!(original_alloc > 10, "test build has an allocated tree");

    // Copy the current spec: same allocation count, becomes active
    tree_specs::copy_spec(lua, 1, "My Copy").expect("copy failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 2);
    assert_eq!(active, 2, "copy becomes active");
    assert_eq!(specs[1].title, "My Copy");
    let copy_alloc: i64 = lua
        .load("local n = 0; for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do n = n + 1 end; return n")
        .eval()
        .expect("count failed");
    assert_eq!(copy_alloc, original_alloc, "copy preserves allocations");

    // New empty spec keeps the class but no allocations
    tree_specs::new_spec(lua, "Empty Tree").expect("new failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 3);
    assert_eq!(active, 3);
    let empty_alloc: i64 = lua
        .load("local n = 0; for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do n = n + 1 end; return n")
        .eval()
        .expect("count failed");
    // Class start + ascendancy start are auto-allocated
    assert!(
        empty_alloc <= 2,
        "new spec should have only start nodes allocated, got {empty_alloc}"
    );

    // Rename
    tree_specs::rename_spec(lua, 3, "Renamed").expect("rename failed");
    let (specs, _) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs[2].title, "Renamed");

    // Switch back to the original: allocations restored
    tree_specs::set_active_spec(lua, 1).expect("switch failed");
    let (_, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(active, 1);
    let back_alloc: i64 = lua
        .load("local n = 0; for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do n = n + 1 end; return n")
        .eval()
        .expect("count failed");
    assert_eq!(back_alloc, original_alloc);

    // Export the active spec, import it back as a new spec. Compare only what
    // the official URL format carries: it skips class/ascendancy start nodes,
    // and its 2-bit secondary-ascendancy field can't encode bloodline ids > 3
    // (upstream limitation).
    let count_real_nodes = r#"
        local n = 0
        for id, node in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do
            if id < 65536 and node.type ~= "ClassStart" and node.type ~= "AscendClassStart" then
                n = n + 1
            end
        end
        return n
    "#;
    let original_real: i64 = lua.load(count_real_nodes).eval().expect("count failed");
    let url = tree_specs::export_tree_url(lua).expect("export failed");
    assert!(
        url.starts_with("https://www.pathofexile.com/passive-skill-tree/"),
        "export should produce an official URL: {url}"
    );
    let err = tree_specs::import_tree_url(lua, &url, "Reimported").expect("import call failed");
    assert!(err.is_none(), "imported URL should decode: {err:?}");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 4);
    assert_eq!(active, 4);
    assert_eq!(specs[3].title, "Reimported");
    let reimport_real: i64 = lua.load(count_real_nodes).eval().expect("count failed");
    assert_eq!(
        reimport_real, original_real,
        "URL round-trip preserves real tree node allocations"
    );

    // Delete the copies; the last spec cannot be deleted
    tree_specs::delete_spec(lua, 4).expect("delete failed");
    tree_specs::delete_spec(lua, 3).expect("delete failed");
    tree_specs::delete_spec(lua, 2).expect("delete failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 1);
    assert_eq!(active, 1);
    tree_specs::delete_spec(lua, 1).expect("delete call failed");
    let (specs, _) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 1, "the last spec must not be deletable");
}

// ---------------------------------------------------------------------------
// Tree comparison: spec allocation extraction and diff
// ---------------------------------------------------------------------------

#[test]
fn test_tree_compare_diff() {
    use pob_egui::data::tree_specs;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Copy the spec; the copy becomes active (index 2), original is index 1
    tree_specs::copy_spec(lua, 1, "Compare Target").expect("copy failed");

    // Identical specs diff to nothing
    let original = tree_specs::spec_allocation(lua, 1).expect("alloc read failed");
    let current = tree_specs::spec_allocation(lua, 2).expect("alloc read failed");
    assert!(!original.allocated.is_empty());
    assert_eq!(original.allocated, current.allocated);
    let diff = tree_specs::compare_diff(&current, &original);
    assert!(diff.to_allocate.is_empty(), "identical specs: no diff");
    assert!(diff.to_deallocate.is_empty(), "identical specs: no diff");
    assert!(diff.mastery_diff.is_empty(), "identical specs: no diff");

    // Allocate one more node on the active copy
    let node_id: u32 = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.spec
            spec:BuildAllDependsAndPaths()
            for id, node in pairs(spec.nodes) do
                if not spec.allocNodes[id]
                   and node.type == "Normal"
                   and not node.ascendancyName
                   and node.path and #node.path > 0 then
                    spec:AllocNode(node)
                    spec:AddUndoState()
                    build.buildFlag = true
                    _runCallback('OnFrame')
                    return id
                end
            end
            return 0
        "#,
        )
        .eval()
        .expect("failed to allocate node");
    assert!(node_id > 0);

    // Now the diff shows exactly the newly allocated nodes as current-only
    // (AllocNode allocates the node plus any path nodes leading to it)
    let current = tree_specs::spec_allocation(lua, 2).expect("alloc read failed");
    let newly: std::collections::HashSet<u32> = current
        .allocated
        .difference(&original.allocated)
        .copied()
        .collect();
    assert!(
        newly.contains(&node_id) && !newly.is_empty(),
        "allocation should add the chosen node"
    );
    let diff = tree_specs::compare_diff(&current, &original);
    assert_eq!(
        diff.to_deallocate, newly,
        "newly allocated nodes are current-only"
    );
    assert!(diff.to_allocate.is_empty());

    // And in the opposite direction they show as compare-only (green)
    let reverse = tree_specs::compare_diff(&original, &current);
    assert_eq!(reverse.to_allocate, newly);
    assert!(reverse.to_deallocate.is_empty());

    // The test build uses masteries: selections should be extracted
    assert!(
        !current.mastery_selections.is_empty(),
        "test build should have mastery selections"
    );
}

// ---------------------------------------------------------------------------
// Tree version conversion
// ---------------------------------------------------------------------------

#[test]
fn test_tree_version_conversion() {
    use pob_egui::data::tree_specs;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Version list: non-empty, exactly one latest, and it is the last entry
    let versions = tree_specs::list_tree_versions(lua).expect("version list failed");
    assert!(versions.len() > 5, "should know many tree versions");
    let latest: Vec<_> = versions.iter().filter(|v| v.is_latest).collect();
    assert_eq!(latest.len(), 1, "exactly one latest version");
    let latest = latest[0].clone();
    assert_eq!(versions.last().unwrap().id, latest.id);
    assert!(!latest.display.is_empty());

    // The loaded build's spec is on the latest version
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(active, 1);
    assert!(specs[0].is_latest_version);

    // Copy + Convert to the previous version: a converted copy is inserted
    // and becomes active, the original stays on the latest version.
    // (Skip subtype variants of the latest version, e.g. 3_28_ruthless.)
    let previous = versions
        .iter()
        .rev()
        .find(|v| !v.id.starts_with(&latest.id))
        .expect("an older version should exist")
        .clone();
    tree_specs::convert_to_version(lua, &previous.id, false, true).expect("convert failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 2, "copy+convert adds a spec");
    assert_eq!(active, 2, "converted copy becomes active");
    assert_eq!(specs[1].tree_version, previous.id);
    assert!(!specs[1].is_latest_version, "converted spec is outdated");
    assert!(specs[0].is_latest_version, "original is untouched");

    // Some allocations should survive the downgrade
    let converted = tree_specs::spec_allocation(lua, 2).expect("alloc read failed");
    assert!(
        converted.allocated.len() > 10,
        "most allocations should survive a one-version downgrade, got {}",
        converted.allocated.len()
    );

    // Convert all trees to the latest version: everything ends up latest,
    // spec count unchanged, active index preserved
    tree_specs::convert_all_to_version(lua, &latest.id).expect("convert all failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs.len(), 2, "convert-all replaces in place");
    assert_eq!(active, 2, "active index preserved");
    assert!(
        specs.iter().all(|s| s.is_latest_version),
        "all specs should be on the latest version: {specs:?}"
    );
}

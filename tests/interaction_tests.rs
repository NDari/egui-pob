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
        if let Some(after_val) = after.stats.get(stat) {
            if (after_val - before_val).abs() > 0.001 {
                println!("  {stat}: {before_val} -> {after_val}");
                any_changed = true;
            }
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
        if let Some(after_val) = after.stats.get(stat) {
            if (after_val - before_val).abs() > 0.001 {
                println!("  {stat}: {before_val} -> {after_val}");
                any_changed = true;
            }
        }
    }

    assert!(
        any_changed,
        "at least one stat should change after allocating a normal node"
    );
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

    let rust_with_bg = tree.groups.iter().filter(|g| g.background.is_some()).count() as u32;
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

    let ascendancy_groups: Vec<_> = tree
        .groups
        .iter()
        .filter(|g| g.is_ascendancy)
        .collect();
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
    assert!(tree_data_dir.is_dir(), "tree data dir should exist: {}", tree_data_dir.display());

    let atlas = TreeSpriteAtlas::load(bridge.lua(), &tree_data_dir)
        .expect("failed to load sprite atlas");

    println!("Ascendancy backgrounds: {:?}", atlas.ascendancy_backgrounds.keys().collect::<Vec<_>>());
    println!("Class backgrounds: {:?}", atlas.class_backgrounds.keys().collect::<Vec<_>>());

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

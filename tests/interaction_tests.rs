//! Integration tests for interactive features: config changes, node allocation,
//! items extraction, and skills extraction.

mod common;

use mlua::prelude::*;

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

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
    let info = pob_egui::data::tree::fetch_hover_info(bridge.lua(), node_id, true)
        .expect("failed to fetch hover info");
    assert!(
        info.path.contains(&node_id),
        "path should include the node itself"
    );
    assert!(
        info.depends.is_empty(),
        "unallocated node should have no dependents"
    );
    assert!(
        info.diff
            .iter()
            .any(|l| l.contains("Allocating this node") || l.contains("No changes from allocating")),
        "diff should describe allocating the node, got: {:?}",
        info.diff
    );

    // Diffs off: no comparison lines
    let info_no_diff = pob_egui::data::tree::fetch_hover_info(bridge.lua(), node_id, false)
        .expect("failed to fetch hover info without diffs");
    assert!(
        info_no_diff.diff.is_empty(),
        "diffs disabled should be empty"
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

    let info = pob_egui::data::tree::fetch_hover_info(bridge.lua(), node_id, true)
        .expect("failed to fetch hover info after alloc");
    assert!(
        info.depends.contains(&node_id),
        "allocated node should depend on itself"
    );
    assert!(
        info.diff
            .iter()
            .any(|l| l.contains("Unallocating this node")
                || l.contains("No changes from unallocating")),
        "diff should describe unallocating the node, got: {:?}",
        info.diff
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
    let lua = bridge.lua();
    let search = |q: &str| pob_egui::data::tree::search_nodes(lua, q).expect("search failed");

    // Common stat text should match plenty of nodes
    let life_matches = search("life");
    assert!(
        life_matches.len() > 50,
        "'life' should match many nodes, got {}",
        life_matches.len()
    );

    // Multi-term search narrows results (AND semantics)
    let narrowed = search("life mana");
    assert!(
        !narrowed.is_empty() && narrowed.len() < life_matches.len(),
        "'life mana' should narrow the match set: {} vs {}",
        narrowed.len(),
        life_matches.len()
    );

    // oil: prefix matches nodes with anoint recipes (notables)
    let oil_matches = search("oil:");
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
    let keystones = search("keystone");
    assert!(!keystones.is_empty(), "'keystone' should match keystones");

    // Lua patterns: '.' wildcards and character classes work per term
    let pattern_matches = search("fire.*damage");
    assert!(
        !pattern_matches.is_empty(),
        "'fire.*damage' should match as a Lua pattern"
    );
    // Anchored pattern only matches names/lines starting with the term
    let anchored = search("^armour");
    let unanchored = search("armour");
    assert!(
        !anchored.is_empty() && anchored.len() < unanchored.len(),
        "'^armour' should be narrower than 'armour': {} vs {}",
        anchored.len(),
        unanchored.len()
    );

    // Upstream's or-group extension: (a|b) matches either alternative
    let fire = search("fire resistance");
    let cold = search("cold resistance");
    let either = search("(fire|cold) resistance");
    assert!(
        either.len() >= fire.len().max(cold.len()),
        "or-group should match at least each alternative: {} vs {}/{}",
        either.len(),
        fire.len(),
        cold.len()
    );
    assert!(
        fire.iter().all(|id| either.contains(id)),
        "or-group is a superset of one alternative"
    );

    // Empty, garbage, and invalid-pattern queries match nothing (upstream's
    // pcall guard)
    assert!(search("").is_empty());
    assert!(search("xyzzy_no_such_stat").is_empty());
    assert!(
        search("[[").is_empty(),
        "invalid Lua pattern matches nothing"
    );
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

    // Every gem carries a socket colour letter (group label indicators),
    // and real builds have more than just white
    let letters: std::collections::HashSet<&str> = groups
        .iter()
        .flat_map(|g| g.gems.iter().map(|gem| gem.color_letter.as_str()))
        .collect();
    assert!(
        letters.iter().all(|l| matches!(*l, "R" | "G" | "B" | "W")),
        "colour letters are R/G/B/W, got {letters:?}"
    );
    assert!(
        letters.len() > 1,
        "a real build should span multiple gem colours, got {letters:?}"
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

    // Every group background must stay inside its own rect in the sheet:
    // linear filtering across the boundary drew the neighbouring sprite's edge
    // as a faint line (the golden row above PSGroupBackground2).
    for (label, region) in [
        ("small", atlas.frames.group_background_small),
        ("medium", atlas.frames.group_background_medium),
        ("large", atlas.frames.group_background_large),
    ] {
        let region = region.unwrap_or_else(|| panic!("{label} group background should load"));
        let sheet = &atlas.sheets[region.sheet_index];
        let (sw, sh) = (sheet.image.width() as f32, sheet.image.height() as f32);
        // The covered pixel span is the sprite minus the half-texel inset
        let px_w = (region.u_max - region.u_min) * sw;
        let px_h = (region.v_max - region.v_min) * sh;
        assert!(
            (px_w - (region.width - 1.0)).abs() < 0.01
                && (px_h - (region.height - 1.0)).abs() < 0.01,
            "{label} should cover its sprite inset by half a texel per side, \
             got {px_w}x{px_h} for a {}x{} sprite",
            region.width,
            region.height
        );
    }
}

/// The 3.29 bloodline emblems (plus Reliquarian / Luminary) ship only as
/// regions of bloodline-3.webp / ascendancy-3.webp, not as loose Classes*.png
/// files, so they have to come out of the spritesheet with a UV sub-rect.
#[test]
fn test_sprite_atlas_loads_bloodline_backgrounds() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_build(common::ALLFLAME_FIXTURE);

    let version: String = bridge
        .lua()
        .load("return mainObject_ref.main.modes['BUILD'].spec.treeVersion")
        .eval()
        .expect("failed to get tree version");
    assert_eq!(version, "3_29", "fixture should be on the bloodline tree");

    let tree_data_dir = common::find_repo_root()
        .join("upstream")
        .join("src")
        .join("TreeData")
        .join(&version);
    let atlas =
        TreeSpriteAtlas::load(bridge.lua(), &tree_data_dir).expect("failed to load sprite atlas");

    let sheet_backed = [
        "Trialmaster",
        "Oshabi",
        "Olroth",
        "Lycia",
        "KingInTheMists",
        "Farrul",
        "Delirious",
        "Catarina",
        "Breachlord",
        "Aul",
        "Necromantic",
        "Brinerot",
        "Abyssal",
        // Not bloodlines, but likewise sheet-only
        "Reliquarian",
        "Luminary",
    ];
    for name in sheet_backed {
        let bg = atlas
            .ascendancy_backgrounds
            .get(name)
            .unwrap_or_else(|| panic!("missing background art for {name}"));
        let uv = bg.uv;
        assert!(
            uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0,
            "{name} uv out of range: {uv:?}"
        );
        assert!(
            uv.width() > 0.0 && uv.height() > 0.0,
            "{name} uv is empty: {uv:?}"
        );
        assert!(
            uv.width() < 1.0 || uv.height() < 1.0,
            "{name} should be a sheet region, not a whole image"
        );
        assert!(
            bg.width > 1.0 && bg.height > 1.0,
            "{name} should carry its sprite size, got {}x{}",
            bg.width,
            bg.height
        );
    }

    // Ascendancies with a loose PNG keep it, as upstream does for the three
    // legacy alternate ascendancies (PassiveTree.lua legacyClasses).
    for name in ["Berserker", "Ascendant", "Warden", "Warlock", "Primalist"] {
        let bg = atlas
            .ascendancy_backgrounds
            .get(name)
            .unwrap_or_else(|| panic!("missing background art for {name}"));
        assert_eq!(
            bg.uv,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            "{name} should still come from its own PNG"
        );
    }
}

/// The Items tab's Ctrl+D drives upstream's own ItemsTab.showStatDifferences,
/// so item tooltips gain and lose their comparison sections with it, and the
/// tip never points the reader at another tab.
#[test]
fn test_item_tooltip_stat_difference_toggle() {
    use pob_egui::data::items;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    assert!(
        items::stat_differences_enabled(lua).expect("read flag failed"),
        "stat differences default to on, as upstream does"
    );

    let equipped = items::extract_equipped_items(lua).expect("failed to extract equipped");
    let slot = equipped
        .iter()
        .find(|s| s.sel_item_id > 0)
        .expect("an equipped slot");

    let tooltip = |lua: &mlua::Lua| {
        items::item_tooltip_lines(lua, slot.sel_item_id, Some(&slot.slot_name))
            .expect("tooltip generation failed")
    };
    let has =
        |lines: &[items::TooltipLine], needle: &str| lines.iter().any(|l| l.text.contains(needle));

    let on = tooltip(lua);
    assert!(has(&on, "Ctrl+D"), "tooltip should carry the Ctrl+D tip");
    assert!(
        !has(&on, "Items tab"),
        "the tip should not point at the tab the reader is already in"
    );
    assert!(
        has(&on, "Removing this item"),
        "an equipped item should preview its removal"
    );

    // Ctrl+D off
    assert!(
        !items::toggle_stat_differences(lua).expect("toggle failed"),
        "toggling from on should return off"
    );
    assert!(!items::stat_differences_enabled(lua).expect("read flag failed"));
    let off = tooltip(lua);
    assert!(
        !has(&off, "Removing this item"),
        "no comparison sections while stat differences are off"
    );
    assert!(
        has(&off, "Ctrl+D") && !has(&off, "Items tab"),
        "the enable-variant tip should show, without a tab qualifier"
    );
    assert!(
        off.len() < on.len(),
        "the off tooltip should be the shorter one ({} vs {})",
        off.len(),
        on.len()
    );

    // ...and back on
    assert!(
        items::toggle_stat_differences(lua).expect("toggle failed"),
        "toggling from off should return on"
    );
    assert!(has(&tooltip(lua), "Removing this item"));
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
        .save_build_as("My: Save/Test?", "")
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

    // Save As into a subfolder (folder browser): the file lands under
    // <buildPath><subPath> and dbFileSubPath tracks the folder
    pob_egui::data::build_list::create_folder(&build_path, "", "SubFolder")
        .expect("create folder failed");
    bridge
        .save_build_as("Nested Build", "SubFolder/")
        .expect("Save As into subfolder failed");
    let nested = tmp_dir.join("SubFolder").join("Nested Build.xml");
    assert!(nested.is_file(), "expected {} to exist", nested.display());
    assert_eq!(
        bridge.build_file_sub_path(),
        "SubFolder/",
        "dbFileSubPath tracks the save folder"
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

    // Remembering the imported character: set_last_character stores an
    // upstream common.sha1 hash of the name, and matching_character_index
    // finds it back among a candidate list (league-filtered, in dropdown
    // order) regardless of position.
    char_import::set_last_character(lua, "TestChar").expect("failed to remember character");
    let hash = char_import::last_character_hash(lua).expect("hash should now be set");
    assert!(!hash.is_empty());
    let names = vec!["AltChar".to_string(), "TestChar".to_string()];
    assert_eq!(
        char_import::matching_character_index(lua, &names).expect("match lookup failed"),
        Some(1),
        "should find TestChar at its list position"
    );
    let no_match = vec!["AltChar".to_string()];
    assert_eq!(
        char_import::matching_character_index(lua, &no_match).expect("match lookup failed"),
        None,
        "TestChar isn't in this list"
    );
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
    let results = gems::search_gems(lua, 1, "Fireball", false, 12, false)
        .expect("search failed")
        .choices;
    assert!(!results.is_empty(), "Fireball should match");
    assert_eq!(results[0].name, "Fireball");
    assert!(!results[0].is_support);

    // Abbreviation matching ("CtF" -> "Cold to Fire")
    let results = gems::search_gems(lua, 1, "CtF", false, 12, false)
        .expect("search failed")
        .choices;
    assert!(
        results.iter().any(|g| g.name == "Cold to Fire"),
        "CtF should match Cold to Fire: {:?}",
        results.iter().map(|g| &g.name).collect::<Vec<_>>()
    );

    // Tag search: ":aura" returns only gems with the aura tag
    let results = gems::search_gems(lua, 1, ":aura", false, 50, false)
        .expect("search failed")
        .choices;
    assert!(!results.is_empty(), "aura tag should match gems");
    assert!(
        results.iter().any(|g| g.name == "Anger"),
        "Anger is an aura"
    );

    // Tag exclusion: ":aura:-fire" excludes Anger
    let results = gems::search_gems(lua, 1, ":aura:-fire", false, 50, false)
        .expect("search failed")
        .choices;
    assert!(!results.is_empty());
    assert!(
        !results.iter().any(|g| g.name == "Anger"),
        "Anger has the fire tag and must be excluded"
    );

    // Support-compatibility marks: search supports for the main socket group
    // (test build's group 1 has an active skill)
    let results = gems::search_gems(lua, 1, "Support", false, 50, false)
        .expect("search failed")
        .choices;
    let supports: Vec<_> = results.iter().filter(|g| g.is_support).collect();
    assert!(!supports.is_empty(), "should find support gems");
    assert!(
        supports.iter().any(|g| g.can_support),
        "some support should be compatible with the main skill"
    );

    // DPS sorting is progressive (upstream DPSBuilder, one ~50ms slice per
    // call): drive it to completion like the GUI's per-frame polling does
    let mut result =
        gems::search_gems(lua, 1, "Support", true, 20, false).expect("dps search failed");
    let mut steps = 0;
    while result.pending {
        result = gems::search_gems(lua, 1, "Support", true, 20, false).expect("dps step failed");
        steps += 1;
        assert!(steps < 1000, "DPS sort did not terminate");
    }
    assert!(
        result
            .choices
            .iter()
            .any(|g| g.dps > 0.0 && !g.dps_color.is_empty()),
        "DPS sort should compute values"
    );
    // A repeat search hits the cache and completes immediately
    let cached =
        gems::search_gems(lua, 1, "Support", true, 20, false).expect("cached search failed");
    assert!(!cached.pending, "cached DPS sort is not pending");
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

    // Reorder: move the active spec (4, "Reimported") up one slot; the
    // active index follows the moved spec
    tree_specs::move_spec(lua, 4, -1).expect("move failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs[2].title, "Reimported", "spec moved up");
    assert_eq!(active, 3, "active index follows the moved spec");
    tree_specs::move_spec(lua, 3, 1).expect("move failed");
    let (specs, active) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs[3].title, "Reimported", "spec moved back down");
    assert_eq!(active, 4);
    tree_specs::move_spec(lua, 4, 1).expect("move call failed");
    let (specs, _) = tree_specs::list_specs(lua).expect("list failed");
    assert_eq!(specs[3].title, "Reimported", "move past the end is a no-op");

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

    // Bring the (older-version) test build to the latest tree first; this
    // also exercises upgrade conversion
    tree_specs::convert_all_to_version(lua, &latest.id).expect("upgrade failed");
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

// ---------------------------------------------------------------------------
// Jewel socket radius data
// ---------------------------------------------------------------------------

#[test]
fn test_jewel_socket_radii() {
    use pob_egui::data::{items, jewels};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Radius table for 3.16+: Small/Medium/Large plain circles plus
    // Variable annuli for Thread of Hope
    let defs = jewels::radius_defs(lua).expect("radius defs failed");
    for label in ["Small", "Medium", "Large"] {
        let def = defs
            .iter()
            .find(|d| d.label == label)
            .unwrap_or_else(|| panic!("{label} radius should exist"));
        assert_eq!(def.inner, 0.0, "{label} is a plain circle");
        assert!(def.outer > 0.0);
    }
    assert!(
        defs.iter().any(|d| d.label == "Variable" && d.inner > 0.0),
        "Variable annuli should exist"
    );

    // Socket extraction: the tree has many sockets, none is a charm socket
    let sockets = jewels::socket_jewels(lua).expect("socket extraction failed");
    assert!(sockets.len() > 10, "the tree has many jewel sockets");
    let allocated: Vec<_> = sockets.iter().filter(|s| s.allocated).collect();
    assert!(
        !allocated.is_empty(),
        "test build should have allocated sockets"
    );

    // Equip a radius jewel into an allocated socket and see it reflected.
    // "Might in All Forms" style rare with a radius mod: use a magic jewel
    // with "in Radius" to get a jewelRadiusIndex... simplest reliable radius
    // jewel is a unique with fixed radius; craft a rare with a radius mod:
    let raw = "Rarity: RARE\nParity Test Jewel\nCobalt Jewel\nAdds 1 to 2 Cold Damage to Spells";
    let err = items::add_item_from_raw(lua, raw).expect("add jewel failed");
    assert!(err.is_none(), "jewel should parse: {err:?}");
    let list = items::extract_item_list(lua).expect("list failed");
    let jewel = list
        .iter()
        .find(|e| e.name.contains("Parity Test Jewel"))
        .expect("jewel in list");

    // Find an allocated socket slot (they appear as equipment slots); free
    // it if occupied, then socket the test jewel
    let equipped = items::extract_equipped_items(lua).expect("equipped failed");
    let socket_slot = equipped
        .iter()
        .find(|s| s.slot_name.starts_with("Jewel"))
        .expect("an allocated jewel slot")
        .slot_name
        .clone();
    items::equip_item(lua, &socket_slot, 0).expect("unequip failed");
    items::equip_item(lua, &socket_slot, jewel.id).expect("equip failed");

    let sockets = jewels::socket_jewels(lua).expect("socket re-extraction failed");
    let filled: Vec<_> = sockets
        .iter()
        .filter(|s| s.has_jewel && s.jewel_title.contains("Parity Test Jewel"))
        .collect();
    assert_eq!(filled.len(), 1, "the jewel should appear in its socket");
    assert!(filled[0].allocated);
    assert!(!filled[0].is_variable, "plain jewel is not Thread of Hope");
    assert_eq!(
        filled[0].active_art.as_deref(),
        Some("JewelSocketActiveBlue"),
        "Cobalt Jewel should select the blue socket art"
    );
    let empty = sockets.iter().find(|s| !s.has_jewel);
    if let Some(empty) = empty {
        assert!(empty.active_art.is_none(), "empty socket has no jewel art");
    }
}

// ---------------------------------------------------------------------------
// Config reset restores default values
// ---------------------------------------------------------------------------

#[test]
fn test_config_reset_to_defaults() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    // Change resistancePenalty away from whatever it currently is
    pob_egui::data::config::set_config_value(
        bridge.lua(),
        "resistancePenalty",
        LuaValue::Number(0.0),
    )
    .expect("failed to set config value");
    let cur: f64 = bridge
        .lua()
        .load("return mainObject_ref.main.modes['BUILD'].configTab.input['resistancePenalty']")
        .eval()
        .expect("failed to read config value");
    assert_eq!(cur, 0.0, "config value should be set");

    // Reset and compare against the upstream default (list defaultIndex)
    pob_egui::data::config::reset_config_to_defaults(bridge.lua()).expect("failed to reset config");
    let (after, expected): (f64, f64) = bridge
        .lua()
        .load(
            r#"
            local input = mainObject_ref.main.modes['BUILD'].configTab.input
            local varList = LoadModule("Modules/ConfigOptions")
            for _, v in ipairs(varList) do
                if v.var == 'resistancePenalty' then
                    return input['resistancePenalty'], v.list[v.defaultIndex or 1].val
                end
            end
            error("resistancePenalty not found in varList")
            "#,
        )
        .eval()
        .expect("failed to read values after reset");
    assert_eq!(
        after, expected,
        "resistancePenalty should be back to its default after reset"
    );
    assert_ne!(after, 0.0, "default resistance penalty is not 0");
}

// ---------------------------------------------------------------------------
// Sidebar display stats extraction
// ---------------------------------------------------------------------------

#[test]
fn test_sidebar_stats_extraction() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();

    let stats = pob_egui::data::display_stats::extract_sidebar_stats(bridge.lua())
        .expect("failed to extract sidebar stats");
    assert!(
        stats.lines.len() > 20,
        "expected a full stat list, got {} lines",
        stats.lines.len()
    );

    // The list should contain the staples every build has
    for expected in ["Life:", "Mana:", "Evasion rating:"] {
        assert!(
            stats
                .lines
                .iter()
                .any(|l| l.lhs.as_deref().is_some_and(|s| s.contains(expected))),
            "stat list should contain a '{expected}' line"
        );
    }
    // Values are present on stat lines
    let life_line = stats
        .lines
        .iter()
        .find(|l| l.lhs.as_deref().is_some_and(|s| s.contains("Life:")))
        .unwrap();
    assert!(life_line.rhs.is_some(), "Life line should have a value");

    // Allocate way too many passive points to trigger the warning path
    bridge
        .lua()
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.spec
            spec:BuildAllDependsAndPaths()
            local added = 0
            for id, node in pairs(spec.nodes) do
                if not spec.allocNodes[id] and node.type == "Normal"
                   and not node.ascendancyName and node.path and #node.path > 0 then
                    spec:AllocNode(node)
                    spec:BuildAllDependsAndPaths()
                    added = added + 1
                    if added >= 130 then break end
                end
            end
            build.buildFlag = true
            _runCallback('OnFrame')
            "#,
        )
        .exec()
        .expect("failed to over-allocate nodes");

    let stats = pob_egui::data::display_stats::extract_sidebar_stats(bridge.lua())
        .expect("failed to re-extract sidebar stats");
    assert!(
        stats
            .warnings
            .iter()
            .any(|w| w.contains("too many passive points")),
        "expected a too-many-points warning, got: {:?}",
        stats.warnings
    );

    // Upstream's v2.64 missing-anoint warning flows through the same list
    // (the fixture's boots have an unanointed eligible enchant)
    assert!(
        stats
            .warnings
            .iter()
            .any(|w| w.contains("missing an anoint")),
        "expected the missing-anoint warning, got: {:?}",
        stats.warnings
    );
}

// ---------------------------------------------------------------------------
// Item edit operations: variants, quality, influence
// ---------------------------------------------------------------------------

#[test]
fn test_item_edit_operations() {
    use pob_egui::data::items::{self, ItemEditOp};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Multi-variant unique: Vessel of Vinktar has 4 variants. Variant lines
    // only exist in database raws (game-copied text has none), so fetch the
    // unique's raw text from upstream's unique database.
    let raw: String = lua
        .load(
            r#"
            -- The unique DB loads via a coroutine resumed once per frame;
            -- pump frames until it finishes
            local main = mainObject_ref.main
            for i = 1, 1000 do
                if not main.uniqueDB.loading then break end
                _runCallback('OnFrame')
            end
            for name, item in pairs(main.uniqueDB.list) do
                if name:find("Vessel of Vinktar", 1, true) then
                    return item:BuildRaw()
                end
            end
            error("Vessel of Vinktar not found in uniqueDB")
            "#,
        )
        .eval()
        .expect("failed to fetch unique raw");
    let raw = raw.as_str();
    let info = items::item_edit_info(lua, raw).expect("edit info failed");
    assert!(
        info.variants.len() >= 4,
        "Vessel of Vinktar should have variants, got {:?}",
        info.variants
    );
    assert!(info.variant > 0, "a variant should be selected by default");

    let new_raw = items::apply_item_edit(lua, raw, &ItemEditOp::Variant(1))
        .expect("apply variant failed")
        .expect("variant edit should produce raw text");
    assert!(
        new_raw.contains("Selected Variant: 1"),
        "raw should record the selected variant"
    );
    let info = items::item_edit_info(lua, &new_raw).expect("edit info failed");
    assert_eq!(info.variant, 1, "round-trip should keep the selection");

    // Quality and influence on an armour base
    let raw = "Rarity: RARE\nTest Plate\nAstral Plate\nQuality: 20";
    let info = items::item_edit_info(lua, raw).expect("edit info failed");
    assert_eq!(info.quality, Some(20));
    assert!(info.can_be_influenced, "body armour can be influenced");
    assert_eq!(info.influence_names.len(), 8);
    assert_eq!(info.influence1, 0, "no influence initially");

    let new_raw = items::apply_item_edit(lua, raw, &ItemEditOp::Quality(30))
        .expect("apply quality failed")
        .expect("quality edit should produce raw text");
    assert!(new_raw.contains("Quality: 30"), "quality should be updated");

    let new_raw = items::apply_item_edit(lua, &new_raw, &ItemEditOp::Influence(1, 2))
        .expect("apply influence failed")
        .expect("influence edit should produce raw text");
    assert!(new_raw.contains("Shaper Item"), "raw: {new_raw}");
    assert!(new_raw.contains("Elder Item"), "raw: {new_raw}");
    let info = items::item_edit_info(lua, &new_raw).expect("edit info failed");
    assert_eq!(info.influence1, 1);
    assert_eq!(info.influence2, 2);

    // Clearing influence works too
    let cleared = items::apply_item_edit(lua, &new_raw, &ItemEditOp::Influence(0, 0))
        .expect("clear influence failed")
        .expect("influence clear should produce raw text");
    assert!(!cleared.contains("Shaper Item"));
    let info = items::item_edit_info(lua, &cleared).expect("edit info failed");
    assert_eq!(info.influence1, 0);
    assert_eq!(info.influence2, 0);
}

// ---------------------------------------------------------------------------
// Item database extraction (uniques + rare templates)
// ---------------------------------------------------------------------------

#[test]
fn test_item_db_extraction() {
    use pob_egui::data::item_db;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Pump the loading coroutine to completion (bounded)
    let mut loading = item_db::is_loading(lua).expect("is_loading failed");
    for _ in 0..200 {
        if !loading {
            break;
        }
        loading = item_db::pump_loading(lua, 100).expect("pump failed");
    }
    assert!(!loading, "item DBs should finish loading");

    let uniques = item_db::extract_db(lua, true).expect("unique extract failed");
    let rares = item_db::extract_db(lua, false).expect("rare extract failed");
    assert!(
        uniques.len() > 1000,
        "expected a full unique DB, got {}",
        uniques.len()
    );
    assert!(
        rares.len() > 50,
        "expected rare templates, got {}",
        rares.len()
    );

    // A staple unique exists, has a type, raw text, and searchable mods
    let shavs = uniques
        .iter()
        .find(|i| i.name.starts_with("Shavronne's Wrappings"))
        .expect("Shavronne's Wrappings in unique DB");
    assert_eq!(shavs.item_type, "Body Armour");
    assert!(shavs.raw.contains("Rarity: UNIQUE"));
    assert!(
        shavs.search_mods.contains("chaos damage"),
        "mods searchable: {}",
        shavs.search_mods
    );

    // Tooltip renders from raw text
    let lines = item_db::tooltip_from_raw(lua, &shavs.raw).expect("tooltip failed");
    assert!(
        lines.len() > 5,
        "tooltip should have lines, got {}",
        lines.len()
    );
    assert!(
        lines
            .iter()
            .any(|l| l.text.contains("Shavronne's Wrappings")),
        "tooltip should contain the item name"
    );

    // DB raw text can be added to the build
    let before = pob_egui::data::items::extract_item_list(lua)
        .expect("list failed")
        .len();
    let err = pob_egui::data::items::add_item_from_raw(lua, &shavs.raw).expect("add failed");
    assert!(err.is_none(), "DB item should parse when added: {err:?}");
    let after = pob_egui::data::items::extract_item_list(lua)
        .expect("list failed")
        .len();
    assert_eq!(after, before + 1, "item should be added to the build");
}

// ---------------------------------------------------------------------------
// Node power calculation (heatmap + report)
// ---------------------------------------------------------------------------

#[test]
fn test_node_power_build() {
    use pob_egui::data::node_power;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let stats = node_power::list_power_stats(lua).expect("stat list failed");
    assert!(
        stats.len() > 10,
        "expected a power stat list, got {}",
        stats.len()
    );
    assert_eq!(stats[0].label, "Offence/Defence");
    assert!(
        !stats.iter().any(|s| s.label == "Name"),
        "item-only stats filtered"
    );

    // Default offence/defence mode with a small depth so the test stays fast
    node_power::set_power_stat(lua, stats[0].index, Some(3)).expect("set stat failed");
    assert!(node_power::power_dirty(lua).expect("dirty failed"));

    let mut done = false;
    for _ in 0..600 {
        let (d, _progress) = node_power::power_step(lua).expect("step failed");
        if d {
            done = true;
            break;
        }
    }
    assert!(done, "power build should finish");
    assert!(!node_power::power_dirty(lua).expect("dirty failed"));

    let colors = node_power::heatmap_colors(lua).expect("colors failed");
    assert!(
        colors.len() > 100,
        "expected heatmap colors for unallocated nodes, got {}",
        colors.len()
    );
    assert!(
        colors.values().any(|c| c.r() > 0 || c.g() > 0 || c.b() > 0),
        "at least some nodes should have non-black power colors"
    );

    // Single-stat mode: Hit DPS produces a report
    let hit_dps = stats
        .iter()
        .find(|s| s.label == "Hit DPS")
        .expect("Hit DPS in stat list");
    node_power::set_power_stat(lua, hit_dps.index, Some(3)).expect("set stat failed");
    let mut done = false;
    for _ in 0..600 {
        let (d, _) = node_power::power_step(lua).expect("step failed");
        if d {
            done = true;
            break;
        }
    }
    assert!(done, "stat-mode power build should finish");

    let report = node_power::power_report(lua).expect("report failed");
    assert!(
        report.len() > 100,
        "report should cover the tree, got {} rows",
        report.len()
    );
    let with_power = report.iter().filter(|r| r.power != 0.0).count();
    assert!(
        with_power > 0,
        "some nodes should have nonzero Hit DPS power"
    );
    assert!(
        report
            .iter()
            .any(|r| r.id > 0 && (r.x != 0.0 || r.y != 0.0)),
        "report rows should carry node positions for panning"
    );

    // Upstream v2.64 added mastery effects to the report (flows through
    // BuildPowerReportList, which we call)
    assert!(
        report.iter().any(|r| r.name.contains("Mastery")),
        "report should include mastery rows"
    );
}

// ---------------------------------------------------------------------------
// Socket group management: slot, Full DPS, count, quality variant, delete all
// ---------------------------------------------------------------------------

#[test]
fn test_socket_group_management() {
    use pob_egui::data::skills::{self, GemProperty};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let groups = skills::extract_skills(lua).expect("extract failed");
    let group = groups
        .iter()
        .find(|g| !g.from_item && !g.gems.is_empty())
        .expect("a user socket group with gems");
    let gi = group.index;

    // Slot assignment round-trips
    skills::set_group_slot(lua, gi, Some("Body Armour")).expect("set slot failed");
    let groups = skills::extract_skills(lua).expect("extract failed");
    let group = groups.iter().find(|g| g.index == gi).unwrap();
    assert_eq!(group.slot.as_deref(), Some("Body Armour"));
    skills::set_group_slot(lua, gi, None).expect("clear slot failed");
    let groups = skills::extract_skills(lua).expect("extract failed");
    assert_eq!(groups.iter().find(|g| g.index == gi).unwrap().slot, None);

    // Full DPS toggle round-trips
    skills::set_group_full_dps(lua, gi, true).expect("full dps failed");
    let groups = skills::extract_skills(lua).expect("extract failed");
    assert!(
        groups
            .iter()
            .find(|g| g.index == gi)
            .unwrap()
            .include_in_full_dps
    );

    // Gem count
    let group = groups.iter().find(|g| g.index == gi).unwrap();
    if let Some((idx0, _)) = group
        .gems
        .iter()
        .enumerate()
        .find(|(_, g)| g.has_count && !g.is_support)
    {
        let gem_idx = idx0 + 1;
        skills::set_gem_property(lua, gi, gem_idx, GemProperty::Count(3))
            .expect("set count failed");
        let groups = skills::extract_skills(lua).expect("extract failed");
        assert_eq!(
            groups.iter().find(|g| g.index == gi).unwrap().gems[idx0].count,
            3,
            "gem count should round-trip"
        );
    }

    // Delete all removes user groups but keeps item-granted ones
    skills::delete_all_socket_groups(lua).expect("delete all failed");
    let groups = skills::extract_skills(lua).expect("extract failed");
    assert!(
        groups.iter().all(|g| g.from_item),
        "only item-granted groups should remain, got {} groups",
        groups.len()
    );
}

// ---------------------------------------------------------------------------
// Calcs tab active skill / part selection
// ---------------------------------------------------------------------------

#[test]
fn test_calcs_skill_selection() {
    use pob_egui::data::calcs;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let sel = calcs::skill_selection(lua).expect("selection failed");
    assert!(
        !sel.skills.is_empty(),
        "the calcs socket group should list active skills"
    );
    assert!(sel.selected_skill < sel.skills.len());

    // Selecting an active skill round-trips (index 0 is always valid)
    calcs::set_active_skill(lua, 0).expect("set active skill failed");
    let sel = calcs::skill_selection(lua).expect("selection failed");
    assert_eq!(sel.selected_skill, 0);

    // If the skill has parts, part selection round-trips too
    if sel.parts.len() > 1 {
        calcs::set_skill_part(lua, 1).expect("set part failed");
        let sel = calcs::skill_selection(lua).expect("selection failed");
        assert_eq!(sel.selected_part, 1);
    }
}

// ---------------------------------------------------------------------------
// Cluster jewel subgraphs appear in the extracted tree
// ---------------------------------------------------------------------------

#[test]
fn test_cluster_jewel_subgraph_extraction() {
    use pob_egui::data::items;
    use pob_egui::data::tree::TreeData;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let before = TreeData::extract(lua).expect("extract failed");
    let cluster_before = before.nodes.keys().filter(|id| **id >= 0x10000).count();

    // Add a Large Cluster Jewel and socket it into a large expansion socket
    let raw = "Rarity: RARE\nTest Cluster\nLarge Cluster Jewel\nItem Level: 84\n\
               Adds 8 Passive Skills\n2 Added Passive Skills are Jewel Sockets\n\
               Added Small Passive Skills grant: 12% increased Attack Damage while Dual Wielding";
    let err = items::add_item_from_raw(lua, raw).expect("add failed");
    assert!(err.is_none(), "cluster jewel should parse: {err:?}");
    let jewel_id = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Test Cluster"))
        .expect("cluster jewel in list")
        .id;

    let socketed: bool = lua
        .load(
            r#"
            local jewelId = ...
            local build = mainObject_ref.main.modes['BUILD']
            local spec = build.spec
            for nodeId in pairs(spec.tree.sockets) do
                local node = spec.tree.nodes[nodeId]
                if node and node.expansionJewel and node.expansionJewel.size == 2 then
                    build.itemsTab.sockets[nodeId]:SetSelItemId(jewelId)
                    build.itemsTab:PopulateSlots()
                    build.buildFlag = true
                    _runCallback('OnFrame')
                    return true
                end
            end
            return false
            "#,
        )
        .call(jewel_id)
        .expect("socketing failed");
    assert!(socketed, "should find a large expansion socket");

    // The Lua side should now have subgraphs...
    let subgraph_count: usize = lua
        .load(
            r#"
            local n = 0
            for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.subGraphs) do
                n = n + 1
            end
            return n
            "#,
        )
        .eval()
        .expect("subgraph count failed");
    assert!(subgraph_count > 0, "cluster subgraph should be built");

    // ...and re-extraction should pick up the cluster nodes with positions
    // and connections
    let after = TreeData::extract(lua).expect("extract failed");
    let cluster_nodes: Vec<_> = after.nodes.values().filter(|n| n.id >= 0x10000).collect();
    assert!(
        cluster_nodes.len() > cluster_before + 5,
        "expected new cluster nodes, got {} (before: {cluster_before})",
        cluster_nodes.len()
    );
    for node in &cluster_nodes {
        assert!(
            node.x.is_finite() && node.y.is_finite(),
            "cluster node {} should have a position",
            node.id
        );
    }
    let cluster_connections = after
        .connections
        .iter()
        .filter(|c| c.from_id >= 0x10000 || c.to_id >= 0x10000)
        .count();
    assert!(
        cluster_connections > 0,
        "cluster nodes should be connected to the tree"
    );

    // The subgraph's inner jewel sockets (from "2 Added Passive Skills are
    // Jewel Sockets") should now appear in the socket list
    use pob_egui::data::jewels;
    let sockets = jewels::socket_jewels(lua).expect("sockets failed");
    let inner: Vec<_> = sockets
        .iter()
        .filter(|s| {
            after
                .nodes
                .get(&s.node_id)
                .is_some_and(|n| n.id != 0 && n.x.is_finite())
                && lua
                    .load(format!(
                        r#"
                        local node = mainObject_ref.main.modes['BUILD'].spec.nodes[{}]
                        return node and node.expansionJewel ~= nil
                            and node.expansionJewel.size ~= 2
                        "#,
                        s.node_id
                    ))
                    .eval::<bool>()
                    .unwrap_or(false)
        })
        .collect();
    assert_eq!(
        inner.len(),
        2,
        "the large cluster jewel should expose 2 inner sockets"
    );

    // Socket a Medium Cluster Jewel into an inner socket: it should get the
    // alt-blue art and spawn its own nested subgraph
    let med_raw = "Rarity: RARE\nTest Medium Cluster\nMedium Cluster Jewel\nItem Level: 84\n\
                   Adds 5 Passive Skills\n1 Added Passive Skill is a Jewel Socket\n\
                   Added Small Passive Skills grant: 10% increased Effect of Non-Damaging Ailments";
    let err = items::add_item_from_raw(lua, med_raw).expect("add failed");
    assert!(err.is_none(), "medium cluster should parse: {err:?}");
    let med_id = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Test Medium Cluster"))
        .expect("medium cluster in list")
        .id;
    let inner_id = inner[0].node_id;
    lua.load(format!(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.itemsTab.sockets[{inner_id}]:SetSelItemId({med_id})
        build.itemsTab:PopulateSlots()
        build.buildFlag = true
        _runCallback('OnFrame')
        "#
    ))
    .exec()
    .expect("socketing medium failed");

    let sockets = jewels::socket_jewels(lua).expect("sockets failed");
    let inner_socket = sockets
        .iter()
        .find(|s| s.node_id == inner_id)
        .expect("inner socket still listed");
    assert!(inner_socket.has_jewel, "inner socket should hold the jewel");
    assert_eq!(
        inner_socket.active_art.as_deref(),
        Some("JewelSocketActiveAltBlue"),
        "medium cluster jewel selects the alt-blue art"
    );
}

// ---------------------------------------------------------------------------
// Hovered socket shows the socketed jewel's item tooltip
// ---------------------------------------------------------------------------

#[test]
fn test_socket_jewel_tooltip() {
    use pob_egui::data::{items, jewels};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // A life roll so the removal comparison is guaranteed a changed stat
    let raw = "Rarity: RARE\nHover Test Jewel\nCobalt Jewel\n+20 to maximum Life";
    let err = items::add_item_from_raw(lua, raw).expect("add failed");
    assert!(err.is_none(), "jewel should parse: {err:?}");
    let jewel_id = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Hover Test Jewel"))
        .expect("jewel in list")
        .id;

    let equipped = items::extract_equipped_items(lua).expect("equipped failed");
    let socket_slot = equipped
        .iter()
        .find(|s| s.slot_name.starts_with("Jewel"))
        .expect("an allocated jewel slot")
        .slot_name
        .clone();
    items::equip_item(lua, &socket_slot, 0).expect("unequip failed");
    items::equip_item(lua, &socket_slot, jewel_id).expect("equip failed");

    let sockets = jewels::socket_jewels(lua).expect("sockets failed");
    let filled = sockets
        .iter()
        .find(|s| s.has_jewel && s.jewel_title.contains("Hover Test Jewel"))
        .expect("filled socket");
    let lines = jewels::socket_jewel_tooltip(lua, filled.node_id, true).expect("tooltip failed");
    assert!(
        lines.iter().any(|l| l.text.contains("Hover Test Jewel")),
        "tooltip should show the jewel name, got {} lines",
        lines.len()
    );

    // Hovering a socketed jewel previews removing it, the way hovering an
    // allocated passive previews unallocating it.
    let texts = || {
        lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join(" | ")
    };
    assert!(
        lines.iter().any(|l| l.text.contains("Removing this item")),
        "tooltip should show the removal stat diff, got: {}",
        texts()
    );
    assert!(
        lines.iter().any(|l| l.text.contains("Life")),
        "removal diff should list the changed Life stat, got: {}",
        texts()
    );

    // The stat-difference tip is driven by the tree's own Ctrl+D here, so it
    // must not send the reader to the Items tab (which has no such binding).
    let tip = lines
        .iter()
        .find(|l| l.text.contains("Ctrl+D"))
        .unwrap_or_else(|| panic!("tooltip should carry the Ctrl+D tip, got: {}", texts()));
    assert!(
        !tip.text.contains("Items tab"),
        "tip should not point at the Items tab: {}",
        tip.text
    );
    println!("stat-diff tip: {}", tip.text);

    // ...and honours the stat-difference toggle
    let no_diffs =
        jewels::socket_jewel_tooltip(lua, filled.node_id, false).expect("tooltip failed");
    assert!(
        no_diffs.iter().any(|l| l.text.contains("Hover Test Jewel")),
        "jewel name should still show with diffs off"
    );
    assert!(
        !no_diffs
            .iter()
            .any(|l| l.text.contains("Removing this item")),
        "no removal diff when stat differences are off"
    );
    // The "press Ctrl+D to enable" variant is retargeted the same way
    assert!(
        no_diffs
            .iter()
            .any(|l| l.text.contains("Ctrl+D") && !l.text.contains("Items tab")),
        "the enable-variant tip should not point at the Items tab"
    );

    // An empty socket yields no tooltip lines
    let empty = sockets.iter().find(|s| !s.has_jewel);
    if let Some(empty) = empty {
        let lines = jewels::socket_jewel_tooltip(lua, empty.node_id, true).expect("tooltip failed");
        assert!(
            lines.is_empty(),
            "empty socket should have no jewel tooltip"
        );
    }
}

// ---------------------------------------------------------------------------
// Skill sets: create, copy, switch, rename, delete
// ---------------------------------------------------------------------------

#[test]
fn test_skill_sets() {
    use pob_egui::data::{skill_sets, skills};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let (sets, active) = skill_sets::list_skill_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 1, "test build starts with one skill set");
    let original_id = active;
    let user_groups = |lua: &mlua::Lua| -> usize {
        skills::extract_skills(lua)
            .expect("skills failed")
            .iter()
            .filter(|g| !g.from_item)
            .count()
    };
    let original_groups = user_groups(lua);
    assert!(original_groups > 0, "the build has socket groups");

    // New set: empty, becomes active
    skill_sets::new_skill_set(lua, "Empty Set").expect("new failed");
    let (sets, active) = skill_sets::list_skill_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, original_id, "new set becomes active");
    assert_eq!(
        user_groups(lua),
        0,
        "new set has no user socket groups (item-granted ones re-inject)"
    );

    // Copy the original set: same group count, does not become active
    skill_sets::copy_skill_set(lua, original_id, "Copied Set").expect("copy failed");
    let (sets, active_after_copy) = skill_sets::list_skill_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 3);
    assert_eq!(active_after_copy, active, "copy does not switch sets");
    let copy = sets
        .iter()
        .find(|s| s.title == "Copied Set")
        .expect("copy listed");

    // Switching to the copy restores the group list
    skill_sets::set_active_skill_set(lua, copy.id).expect("switch failed");
    assert_eq!(
        user_groups(lua),
        original_groups,
        "copied set has the original socket groups"
    );

    // Rename round-trips
    skill_sets::rename_skill_set(lua, copy.id, "Renamed Set").expect("rename failed");
    let (sets, _) = skill_sets::list_skill_sets(lua).expect("list failed");
    assert!(sets.iter().any(|s| s.title == "Renamed Set"));

    // Deleting the active set falls back to a neighbour
    skill_sets::delete_skill_set(lua, copy.id).expect("delete failed");
    let (sets, active) = skill_sets::list_skill_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, copy.id, "deleted set is no longer active");
    assert!(!sets.iter().any(|s| s.id == copy.id));
}

// ---------------------------------------------------------------------------
// Item sets: create, copy, switch, weapon swap, delete
// ---------------------------------------------------------------------------

#[test]
fn test_item_sets() {
    use pob_egui::data::{item_sets, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let (sets, original_id) = item_sets::list_item_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 1, "test build starts with one item set");

    // Record the currently equipped body armour
    let equipped_body = |lua: &mlua::Lua| -> i64 {
        items::extract_equipped_items(lua)
            .expect("equipped failed")
            .iter()
            .find(|s| s.slot_name == "Body Armour")
            .map(|s| s.sel_item_id)
            .unwrap_or(0)
    };
    let original_body = equipped_body(lua);
    assert!(original_body > 0, "test build has a body armour equipped");

    // New set: empty equipment, becomes active
    item_sets::new_item_set(lua, "Naked Set").expect("new failed");
    let (sets, active) = item_sets::list_item_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, original_id);
    assert_eq!(equipped_body(lua), 0, "new set has nothing equipped");

    // Switching back restores the original equipment
    item_sets::set_active_item_set(lua, original_id).expect("switch failed");
    assert_eq!(equipped_body(lua), original_body, "original set restored");

    // Copy preserves equipment
    item_sets::copy_item_set(lua, original_id, "Copied Gear").expect("copy failed");
    let (sets, _) = item_sets::list_item_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 3);
    let copy = sets
        .iter()
        .find(|s| s.title == "Copied Gear")
        .expect("copy listed");
    item_sets::set_active_item_set(lua, copy.id).expect("switch failed");
    assert_eq!(equipped_body(lua), original_body, "copy has the same gear");

    // Weapon swap flag round-trips and reveals swap slots
    assert!(!item_sets::use_second_weapon_set(lua).expect("swap read failed"));
    item_sets::set_use_second_weapon_set(lua, true).expect("swap set failed");
    assert!(item_sets::use_second_weapon_set(lua).expect("swap read failed"));
    let slots = items::extract_equipped_items(lua).expect("equipped failed");
    assert!(
        slots.iter().any(|s| s.slot_name.contains("Swap")),
        "swap slots should be visible while weapon swap is enabled"
    );
    item_sets::set_use_second_weapon_set(lua, false).expect("swap unset failed");

    // Rename + delete (active falls back to a neighbour)
    item_sets::rename_item_set(lua, copy.id, "Renamed Gear").expect("rename failed");
    let (sets, _) = item_sets::list_item_sets(lua).expect("list failed");
    assert!(sets.iter().any(|s| s.title == "Renamed Gear"));
    item_sets::delete_item_set(lua, copy.id).expect("delete failed");
    let (sets, active) = item_sets::list_item_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, copy.id);
}

// ---------------------------------------------------------------------------
// Config sets: create, copy, switch, delete
// ---------------------------------------------------------------------------

#[test]
fn test_config_sets() {
    use pob_egui::data::{config, config_sets};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let (sets, original_id) = config_sets::list_config_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 1, "test build starts with one config set");

    let read_penalty = |lua: &mlua::Lua| -> f64 {
        lua.load(
            "return mainObject_ref.main.modes['BUILD'].configTab.input['resistancePenalty'] or -60",
        )
        .eval()
        .expect("read failed")
    };

    // Change a value in the original set
    config::set_config_value(lua, "resistancePenalty", mlua::Value::Number(0.0))
        .expect("set failed");
    assert_eq!(read_penalty(lua), 0.0);

    // New set starts from defaults and becomes active
    config_sets::new_config_set(lua, "Boss Config").expect("new failed");
    let (sets, active) = config_sets::list_config_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, original_id);
    assert_eq!(read_penalty(lua), -60.0, "new set has default values");

    // Copy of the original preserves the changed value
    config_sets::copy_config_set(lua, original_id, "Copied Config").expect("copy failed");
    let (sets, _) = config_sets::list_config_sets(lua).expect("list failed");
    let copy = sets
        .iter()
        .find(|s| s.title == "Copied Config")
        .expect("copy listed");
    config_sets::set_active_config_set(lua, copy.id).expect("switch failed");
    assert_eq!(read_penalty(lua), 0.0, "copied set kept the changed value");

    // Rename + delete active falls back
    config_sets::rename_config_set(lua, copy.id, "Renamed Config").expect("rename failed");
    config_sets::delete_config_set(lua, copy.id).expect("delete failed");
    let (sets, active) = config_sets::list_config_sets(lua).expect("list failed");
    assert_eq!(sets.len(), 2);
    assert_ne!(active, copy.id);

    // Switching back to the original restores its value
    config_sets::set_active_config_set(lua, original_id).expect("switch failed");
    assert_eq!(read_penalty(lua), 0.0);
}

// ---------------------------------------------------------------------------
// Loadouts: creation, listing, activation across all four set systems
// ---------------------------------------------------------------------------

#[test]
fn test_loadouts() {
    use pob_egui::data::{config_sets, item_sets, loadouts, skill_sets, tree_specs};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Specs on outdated tree versions get a "[3.xx]" prefix and stop
    // matching loadout titles (upstream behavior), so bring the build to
    // the latest tree first
    let latest = tree_specs::list_tree_versions(lua)
        .expect("versions failed")
        .into_iter()
        .find(|v| v.is_latest)
        .expect("a latest version");
    tree_specs::convert_all_to_version(lua, &latest.id).expect("upgrade failed");

    // With single sets everywhere, the sole tree spec forms a loadout
    let (list, _) = loadouts::list_loadouts(lua).expect("list failed");
    let initial_count = list.len();

    // New loadout creates a spec + item/skill/config sets sharing the name
    loadouts::new_loadout(lua, "Bossing").expect("new loadout failed");
    let (list, _) = loadouts::list_loadouts(lua).expect("list failed");
    assert!(
        list.iter().any(|l| l == "Bossing"),
        "new loadout listed, got {list:?}"
    );
    assert!(list.len() > initial_count);
    let (sets, _) = skill_sets::list_skill_sets(lua).expect("skill sets failed");
    assert!(sets.iter().any(|s| s.title == "Bossing"));
    let (sets, _) = item_sets::list_item_sets(lua).expect("item sets failed");
    assert!(sets.iter().any(|s| s.title == "Bossing"));
    let (sets, _) = config_sets::list_config_sets(lua).expect("config sets failed");
    assert!(sets.iter().any(|s| s.title == "Bossing"));

    // Activating it switches all four actives
    assert!(
        loadouts::activate_loadout(lua, "Bossing").expect("activate failed"),
        "activation should find the loadout"
    );
    let (specs, active_spec) = tree_specs::list_specs(lua).expect("specs failed");
    assert_eq!(
        specs
            .get(active_spec - 1)
            .map(|s| s.title.as_str())
            .unwrap_or(""),
        "Bossing",
        "tree spec switched"
    );
    let (sets, active) = skill_sets::list_skill_sets(lua).expect("skill sets failed");
    assert_eq!(
        sets.iter().find(|s| s.id == active).unwrap().title,
        "Bossing",
        "skill set switched"
    );
    let (sets, active) = item_sets::list_item_sets(lua).expect("item sets failed");
    assert_eq!(
        sets.iter().find(|s| s.id == active).unwrap().title,
        "Bossing",
        "item set switched"
    );
    let (sets, active) = config_sets::list_config_sets(lua).expect("config sets failed");
    assert_eq!(
        sets.iter().find(|s| s.id == active).unwrap().title,
        "Bossing",
        "config set switched"
    );

    // The matched loadout is reported as selected
    let (_, selected) = loadouts::list_loadouts(lua).expect("list failed");
    assert_eq!(selected.as_deref(), Some("Bossing"));

    // Unknown names are rejected
    assert!(!loadouts::activate_loadout(lua, "Nope").expect("activate failed"));
}

// ---------------------------------------------------------------------------
// Crafting: create item, select affixes, roll range, custom mods
// ---------------------------------------------------------------------------

#[test]
fn test_crafting() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let types = crafting::base_type_list(lua).expect("types failed");
    assert!(
        types.iter().any(|t| t == "Body Armour: Armour"),
        "type list should contain Body Armour: Armour, got {types:?}"
    );
    let bases = crafting::base_list(lua, "Body Armour: Armour").expect("bases failed");
    let astral_idx = bases
        .iter()
        .position(|b| b.contains("Astral Plate"))
        .expect("Astral Plate in base list");

    // Craft a rare Astral Plate
    let item_id = crafting::craft_item(
        lua,
        "RARE",
        "Body Armour: Armour",
        astral_idx + 1,
        "Test Craft",
    )
    .expect("craft failed")
    .expect("craft should return an id");
    let list = items::extract_item_list(lua).expect("list failed");
    assert!(
        list.iter()
            .any(|e| e.id == item_id && e.name.contains("Test Craft")),
        "crafted item in build list"
    );

    // Affix slots: 3 prefixes + 3 suffixes with populated option lists
    let info = crafting::craft_info(lua, item_id)
        .expect("info failed")
        .expect("crafted item has craft info");
    assert_eq!(info.slots.len(), 6, "3 prefixes + 3 suffixes");
    let prefix_count = info.slots.iter().filter(|s| s.is_prefix).count();
    assert_eq!(prefix_count, 3);
    let first = &info.slots[0];
    assert!(
        first.options.len() > 20,
        "body armour prefixes should have many options, got {}",
        first.options.len()
    );
    assert_eq!(first.selected, "None");

    // Select a life prefix and confirm it lands on the item text
    let life = first
        .options
        .iter()
        .find(|o| o.label.contains("to maximum Life") && !o.label.contains("Mana"))
        .expect("a life prefix option");
    crafting::set_affix(lua, item_id, true, 1, &life.mod_id, 1.0).expect("set affix failed");
    let raw = items::get_item_raw(lua, item_id).expect("raw failed");
    assert!(
        raw.contains("to maximum Life"),
        "life affix should appear in the item: {raw}"
    );
    let info = crafting::craft_info(lua, item_id)
        .expect("info failed")
        .expect("still crafted");
    assert_eq!(info.slots[0].selected, life.mod_id, "selection round-trips");
    assert!((info.slots[0].range - 1.0).abs() < 0.001, "range kept");

    // Clearing the slot removes the mod
    crafting::set_affix(lua, item_id, true, 1, "None", 0.5).expect("clear failed");
    let raw = items::get_item_raw(lua, item_id).expect("raw failed");
    assert!(!raw.contains("to maximum Life"), "cleared: {raw}");

    // Custom (bench) mod appends and survives re-crafting
    crafting::add_custom_mod(lua, item_id, "+1 to Level of Socketed Gems", true)
        .expect("custom failed");
    let raw = items::get_item_raw(lua, item_id).expect("raw failed");
    assert!(
        raw.contains("+1 to Level of Socketed Gems"),
        "custom mod present: {raw}"
    );
    if let Some(other) = info.slots[0]
        .options
        .iter()
        .find(|o| o.label.contains("to maximum Mana"))
    {
        crafting::set_affix(lua, item_id, true, 1, &other.mod_id, 0.5).expect("set failed");
        let raw = items::get_item_raw(lua, item_id).expect("raw failed");
        assert!(
            raw.contains("+1 to Level of Socketed Gems"),
            "custom mod survives Craft(): {raw}"
        );
    }

    // Non-crafted items have no craft info
    let plain = list.iter().find(|e| e.id != item_id).unwrap();
    assert!(
        crafting::craft_info(lua, plain.id)
            .expect("info failed")
            .is_none(),
        "non-crafted items are not craftable"
    );
}

// ---------------------------------------------------------------------------
// Cluster jewel crafting and anoints
// ---------------------------------------------------------------------------

#[test]
fn test_cluster_craft_and_anoint() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Craft a large cluster jewel and pick a skill + node count
    let bases = crafting::base_list(lua, "Jewel: Cluster").expect("bases failed");
    let large_idx = bases
        .iter()
        .position(|b| b.contains("Large Cluster Jewel"))
        .expect("large cluster base");
    let jewel_id = crafting::craft_item(lua, "RARE", "Jewel: Cluster", large_idx + 1, "Cluster")
        .expect("craft failed")
        .expect("id");

    let info = crafting::cluster_craft_info(lua, jewel_id)
        .expect("info failed")
        .expect("crafted cluster has cluster info");
    assert!(
        info.skills.len() > 10,
        "large cluster should offer many skills, got {}",
        info.skills.len()
    );
    assert!(info.min_nodes < info.max_nodes);

    let attack_skill = info
        .skills
        .iter()
        .find(|(_, name)| name.contains("Attack Damage while Dual Wielding"))
        .expect("dual wield skill listed");
    crafting::set_cluster_jewel(lua, jewel_id, &attack_skill.0, 8).expect("set failed");
    let raw = items::get_item_raw(lua, jewel_id).expect("raw failed");
    assert!(raw.contains("Adds 8 Passive Skills"), "node count: {raw}");
    assert!(
        raw.contains("2 Added Passive Skills are Jewel Sockets"),
        "large sockets: {raw}"
    );
    assert!(
        raw.contains("Attack Damage while Dual Wielding"),
        "skill enchant: {raw}"
    );
    let info = crafting::cluster_craft_info(lua, jewel_id)
        .expect("info failed")
        .expect("still cluster");
    assert_eq!(info.selected_skill, attack_skill.0);
    assert_eq!(info.node_count, 8);

    // Anoints: the notable list is populated and applying one works
    let notables = crafting::anoint_notables(lua).expect("notables failed");
    assert!(
        notables.len() > 100,
        "many anointable notables, got {}",
        notables.len()
    );
    let notable = &notables[0];
    assert!(!notable.oils.is_empty(), "notables carry oil recipes");

    // Find an amulet to anoint (craft one if none present)
    let amulet_bases = crafting::base_list(lua, "Amulet").expect("amulet bases failed");
    let amulet_id = crafting::craft_item(lua, "RARE", "Amulet", 1, "Anoint Target")
        .expect("craft failed")
        .expect("id");
    let _ = amulet_bases;

    crafting::anoint_item(lua, amulet_id, Some(&notable.name), 1).expect("anoint failed");
    let anoints = crafting::get_anoints(lua, amulet_id).expect("get failed");
    assert_eq!(anoints, vec![notable.name.clone()]);
    let raw = items::get_item_raw(lua, amulet_id).expect("raw failed");
    assert!(
        raw.contains(&format!("Allocates {}", notable.name)),
        "anoint line present: {raw}"
    );

    // Replacing and removing
    let other = &notables[1];
    crafting::anoint_item(lua, amulet_id, Some(&other.name), 1).expect("re-anoint failed");
    let anoints = crafting::get_anoints(lua, amulet_id).expect("get failed");
    assert_eq!(anoints, vec![other.name.clone()], "anoint replaced");
    crafting::anoint_item(lua, amulet_id, None, 1).expect("remove failed");
    assert!(
        crafting::get_anoints(lua, amulet_id)
            .expect("get failed")
            .is_empty(),
        "anoint removed"
    );
}

// ---------------------------------------------------------------------------
// Enchantments: catalog, apply, remove
// ---------------------------------------------------------------------------

#[test]
fn test_enchantments() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Craft boots: boot enchants are not skill-keyed
    let boots_id = crafting::craft_item(lua, "RARE", "Boots: Armour", 1, "Enchant Boots")
        .expect("craft failed")
        .expect("id");
    let opts = crafting::enchant_options(lua, boots_id)
        .expect("options failed")
        .expect("boots have an enchant catalog");
    assert!(!opts.has_skills, "boot enchants are not per-skill");

    let catalog = crafting::enchant_catalog(lua, boots_id, None).expect("catalog failed");
    assert!(!catalog.is_empty(), "boot enchant sources exist");

    // Popup sorting: per-line power-stat values nested in the catalog shape
    // (upstream's v2.66 popup sort). The movement-speed enchant moves the
    // Move speed stat while regen/leech lines do not, so values spread.
    let move_speed = pob_egui::data::node_power::list_power_stats(lua)
        .expect("stats failed")
        .into_iter()
        .find(|s| s.label == "Move speed")
        .expect("a Move speed power stat");
    let values = crafting::enchant_sort_values(lua, boots_id, None, 1, move_speed.index)
        .expect("sort values failed");
    assert_eq!(values.len(), catalog.len(), "one value group per source");
    for ((source, lines), vals) in catalog.iter().zip(&values) {
        assert_eq!(
            vals.len(),
            lines.len(),
            "one value per line in {}",
            source.label
        );
    }
    let (min, max) = values
        .iter()
        .flatten()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(
        max > min,
        "movement-speed enchants should spread Move speed values: {min}..{max}"
    );

    let (source, lines) = catalog
        .iter()
        .find(|(_, lines)| !lines.is_empty())
        .expect("a source with lines");
    crafting::apply_enchant(lua, boots_id, None, &source.name, 1, 1).expect("apply failed");
    let raw = items::get_item_raw(lua, boots_id).expect("raw failed");
    let applied_line = lines[0].split('/').next().unwrap();
    assert!(
        raw.contains(applied_line.trim()),
        "enchant line '{applied_line}' should be on the item: {raw}"
    );

    crafting::remove_enchant(lua, boots_id, 1).expect("remove failed");
    let raw = items::get_item_raw(lua, boots_id).expect("raw failed");
    assert!(!raw.contains(applied_line.trim()), "enchant removed: {raw}");

    // Helmets: skill-keyed catalog
    let helm_id = crafting::craft_item(lua, "RARE", "Helmet: Armour", 1, "Enchant Helm")
        .expect("craft failed")
        .expect("id");
    let opts = crafting::enchant_options(lua, helm_id)
        .expect("options failed")
        .expect("helmets have an enchant catalog");
    assert!(opts.has_skills, "helmet enchants are per-skill");
    assert!(
        opts.skills.len() > 100,
        "many skills have helmet enchants, got {}",
        opts.skills.len()
    );
    let skill = &opts.skills[0];
    let catalog = crafting::enchant_catalog(lua, helm_id, Some(skill)).expect("catalog failed");
    assert!(
        catalog.iter().any(|(_, lines)| !lines.is_empty()),
        "skill '{skill}' has enchant lines"
    );

    // Non-enchantable items return None
    let plain = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| !e.has_enchantments)
        .map(|e| e.id);
    if let Some(id) = plain {
        assert!(
            crafting::enchant_options(lua, id)
                .expect("options failed")
                .is_none()
        );
    }
}

// ---------------------------------------------------------------------------
// Multi-anoint slots and anoint preview
// ---------------------------------------------------------------------------

#[test]
fn test_anoint_slots_and_preview() {
    use pob_egui::data::crafting;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Plain amulet: single slot
    let amulet_id = crafting::craft_item(lua, "RARE", "Amulet", 1, "Slots Test")
        .expect("craft failed")
        .expect("id");
    assert_eq!(
        crafting::anoint_slot_count(lua, amulet_id).expect("count failed"),
        1
    );

    // Preview reports stat differences before committing
    let notables = crafting::anoint_notables(lua).expect("notables failed");
    // Find a life notable that is not already allocated in the test build
    let (notable, preview) = notables
        .iter()
        .filter(|n| n.stats.iter().any(|s| s.contains("maximum Life")))
        .find_map(|n| {
            let preview =
                crafting::anoint_preview(lua, amulet_id, &n.name, 1).expect("preview failed");
            preview
                .iter()
                .any(|l| l.contains("will give you"))
                .then_some((n, preview))
        })
        .expect("an unallocated life notable with a preview");
    // A life notable on an unallocated node should show a Life change
    assert!(
        preview.iter().any(|l| l.contains("Life")),
        "life notable preview mentions Life: {preview:?}"
    );

    // Anointing the same notable then previewing it reports no change
    crafting::anoint_item(lua, amulet_id, Some(&notable.name), 1).expect("anoint failed");
    let preview =
        crafting::anoint_preview(lua, amulet_id, &notable.name, 1).expect("preview failed");
    assert!(
        preview.iter().any(|l| l.contains("already anointed")),
        "duplicate anoint detected: {preview:?}"
    );

    // Stranglegasp-style flag opens further slots as previous ones fill
    let stranglegasp =
        "Rarity: UNIQUE\nStranglegasp\nOnyx Amulet\nCan have 3 additional Enchantment Modifiers";
    let err = pob_egui::data::items::add_item_from_raw(lua, stranglegasp).expect("add failed");
    assert!(err.is_none(), "stranglegasp should parse: {err:?}");
    let sg_id = pob_egui::data::items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Stranglegasp"))
        .expect("stranglegasp in list")
        .id;
    assert_eq!(
        crafting::anoint_slot_count(lua, sg_id).expect("count failed"),
        1,
        "empty stranglegasp starts with one open slot"
    );
    crafting::anoint_item(lua, sg_id, Some(&notables[0].name), 1).expect("anoint failed");
    assert_eq!(
        crafting::anoint_slot_count(lua, sg_id).expect("count failed"),
        2,
        "second slot opens after the first is filled"
    );
    crafting::anoint_item(lua, sg_id, Some(&notables[1].name), 2).expect("anoint failed");
    let anoints = crafting::get_anoints(lua, sg_id).expect("get failed");
    assert_eq!(anoints.len(), 2, "two anoints on stranglegasp: {anoints:?}");
}

// ---------------------------------------------------------------------------
// Corruption and implicit popups
// ---------------------------------------------------------------------------

#[test]
fn test_corrupt_and_implicits() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let helm_id = crafting::craft_item(lua, "RARE", "Helmet: Armour", 1, "Corrupt Helm")
        .expect("craft failed")
        .expect("id");

    // Corrupted implicit options exist and applying one corrupts the item
    let options = crafting::corrupt_options(lua, helm_id).expect("options failed");
    assert!(
        options.len() > 5,
        "helmet has corrupted implicits, got {}",
        options.len()
    );
    // Popup sorting: per-option power-stat values (upstream's v2.66 popup
    // sort), parallel to the options and with real spread
    let ehp = pob_egui::data::node_power::list_power_stats(lua)
        .expect("stats failed")
        .into_iter()
        .find(|s| s.label.contains("Effective Hit Pool") || s.label.contains("EHP"))
        .expect("an EHP power stat");
    let values =
        crafting::corrupt_sort_values(lua, helm_id, ehp.index).expect("sort values failed");
    assert_eq!(
        values.len(),
        options.len(),
        "one sort value per corrupt option"
    );
    let (min, max) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(
        max > min,
        "life/resistance implicits should spread EHP values: {min}..{max}"
    );

    let first = &options[0];
    let second = options
        .iter()
        .find(|o| o.group != first.group)
        .expect("a second option in another group");
    crafting::corrupt_item(lua, helm_id, Some(first.index), Some(second.index))
        .expect("corrupt failed");
    let raw = items::get_item_raw(lua, helm_id).expect("raw failed");
    assert!(raw.contains("Corrupted"), "item is corrupted: {raw}");

    // Implicit sources: no eldritch without influence, Delve + Custom present
    let sources = crafting::implicit_sources(lua, helm_id).expect("sources failed");
    let ids: Vec<&str> = sources.iter().map(|(_, id)| id.as_str()).collect();
    assert!(ids.contains(&"DelveImplicit"));
    assert!(ids.contains(&"CUSTOM"));
    assert!(!ids.contains(&"EXARCH"), "no exarch without influence");

    // Popup sorting for implicit groups: one value per group (this helm's
    // delve pool is mostly calc-neutral, so only the shape is asserted here;
    // spread is asserted on the eldritch source below)
    let delve_groups = crafting::implicit_mods(lua, helm_id, "DelveImplicit").expect("mods failed");
    let values = crafting::implicit_sort_values(lua, helm_id, "DelveImplicit", ehp.index)
        .expect("sort values failed");
    assert_eq!(
        values.len(),
        delve_groups.len(),
        "one sort value per implicit group"
    );

    // Adding Searing Exarch influence exposes the eldritch source
    let raw = items::get_item_raw(lua, helm_id).expect("raw failed");
    let info = items::item_edit_info(lua, &raw).expect("edit info failed");
    let exarch_idx = info
        .influence_names
        .iter()
        .position(|n| n == "Searing Exarch")
        .expect("exarch influence listed")
        + 1;
    let new_raw = items::apply_item_edit(lua, &raw, &items::ItemEditOp::Influence(exarch_idx, 0))
        .expect("apply failed")
        .expect("raw");
    let err = items::replace_item_from_raw(lua, helm_id, &new_raw).expect("replace failed");
    assert!(err.is_none(), "influenced helm should parse: {err:?}");

    let sources = crafting::implicit_sources(lua, helm_id).expect("sources failed");
    assert!(
        sources.iter().any(|(_, id)| id == "EXARCH"),
        "exarch source available with influence, got {sources:?}"
    );

    // Eldritch implicit groups have tiers; applying one lands on the item
    let groups = crafting::implicit_mods(lua, helm_id, "EXARCH").expect("mods failed");
    assert!(!groups.is_empty(), "exarch implicits exist");

    // Eldritch implicits move defensive stats, so sort values spread here
    let exarch_values = crafting::implicit_sort_values(lua, helm_id, "EXARCH", ehp.index)
        .expect("sort values failed");
    assert_eq!(exarch_values.len(), groups.len());
    let (vmin, vmax) = exarch_values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(
        vmax > vmin,
        "eldritch implicits should spread EHP values: {vmin}..{vmax}"
    );
    let group = &groups[0];
    assert!(!group.tiers.is_empty());
    crafting::add_implicit(lua, helm_id, "EXARCH", 1, 1).expect("add failed");
    let raw = items::get_item_raw(lua, helm_id).expect("raw failed");
    let tier_line = group.tiers[0].split('/').next().unwrap();
    // Compare ignoring rolled numbers: match the alpha suffix of the line
    let pattern: String = tier_line
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .collect();
    let pattern = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
    let raw_normalized: String = raw
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .collect();
    let raw_normalized = raw_normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        raw_normalized.contains(&pattern),
        "eldritch implicit '{tier_line}' should be on the item: {raw}"
    );

    // Replacing with another tier keeps a single eldritch implicit
    let implicits_before = raw.matches("Implicits:").count();
    crafting::add_implicit(lua, helm_id, "EXARCH", 1, group.tiers.len()).expect("add failed");
    let raw2 = items::get_item_raw(lua, helm_id).expect("raw failed");
    assert_eq!(
        raw2.matches("Implicits:").count(),
        implicits_before,
        "replacement does not duplicate implicits"
    );

    // Custom implicit appends
    crafting::add_custom_implicit(lua, helm_id, "+13 to maximum Life").expect("custom failed");
    let raw3 = items::get_item_raw(lua, helm_id).expect("raw failed");
    assert!(
        raw3.contains("+13 to maximum Life"),
        "custom implicit: {raw3}"
    );
}

#[test]
fn test_advanced_copy_paste_format() {
    use pob_egui::data::items;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // GGG's advanced item copy: mod headers like
    // { Prefix Modifier "Healthy" (Tier: 5) - Life } before each line
    let raw = "Item Class: Body Armours\n\
               Rarity: Rare\n\
               Corpse Shell\n\
               Astral Plate\n\
               --------\n\
               Item Level: 84\n\
               --------\n\
               { Prefix Modifier \"Healthy\" (Tier: 5) - Life }\n\
               +95 to maximum Life\n\
               { Suffix Modifier \"of the Lion\" (Tier: 4) - Attribute }\n\
               +40 to Strength\n";
    let err = items::add_item_from_raw(lua, raw).expect("add failed");
    assert!(err.is_none(), "advanced-format item parses: {err:?}");
    let entry_id = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Corpse Shell"))
        .map(|e| e.id)
        .expect("advanced item in list");
    let rebuilt = items::get_item_raw(lua, entry_id).expect("raw failed");
    assert!(
        rebuilt.contains("+95 to maximum Life") && rebuilt.contains("+40 to Strength"),
        "mods survive the advanced parse: {rebuilt}"
    );
    let advanced: bool = lua
        .load("local id = ... return mainObject_ref.main.modes['BUILD'].itemsTab.items[id].advancedCopy == true")
        .call(entry_id)
        .expect("flag check failed");
    assert!(advanced, "item recognised as advanced copy format");
}

#[test]
fn test_imbued_supports() {
    use pob_egui::data::{gems, skills};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // The main group is socketed; imbued applies per item slot
    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| g.is_main && g.slot.is_some() && !g.from_item)
        .expect("a socketed main group");
    assert!(group.imbued_support.is_none(), "starts without imbued");
    let slot = group.slot.clone().unwrap();

    // Imbued search mode: supports only, exceptional supports excluded
    let options = gems::search_gems(lua, group.index, "", false, 500, true)
        .expect("search failed")
        .choices;
    assert!(!options.is_empty(), "imbued choices exist");
    assert!(
        options.iter().all(|c| c.is_support),
        "imbued list contains only supports"
    );
    assert!(
        !options.iter().any(|c| c.name.contains("Enlighten")
            || c.name.contains("Empower")
            || c.name.contains("Enhance")),
        "exceptional supports are excluded"
    );

    // Setting one lands on the group, fills the runtime slot map, and the
    // calc engine adds it to the main skill's support list
    let pick = options[0].name.clone();
    let err = skills::set_imbued_support(lua, group.index, Some(&pick)).expect("set failed");
    assert!(err.is_none(), "set imbued failed: {err:?}");
    let groups = skills::extract_skills(lua).expect("skills failed");
    let group_after = groups.iter().find(|g| g.index == group.index).unwrap();
    assert_eq!(
        group_after.imbued_support.as_deref(),
        Some(pick.as_str()),
        "imbued support stored on the group"
    );
    let slot_mapped: bool = lua
        .load("local slot = ... return mainObject_ref.main.modes['BUILD'].skillsTab.imbuedSupportBySlot[slot] ~= nil")
        .call(slot.as_str())
        .expect("map check failed");
    assert!(slot_mapped, "imbuedSupportBySlot populated for {slot}");
    let support_names: String = lua
        .load(
            r#"
        local names = {}
        local skill = mainObject_ref.main.modes['BUILD'].calcsTab.mainEnv.player.mainSkill
        for _, effect in ipairs(skill.supportList or {}) do
            table.insert(names, effect.grantedEffect and effect.grantedEffect.name or "?")
        end
        return table.concat(names, "\n")
    "#,
        )
        .eval()
        .expect("support list failed");
    assert!(
        support_names.contains(&pick),
        "calc picks up the imbued support '{pick}': {support_names}"
    );

    // Persists as the imbuedSupport attribute via upstream save
    let tmp_dir = std::env::temp_dir().join(format!("egui-pob-imbued-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let mut build_path = tmp_dir.to_string_lossy().to_string();
    build_path.push('/');
    lua.load("mainObject_ref.main.buildPath = ...")
        .call::<()>(build_path.as_str())
        .expect("failed to redirect buildPath");
    bridge
        .save_build_as("Imbued Test", "")
        .expect("save failed");
    let xml = std::fs::read_to_string(tmp_dir.join("Imbued Test.xml")).expect("read failed");
    assert!(
        xml.contains("imbuedSupport="),
        "imbuedSupport attribute persists"
    );
    let _ = std::fs::remove_dir_all(&tmp_dir);

    // Clearing removes it again
    skills::set_imbued_support(lua, group.index, None).expect("clear failed");
    let groups = skills::extract_skills(lua).expect("skills failed");
    assert!(
        groups
            .iter()
            .find(|g| g.index == group.index)
            .unwrap()
            .imbued_support
            .is_none(),
        "imbued support cleared"
    );
}

#[test]
fn test_export_support_toggle_persists() {
    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // The same mutation the Export Support checkbox performs
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        if build.partyTab then
            build.partyTab.enableExportBuffs = true
        end
        local ctrl = build.importTab and build.importTab.controls
            and build.importTab.controls.enablePartyExportBuffs
        if ctrl then
            ctrl.state = true
        end
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
    .expect("toggle failed");

    // Upstream's ImportTab saver persists it as exportParty="true"
    let tmp_dir =
        std::env::temp_dir().join(format!("egui-pob-export-support-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let mut build_path = tmp_dir.to_string_lossy().to_string();
    build_path.push('/');
    lua.load("mainObject_ref.main.buildPath = ...")
        .call::<()>(build_path.as_str())
        .expect("failed to redirect buildPath");
    bridge
        .save_build_as("Export Support Test", "")
        .expect("save failed");
    let xml =
        std::fs::read_to_string(tmp_dir.join("Export Support Test.xml")).expect("read failed");
    assert!(
        xml.contains("exportParty=\"true\""),
        "exportParty persists in the build XML"
    );
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_compare_tab() {
    use pob_egui::data::compare;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Import the same build as a comparison entry (upstream CompareEntry:
    // a full second calc environment in the same VM)
    let xml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_builds")
        .join("3.28-migrage")
        .join("reliqlone.xml");
    let xml = std::fs::read_to_string(&xml_path).expect("read test build");
    let ok = compare::import_build(lua, &xml, "Mirror").expect("import failed");
    assert!(ok, "compare import succeeds");
    let (entries, active) = compare::list_entries(lua).expect("list failed");
    assert_eq!(entries, ["Mirror"], "one entry with its label");
    assert_eq!(active, 1, "imported entry becomes active");

    // A compare entry loads its Skills section before its Items section, so its
    // socket groups have to be reprocessed once the items are in place or its
    // gems miss the matching-socket quality bonus (see DIVERGENCES.md).
    let (primary_quality, compare_quality): (f64, f64) = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local entry = build.compareTab.compareEntries[1]
            return build.calcsTab.mainOutput.GemQuality or 0, entry:GetOutput().GemQuality or 0
        "#,
        )
        .eval()
        .expect("gem quality query failed");
    assert_eq!(
        compare_quality, primary_quality,
        "the compare entry picks up the matching-socket quality bonus"
    );

    // Identical builds: real stat rows, no differences
    let rows = compare::stat_rows(lua, 1).expect("rows failed");
    let stat_rows: Vec<_> = rows.iter().filter(|r| !r.spacer).collect();
    assert!(
        stat_rows.len() > 10,
        "summary has stat rows, got {}",
        stat_rows.len()
    );
    assert!(
        stat_rows.iter().all(|r| r.better == 0 && r.diff.is_empty()),
        "identical builds show no diffs: {:?}",
        stat_rows
            .iter()
            .find(|r| r.better != 0)
            .map(|r| (&r.label, &r.diff))
    );
    assert!(
        stat_rows.iter().any(|r| r.label.contains("Life")),
        "well-known stats present"
    );

    // Lower the primary's character level: Life drops, so diffs appear and
    // the compare side reads as better on Life
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.characterLevel = build.characterLevel - 20
        build.configTab:BuildModList()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
    .expect("level change failed");
    let rows = compare::stat_rows(lua, 1).expect("rows failed");
    assert!(
        rows.iter().any(|r| r.better != 0),
        "level change on the primary produces diffs"
    );

    // Tree diff of a mirror build is empty (level change doesn't touch it)
    let diff = compare::tree_diff(lua, 1).expect("tree diff failed");
    assert!(
        diff.added.is_empty() && diff.removed.is_empty() && diff.mastery.is_empty(),
        "identical trees show no node diff: +{:?} -{:?}",
        diff.added,
        diff.removed
    );

    // Item rows: slot union with match statuses for a mirror build
    let items = compare::item_rows(lua, 1).expect("item rows failed");
    assert!(items.len() > 10, "item rows exist, got {}", items.len());
    assert!(
        items
            .iter()
            .all(|r| r.status == "(match)" || r.status == "(both empty)"),
        "mirror build items all match: {:?}",
        items
            .iter()
            .find(|r| r.status != "(match)" && r.status != "(both empty)")
            .map(|r| (&r.slot, &r.status))
    );

    // Skills: groups pair up, every gem is common
    let skills = compare::skill_rows(lua, 1).expect("skill rows failed");
    assert!(!skills.is_empty(), "skill rows exist");
    assert!(
        skills.iter().all(|row| row
            .primary_gems
            .iter()
            .chain(&row.compare_gems)
            .all(|g| g.status == "common")),
        "mirror build gems are all common"
    );

    // Config: sections exist, no diffs on a mirror build
    let config = compare::config_rows(lua, 1).expect("config rows failed");
    assert!(
        config.iter().all(|s| s.diffs.is_empty()),
        "mirror build config has no diffs: {:?}",
        config
            .iter()
            .find(|s| !s.diffs.is_empty())
            .map(|s| (&s.name, &s.diffs[0].label))
    );

    // Copy-to-primary actions: item copy adds an item, spec copy adds a spec
    let item_count_before: usize = lua
        .load("local n = 0 for _ in pairs(mainObject_ref.main.modes['BUILD'].itemsTab.items) do n = n + 1 end return n")
        .eval()
        .expect("count failed");
    let copy_row = items
        .iter()
        .find(|r| r.can_copy && !r.is_jewel)
        .expect("a copyable item row");
    compare::copy_item(
        lua,
        1,
        copy_row.copy_slot.as_deref().unwrap_or(&copy_row.slot),
        false,
    )
    .expect("copy item failed");
    let item_count_after: usize = lua
        .load("local n = 0 for _ in pairs(mainObject_ref.main.modes['BUILD'].itemsTab.items) do n = n + 1 end return n")
        .eval()
        .expect("count failed");
    assert_eq!(item_count_after, item_count_before + 1, "item copied over");
    let specs_before: usize = lua
        .load("return #mainObject_ref.main.modes['BUILD'].treeTab.specList")
        .eval()
        .expect("spec count failed");
    compare::copy_spec(lua, false).expect("copy spec failed");
    let specs_after: usize = lua
        .load("return #mainObject_ref.main.modes['BUILD'].treeTab.specList")
        .eval()
        .expect("spec count failed");
    assert_eq!(specs_after, specs_before + 1, "spec copied over");
    let copied_title: String = lua
        .load("local l = mainObject_ref.main.modes['BUILD'].treeTab.specList return l[#l].title or ''")
        .eval()
        .expect("title failed");
    assert!(
        copied_title.contains("(Compared)"),
        "copied spec is titled: {copied_title}"
    );
    compare::copy_config(lua).expect("copy config failed");

    // Power report: driving completes; a mirror build yields no impact rows
    let ehp_index: usize = pob_egui::data::node_power::list_power_stats(lua)
        .expect("stats failed")
        .into_iter()
        .find(|s| s.label.contains("Effective Hit Pool"))
        .expect("EHP stat")
        .index;
    compare::power_set_stat(lua, ehp_index, [true; 5]).expect("set stat failed");
    let mut steps = 0;
    loop {
        let (done, _progress) = compare::power_step(lua, 1).expect("power step failed");
        if done {
            break;
        }
        steps += 1;
        assert!(steps < 10_000, "power report did not terminate");
    }
    let results = compare::power_results(lua).expect("results failed");
    assert!(
        results.iter().all(|r| r.impact != 0.0),
        "zero-impact rows are excluded"
    );

    // Calcs comparison: the level change makes Life-related rows differ, so
    // the only-differences filter keeps something; without the filter the
    // full section set is larger
    let diff_sections = compare::calc_sections(lua, 1, true).expect("calc sections failed");
    assert!(
        !diff_sections.is_empty(),
        "level change produces calc differences"
    );
    let all_sections = compare::calc_sections(lua, 1, false).expect("calc sections failed");
    let count_rows = |sections: &[pob_egui::data::compare::CalcCompareSection]| -> usize {
        sections
            .iter()
            .flat_map(|s| &s.subsections)
            .map(|s| s.rows.len())
            .sum()
    };
    assert!(
        count_rows(&all_sections) > count_rows(&diff_sections),
        "filter reduces rows: {} vs {}",
        count_rows(&all_sections),
        count_rows(&diff_sections)
    );
    // Kept rows differ in value, mods, or breakdown; at least some show a
    // visible value difference (rows kept for mod/breakdown-only diffs may
    // display identical values, matching upstream)
    assert!(
        diff_sections
            .iter()
            .flat_map(|s| &s.subsections)
            .flat_map(|s| &s.rows)
            .any(|r| r.primary != r.compare),
        "some kept rows show a visible difference"
    );

    // Removing the entry empties the list
    compare::remove_entry(lua, 1).expect("remove failed");
    let (entries, active) = compare::list_entries(lua).expect("list failed");
    assert!(entries.is_empty(), "entry removed");
    assert_eq!(active, 0, "no active entry");
}

#[test]
fn test_foulborn_mutate_and_mod_ranges() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Add Abyssus (a data.foulbornMap unique) from upstream's uniques data
    let raw: String = lua
        .load(
            r#"
        for _, raw in ipairs(data.uniques.helmet) do
            if raw:match("^Abyssus") then
                return "Rarity: UNIQUE\n" .. raw
            end
        end
        return ""
    "#,
        )
        .eval()
        .expect("uniques lookup failed");
    assert!(!raw.is_empty(), "Abyssus exists in data.uniques");
    let err = items::add_item_from_raw(lua, &raw).expect("add failed");
    assert!(err.is_none(), "Abyssus should parse: {err:?}");
    let abyssus_id = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.name.contains("Abyssus"))
        .map(|e| e.id)
        .expect("Abyssus in item list");

    // The range list has rows; at least one is Foulborn-mappable and at
    // least one is scalable
    let rows = crafting::mod_range_lines(lua, abyssus_id).expect("ranges failed");
    assert!(!rows.is_empty(), "Abyssus has range lines");
    let mutable = rows
        .iter()
        .find(|r| r.can_mutate)
        .expect("Abyssus has a foulborn-mappable mod");
    assert!(!mutable.mutated, "starts unmutated");
    let scalable = rows
        .iter()
        .find(|r| r.has_slider)
        .expect("Abyssus has a scalable mod");

    // Mutating swaps the line, marks the raw {mutated}, and prefixes the title
    crafting::mutate_mod(lua, abyssus_id, mutable.position).expect("mutate failed");
    let raw = items::get_item_raw(lua, abyssus_id).expect("raw failed");
    assert!(raw.contains("{mutated}"), "mutated flag persists: {raw}");
    assert!(
        raw.contains("Foulborn Abyssus"),
        "title gains the Foulborn prefix: {raw}"
    );
    let rows = crafting::mod_range_lines(lua, abyssus_id).expect("ranges failed");
    let mutated_row = rows
        .iter()
        .find(|r| r.mutated)
        .expect("a mutated row after MutateMod");
    assert!(mutated_row.can_mutate, "mutated row can be reverted");

    // Toggling again reverts (upstream matches both directions)
    crafting::mutate_mod(lua, abyssus_id, mutated_row.position).expect("revert failed");
    let raw = items::get_item_raw(lua, abyssus_id).expect("raw failed");
    assert!(!raw.contains("{mutated}"), "mutation reverted: {raw}");
    assert!(!raw.contains("Foulborn"), "title prefix removed: {raw}");

    // The roll slider persists a {range} marker
    crafting::set_mod_range(lua, abyssus_id, scalable.position, 1.0).expect("range failed");
    let raw = items::get_item_raw(lua, abyssus_id).expect("raw failed");
    assert!(raw.contains("{range:1}"), "range roll persists: {raw}");
}

#[test]
fn test_volatile_vaal_roll_ranges() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Find a unique with scalable explicits (jewels like Forbidden Flesh
    // have none, so scan rather than take the first)
    let uniques: Vec<i64> = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .filter(|e| e.rarity == "UNIQUE")
        .map(|e| e.id)
        .collect();
    let (unique_id, rolls) = uniques
        .iter()
        .find_map(|&id| {
            let rolls = crafting::corrupt_roll_ranges(lua, id).ok()?;
            (!rolls.is_empty()).then_some((id, rolls))
        })
        .expect("a unique with scalable mods for roll ranges");
    assert!(
        rolls.iter().all(|r| (r.current - 1.0).abs() < f64::EPSILON),
        "uncorrupted mods default to 1.00"
    );

    // Preview responds to the multiplier
    let first = &rolls[0];
    let low = crafting::preview_roll_range(lua, unique_id, first.index, 0.78).expect("preview");
    let high = crafting::preview_roll_range(lua, unique_id, first.index, 1.22).expect("preview");
    assert_ne!(low, high, "preview scales with corruptedRange: {low}");

    // Rares are not eligible (upstream gates on UNIQUE/RELIC)
    let rare_id = crafting::craft_item(lua, "RARE", "Helmet: Armour", 1, "No Rolls")
        .expect("craft failed")
        .expect("id");
    assert!(
        crafting::corrupt_roll_ranges(lua, rare_id)
            .expect("rolls failed")
            .is_empty(),
        "rares have no roll-range mode"
    );

    // Applying persists {corruptedRange} on the line and corrupts the item
    crafting::corrupt_item_rolls(lua, unique_id, &[first.index], &[1.22]).expect("corrupt failed");
    let raw = items::get_item_raw(lua, unique_id).expect("raw failed");
    assert!(raw.contains("Corrupted"), "item is corrupted: {raw}");
    assert!(
        raw.contains("{corruptedRange:1.22}"),
        "corruptedRange persists in raw: {raw}"
    );
    let rolls = crafting::corrupt_roll_ranges(lua, unique_id).expect("rolls failed");
    let row = rolls
        .iter()
        .find(|r| r.index == first.index)
        .expect("row survives");
    assert!(
        (row.current - 1.22).abs() < 1e-9,
        "corruptedRange round-trips: {}",
        row.current
    );
}

#[test]
fn test_spectre_library() {
    use pob_egui::data::spectres;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Available list: everything in data.spectres, name-sorted like upstream
    let available = spectres::list_available(lua).expect("list failed");
    assert!(
        available.len() > 100,
        "many spectres exist, got {}",
        available.len()
    );
    assert!(
        available.windows(2).all(|w| w[0].name <= w[1].name),
        "available spectres sorted by name"
    );
    assert!(
        available.iter().any(|e| !e.skills.is_empty()),
        "spectres carry skill names for search"
    );

    // The test build has no spectres; staging two and committing lands them
    // on build.spectreList in order
    assert!(
        spectres::list_in_build(lua)
            .expect("list failed")
            .is_empty(),
        "test build starts without spectres"
    );
    let picks: Vec<String> = available.iter().take(2).map(|e| e.id.clone()).collect();
    spectres::set_spectre_list(lua, &picks).expect("set failed");
    let in_build = spectres::list_in_build(lua).expect("list failed");
    let ids: Vec<String> = in_build.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(ids, picks, "committed spectre list round-trips");

    // Tooltip comes from upstream MinionListControl:AddValueTooltip
    let tooltip = spectres::spectre_tooltip(lua, &picks[0]).expect("tooltip failed");
    let text: String = tooltip
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Life Multiplier"),
        "tooltip has minion stats: {text}"
    );

    // Persistence: upstream Save writes one <Spectre id=.../> per entry
    let tmp_dir =
        std::env::temp_dir().join(format!("egui-pob-spectre-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let mut build_path = tmp_dir.to_string_lossy().to_string();
    build_path.push('/');
    lua.load("mainObject_ref.main.buildPath = ...")
        .call::<()>(build_path.as_str())
        .expect("failed to redirect buildPath");
    bridge
        .save_build_as("Spectre Test", "")
        .expect("save failed");
    let xml = std::fs::read_to_string(tmp_dir.join("Spectre Test.xml")).expect("read failed");
    for id in &picks {
        assert!(
            xml.contains(&format!("id=\"{id}\"")),
            "saved XML contains spectre {id}"
        );
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_gem_tooltip_lines() {
    use pob_egui::data::skills;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| !g.gems.is_empty())
        .expect("a group with gems");
    let gem_name = &group.gems[0].name;

    let lines = skills::gem_tooltip_lines(lua, group.index, 1).expect("tooltip failed");
    assert!(
        lines.len() > 5,
        "gem tooltip has content, got {} lines",
        lines.len()
    );
    let text: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let first_word = gem_name.split(' ').next().unwrap_or("");
    assert!(
        text.contains(first_word),
        "tooltip should mention '{gem_name}': {text}"
    );

    // Out-of-range indices return no lines instead of erroring
    let empty = skills::gem_tooltip_lines(lua, 999, 1).expect("tooltip failed");
    assert!(empty.is_empty(), "missing gem yields no lines");
}

#[test]
fn test_add_modifier_catalog() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Crafted rare helmet: bench/essence/veiled/beastcraft/necropolis/delve
    // sources, but no Prefix/Suffix (crafted items use the affix editor)
    let helm_id = crafting::craft_item(lua, "RARE", "Helmet: Armour", 1, "Catalog Helm")
        .expect("craft failed")
        .expect("id");
    let sources = crafting::mod_catalog_sources(lua, helm_id).expect("sources failed");
    let ids: Vec<&str> = sources.iter().map(|(_, id)| id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "MASTER",
            "ESSENCE",
            "VEILED",
            "BEASTCRAFT",
            "NECROPOLIS",
            "DELVE",
            "CUSTOM"
        ],
        "crafted helmet sources"
    );

    // Bench catalog exists for helmets
    let bench = crafting::mod_catalog(lua, helm_id, "MASTER").expect("catalog failed");
    assert!(bench.len() > 10, "bench mods for a helmet: {}", bench.len());

    // Essence catalog: one entry per essence, labeled with the essence name
    let essences = crafting::mod_catalog(lua, helm_id, "ESSENCE").expect("catalog failed");
    assert!(essences.len() > 50, "essence mods: {}", essences.len());
    assert!(
        essences[0].label.contains("Essence"),
        "essence label: {}",
        essences[0].label
    );
    assert!(!essences[0].lines.is_empty(), "essence entry has mod lines");

    // Popup sorting: one value per entry; life essences move the Life stat
    // while mana/resistance essences do not, so values spread
    let life = pob_egui::data::node_power::list_power_stats(lua)
        .expect("stats failed")
        .into_iter()
        .find(|s| s.label == "Life")
        .expect("a Life power stat");
    let values = crafting::mod_catalog_sort_values(lua, helm_id, "ESSENCE", life.index)
        .expect("sort values failed");
    assert_eq!(
        values.len(),
        essences.len(),
        "one sort value per essence entry"
    );
    let (min, max) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(
        max > min,
        "life essences should spread Life values: {min}..{max}"
    );

    // Applying an entry lands its lines on the item as explicit mods
    let entry = essences[0].clone();
    crafting::apply_catalog_mod(lua, helm_id, "ESSENCE", 1).expect("apply failed");
    let raw = items::get_item_raw(lua, helm_id).expect("raw failed");
    let line = &entry.lines[0];
    // Compare ignoring rolled numbers: match the alpha part of the line
    let pattern: String = line
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .collect();
    let pattern = pattern.split_whitespace().collect::<Vec<_>>().join(" ");
    let raw_normalized: String = raw
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .collect();
    let raw_normalized = raw_normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        raw_normalized.contains(&pattern),
        "essence mod '{line}' should be on the item: {raw}"
    );

    // An uncrafted rare from the build exposes Prefix/Suffix sources with
    // spawn-weight-filtered mod pools
    let uncrafted = items::extract_item_list(lua)
        .expect("list failed")
        .iter()
        .find(|e| e.rarity == "RARE" && !e.crafted)
        .map(|e| e.id);
    if let Some(id) = uncrafted {
        let sources = crafting::mod_catalog_sources(lua, id).expect("sources failed");
        let ids: Vec<&str> = sources.iter().map(|(_, id)| id.as_str()).collect();
        assert!(
            ids.contains(&"PREFIX") && ids.contains(&"SUFFIX"),
            "uncrafted rare offers prefix/suffix sources: {ids:?}"
        );
        let prefixes = crafting::mod_catalog(lua, id, "PREFIX").expect("catalog failed");
        assert!(!prefixes.is_empty(), "prefix pool for an uncrafted rare");
    }
}

// ---------------------------------------------------------------------------
// Socket editing and catalysts
// ---------------------------------------------------------------------------

#[test]
fn test_sockets_and_catalysts() {
    use pob_egui::data::{crafting, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let body_id = crafting::craft_item(lua, "RARE", "Body Armour: Armour", 1, "Socket Test")
        .expect("craft failed")
        .expect("id");

    let sockets = crafting::item_sockets(lua, body_id)
        .expect("sockets failed")
        .expect("body armour has sockets");
    assert_eq!(sockets.selectable_count, 6, "body armour allows 6 sockets");
    let initial = sockets.sockets.len();

    // Add sockets to the cap
    while crafting::item_sockets(lua, body_id)
        .expect("sockets failed")
        .expect("sockets")
        .sockets
        .len()
        < 6
    {
        crafting::add_socket(lua, body_id).expect("add socket failed");
    }
    let sockets = crafting::item_sockets(lua, body_id)
        .expect("sockets failed")
        .expect("sockets");
    assert_eq!(sockets.sockets.len(), 6);
    assert!(initial <= 6);

    // Color a socket white and verify in the raw text
    crafting::set_socket_color(lua, body_id, 1, "W").expect("color failed");
    let raw = items::get_item_raw(lua, body_id).expect("raw failed");
    assert!(
        raw.contains("Sockets: W") || raw.contains("W-") || raw.contains("-W"),
        "white socket in raw: {raw}"
    );

    // Link the first two sockets, then unlink
    crafting::set_socket_link(lua, body_id, 1, true).expect("link failed");
    let sockets = crafting::item_sockets(lua, body_id)
        .expect("sockets failed")
        .expect("sockets");
    assert_eq!(
        sockets.sockets[0].1, sockets.sockets[1].1,
        "linked sockets share a group"
    );
    crafting::set_socket_link(lua, body_id, 1, false).expect("unlink failed");
    let sockets = crafting::item_sockets(lua, body_id)
        .expect("sockets failed")
        .expect("sockets");
    assert_ne!(
        sockets.sockets[0].1, sockets.sockets[1].1,
        "unlinked sockets have different groups"
    );

    // Catalysts: not applicable to body armour, applicable to a crafted ring
    assert!(
        crafting::catalyst_info(lua, body_id)
            .expect("info failed")
            .is_none(),
        "no catalysts on body armour"
    );
    let ring_id = crafting::craft_item(lua, "RARE", "Ring", 1, "Catalyst Ring")
        .expect("craft failed")
        .expect("id");
    let (catalyst, quality) = crafting::catalyst_info(lua, ring_id)
        .expect("info failed")
        .expect("rings can take catalysts");
    assert_eq!((catalyst, quality), (0, 20));
    crafting::set_catalyst(lua, ring_id, 3, 20).expect("set failed");
    let (catalyst, _) = crafting::catalyst_info(lua, ring_id)
        .expect("info failed")
        .expect("still applicable");
    assert_eq!(catalyst, 3, "catalyst round-trips");
    let raw = items::get_item_raw(lua, ring_id).expect("raw failed");
    assert!(raw.contains("Catalyst"), "catalyst recorded in raw: {raw}");
}

// ---------------------------------------------------------------------------
// Gem options: defaults and round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_gem_options() {
    use pob_egui::data::skills::{self, GemOptions};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let opts = skills::gem_options(lua).expect("options failed");
    assert_eq!(opts.default_level, "normalMaximum");

    // Set defaults and confirm a newly added gem uses them
    skills::set_gem_options(
        lua,
        &GemOptions {
            sort_by_dps: true,
            sort_field: "TotalDPS".to_string(),
            default_level: "levelOne".to_string(),
            default_quality: 17,
            show_support_types: "ALL".to_string(),
            show_legacy_gems: false,
        },
    )
    .expect("set failed");
    let opts = skills::gem_options(lua).expect("options failed");
    assert_eq!(opts.default_level, "levelOne");
    assert_eq!(opts.default_quality, 17);
    assert_eq!(opts.sort_field, "TotalDPS");

    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| !g.from_item)
        .expect("a user socket group");
    let err = skills::add_gem(lua, group.index, "Herald of Ash").expect("add failed");
    assert!(err.is_none(), "gem should be found: {err:?}");
    let groups = skills::extract_skills(lua).expect("skills failed");
    let gem = groups
        .iter()
        .find(|g| g.index == group.index)
        .unwrap()
        .gems
        .iter()
        .find(|g| g.name.contains("Herald of Ash"))
        .expect("added gem");
    assert_eq!(gem.level, 1, "levelOne default applies");
    assert_eq!(gem.quality, 17, "default quality applies");

    // Normal maximum default resolves the natural max level
    skills::set_gem_options(
        lua,
        &GemOptions {
            default_level: "normalMaximum".to_string(),
            default_quality: 0,
            ..opts
        },
    )
    .expect("set failed");
    let err = skills::add_gem(lua, group.index, "Clarity").expect("add failed");
    assert!(err.is_none(), "gem should be found: {err:?}");
    let groups = skills::extract_skills(lua).expect("skills failed");
    let gem = groups
        .iter()
        .find(|g| g.index == group.index)
        .unwrap()
        .gems
        .iter()
        .find(|g| g.name.contains("Clarity"))
        .expect("added gem");
    assert!(
        gem.level >= 20,
        "clarity max level is 20+, got {}",
        gem.level
    );
}

// ---------------------------------------------------------------------------
// Tattoos: options, apply, count, remove
// ---------------------------------------------------------------------------

#[test]
fn test_tattoos() {
    use pob_egui::data::{tattoos, tree::TreeData};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Find a small attribute node (Strength/Dexterity/Intelligence)
    let node_id: u32 = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            for id, node in pairs(spec.nodes) do
                if node.dn == "Strength" and node.type == "Normal" and not node.ascendancyName then
                    return id
                end
            end
            return 0
            "#,
        )
        .eval()
        .expect("eval failed");
    assert!(node_id > 0, "found a Strength node");

    let options = tattoos::tattoo_options(lua, node_id, false).expect("options failed");
    assert!(
        options.len() > 5,
        "attribute nodes have many tattoo options, got {}",
        options.len()
    );
    let warrior = options
        .iter()
        .find(|o| o.name.contains("Tattoo"))
        .expect("a tattoo option");
    assert!(!warrior.descriptions.is_empty());

    // Applying replaces the node's name/stats in the extracted tree
    assert!(!tattoos::is_tattooed(lua, node_id).expect("check failed"));
    tattoos::apply_tattoo(lua, node_id, &warrior.id).expect("apply failed");
    assert!(tattoos::is_tattooed(lua, node_id).expect("check failed"));
    assert_eq!(tattoos::tattoo_count(lua).expect("count failed"), 1);
    let tree = TreeData::extract(lua).expect("extract failed");
    let node = tree.nodes.get(&node_id).expect("node still in tree");
    assert_eq!(node.name, warrior.name, "node renamed to the tattoo");

    // Removing restores the original node
    tattoos::remove_tattoo(lua, node_id).expect("remove failed");
    assert!(!tattoos::is_tattooed(lua, node_id).expect("check failed"));
    assert_eq!(tattoos::tattoo_count(lua).expect("count failed"), 0);
    let tree = TreeData::extract(lua).expect("extract failed");
    let node = tree.nodes.get(&node_id).expect("node still in tree");
    assert_eq!(node.name, "Strength", "node restored");

    // Legacy toggle expands the pool (or at least never shrinks it)
    let with_legacy = tattoos::tattoo_options(lua, node_id, true).expect("options failed");
    assert!(with_legacy.len() >= options.len());

    // A keystone node yields keystone-targeted options only (or none)
    let keystone_id: u32 = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            for id, node in pairs(spec.nodes) do
                if node.type == "Keystone" and not node.ascendancyName then
                    return id
                end
            end
            return 0
            "#,
        )
        .eval()
        .expect("eval failed");
    if keystone_id > 0 {
        let keystone_options =
            tattoos::tattoo_options(lua, keystone_id, false).expect("options failed");
        // Keystone tattoos exist (Karui runegraft-likes); list may be empty
        // for some keystones, but must never contain small-attribute tattoos
        assert!(
            !keystone_options
                .iter()
                .any(|o| o.name.contains("Tattoo of the Arohongui")),
            "keystones don't take small-node tattoos"
        );
    }
}

// ---------------------------------------------------------------------------
// Timeless jewel seed search
// ---------------------------------------------------------------------------

#[test]
fn test_timeless_search() {
    use pob_egui::data::{items, timeless};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let sockets = timeless::timeless_sockets(lua).expect("sockets failed");
    assert!(
        sockets.len() > 10,
        "the tree has many jewel sockets, got {}",
        sockets.len()
    );
    assert_eq!(
        sockets[0].node_id,
        timeless::ALL_SOCKETS_ID,
        "first entry is the All Sockets search"
    );
    let socket = &sockets[1];

    // Lethal Pride (karui): presence-weighted notable search
    let stats = timeless::timeless_stats(lua, "karui").expect("stats failed");
    assert!(
        stats.len() > 10,
        "karui legion stats exist, got {}",
        stats.len()
    );
    // Lethal Pride's searchable list is its notable additions ("Add X");
    // it has no small replacements after upstream's ignored-mod filter
    let mut main_stats = stats
        .iter()
        .filter(|s| s.is_notable && !s.id.starts_with("total_"));
    let notable = main_stats.next().expect("a karui addition");
    let notable2 = main_stats.next().expect("a second karui addition");
    let desired = vec![
        (notable.id.clone(), 10.0, 0.0),
        (notable2.id.clone(), 1.0, 0.0),
    ];
    let results = timeless::find_timeless_seeds(lua, 2, socket.node_id, &desired, &[], 10)
        .expect("search failed");
    assert!(
        !results.is_empty(),
        "some seed should match karui stats at socket {}",
        socket.label
    );
    assert!(
        results[0].weight >= results[results.len() - 1].weight,
        "sorted"
    );
    assert!(!results[0].matches.is_empty(), "matches described");
    let (min, max) = (10000, 18000);
    assert!(
        results.iter().all(|r| r.seed >= min && r.seed <= max),
        "karui seeds in range"
    );
    assert!(
        results.iter().all(|r| r.socket_id.is_none()),
        "single-socket results carry no socket id"
    );

    // Total Strength pseudo-stat: offered at the top of the karui list and
    // searchable (every seed scores via the small-node bonus)
    assert_eq!(stats[0].id, "total_strength", "total pseudo-stat first");
    let total_results = timeless::find_timeless_seeds(
        lua,
        2,
        socket.node_id,
        &[("total_strength".to_string(), 1.0, 0.0)],
        &[],
        5,
    )
    .expect("total search failed");
    assert!(
        !total_results.is_empty(),
        "total strength search returns seeds"
    );

    // Glorious Vanity: value-weighted search also returns results
    let gv_stats = timeless::timeless_stats(lua, "vaal").expect("stats failed");
    let gv_first = &gv_stats[0];
    let gv_results = timeless::find_timeless_seeds(
        lua,
        1,
        socket.node_id,
        &[(gv_first.id.clone(), 1.0, 0.5)],
        &[],
        5,
    )
    .expect("search failed");
    // GV transforms everything; a single desired stat may or may not appear,
    // but the search must complete without error
    let _ = gv_results;

    // Fallback weights: generate for a few karui stats against a defensive
    // power stat (Total Strength raises life, so some weight is non-zero)
    let fallback_stats = timeless::list_fallback_stats(lua).expect("fallback stats failed");
    assert!(!fallback_stats.is_empty(), "fallback power stats exist");
    let stat_index = fallback_stats
        .iter()
        .find(|s| s.label.contains("EHP"))
        .map(|s| s.index)
        .unwrap_or(fallback_stats[0].index);
    let ids: Vec<String> = stats.iter().take(8).map(|s| s.id.clone()).collect();
    let weights =
        timeless::generate_fallback_weights(lua, &ids, stat_index).expect("generate failed");
    assert!(
        !weights.is_empty(),
        "some karui stat should move the selected power stat"
    );
    assert!(
        weights.iter().all(|w| w.weight1 != 0.0 || w.weight2 != 0.0),
        "zero-weight rows are dropped"
    );

    // Fallback rows merge into the search for ids not already desired
    let fallback: Vec<(String, f64, f64)> = weights
        .iter()
        .map(|w| (w.id.clone(), w.weight1, w.weight2))
        .collect();
    let merged_results = timeless::find_timeless_seeds(lua, 2, socket.node_id, &[], &fallback, 5)
        .expect("fallback search failed");
    let _ = merged_results;

    // All Sockets search tags every result with the socket it was found at
    let all_results =
        timeless::find_timeless_seeds(lua, 2, timeless::ALL_SOCKETS_ID, &desired, &[], 10)
            .expect("all-sockets search failed");
    assert!(!all_results.is_empty(), "all-sockets search returns seeds");
    assert!(
        all_results.iter().all(|r| r.socket_id.is_some()),
        "all-sockets results carry socket ids"
    );
    assert!(
        all_results[0].weight >= results[0].weight,
        "best all-sockets result at least matches the single-socket best"
    );

    // Creating the jewel from a result adds it to the build
    let err = timeless::create_timeless_jewel(lua, 2, 0, results[0].seed).expect("create failed");
    assert!(err.is_none(), "timeless jewel should parse: {err:?}");
    let list = items::extract_item_list(lua).expect("list failed");
    assert!(
        list.iter().any(|e| e.name.contains("Lethal Pride")),
        "jewel in build"
    );
}

#[test]
fn test_ascendancy_click_switching() {
    use pob_egui::data::tree::{self, NodeClickOutcome};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Find a node of a different ascendancy within the current class, and a
    // node of an ascendancy belonging to a different class
    let (same_class_node, same_class_ascend, cross_class_node, cross_class_name): (
        u32,
        String,
        u32,
        String,
    ) = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            local sameNode, sameAscend, crossNode, crossClass
            for _, ascendClass in pairs(spec.curClass.classes) do
                if ascendClass.id and ascendClass.id ~= spec.curAscendClassBaseName then
                    for nodeId, node in pairs(spec.nodes) do
                        if node.ascendancyName == ascendClass.id and not node.isBloodline
                           and node.type ~= "AscendClassStart" then
                            sameNode, sameAscend = nodeId, ascendClass.id
                            break
                        end
                    end
                end
                if sameNode then break end
            end
            for classId, classData in pairs(spec.tree.classes) do
                if classId ~= spec.curClassId then
                    for _, ascendClass in pairs(classData.classes) do
                        for nodeId, node in pairs(spec.nodes) do
                            if ascendClass.id and node.ascendancyName == ascendClass.id
                               and not node.isBloodline
                               and node.type ~= "AscendClassStart" then
                                crossNode, crossClass = nodeId, classData.name
                                break
                            end
                        end
                        if crossNode then break end
                    end
                end
                if crossNode then break end
            end
            return sameNode, sameAscend, crossNode, crossClass
        "#,
        )
        .eval()
        .expect("failed to find ascendancy nodes");

    // Same-class switching happens immediately
    let outcome = tree::click_node(lua, same_class_node).expect("click failed");
    assert_eq!(outcome, NodeClickOutcome::Switched, "same-class switch");
    let cur_ascend: String = lua
        .load("return mainObject_ref.main.modes['BUILD'].spec.curAscendClassBaseName or ''")
        .eval()
        .expect("failed to read ascendancy");
    assert_eq!(cur_ascend, same_class_ascend, "ascendancy switched");

    // Cross-class switching with points allocated and no connection asks for
    // confirmation
    let outcome = tree::click_node(lua, cross_class_node).expect("click failed");
    assert_eq!(
        outcome,
        NodeClickOutcome::NeedsConfirm {
            class_name: cross_class_name.clone()
        },
        "cross-class switch needs confirmation"
    );

    // Confirming with reset switches the class and allocates the clicked node
    let done = tree::confirm_class_switch(lua, cross_class_node, false).expect("confirm failed");
    assert!(done, "confirmed switch succeeds");
    let (class_name, node_alloc): (String, bool) = lua
        .load(format!(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            return spec.curClass.name, spec.allocNodes[{cross_class_node}] ~= nil
        "#
        ))
        .eval()
        .expect("failed to read class");
    assert_eq!(class_name, cross_class_name, "class switched");
    assert!(node_alloc, "clicked ascendancy node allocated");

    // Clicking an allocated node deallocates it (normal toggle path)
    let outcome = tree::click_node(lua, cross_class_node).expect("click failed");
    assert_eq!(outcome, NodeClickOutcome::Toggled, "dealloc is a toggle");
    let node_alloc: bool = lua
        .load(format!(
            "return mainObject_ref.main.modes['BUILD'].spec.allocNodes[{cross_class_node}] ~= nil"
        ))
        .eval()
        .expect("failed to read alloc");
    assert!(!node_alloc, "node deallocated");
}

// ---------------------------------------------------------------------------
// Items and config undo/redo (upstream UndoHandler)
// ---------------------------------------------------------------------------

#[test]
fn test_items_config_undo_redo() {
    use pob_egui::data::{config, items};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Items: adding an item is undoable
    let before = items::extract_item_list(lua).expect("list failed").len();
    let raw = "Rarity: RARE\nUndo Test Ring\nRuby Ring\n+50 to maximum Life";
    let err = items::add_item_from_raw(lua, raw).expect("add failed");
    assert!(err.is_none(), "item should parse: {err:?}");
    let after = items::extract_item_list(lua).expect("list failed").len();
    assert_eq!(after, before + 1, "item added");

    items::undo(lua).expect("undo failed");
    let count = items::extract_item_list(lua).expect("list failed").len();
    assert_eq!(count, before, "undo removes the added item");

    items::redo(lua).expect("redo failed");
    let count = items::extract_item_list(lua).expect("list failed").len();
    assert_eq!(count, before + 1, "redo restores the added item");

    // Equipping is undoable too: the ring lands in a ring slot on add (it
    // auto-equips); unequip it, then undo brings it back
    let equipped_slot: String = lua
        .load(
            r#"
            local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
            for slotName, slot in pairs(itemsTab.slots) do
                local item = itemsTab.items[slot.selItemId or 0]
                if item and item.name == "Undo Test Ring" then
                    return slotName
                end
            end
            return ""
        "#,
        )
        .eval()
        .expect("slot scan failed");
    if !equipped_slot.is_empty() {
        items::equip_item(lua, &equipped_slot, 0).expect("unequip failed");
        let empty: bool = lua
            .load(format!(
                r#"
                local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
                return itemsTab.slots["{equipped_slot}"].selItemId == 0
            "#
            ))
            .eval()
            .expect("read failed");
        assert!(empty, "slot emptied");
        items::undo(lua).expect("undo failed");
        let refilled: bool = lua
            .load(format!(
                r#"
                local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
                local item = itemsTab.items[itemsTab.slots["{equipped_slot}"].selItemId or 0]
                return item ~= nil and item.name == "Undo Test Ring"
            "#
            ))
            .eval()
            .expect("read failed");
        assert!(refilled, "undo re-equips the ring");
    }

    // Config: value changes are undoable
    let read_level = r#"
        local configTab = mainObject_ref.main.modes['BUILD'].configTab
        return configTab.input["enemyLevel"] or 0
    "#;
    let original: i64 = lua.load(read_level).eval().expect("read failed");
    assert_ne!(original, 42, "test premise: enemyLevel is not 42");
    config::set_config_value(lua, "enemyLevel", mlua::Value::Number(42.0)).expect("set failed");
    let set: i64 = lua.load(read_level).eval().expect("read failed");
    assert_eq!(set, 42, "config value set");

    config::undo(lua).expect("undo failed");
    let undone: i64 = lua.load(read_level).eval().expect("read failed");
    assert_eq!(undone, original, "undo restores the old value");

    config::redo(lua).expect("redo failed");
    let redone: i64 = lua.load(read_level).eval().expect("read failed");
    assert_eq!(redone, 42, "redo restores the new value");
}

#[test]
fn test_move_item_between_slots() {
    use pob_egui::data::items;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Add two rings and pin them to known slots
    for raw in [
        "Rarity: RARE\nSwap Ring A\nRuby Ring\n+50 to maximum Life",
        "Rarity: RARE\nSwap Ring B\nTopaz Ring\n+40% to Lightning Resistance",
    ] {
        let err = items::add_item_from_raw(lua, raw).expect("add failed");
        assert!(err.is_none(), "ring should parse: {err:?}");
    }
    // List names are "Name, BaseName"
    let list = items::extract_item_list(lua).expect("list failed");
    let id_a = list
        .iter()
        .find(|e| e.name.starts_with("Swap Ring A"))
        .expect("A")
        .id;
    let id_b = list
        .iter()
        .find(|e| e.name.starts_with("Swap Ring B"))
        .expect("B")
        .id;
    items::equip_item(lua, "Ring 1", id_a).expect("equip A failed");
    items::equip_item(lua, "Ring 2", id_b).expect("equip B failed");

    let read_slots = r#"
        local itemsTab = mainObject_ref.main.modes['BUILD'].itemsTab
        return itemsTab.slots["Ring 1"].selItemId, itemsTab.slots["Ring 2"].selItemId
    "#;

    // Move A from Ring 1 onto Ring 2: the rings swap
    let moved = items::move_item_between_slots(lua, id_a, "Ring 1", "Ring 2").expect("move failed");
    assert!(moved, "ring is valid for the other ring slot");
    let (ring1, ring2): (i64, i64) = lua.load(read_slots).eval().expect("read failed");
    assert_eq!(ring2, id_a, "dragged ring lands in the target slot");
    assert_eq!(ring1, id_b, "displaced ring swaps back to the source slot");

    // A ring is not valid for a weapon slot: no-op
    let moved =
        items::move_item_between_slots(lua, id_a, "Ring 2", "Weapon 1").expect("move failed");
    assert!(!moved, "ring cannot move to a weapon slot");
    let (ring1, ring2): (i64, i64) = lua.load(read_slots).eval().expect("read failed");
    assert_eq!(
        (ring1, ring2),
        (id_b, id_a),
        "slots unchanged after invalid move"
    );

    // The move is undoable as a single step
    items::undo(lua).expect("undo failed");
    let (ring1, ring2): (i64, i64) = lua.load(read_slots).eval().expect("read failed");
    assert_eq!(
        (ring1, ring2),
        (id_a, id_b),
        "undo restores the pre-swap slots"
    );
}

#[test]
fn test_skills_drag_reorder() {
    use pob_egui::data::skills;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let groups = skills::extract_skills(lua).expect("skills failed");
    assert!(groups.len() >= 2, "test build has several socket groups");
    let main_before: usize = lua
        .load("return mainObject_ref.main.modes['BUILD'].mainSocketGroup")
        .eval()
        .expect("read failed");

    // Move the first group to position 2: the two swap, and the main group
    // index follows the move (upstream OnOrderChange)
    let first_title = groups[0].gems.first().map(|g| g.name.clone());
    skills::move_socket_group(lua, 1, 2).expect("move failed");
    let moved = skills::extract_skills(lua).expect("skills failed");
    assert_eq!(
        moved[1].gems.first().map(|g| g.name.clone()),
        first_title,
        "group moved to position 2"
    );
    let main_after: usize = lua
        .load("return mainObject_ref.main.modes['BUILD'].mainSocketGroup")
        .eval()
        .expect("read failed");
    let expected = match main_before {
        1 => 2,
        2 => 1,
        other => other,
    };
    assert_eq!(main_after, expected, "main socket group follows the move");

    // Move it back
    skills::move_socket_group(lua, 2, 1).expect("move failed");
    let restored = skills::extract_skills(lua).expect("skills failed");
    assert_eq!(
        restored[0].gems.first().map(|g| g.name.clone()),
        first_title,
        "group moved back"
    );

    // Gem reorder within a group
    let group = restored
        .iter()
        .find(|g| g.gems.len() >= 2)
        .expect("a group with two gems");
    let names: Vec<String> = group.gems.iter().map(|g| g.name.clone()).collect();
    skills::move_gem(lua, group.index, 1, 2).expect("move gem failed");
    let after = skills::extract_skills(lua).expect("skills failed");
    let group_after = after
        .iter()
        .find(|g| g.index == group.index)
        .expect("group still there");
    assert_eq!(group_after.gems[0].name, names[1], "gems swapped");
    assert_eq!(group_after.gems[1].name, names[0], "gems swapped");

    // Out-of-range moves are no-ops
    skills::move_gem(lua, group.index, 1, 99).expect("call failed");
    let unchanged = skills::extract_skills(lua).expect("skills failed");
    let group_unchanged = unchanged
        .iter()
        .find(|g| g.index == group.index)
        .expect("group still there");
    assert_eq!(
        group_unchanged.gems[0].name, names[1],
        "invalid move changes nothing"
    );
}

// ---------------------------------------------------------------------------
// Build XML round-trip fidelity
// ---------------------------------------------------------------------------

#[test]
fn test_build_xml_roundtrip() {
    let _ = env_logger::builder().is_test(true).try_init();
    for fixture in common::ALL_FIXTURES {
        roundtrip_one_build(fixture);
    }
}

/// Load a fixture, save it, reload the save, and assert the build is a fixed
/// point both structurally and in its calculated output.
fn roundtrip_one_build(fixture: &str) {
    let bridge = common::boot_and_load_build(fixture);
    let lua = bridge.lua();

    let save_db = r#"
        return mainObject_ref.main.modes['BUILD']:SaveDB("roundtrip")
    "#;
    let read_stats = r#"
        local build = mainObject_ref.main.modes['BUILD']
        local o = build.calcsTab.mainOutput or {}
        local spec = build.spec
        local allocCount = 0
        for _ in pairs(spec.allocNodes) do allocCount = allocCount + 1 end
        local itemCount = 0
        for _ in pairs(build.itemsTab.items) do itemCount = itemCount + 1 end
        return o.CombinedDPS or 0, o.Life or 0, o.TotalEHP or 0,
            allocCount, itemCount, #build.skillsTab.socketGroupList
    "#;

    // Saving twice without changes must be deterministic
    let save1: String = lua.load(save_db).eval().expect("first save failed");
    assert!(
        save1.starts_with("<?xml") && save1.contains("<Build"),
        "{fixture}: save produces XML"
    );
    let save1b: String = lua.load(save_db).eval().expect("second save failed");
    assert_eq!(save1, save1b, "{fixture}: saving twice is deterministic");

    let stats1: (f64, f64, f64, i64, i64, i64) =
        lua.load(read_stats).eval().expect("stats read failed");

    // Reload from the saved XML, then save again: the XML must be a fixed
    // point, and the calculated build must be identical
    bridge
        .load_build_from_xml(&save1, "Roundtrip", None)
        .expect("reload failed");
    let save2: String = lua.load(save_db).eval().expect("save after reload failed");
    let stats2: (f64, f64, f64, i64, i64, i64) =
        lua.load(read_stats).eval().expect("stats read failed");

    assert_eq!(
        stats1.3, stats2.3,
        "{fixture}: allocated node count survives the round-trip"
    );
    assert_eq!(
        stats1.4, stats2.4,
        "{fixture}: item count survives the round-trip"
    );
    assert_eq!(
        stats1.5, stats2.5,
        "{fixture}: socket group count survives the round-trip"
    );
    let close = |a: f64, b: f64| (a - b).abs() <= (a.abs() * 1e-9).max(1e-6);
    assert!(
        close(stats1.0, stats2.0),
        "{fixture}: DPS survives the round-trip: {} vs {}",
        stats1.0,
        stats2.0
    );
    assert!(
        close(stats1.1, stats2.1),
        "{fixture}: Life survives the round-trip: {} vs {}",
        stats1.1,
        stats2.1
    );
    assert!(
        close(stats1.2, stats2.2),
        "{fixture}: EHP survives the round-trip: {} vs {}",
        stats1.2,
        stats2.2
    );

    // Structural equality: upstream serializes hash-iterated sections (e.g.
    // config Inputs) in nondeterministic order, so canonicalize both saves
    // with upstream's own XML parser (sorted attributes, sorted children)
    // before comparing
    let (canon1, canon2): (String, String) = lua
        .load(
            r#"
            local a, b = ...
            local function canon(node)
                if type(node) == "string" then
                    return "T:" .. node
                end
                local parts = { "E:" .. (node.elem or "?") }
                local attrs = {}
                for k, v in pairs(node.attrib or {}) do
                    -- Spec "nodes" and "masteryEffects" are hash-iterated
                    -- sets upstream; sort their components
                    if k == "nodes" then
                        local ids = {}
                        for part in tostring(v):gmatch("[^,]+") do
                            table.insert(ids, part)
                        end
                        table.sort(ids)
                        v = table.concat(ids, ",")
                    elseif k == "masteryEffects" then
                        local sels = {}
                        for part in tostring(v):gmatch("%b{}") do
                            table.insert(sels, part)
                        end
                        table.sort(sels)
                        v = table.concat(sels, ",")
                    end
                    table.insert(attrs, k .. "=" .. tostring(v))
                end
                table.sort(attrs)
                table.insert(parts, table.concat(attrs, ";"))
                local kids = {}
                for _, child in ipairs(node) do
                    -- The legacy URL element re-encodes the allocated-node
                    -- set in hash order; it duplicates the authoritative
                    -- (and separately compared) "nodes" attribute
                    if node.elem == "URL" and type(child) == "string" then
                        table.insert(kids, "T:<url>")
                    else
                        table.insert(kids, canon(child))
                    end
                end
                table.sort(kids)
                table.insert(parts, table.concat(kids, "\n"))
                return table.concat(parts, "|")
            end
            local ta, errA = common.xml.ParseXML(a)
            local tb, errB = common.xml.ParseXML(b)
            if not ta or not tb then
                error("parse failed: " .. tostring(errA or errB))
            end
            return canon(ta[1]), canon(tb[1])
        "#,
        )
        .call((save1.as_str(), save2.as_str()))
        .expect("canonicalization failed");
    if canon1 != canon2 {
        let pos = canon1
            .bytes()
            .zip(canon2.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(canon1.len().min(canon2.len()));
        let start = pos.saturating_sub(150);
        panic!(
            "{fixture}: saved XML differs structurally after reload; first \
             difference near byte {pos}:\n--- first save ---\n{}\n--- second save ---\n{}",
            &canon1[start..(pos + 150).min(canon1.len())],
            &canon2[start..(pos + 150).min(canon2.len())],
        );
    }
}

#[test]
fn test_trace_path() {
    use pob_egui::data::tree;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Find an unallocated node adjacent to the allocated tree (path length 1)
    // and one of its unallocated neighbours (for a two-step trace)
    let (first, second): (u32, u32) = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            for _, node in pairs(spec.nodes) do
                if not node.alloc and node.path and #node.path == 1
                   and not node.isKeystone and node.type ~= "Mastery" then
                    for _, linked in ipairs(node.linked or {}) do
                        if not linked.alloc and linked.path and linked.id < 65536
                           and linked.type ~= "Mastery" and not linked.isJewelSocket then
                            return node.id, linked.id
                        end
                    end
                end
            end
            error("no traceable node pair found")
        "#,
        )
        .eval()
        .expect("failed to find trace candidates");

    // Empty trace initializes with the node's shortest path
    let trace = tree::extend_trace_path(lua, &[], first)
        .expect("extend failed")
        .expect("adjacent node is traceable");
    assert_eq!(trace.last(), Some(&first), "trace ends at the hovered node");

    // Hovering a linked neighbour appends it
    let trace2 = tree::extend_trace_path(lua, &trace, second)
        .expect("extend failed")
        .expect("linked neighbour extends the trace");
    assert_eq!(trace2.len(), trace.len() + 1, "one node appended");
    assert_eq!(trace2.last(), Some(&second), "trace ends at the neighbour");

    // Hovering a node not linked to the trace end leaves it unchanged
    let far: u32 = lua
        .load(
            r#"
            local spec = mainObject_ref.main.modes['BUILD'].spec
            local target = ...
            local targetNode = spec.nodes[target]
            for _, node in pairs(spec.nodes) do
                if not node.alloc and node.path and node.id < 65536 then
                    local linked = false
                    for _, l in ipairs(node.linked or {}) do
                        if l == targetNode then linked = true break end
                    end
                    if not linked and node.id ~= target then
                        return node.id
                    end
                end
            end
            error("no far node found")
        "#,
        )
        .call(second)
        .expect("failed to find far node");
    let unchanged = tree::extend_trace_path(lua, &trace2, far).expect("extend failed");
    assert!(unchanged.is_none(), "unlinked node cannot extend the trace");

    // Hovering back onto an earlier trace node moves it to the end
    let back = tree::extend_trace_path(lua, &trace2, first)
        .expect("extend failed")
        .expect("earlier trace node re-hovers");
    assert_eq!(back.len(), trace2.len(), "no growth when moving within");
    assert_eq!(back.last(), Some(&first), "moved to the end");

    // Allocating the trace allocates every node on it in one undo step
    let count_alloc = r#"
        local n = 0
        for _ in pairs(mainObject_ref.main.modes['BUILD'].spec.allocNodes) do n = n + 1 end
        return n
    "#;
    let before: i64 = lua.load(count_alloc).eval().expect("count failed");
    tree::alloc_trace_path(lua, &trace2).expect("alloc failed");
    let after: i64 = lua.load(count_alloc).eval().expect("count failed");
    assert_eq!(
        after,
        before + trace2.len() as i64,
        "every traced node allocated"
    );
    let allocated: bool = lua
        .load(format!(
            "local s = mainObject_ref.main.modes['BUILD'].spec return s.allocNodes[{first}] ~= nil and s.allocNodes[{second}] ~= nil"
        ))
        .eval()
        .expect("read failed");
    assert!(allocated, "both traced nodes are allocated");

    tree::undo(lua).expect("undo failed");
    let undone: i64 = lua.load(count_alloc).eval().expect("count failed");
    assert_eq!(undone, before, "trace allocation is a single undo step");
}

#[test]
#[ignore = "network: hits poeurl.com, which is frequently down"]
fn test_poeurl_shrink() {
    use pob_egui::data::tree_specs;

    let short =
        tree_specs::shrink_tree_url("https://www.pathofexile.com/passive-skill-tree/AAAABgAAAAAA")
            .expect("shrink failed (service may be down)");
    assert!(
        short.starts_with("http://poeurl.com/") || short.starts_with("https://poeurl.com/"),
        "got {short}"
    );
    // The shortlink must expand back to a pathofexile.com URL
    let expanded = tree_specs::expand_shortlink(&short).expect("expand failed");
    assert!(
        expanded.contains("pathofexile.com"),
        "shortlink round-trips: {expanded}"
    );
}

#[test]
fn test_socket_group_copy_paste() {
    use pob_egui::data::skills;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let groups = skills::extract_skills(lua).expect("skills failed");
    let source = groups
        .iter()
        .find(|g| !g.gems.is_empty())
        .expect("a group with gems");

    // Copy serializes to upstream's text format
    let text = skills::copy_socket_group_text(lua, source.index)
        .expect("copy failed")
        .expect("group exists");
    assert!(
        text.contains(&source.gems[0].name),
        "copied text lists the gems: {text}"
    );

    // Pasting appends a new group with the same gems
    let before = groups.len();
    let added = skills::paste_socket_group_text(lua, &text).expect("paste failed");
    assert!(added, "valid text pastes");
    let after = skills::extract_skills(lua).expect("skills failed");
    assert_eq!(after.len(), before + 1, "one group added");
    let pasted = after.last().expect("pasted group");
    assert_eq!(
        pasted.gems.len(),
        source.gems.len(),
        "all gems survive the round-trip"
    );
    for (a, b) in source.gems.iter().zip(pasted.gems.iter()) {
        assert_eq!(a.name, b.name, "gem name survives");
        assert_eq!(a.level, b.level, "gem level survives");
        assert_eq!(a.quality, b.quality, "gem quality survives");
        assert_eq!(a.enabled, b.enabled, "gem enabled state survives");
    }

    // Garbage text pastes nothing
    let added = skills::paste_socket_group_text(lua, "not a socket group").expect("call failed");
    assert!(!added, "garbage text is rejected");
    let unchanged = skills::extract_skills(lua).expect("skills failed");
    assert_eq!(unchanged.len(), before + 1, "no group added");

    // The paste is undoable
    lua.load(
        r#"
        local build = mainObject_ref.main.modes['BUILD']
        build.skillsTab:Undo()
        build.buildFlag = true
        _runCallback('OnFrame')
    "#,
    )
    .exec()
    .expect("undo failed");
    let undone = skills::extract_skills(lua).expect("skills failed");
    assert_eq!(undone.len(), before, "paste undone");
}

// ---------------------------------------------------------------------------
// Conformance: our search port vs upstream's live matcher
// ---------------------------------------------------------------------------

#[test]
fn test_search_conforms_to_upstream_matcher() {
    use std::collections::HashSet;

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Single-token queries produce identical search params on both sides
    // (multi-term splitting is covered by the ports.toml hash on prepSearch),
    // so any difference here is matcher drift.
    for query in [
        "life",
        "fire.*damage",
        "(fire|cold)",
        "keystone",
        "oil:",
        "^armour",
        "%d+%%",
        "[[",
    ] {
        let ours = pob_egui::data::tree::search_nodes(lua, query).expect("our search failed");
        let theirs: HashSet<u32> = lua
            .load(
                r#"
                local query = ...
                local spec = mainObject_ref.main.modes['BUILD'].spec
                local view = new("PassiveTreeView")
                view.searchParams = { query:lower() }
                local out = {}
                for nodeId, node in pairs(spec.nodes) do
                    if view:DoesNodeMatchSearchParams(node) then
                        table.insert(out, nodeId)
                    end
                end
                return out
            "#,
            )
            .call::<mlua::Table>(query)
            .expect("upstream matcher failed")
            .sequence_values::<u32>()
            .flatten()
            .collect();
        let only_ours: Vec<_> = ours.difference(&theirs).take(5).collect();
        let only_theirs: Vec<_> = theirs.difference(&ours).take(5).collect();
        assert_eq!(
            ours, theirs,
            "query {query:?}: our matcher diverged from upstream's \
             (only ours: {only_ours:?}, only upstream: {only_theirs:?})"
        );
    }
}

#[test]
fn test_vaal_gem_global_toggles() {
    use pob_egui::data::skills::{self, GemProperty};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Add a vaal gem to a user socket group
    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| !g.from_item)
        .expect("a user socket group");
    let err = skills::add_gem(lua, group.index, "Vaal Haste").expect("add failed");
    assert!(err.is_none(), "vaal gem should resolve: {err:?}");

    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| g.index == group.index)
        .expect("group");
    let (gem_idx0, gem) = group
        .gems
        .iter()
        .enumerate()
        .find(|(_, g)| g.name.contains("Vaal Haste"))
        .expect("vaal gem in group");
    let gem_idx = gem_idx0 + 1;

    // A vaal gem exposes both toggle labels; defaults match upstream
    // (first effect enabled, second disabled)
    assert!(
        gem.global1_label.is_some() && gem.global2_label.is_some(),
        "vaal gem has both global toggles: {:?} / {:?}",
        gem.global1_label,
        gem.global2_label
    );
    // Newly added gems default both effects on (upstream's new-gem path;
    // XML loads default the second one off instead)
    assert!(gem.enable_global1, "first effect enabled by default");
    assert!(
        gem.enable_global2,
        "second effect enabled on freshly added gems"
    );

    // Toggling round-trips
    skills::set_gem_property(lua, group.index, gem_idx, GemProperty::EnableGlobal2(false))
        .expect("toggle failed");
    let groups = skills::extract_skills(lua).expect("skills failed");
    let gem = &groups
        .iter()
        .find(|g| g.index == group.index)
        .expect("group")
        .gems[gem_idx0];
    assert!(!gem.enable_global2, "toggle round-trips");

    // A non-vaal gem shows no toggles
    let plain = groups
        .iter()
        .flat_map(|g| g.gems.iter())
        .find(|g| !g.name.contains("Vaal"))
        .expect("a non-vaal gem");
    assert!(
        plain.global1_label.is_none() && plain.global2_label.is_none(),
        "non-vaal gems have no global toggles"
    );
}

#[test]
fn test_calcs_minion_selection() {
    use pob_egui::data::{calcs, skills};

    let _ = env_logger::builder().is_test(true).try_init();
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Add a golem: a minion skill with its own skill list
    let groups = skills::extract_skills(lua).expect("skills failed");
    let group = groups
        .iter()
        .find(|g| !g.from_item)
        .expect("a user socket group");
    let err = skills::add_gem(lua, group.index, "Summon Stone Golem").expect("add failed");
    assert!(err.is_none(), "golem gem should resolve: {err:?}");

    // Point the calcs tab at that group and select the golem skill
    calcs::set_skill_number(lua, group.index).expect("set group failed");
    let sel = calcs::skill_selection(lua).expect("selection failed");
    let golem_idx = sel
        .skills
        .iter()
        .position(|s| s.contains("Golem"))
        .expect("golem in calcs skill list");
    calcs::set_active_skill(lua, golem_idx).expect("set skill failed");

    let sel = calcs::skill_selection(lua).expect("selection failed");
    assert!(
        !sel.minions.is_empty(),
        "golem skill exposes a minion list, got {:?}",
        sel.minions
    );
    assert!(
        !sel.minion_skills.is_empty(),
        "the golem has minion skills, got {:?}",
        sel.minion_skills
    );

    // Selecting a minion skill round-trips through srcInstance
    if sel.minion_skills.len() > 1 {
        let target = sel.minion_skills.len(); // 1-based last
        calcs::set_calcs_minion_skill(lua, target).expect("set minion skill failed");
        let sel = calcs::skill_selection(lua).expect("selection failed");
        assert_eq!(
            sel.selected_minion_skill,
            target - 1,
            "minion skill selection round-trips"
        );
    }

    // Selecting the minion round-trips too
    let (_, first_id) = sel.minions[0].clone();
    calcs::set_calcs_minion(lua, &first_id).expect("set minion failed");
    let sel = calcs::skill_selection(lua).expect("selection failed");
    assert_eq!(sel.selected_minion, 0, "minion selection round-trips");
}

// ---------------------------------------------------------------------------
// Config visibility conformance
// ---------------------------------------------------------------------------

/// Tier 1 conformance: our per-option visibility must equal upstream's shared
/// `Modules/ConfigVisibility` verdict for every option, on every fixture. This
/// is what keeps the Config tab and the Compare tab's config view in step.
#[test]
fn test_config_visibility_matches_upstream() {
    for fixture in common::ALL_FIXTURES {
        let bridge = common::boot_and_load_build(fixture);
        let lua = bridge.lua();

        let upstream: LuaTable = lua
            .load(
                r#"
                local build = mainObject_ref.main.modes['BUILD']
                local configVisibility = LoadModule("Modules/ConfigVisibility")
                local varList = LoadModule("Modules/ConfigOptions")
                local out = {}
                for _, varData in ipairs(varList) do
                    if varData.var then
                        out[varData.var] = {
                            relevant = configVisibility.isRelevantForBuild(varData, build) and true or false,
                            excluded = configVisibility.isShowAllExcluded(varData) and true or false,
                        }
                    end
                end
                return out
            "#,
            )
            .eval()
            .expect("upstream visibility query failed");

        let options = pob_egui::data::config::extract_config_options(lua)
            .expect("failed to extract config options");

        let mut relevant_count = 0;
        let mut checked = 0;
        for option in &options {
            let Some(var) = option.var() else { continue };
            let expected: LuaTable = upstream
                .get(var)
                .unwrap_or_else(|_| panic!("{fixture}: no upstream verdict for '{var}'"));
            let vis = option.vis();
            assert_eq!(
                vis.relevant,
                expected.get::<bool>("relevant").unwrap(),
                "{fixture}: relevance mismatch for config option '{var}'"
            );
            assert_eq!(
                vis.show_all_excluded,
                expected.get::<bool>("excluded").unwrap(),
                "{fixture}: show-all exclusion mismatch for config option '{var}'"
            );
            relevant_count += usize::from(vis.relevant);
            checked += 1;
        }

        assert!(checked > 500, "{fixture}: only checked {checked} options");
        // Guard against a regression that makes everything eligible again: a
        // loaded build uses a small fraction of the ~1000 defined options.
        assert!(
            relevant_count < checked / 3,
            "{fixture}: {relevant_count} of {checked} options eligible, expected far fewer"
        );
    }
}

/// An option whose predicates fail stays listed when its value is off-default,
/// flagged invalid - upstream ConfigTab's `hideIfInvalid` wrapper.
#[test]
fn test_modified_ineligible_option_stays_visible() {
    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Shrine buffs are gated on ifFlag/ifCond predicates this build does not meet.
    let var = "buffDiamondShrine";
    let before = pob_egui::data::config::extract_config_options(lua).expect("extract failed");
    let vis = before
        .iter()
        .find(|o| o.var() == Some(var))
        .expect("option not found")
        .vis();
    assert!(!vis.relevant, "{var} should be ineligible for this build");
    assert!(!vis.shown(false), "{var} should be hidden while at default");

    pob_egui::data::config::set_config_value(lua, var, LuaValue::Boolean(true))
        .expect("failed to set config value");

    let after = pob_egui::data::config::extract_config_options(lua).expect("extract failed");
    let vis = after
        .iter()
        .find(|o| o.var() == Some(var))
        .expect("option not found")
        .vis();
    assert!(vis.modified, "{var} should read as off-default");
    assert!(
        vis.shown(false),
        "{var} should stay listed once set, even with 'show all' off"
    );
    assert!(
        vis.invalid(false),
        "{var} should be flagged invalid rather than shown as normal"
    );
}

// ---------------------------------------------------------------------------
// Custom modifier groups
// ---------------------------------------------------------------------------

/// Current maximum Life, the stat the custom-mod tests move.
fn life(lua: &Lua) -> f64 {
    pob_egui::data::CalcOutput::extract(lua)
        .expect("calc extract failed")
        .stats
        .get("Life")
        .copied()
        .unwrap_or(0.0)
}

/// Groups round-trip through upstream's data model: adding, renaming,
/// disabling and deleting each land in `customModsList`, and an enabled
/// group's mods reach the calc output sourced as `Custom:<title>`.
#[test]
fn test_custom_mod_groups_roundtrip() {
    use pob_egui::data::custom_mods::{self, CustomModGroup};

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // A build always starts with one empty group to type into.
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(
        groups,
        vec![CustomModGroup {
            title: "Default".to_string(),
            enabled: true,
            text: String::new(),
        }],
        "a fresh build has one empty Default group"
    );

    let life_before = life(lua);

    custom_mods::set_title(lua, 0, "Testing").expect("rename failed");
    custom_mods::set_text(lua, 0, "+100 to maximum Life").expect("set text failed");

    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(groups[0].title, "Testing");
    assert_eq!(groups[0].text, "+100 to maximum Life");

    // Not an exact +100: the flat life scales with the build's % life
    // modifiers, which is the point - it goes through the real calc path.
    let life_with = life(lua);
    assert!(
        life_with > life_before,
        "an enabled group's mods reach the calc output ({life_before} -> {life_with})"
    );

    // The group title is the mod source.
    let source_found: bool = lua
        .load(
            r#"
            local ct = mainObject_ref.main.modes['BUILD'].configTab
            for _, mod in ipairs(ct.modList) do
                if mod.source == "Custom:Testing" then return true end
            end
            return false
        "#,
        )
        .eval()
        .expect("source query failed");
    assert!(source_found, "mods are sourced as Custom:<title>");

    // Disabling keeps the text but drops the mods.
    custom_mods::set_enabled(lua, 0, false).expect("disable failed");
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert!(!groups[0].enabled);
    assert_eq!(groups[0].text, "+100 to maximum Life", "text is kept");
    assert_eq!(
        life(lua),
        life_before,
        "a disabled group contributes nothing"
    );
    custom_mods::set_enabled(lua, 0, true).expect("enable failed");

    // Adding, then deleting back down past empty.
    custom_mods::add_group(lua).expect("add failed");
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[1].title, "Group 2",
        "new groups are named by position"
    );

    custom_mods::delete_group(lua, 1).expect("delete failed");
    assert_eq!(custom_mods::list_groups(lua).expect("list failed").len(), 1);

    custom_mods::delete_group(lua, 0).expect("delete failed");
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(
        groups,
        vec![CustomModGroup {
            title: "Default".to_string(),
            enabled: true,
            text: String::new(),
        }],
        "deleting the last group leaves an empty Default behind"
    );
    assert_eq!(
        life(lua),
        life_before,
        "deleting the group removes its mods"
    );
}

/// Each edit records an undo state, and undo restores the group list - the
/// shape upstream's `CreateUndoState` stores alongside `input`.
#[test]
fn test_custom_mod_groups_undo() {
    use pob_egui::data::custom_mods;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    custom_mods::set_text(lua, 0, "+100 to maximum Life").expect("set text failed");
    let life_with = life(lua);

    pob_egui::data::config::undo(lua).expect("undo failed");
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(groups[0].text, "", "undo restores the group text");
    assert!(
        life(lua) < life_with,
        "undo drops the group's mods from the calc"
    );

    pob_egui::data::config::redo(lua).expect("redo failed");
    let groups = custom_mods::list_groups(lua).expect("list failed");
    assert_eq!(groups[0].text, "+100 to maximum Life", "redo restores it");
}

/// Groups persist to the build XML as `<CustomModifierBlock>` and come back on
/// load, including the disabled flag.
#[test]
fn test_custom_mod_groups_persist_to_xml() {
    use pob_egui::data::custom_mods::{self, CustomModGroup};

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    custom_mods::set_title(lua, 0, "Kept").expect("rename failed");
    custom_mods::set_text(lua, 0, "+100 to maximum Life").expect("set text failed");
    custom_mods::add_group(lua).expect("add failed");
    custom_mods::set_text(lua, 1, "10% increased Attack Speed").expect("set text failed");
    custom_mods::set_enabled(lua, 1, false).expect("disable failed");

    let xml: String = lua
        .load("return mainObject_ref.main.modes['BUILD']:SaveDB('dummy')")
        .eval()
        .expect("SaveDB failed");
    assert!(
        xml.contains(r#"<CustomModifierBlock title="Kept" enabled="true">"#)
            || xml.contains(r#"<CustomModifierBlock enabled="true" title="Kept">"#),
        "groups are written as CustomModifierBlock elements:\n{}",
        xml.lines()
            .filter(|l| l.contains("CustomModifierBlock"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Reload the saved XML into a fresh VM and read the groups back.
    let reloaded = common::boot_and_load_test_build();
    reloaded
        .load_build_from_xml(&xml, "reloaded", None)
        .expect("reload failed");
    let groups = custom_mods::list_groups(reloaded.lua()).expect("list failed");
    assert_eq!(
        groups,
        vec![
            CustomModGroup {
                title: "Kept".to_string(),
                enabled: true,
                text: "+100 to maximum Life".to_string(),
            },
            CustomModGroup {
                title: "Group 2".to_string(),
                enabled: false,
                text: "10% increased Attack Speed".to_string(),
            },
        ],
        "groups survive a save/load round trip"
    );
}

/// Line classification comes from upstream's own modifier parser.
#[test]
fn test_custom_mod_line_status() {
    use pob_egui::data::custom_mods::{self, LineStatus};

    let bridge = common::boot_and_load_test_build();
    let status = custom_mods::line_status(
        bridge.lua(),
        "+100 to maximum Life\n\nthis is not a modifier\n  10% increased Attack Speed  ",
    )
    .expect("line status failed");

    assert_eq!(
        status,
        vec![
            LineStatus::Parsed,
            LineStatus::Blank,
            LineStatus::Unsupported,
            LineStatus::Parsed,
        ],
        "one status per line, indexed like the editor's lines"
    );
}

/// Fallback weights for Glorious Vanity, the jewel whose replacements carry
/// variable rolls: v2.67 rebuilt how those are scored (one mod list per stat,
/// and a divisor only where a value was actually substituted), so this path
/// needs its own coverage - the search test above uses Lethal Pride, whose
/// nodes take the fixed-mod branch.
#[test]
fn test_timeless_fallback_weights_glorious_vanity() {
    use pob_egui::data::timeless;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let stats = timeless::timeless_stats(lua, "vaal").expect("stat list failed");
    assert!(
        stats.len() > 20,
        "Glorious Vanity offers many replacements, got {}",
        stats.len()
    );
    assert!(
        !stats
            .iter()
            .any(|s| s.id.starts_with("abyss_special_ascendancy_notable_")),
        "abyss ascendancy notables are excluded from the stat list"
    );

    // powerStatList is 1-based and its order is upstream data, so find the
    // entry rather than hardcoding a position.
    let stat_index: usize = lua
        .load(
            r#"
            for i, entry in ipairs(data.powerStatList) do
                if entry.stat == "FullDPS" then return i end
            end
            return 0
        "#,
        )
        .eval()
        .expect("power stat lookup failed");
    assert!(stat_index > 0, "FullDPS is a selectable power stat");

    let ids: Vec<String> = stats.iter().take(40).map(|s| s.id.clone()).collect();
    let weights =
        timeless::generate_fallback_weights(lua, &ids, stat_index).expect("weights failed");
    assert!(
        !weights.is_empty(),
        "some Glorious Vanity replacements move the selected stat"
    );
    for w in &weights {
        assert!(
            w.weight1.is_finite() && w.weight2.is_finite(),
            "weight for {} is not finite: {:?}",
            w.id,
            w
        );
        assert!(
            w.weight1.abs() < 1.0e6 && w.weight2.abs() < 1.0e6,
            "weight for {} is implausible, suggesting a bad divisor: {:?}",
            w.id,
            w
        );
    }
}

// ---------------------------------------------------------------------------
// Build list ordering
// ---------------------------------------------------------------------------

/// Conformance: our build-list ordering is upstream's. `sort_entries` calls
/// upstream's own `naturalSortCompare`, so this drives the same names through
/// upstream's `BuildListHelpers.SortList` shape and checks we agree - which
/// also proves the comparator is reachable headless.
#[test]
fn test_build_list_natural_sort_matches_upstream() {
    use pob_egui::data::build_list::{self, BuildEntry, BuildInfo, FolderInfo, SortMode};

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Names chosen for the cases plain lexicographic ordering gets wrong.
    let names = [
        "Build 10.xml",
        "Build 9.xml",
        "Build 2.xml",
        "build 1.xml",
        "Zealot.xml",
        "apple.xml",
        "Item 01.xml",
        "Item 1.xml",
    ];
    let build = |name: &str| {
        BuildEntry::Build(BuildInfo {
            file_name: name.to_string(),
            build_name: name.trim_end_matches(".xml").to_string(),
            full_path: std::path::PathBuf::from(name),
            sub_path: String::new(),
            level: None,
            class_name: None,
            ascend_class_name: None,
            modified: 0.0,
        })
    };
    let mut entries: Vec<BuildEntry> = names.iter().map(|n| build(n)).collect();
    entries.push(BuildEntry::Folder(FolderInfo {
        folder_name: "Zulu".to_string(),
        full_path: std::path::PathBuf::from("Zulu"),
        sub_path: String::new(),
        modified: 0.0,
    }));

    build_list::sort_entries(lua, &mut entries, SortMode::Name).expect("sort failed");
    let ours: Vec<String> = entries
        .iter()
        .map(|e| match e {
            BuildEntry::Build(b) => b.file_name.clone(),
            BuildEntry::Folder(f) => f.folder_name.clone(),
        })
        .collect();

    // The same list ordered by upstream's comparator directly.
    let expected: Vec<String> = lua
        .load(
            r#"
            local names = ...
            local list = {}
            for i = 1, #names do list[i] = names[i] end
            table.sort(list, naturalSortCompare)
            return list
        "#,
        )
        .call::<LuaTable>(names.to_vec())
        .expect("upstream sort failed")
        .sequence_values::<String>()
        .flatten()
        .collect();

    assert_eq!(
        ours.first().map(String::as_str),
        Some("Zulu"),
        "folders sort before builds regardless of name"
    );
    assert_eq!(
        &ours[1..],
        expected.as_slice(),
        "build order matches upstream's naturalSortCompare"
    );

    // The specific thing plain lexicographic ordering gets wrong.
    let pos = |n: &str| ours.iter().position(|x| x == n).unwrap();
    assert!(
        pos("Build 2.xml") < pos("Build 9.xml") && pos("Build 9.xml") < pos("Build 10.xml"),
        "numbers order numerically, not as text: {ours:?}"
    );
}

/// Flask slots carry an activation toggle, and an inactive flask contributes
/// nothing to the calcs. The state has to reach two places to work: the slot
/// (read by `CalcSetup` into `env.flasks`) and the active item set (persisted
/// by `ItemsTab:Save`), so this checks both plus the calc effect.
#[test]
fn test_flask_activation() {
    use pob_egui::data::items;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // Find a flask slot with something equipped in the fixture.
    let equipped = items::extract_equipped_items(lua).expect("equipped failed");
    let flask = equipped
        .iter()
        .find(|s| s.can_activate && s.sel_item_id > 0)
        .expect("the fixture has at least one flask equipped");
    assert!(
        flask.slot_name.contains("Flask"),
        "activation is offered on flask slots: {}",
        flask.slot_name
    );
    let slot_name = flask.slot_name.clone();

    // Non-flask slots offer no toggle.
    assert!(
        equipped
            .iter()
            .any(|s| !s.can_activate && !s.slot_name.contains("Flask")),
        "ordinary gear slots have no activation checkbox"
    );

    // Reads the live state back, from the slot and the item set both.
    let state = |lua: &mlua::Lua| -> (bool, bool, bool) {
        lua.load(
            r#"
            local slotName = ...
            local build = mainObject_ref.main.modes['BUILD']
            local itemsTab = build.itemsTab
            local slot = itemsTab.slots[slotName]
            local set = itemsTab.activeItemSet and itemsTab.activeItemSet[slotName]
            local item = itemsTab.items[slot.selItemId]
            local counted = false
            local env = build.calcsTab.mainEnv
            if env and env.flasks and item then counted = env.flasks[item] == true end
            return slot.active == true, set and set.active == true, counted
        "#,
        )
        .call(slot_name.as_str())
        .expect("state read failed")
    };

    items::set_slot_active(lua, &slot_name, true).expect("activate failed");
    assert_eq!(
        state(lua),
        (true, true, true),
        "activating sets the slot, the item set, and puts the flask in env.flasks"
    );
    assert!(
        items::extract_equipped_items(lua)
            .expect("equipped failed")
            .iter()
            .any(|s| s.slot_name == slot_name && s.active),
        "the new state reads back through extract_equipped_items"
    );

    // Activation is worthless if it does not survive a save, and it rides on
    // the item set rather than the item, so check the XML carries it.
    //
    // Upstream builds each element's attributes as a Lua hash map and writes
    // them with `pairs()`, so their order varies run to run. Pull out the one
    // <Slot> element by name and test its contents rather than matching a
    // sequence of attributes.
    let save = || -> String {
        lua.load(r#"return mainObject_ref.main.modes['BUILD']:SaveDB("flaskactive")"#)
            .eval()
            .expect("SaveDB failed")
    };
    let slot_elem = |xml: &str| -> String {
        xml.lines()
            .find(|l| {
                l.trim_start().starts_with("<Slot ")
                    && l.contains(&format!(r#"name="{slot_name}""#))
            })
            .unwrap_or_else(|| panic!("no <Slot> element for {slot_name}"))
            .to_string()
    };

    items::set_slot_active(lua, &slot_name, true).expect("activate failed");
    assert!(
        slot_elem(&save()).contains(r#"active="true""#),
        "the active flag is written into the item set's slot element"
    );

    items::set_slot_active(lua, &slot_name, false).expect("deactivate failed");
    assert_eq!(
        state(lua),
        (false, false, false),
        "deactivating clears all three"
    );
    assert!(
        !slot_elem(&save()).contains(r#"active="#),
        "and drops the flag again when deactivated"
    );
}

/// Tinctures equip into flask slots and share their activation toggle, but the
/// calcs track them in `env.tinctures` rather than `env.flasks`, so they need
/// their own coverage.
#[test]
fn test_tincture_activation() {
    use pob_egui::data::items;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    let raw = "Rarity: UNIQUE\n\
               The Battle Within\n\
               Oakbranch Tincture\n\
               Requires Level 18\n\
               Implicits: 0\n\
               Melee Weapon Attacks have Culling Strike\n";
    assert!(
        items::add_item_from_raw(lua, raw)
            .expect("add failed")
            .is_none(),
        "the tincture parses"
    );
    let id = items::extract_item_list(lua)
        .expect("list failed")
        .into_iter()
        .find(|e| e.name.contains("The Battle Within"))
        .expect("tincture in the item list")
        .id;

    // A tincture is valid for a flask slot, which is what gives it the toggle.
    let slot = items::extract_equipped_items(lua)
        .expect("equipped failed")
        .into_iter()
        .find(|s| s.can_activate && s.valid_items.iter().any(|c| c.id == id))
        .expect("a flask slot accepts the tincture");
    items::equip_item(lua, &slot.slot_name, id).expect("equip failed");

    let counted = |lua: &mlua::Lua| -> (bool, bool) {
        lua.load(
            r#"
            local id = ...
            local build = mainObject_ref.main.modes['BUILD']
            local item = build.itemsTab.items[id]
            local env = build.calcsTab.mainEnv
            return env.tinctures ~= nil and env.tinctures[item] == true,
                   env.flasks ~= nil and env.flasks[item] == true
        "#,
        )
        .call(id)
        .expect("read failed")
    };

    items::set_slot_active(lua, &slot.slot_name, true).expect("activate failed");
    assert_eq!(
        counted(lua),
        (true, false),
        "an active tincture lands in env.tinctures, not env.flasks"
    );

    items::set_slot_active(lua, &slot.slot_name, false).expect("deactivate failed");
    assert_eq!(counted(lua), (false, false), "deactivating removes it");
}

/// Build a small nested build tree and point `main.buildPath` at it, so the
/// build-list tests exercise upstream's scanner against real directories.
/// Returns the temp dir (kept alive by the caller) and the previous build path.
fn with_build_tree(lua: &mlua::Lua) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("Nested/Deep")).unwrap();
    let build = |level: &str, class: &str, ascend: &str| {
        format!(
            "<PathOfBuilding><Build level=\"{level}\" className=\"{class}\" \
             ascendClassName=\"{ascend}\"/></PathOfBuilding>"
        )
    };
    std::fs::write(root.join("Top.xml"), build("93", "Ranger", "Deadeye")).unwrap();
    std::fs::write(
        root.join("Nested/Inner.xml"),
        build("42", "Witch", "Occultist"),
    )
    .unwrap();
    std::fs::write(
        root.join("Nested/Deep/Buried.xml"),
        build("7", "Marauder", "Juggernaut"),
    )
    .unwrap();

    let old: String = lua
        .load("local p = ... local old = main.buildPath main.buildPath = p return old")
        .call(format!("{}/", root.to_str().unwrap()))
        .expect("set buildPath");
    (dir, old)
}

fn restore_build_path(lua: &mlua::Lua, old: &str) {
    lua.load("main.buildPath = ...")
        .call::<()>(old)
        .expect("restore buildPath");
}

/// Searching the build list reaches into subfolders instead of only the folder
/// on screen, and understands upstream's `class:` term. Both come from calling
/// `BuildListHelpers.ScanFolder`/`FilterList` rather than filtering the current
/// directory listing ourselves.
#[test]
fn test_build_list_recursive_search() {
    use pob_egui::data::build_list::{self, BuildEntry, IndexKey, SortMode};

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();
    let (_dir, old_path) = with_build_tree(lua);

    build_list::refresh_index(lua, IndexKey::BuildList, "").expect("index failed");
    let names = |filter: &str| -> Vec<String> {
        build_list::filter_index(lua, IndexKey::BuildList, "", filter, SortMode::Name)
            .expect("filter failed")
            .into_iter()
            .map(|e| match e {
                BuildEntry::Build(b) => format!("{}{}", b.sub_path, b.file_name),
                BuildEntry::Folder(f) => format!("[dir]{}{}", f.sub_path, f.folder_name),
            })
            .collect()
    };

    // No search: just this folder's own contents, folders first.
    assert_eq!(names(""), vec!["[dir]Nested", "Top.xml"]);

    // A search reaches all the way down.
    assert_eq!(names("buried"), vec!["Nested/Deep/Buried.xml"]);
    assert_eq!(names("inner"), vec!["Nested/Inner.xml"]);

    // class: matches the class or ascendancy, not the file name.
    assert_eq!(names("class:occultist"), vec!["Nested/Inner.xml"]);
    assert_eq!(names("class:witch"), vec!["Nested/Inner.xml"]);
    assert!(
        names("class:top").is_empty(),
        "class: does not fall back to matching the build name"
    );

    // Terms combine, as upstream requires all of them to match.
    assert_eq!(
        names("class:juggernaut buried"),
        vec!["Nested/Deep/Buried.xml"]
    );
    assert!(names("class:juggernaut inner").is_empty());

    // The header fields the 2KB read pulls out reach us intact.
    let hit = build_list::filter_index(lua, IndexKey::BuildList, "", "buried", SortMode::Name)
        .expect("filter failed");
    let BuildEntry::Build(b) = &hit[0] else {
        panic!("expected a build")
    };
    assert_eq!(
        (
            b.level,
            b.class_name.as_deref(),
            b.ascend_class_name.as_deref()
        ),
        (Some(7), Some("Marauder"), Some("Juggernaut"))
    );

    restore_build_path(lua, &old_path);
}

/// A folder cannot be moved or copied into itself: the recursive move and copy
/// would otherwise walk into the destination they are still creating.
#[test]
fn test_folder_move_guard() {
    use pob_egui::data::build_list;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // "Nested" lives at the root; "Nested/Deep/" is inside it.
    assert!(
        !build_list::can_move_to_sub_path(lua, "", Some("Nested"), "Nested/Deep/").unwrap(),
        "a folder cannot move inside itself"
    );
    assert!(
        !build_list::can_move_to_sub_path(lua, "", Some("Nested"), "Nested/").unwrap(),
        "nor directly into itself"
    );
    assert!(
        build_list::can_move_to_sub_path(lua, "", Some("Nested"), "Other/").unwrap(),
        "an unrelated folder is a fine destination"
    );
    // Builds carry no folder name and only need the destination to differ.
    assert!(build_list::can_move_to_sub_path(lua, "", None, "Nested/").unwrap());
    assert!(
        !build_list::can_move_to_sub_path(lua, "Nested/", None, "Nested/").unwrap(),
        "moving to where it already is does nothing"
    );
}

/// A name collision when moving appends `[2]`, `[3]`, ... *before* the
/// extension, so the moved build stays a loadable `.xml`.
#[test]
fn test_dest_name_keeps_xml_extension() {
    use pob_egui::data::build_list;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();
    let (dir, old_path) = with_build_tree(lua);

    let taken = build_list::dest_name(lua, "", "Top.xml").expect("dest_name failed");
    assert_eq!(taken, dir.path().join("Top[2].xml"));

    // A second collision keeps counting rather than reusing [2].
    std::fs::write(dir.path().join("Top[2].xml"), "<PathOfBuilding/>").unwrap();
    let taken2 = build_list::dest_name(lua, "", "Top.xml").expect("dest_name failed");
    assert_eq!(taken2, dir.path().join("Top[3].xml"));

    // A free name is returned untouched.
    let free = build_list::dest_name(lua, "", "Fresh.xml").expect("dest_name failed");
    assert_eq!(free, dir.path().join("Fresh.xml"));

    restore_build_path(lua, &old_path);
}

/// Upstream's `isAnointable` is a file-local function in `ItemsTab.lua`, so the
/// eligibility rule is ported (`ports.toml: item-is-anointable`) rather than
/// called. The case that motivated it: Talismans carry `base.type == "Amulet"`,
/// so gating on the type alone offers anointing on bases the game disallows.
#[test]
fn test_anointable_excludes_talismans() {
    use pob_egui::data::items;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    // A legacy Talisman and an ordinary amulet, both base.type == "Amulet".
    let talisman = lua
        .load(
            r#"
            for name, base in pairs(data.itemBases) do
                if base.subType == "Talisman" then return name end
            end
        "#,
        )
        .eval::<String>()
        .expect("a Talisman base exists in 3.29 data");

    for (raw_name, expected) in [(talisman.as_str(), false), ("Amber Amulet", true)] {
        let raw = format!("Rarity: NORMAL\n{raw_name}\n{raw_name}\n");
        items::add_item_from_raw(lua, &raw).expect("add failed");
        let entry = items::extract_item_list(lua)
            .expect("list failed")
            .into_iter()
            .find(|e| e.name == raw_name)
            .unwrap_or_else(|| panic!("{raw_name} in the item list"));
        assert_eq!(
            entry.anointable, expected,
            "{raw_name} (type {}) anointable should be {expected}",
            entry.item_type
        );
    }
}

/// `GetDrawColor` arrived upstream in v2.67.0 and is read inside
/// `Tooltip:Draw`. We read `tooltip.lines` instead of calling Draw, so nothing
/// should reach it - but a missing global would be a hard error rather than a
/// no-op, so the stub must exist and return four channels.
#[test]
fn test_get_draw_color_stub_present() {
    let bridge = common::boot_and_load_test_build();
    let (r, g, b, a): (f64, f64, f64, f64) = bridge
        .lua()
        .load("return GetDrawColor()")
        .eval()
        .expect("GetDrawColor is registered");
    assert_eq!((r, g, b, a), (1.0, 1.0, 1.0, 1.0));
}

/// The secondary build browsers - the compare tab's picker and the Save As
/// folder list - go through `scan_builds_sorted` rather than sorting for
/// themselves, so a real directory read through it must come back in the same
/// natural order the build list panel shows, not `scan_builds`' plain
/// lowercase ordering.
#[test]
fn test_scan_builds_sorted_orders_naturally() {
    use pob_egui::data::build_list::{self, BuildEntry};

    let bridge = common::boot_and_load_test_build();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    for name in ["Build 10.xml", "Build 9.xml", "Build 2.xml", "apple.xml"] {
        std::fs::write(root.join(name), "<PathOfBuilding></PathOfBuilding>").unwrap();
    }
    std::fs::create_dir(root.join("Zulu")).unwrap();

    let entries = build_list::scan_builds_sorted(bridge.lua(), root.to_str().unwrap(), "");
    let names: Vec<String> = entries
        .iter()
        .map(|e| match e {
            BuildEntry::Build(b) => b.file_name.clone(),
            BuildEntry::Folder(f) => f.folder_name.clone(),
        })
        .collect();

    assert_eq!(
        names,
        vec![
            "Zulu".to_string(),
            "apple.xml".to_string(),
            "Build 2.xml".to_string(),
            "Build 9.xml".to_string(),
            "Build 10.xml".to_string(),
        ],
        "folders first, then upstream's naturalSortCompare order"
    );
}

/// Abyss timeless jewels (types 7-11) search through a different lookup table
/// and conquer allocated nodes rather than a radius, so they need their own
/// coverage: the Legion tests above never touch `readAbyssJewelLUT`.
#[test]
fn test_abyss_timeless_search() {
    use pob_egui::data::timeless;

    let bridge = common::boot_and_load_test_build();
    let lua = bridge.lua();

    assert_eq!(
        timeless::TIMELESS_JEWEL_TYPES.len(),
        11,
        "all five Abyss types are offered"
    );
    assert!(timeless::is_abyss(7) && timeless::is_abyss(timeless::ZORATH_TYPE));
    assert!(
        !timeless::is_abyss(6),
        "Heroic Tragedy is not an Abyss type"
    );

    // Festering Vengeance: its stat list comes from abyss_murderous_* nodes.
    let stats = timeless::timeless_stats(lua, "abyss_murderous").expect("stat list failed");
    assert!(
        !stats.is_empty(),
        "Festering Vengeance offers replacements to search for"
    );

    let socket = timeless::timeless_sockets(lua)
        .expect("sockets failed")
        .into_iter()
        .find(|s| s.allocated)
        .expect("the fixture has an allocated jewel socket");

    let desired: Vec<(String, f64, f64)> = stats
        .iter()
        .take(8)
        .map(|s| (s.id.clone(), 1.0, 0.0))
        .collect();
    let results = timeless::find_timeless_seeds(lua, 7, socket.node_id, &desired, &[], 20)
        .expect("abyss search failed");
    assert!(
        !results.is_empty(),
        "an Abyss search over 8 desired stats finds seeds"
    );
    for r in &results {
        assert!(r.weight > 0.0, "results are weighted: {r:?}");
        assert!(
            (100..=8000).contains(&r.seed),
            "seed {} is inside the Abyss range",
            r.seed
        );
        assert!(!r.matches.is_empty(), "each result names what it matched");
    }

    // Creating the jewel produces an item the engine accepts.
    let err = timeless::create_timeless_jewel(lua, 7, 0, results[0].seed).expect("create failed");
    assert!(err.is_none(), "jewel creation reported: {err:?}");
    let raw: String = lua
        .load(
            r#"
            local build = mainObject_ref.main.modes['BUILD']
            local last
            for _, item in pairs(build.itemsTab.items) do
                if item.name and item.name:match("Festering Vengeance") then last = item end
            end
            if not last then return "" end
            return (last.baseName or "") .. "|" .. tostring(last.jewelData and last.jewelData.conqueredBy
                and last.jewelData.conqueredBy.conqueror.type)
        "#,
        )
        .eval()
        .expect("item query failed");
    assert!(
        raw.starts_with("Murderous Eye Jewel"),
        "the Abyss jewel uses its eye-jewel base: {raw}"
    );
    assert!(
        raw.ends_with("abyss_murderous"),
        "the engine parsed the conquest line into jewelData: {raw}"
    );
}

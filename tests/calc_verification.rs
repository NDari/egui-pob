//! Integration test: load a build XML, run calcs, and verify output matches
//! the reference stats embedded in the XML's <PlayerStat> elements.

use std::collections::HashMap;
use std::path::PathBuf;

/// Parse expected stats from the build XML's <PlayerStat> elements.
fn parse_expected_stats(xml: &str) -> HashMap<String, f64> {
    let mut stats = HashMap::new();
    for line in xml.lines() {
        let line = line.trim();
        if line.starts_with("<PlayerStat ")
            && let (Some(val), Some(stat)) =
                (extract_attr(line, "value"), extract_attr(line, "stat"))
            && let Ok(v) = val.parse::<f64>()
        {
            stats.insert(stat.to_string(), v);
        }
    }
    stats
}

/// Extract an XML attribute value from a tag line.
fn extract_attr<'a>(line: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{attr}=\"");
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

/// Find the repo root by walking up from the test executable.
fn find_repo_root() -> PathBuf {
    let exe = std::env::current_exe().expect("could not get exe path");
    let mut candidate = exe.parent().unwrap().to_path_buf();
    for _ in 0..5 {
        if candidate.join("upstream").join("src").is_dir() {
            return candidate;
        }
        if !candidate.pop() {
            break;
        }
    }
    panic!(
        "Could not find repo root with upstream/src/. \
         Make sure the git submodule is initialized: git submodule update --init"
    );
}

#[test]
fn test_calc_output_matches_xml_reference() {
    let _ = env_logger::builder().is_test(true).try_init();

    let repo_root = find_repo_root();
    let src_path = repo_root.join("upstream").join("src");

    // Find test build XML
    let test_builds = repo_root.join("test_builds");
    assert!(
        test_builds.is_dir(),
        "test_builds/ directory not found at {}",
        test_builds.display()
    );

    let xml_paths = find_all_xml(&test_builds);
    assert!(!xml_paths.is_empty(), "no XML files found in test_builds/");

    let mut failures = Vec::new();
    for xml_path in &xml_paths {
        if let Err(report) = verify_build(xml_path, &src_path, &repo_root) {
            failures.push(report);
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} reference build(s) did not match:\n\n{}",
        failures.len(),
        xml_paths.len(),
        failures.join("\n\n")
    );
}

/// Verify one build's calc output against its embedded reference stats.
/// Returns a human-readable report on mismatch.
fn verify_build(
    xml_path: &std::path::Path,
    src_path: &std::path::Path,
    repo_root: &std::path::Path,
) -> Result<(), String> {
    let name = xml_path
        .strip_prefix(repo_root)
        .unwrap_or(xml_path)
        .display()
        .to_string();
    let xml_text = std::fs::read_to_string(xml_path).expect("failed to read test build XML");

    // Parse expected stats from XML
    let expected = parse_expected_stats(&xml_text);
    assert!(
        !expected.is_empty(),
        "no <PlayerStat> entries found in {name}"
    );

    // Boot Lua and load the build
    let bridge = pob_egui::lua_bridge::LuaBridge::new(src_path, repo_root)
        .expect("failed to init Lua bridge");
    bridge.verify_boot().expect("boot verification failed");
    bridge
        .load_build_from_xml(&xml_text, "Test Build", None)
        .expect("failed to load build");

    // Extract calc output
    let output =
        pob_egui::data::CalcOutput::extract(bridge.lua()).expect("failed to extract calc output");

    // Compare each expected stat
    let mut mismatches = Vec::new();
    let mut missing = Vec::new();

    for (stat, expected_val) in &expected {
        match output.stats.get(stat) {
            Some(actual_val) => {
                // Allow small floating point tolerance (0.01% relative or 0.01 absolute)
                let diff = (actual_val - expected_val).abs();
                let rel_diff = if expected_val.abs() > 0.0 {
                    diff / expected_val.abs()
                } else {
                    diff
                };

                if diff > 0.01 && rel_diff > 0.0001 {
                    mismatches.push(format!(
                        "  {stat}: expected {expected_val}, got {actual_val} (diff: {diff:.6})"
                    ));
                }
            }
            None => {
                missing.push(format!(
                    "  {stat}: expected {expected_val}, not found in output"
                ));
            }
        }
    }

    let total = expected.len();
    let matched = total - mismatches.len() - missing.len();

    println!("\n=== Calc Verification: {name} ===");
    println!("Total expected stats: {total}");
    println!("Matched: {matched}");
    println!("Mismatched: {}", mismatches.len());
    println!("Missing: {}", missing.len());

    if mismatches.is_empty() && missing.is_empty() {
        return Ok(());
    }

    mismatches.sort();
    missing.sort();
    let mut report = format!(
        "{name}: {} mismatches and {} missing stats out of {total}",
        mismatches.len(),
        missing.len()
    );
    if !mismatches.is_empty() {
        report.push_str("\nMismatches:\n");
        report.push_str(&mismatches.join("\n"));
    }
    if !missing.is_empty() {
        report.push_str("\nMissing:\n");
        report.push_str(&missing.join("\n"));
    }
    println!("{report}");
    Err(report)
}

/// Every build XML under `test_builds/`, in a stable order.
fn find_all_xml(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "xml"))
        .map(|e| e.path().to_path_buf())
        .collect();
    paths.sort();
    paths
}

//! Drift detection for upstream logic ports.
//!
//! `ports.toml` registers every Lua chunk that reimplements an upstream
//! function. This test extracts each registered snippet from the pinned
//! upstream submodule, hashes it, and fails when upstream changed something
//! we ported - producing a precise review list for submodule upgrades
//! instead of silent drift. See docs/upstream-upgrade.md.

mod common;

use sha2::{Digest, Sha256};

/// Extract a snippet from `source`: from the first line containing `anchor`
/// up to either the first subsequent line containing `end_anchor`
/// (exclusive), or the `end` keyword at the anchor line's indentation.
/// Trailing whitespace is trimmed per line.
fn extract_snippet(source: &str, anchor: &str, end_anchor: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines.iter().position(|l| l.contains(anchor))?;
    let indent: String = lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let terminator = format!("{indent}end");
    let mut out = vec![lines[start].trim_end()];
    for line in &lines[start + 1..] {
        if let Some(ea) = end_anchor {
            if line.contains(ea) {
                return Some(out.join("\n"));
            }
            out.push(line.trim_end());
        } else {
            out.push(line.trim_end());
            if line.trim_end() == terminator {
                return Some(out.join("\n"));
            }
        }
    }
    None
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[test]
fn ports_in_sync_with_upstream() {
    let repo_root = common::find_repo_root();
    let manifest_path = repo_root.join("ports.toml");
    let manifest_text =
        std::fs::read_to_string(&manifest_path).expect("ports.toml should exist at the repo root");
    let manifest: toml::Value = manifest_text.parse().expect("ports.toml should parse");

    let ports = manifest
        .get("port")
        .and_then(|p| p.as_array())
        .expect("ports.toml should contain [[port]] entries");
    assert!(!ports.is_empty(), "port registry should not be empty");

    let mut failures = Vec::new();
    for port in ports {
        let get = |key: &str| port.get(key).and_then(|v| v.as_str());
        let name = get("name").expect("port entry needs a name");
        let upstream_rel = get("upstream").expect("port entry needs an upstream path");
        let anchor = get("anchor").expect("port entry needs an anchor");
        let end_anchor = get("end_anchor");
        let expected = get("sha256").expect("port entry needs a sha256");

        let upstream_path = repo_root.join("upstream").join(upstream_rel);
        let source = match std::fs::read_to_string(&upstream_path) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{name}: cannot read {upstream_rel}: {e}"));
                continue;
            }
        };
        let snippet = match extract_snippet(&source, anchor, end_anchor) {
            Some(s) => s,
            None => {
                failures.push(format!(
                    "{name}: anchor {anchor:?} not found in {upstream_rel} \
                     (upstream removed or renamed it - review the port)"
                ));
                continue;
            }
        };
        let actual = sha256_hex(&snippet);
        if actual != expected {
            failures.push(format!(
                "{name}: upstream function changed since the port was synced\n\
                 \x20   file:     {upstream_rel} ({anchor:?})\n\
                 \x20   ours:     {}\n\
                 \x20   expected: {expected}\n\
                 \x20   actual:   {actual}",
                get("ours").unwrap_or("?"),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} port(s) drifted from upstream - diff each upstream function \
         against the port, re-sync (or record an intentional divergence in \
         DIVERGENCES.md), then update the hash in ports.toml:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

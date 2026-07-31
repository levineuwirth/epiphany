//! Enforces the neutrality boundary `src/lib.rs`'s crate doc comment states:
//! `round2-candidatekit` must not depend on any rendering, windowing, GPU,
//! or platform-accessibility crate — those stay candidate-owned (C1 =
//! egui + lyon, C2 = vello). This test reads this crate's own `Cargo.toml`
//! **at test time** rather than hard-coding "the current dependency list is
//! X" — the point is to catch a *future* dependency add, not merely to
//! assert today's file is fine.

/// Rendering, windowing, GPU, and platform-accessibility crates that must
/// never appear in `round2-candidatekit`'s own `[dependencies]`. This list
/// is the thing under test — it is deliberately hard-coded, unlike the
/// dependency names it is checked against, which are always read fresh from
/// the manifest.
const DENY_LIST: &[&str] = &[
    "egui",
    "eframe",
    "egui-wgpu",
    "lyon",
    "lyon_path",
    "lyon_tessellation",
    "vello",
    "wgpu",
    "winit",
    "accesskit",
    "accesskit_winit",
    "tiny-skia",
    "resvg",
    "usvg",
];

/// If `header` (the contents of a `[...]` line, already trimmed) names a
/// dependency **sub-table** — TOML's `[dependencies.name]` form, or the
/// same thing nested under a target, `[target.'cfg(...)'.dependencies.name]`
/// — returns `name`. `Cargo.toml` lets a single dependency spread across
/// its own `[...]` header when it needs more than a version string (e.g.
/// `[dependencies.wgpu]\nversion = "0.19"`), and that header names the
/// dependency directly rather than introducing a block of `key = value`
/// lines the way `[dependencies]` does — a scanner that only recognizes the
/// block form misses this shape entirely (confirmed empirically: it
/// returned `[]` for a manifest whose only dependency used this form).
fn dependency_subtable_name(header: &str) -> Option<String> {
    let rest = if let Some(r) = header.strip_prefix("dependencies.") {
        r
    } else if let Some(idx) = header.find(".dependencies.") {
        &header[idx + ".dependencies.".len()..]
    } else {
        return None;
    };
    // A dependency's own sub-table (e.g. hand-spread build metadata) would
    // add a further dot, as in `dependencies.foo.metadata`; only the first
    // segment is the crate name.
    let name = rest.split('.').next().unwrap_or(rest);
    Some(name.trim_matches('"').trim_matches('\'').to_string())
}

/// True if `header` opens a **block** of `key = value` dependency lines —
/// `[dependencies]` itself, or the same thing nested under a target
/// (`[target.'cfg(unix)'.dependencies]`). Deliberately does not match
/// `dev-dependencies` or `build-dependencies`: both end in "dependencies"
/// but with a hyphen, not a dot, immediately before it, so
/// `.ends_with(".dependencies")` is false for them — those tables are out
/// of scope for this guard on purpose (see
/// `the_line_scanner_finds_dependencies_and_ignores_other_sections`).
fn opens_dependency_block(header: &str) -> bool {
    header == "dependencies" || header.ends_with(".dependencies")
}

/// Extracts dependency names from a `Cargo.toml`, covering both shapes
/// Cargo accepts: the block form (`[dependencies]` followed by `key =
/// value` lines) and the sub-table form (`[dependencies.name]`), each
/// optionally nested under `[target.'cfg(...)'. ...]`. Deliberately not a
/// TOML parser — pulling one in as a dependency of a crate whose whole
/// point is a short, auditable dependency list would be self-defeating —
/// but a plain line scan that recognizes both header shapes, not just the
/// block one.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_dependency_block = false;
    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let header = header.trim();
            if let Some(name) = dependency_subtable_name(header) {
                names.push(name);
                in_dependency_block = false;
                continue;
            }
            in_dependency_block = opens_dependency_block(header);
            continue;
        }
        if !in_dependency_block || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            names.push(key.trim().trim_matches('"').to_string());
        }
    }
    names
}

fn manifest_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

#[test]
fn dependencies_do_not_include_a_denied_rendering_windowing_or_a11y_crate() {
    let manifest = std::fs::read_to_string(manifest_path())
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path().display()));
    let names = dependency_names(&manifest);
    assert!(
        !names.is_empty(),
        "the line scanner found zero dependencies in {} — that means this test is vacuous, not \
         that the crate has no dependencies (it depends on round2-textkit, round2-diff, serde, \
         serde_json); check the scanner, not the crate",
        manifest_path().display()
    );
    let violations: Vec<&String> = names
        .iter()
        .filter(|n| DENY_LIST.contains(&n.as_str()))
        .collect();
    assert!(
        violations.is_empty(),
        "round2-candidatekit/Cargo.toml [dependencies] names denied crate(s) {violations:?} — \
         this crate is candidate-neutral apparatus only (rendering, hit-test resolution, and \
         accessibility integration are candidate-owned; see src/lib.rs's crate doc comment for \
         the ruling this enforces). Denied list: {DENY_LIST:?}"
    );
}

/// Sanity check on the scanner itself, against a synthetic manifest
/// fragment: if this fails, the test above could be silently vacuous no
/// matter what `[dependencies]` actually contains. Also confirms
/// `[dev-dependencies]` is not scanned — a denied crate under dev-only use
/// (impossible here, since this crate declares none, but stated as a
/// property of the scanner) must not trip the production-dependency check.
#[test]
fn the_line_scanner_finds_dependencies_and_ignores_other_sections() {
    let synthetic = "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \
        \"1\"\nwgpu = \"0.19\"\n\n[dev-dependencies]\nwgpu = \"0.19\"\n";
    let names = dependency_names(synthetic);
    assert_eq!(names, vec!["serde".to_string(), "wgpu".to_string()]);
}

/// Confirms the scanner (and by extension the test above) actually flags a
/// denied name when one is present — otherwise `violations.is_empty()`
/// could be vacuously true because the scanner finds nothing, not because
/// the manifest is clean.
#[test]
fn a_synthetic_manifest_with_a_denied_dependency_is_flagged() {
    let synthetic = "[dependencies]\nserde = \"1\"\ntiny-skia = \"0.11\"\n";
    let names = dependency_names(synthetic);
    let violations: Vec<&String> = names
        .iter()
        .filter(|n| DENY_LIST.contains(&n.as_str()))
        .collect();
    assert_eq!(violations, vec![&"tiny-skia".to_string()]);
}

// ---- F2: the dotted sub-table form, confirmed empirically to be missed ----
//
// Feeding the original scanner
// `"[dependencies]\nserde = \"1\"\n\n[dependencies.wgpu]\nversion = \"0.19\"\n"`
// returned `["serde"]` — `wgpu` never appeared, because the scanner only
// recognized `[dependencies]` as a block header and had no notion of a
// dependency named directly by its own `[...]` header. Each test below
// would fail if `dependency_subtable_name`'s handling were removed (i.e.
// if `dependency_names` fell back to the old block-only logic).

/// The bare sub-table form: `[dependencies.wgpu]`.
#[test]
fn the_scanner_detects_a_dependency_named_via_a_dotted_subtable_header() {
    let synthetic = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n\n\
        [dependencies.wgpu]\nversion = \"0.19\"\n";
    let names = dependency_names(synthetic);
    assert!(
        names.contains(&"wgpu".to_string()),
        "sub-table form missed: {names:?}"
    );
    let violations: Vec<&String> = names
        .iter()
        .filter(|n| DENY_LIST.contains(&n.as_str()))
        .collect();
    assert_eq!(violations, vec![&"wgpu".to_string()]);
}

/// The block form nested under a target: `[target.'cfg(unix)'.dependencies]`.
#[test]
fn the_scanner_detects_a_dependency_block_under_a_target_cfg_table() {
    let synthetic =
        "[dependencies]\nserde = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nwgpu = \"0.19\"\n";
    let names = dependency_names(synthetic);
    assert!(
        names.contains(&"wgpu".to_string()),
        "target-cfg block form missed: {names:?}"
    );
    let violations: Vec<&String> = names
        .iter()
        .filter(|n| DENY_LIST.contains(&n.as_str()))
        .collect();
    assert_eq!(violations, vec![&"wgpu".to_string()]);
}

/// Both forms combined: the sub-table form nested under a target,
/// `[target.'cfg(windows)'.dependencies.tiny-skia]`.
#[test]
fn the_scanner_detects_a_dotted_subtable_header_under_a_target_cfg_table() {
    let synthetic = "[target.'cfg(windows)'.dependencies.tiny-skia]\nversion = \"0.11\"\n";
    let names = dependency_names(synthetic);
    assert!(
        names.contains(&"tiny-skia".to_string()),
        "target-cfg sub-table form missed: {names:?}"
    );
}

/// A dependency's own further sub-table (e.g. a spread-out `package`
/// rename) must still resolve to the crate name, the first dotted segment
/// after `dependencies.`, not the whole trailing path.
#[test]
fn a_deeper_dotted_path_still_resolves_to_the_leading_crate_name() {
    let synthetic = "[dependencies.serde.metadata]\nfoo = 1\n";
    let names = dependency_names(synthetic);
    assert_eq!(names, vec!["serde".to_string()]);
}

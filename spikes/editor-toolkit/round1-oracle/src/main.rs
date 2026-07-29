//! Generates the committed Round-1 oracle: `oracle.json` (machine-readable,
//! consumed by candidate runs in a later packet) and `ORACLE_SUMMARY.md`
//! (human-readable, for the user's pin-13 review). Run with `cargo run -p
//! round1-oracle` from `spikes/editor-toolkit/round1-oracle/`.

use epiphany_glyphs::BravuraGlyphCatalog;
use epiphany_layout_ir::GlyphCatalog;
use round1_oracle::{derive_glyph_oracle, GlyphOracle, Requirement, SampleClass};
use serde::Serialize;
use std::fmt::Write as _;
use std::fs;

/// Round 1's bounded-hole check class: "≥3 must-be-ink and ≥3 must-be-
/// background points, each inside a bounded hole".
const HOLE_GLYPHS: &[(&str, usize)] = &[
    ("gClef", 4),
    ("timeSig8", 3),
    ("accidentalFlat", 2),
    ("noteheadHalf", 2),
];

/// Round 1's disjoint-component check class: "no bounded hole ... ≥1 ink
/// point inside each of its three filled subpaths".
const DISJOINT_GLYPHS: &[(&str, usize)] = &[("fClef", 3)];

#[derive(Serialize)]
struct OracleFile {
    contract: &'static str,
    round: &'static str,
    render_transform_rule: &'static str,
    flatten_tolerance_staff_space: f64,
    clearance_floor_device_px: f64,
    glyphs: Vec<GlyphOracle>,
}

fn main() {
    let catalog = BravuraGlyphCatalog;
    let mut glyphs = Vec::new();
    let mut findings = Vec::new();

    for (name, expected_subpaths) in HOLE_GLYPHS {
        let data = catalog
            .render_data(name)
            .unwrap_or_else(|| panic!("{name}: BravuraGlyphCatalog has no render data"));
        match derive_glyph_oracle(
            name,
            &data.outline,
            Some(*expected_subpaths),
            Requirement::BoundedHole,
        ) {
            Ok(oracle) => {
                if oracle.subpath_count != *expected_subpaths {
                    findings.push(format!(
                        "{name}: parsed {} subpaths, contract's verified-starting-point section \
                         records {}",
                        oracle.subpath_count, expected_subpaths
                    ));
                }
                if !oracle.satisfied {
                    let d = &oracle.hole_diagnostic;
                    let detail = if d.raw_hole_grid_hits == 0 {
                        "this glyph has NO bounded hole at all under even-odd fill — every \
                         subpath is either the outer silhouette or a disjoint ink island, none \
                         nested inside another"
                            .to_string()
                    } else {
                        format!(
                            "this glyph has a bounded hole ({} grid hits) but the best \
                             achievable clearance in it is {:.3} device px, below the \
                             {}px floor",
                            d.raw_hole_grid_hits,
                            d.best_hole_clearance_device_px.unwrap_or(f64::NAN),
                            round1_oracle::CLEARANCE_MIN_DEVICE_PX
                        )
                    };
                    findings.push(format!(
                        "{name}: fewer than {} must-be-background sample points could be \
                         derived — {detail}. Per Round 1's hole-check clause ('each inside a \
                         bounded hole ... asserted by the derivation, not assumed'), no fallback \
                         point was substituted; {name}'s oracle carries ink points only and is \
                         NOT satisfied.",
                        round1_oracle::POINTS_PER_CLASS
                    ));
                }
                glyphs.push(oracle);
            }
            Err(e) => findings.push(e),
        }
    }

    for (name, expected_subpaths) in DISJOINT_GLYPHS {
        let data = catalog
            .render_data(name)
            .unwrap_or_else(|| panic!("{name}: BravuraGlyphCatalog has no render data"));
        match derive_glyph_oracle(
            name,
            &data.outline,
            Some(*expected_subpaths),
            Requirement::DisjointComponents,
        ) {
            Ok(oracle) => {
                if oracle.subpath_count != *expected_subpaths {
                    findings.push(format!(
                        "{name}: parsed {} subpaths, contract's verified-starting-point section \
                         records {}",
                        oracle.subpath_count, expected_subpaths
                    ));
                }
                // `oracle.satisfied` is expected true here (no bounded-hole
                // shortfall is possible for this class — see
                // `derive_glyph_oracle`'s DisjointComponents arm, which only
                // returns Ok once every subpath is covered), but this is
                // reported as a finding rather than assumed, matching every
                // other status check in this file.
                if !oracle.satisfied {
                    findings.push(format!(
                        "{name}: disjoint-component coverage requirement not met — see \
                         subpath_coverage_satisfied in oracle.json"
                    ));
                }
                glyphs.push(oracle);
            }
            Err(e) => findings.push(e),
        }
    }

    let oracle_file = OracleFile {
        contract: "spec/CONTRACT_EDITOR_T4_SPIKE.md Round 1 (criterion 1, compound-path fill \
                   correctness)",
        round: "round1",
        render_transform_rule: "device = (staff.x * scale + tx, ty - staff.y * scale); scale = \
            100 device px per staff space; tx/ty center the glyph's own flattened bounding box \
            in a 1920x1080 target (pin 4); staff-space is y-up, device space is y-down",
        flatten_tolerance_staff_space: round1_oracle::FLATTEN_TOLERANCE,
        clearance_floor_device_px: round1_oracle::CLEARANCE_MIN_DEVICE_PX,
        glyphs,
    };

    let json = serde_json::to_string_pretty(&oracle_file).expect("oracle serializes");
    fs::write("oracle.json", &json).expect("write oracle.json");

    let summary = render_summary(&oracle_file, &findings);
    fs::write("ORACLE_SUMMARY.md", &summary).expect("write ORACLE_SUMMARY.md");

    println!("wrote oracle.json ({} bytes)", json.len());
    println!("wrote ORACLE_SUMMARY.md");
    if !findings.is_empty() {
        println!("\nFINDINGS:");
        for f in &findings {
            println!("  - {f}");
        }
    }
}

fn render_summary(oracle: &OracleFile, findings: &[String]) -> String {
    let mut s = String::new();
    writeln!(
        s,
        "# Round 1 precommitted oracle — compound-path fill correctness\n"
    )
    .unwrap();
    writeln!(
        s,
        "Generated by `round1-oracle` (`spikes/editor-toolkit/round1-oracle`). Governed by \
         `spec/CONTRACT_EDITOR_T4_SPIKE.md` Revision 6, Round 1 (\"criterion 1, compound-path \
         fill correctness\"). No rendering crate was used to produce this file — every point \
         below is derived from the typed `PathCommand` outline alone.\n\n\
         Two check classes, testing two different properties: **bounded-hole** (`gClef`, \
         `timeSig8`, `accidentalFlat`, `noteheadHalf`) requires ≥3 ink and ≥3 must-be-background \
         points inside a bounded hole; **disjoint-component** (`fClef`) has no bounded hole by \
         design and instead requires ≥1 ink point inside each of its three filled subpaths, \
         tagged with `subpath_index`.\n"
    )
    .unwrap();

    writeln!(s, "## Render transform (pinned)\n").unwrap();
    writeln!(
        s,
        "`{}`\n\n- Flatten tolerance: `{}` staff-space units (Bezier chord deviation).\n\
         - Clearance floor: `{}` device px.\n",
        oracle.render_transform_rule,
        oracle.flatten_tolerance_staff_space,
        oracle.clearance_floor_device_px
    )
    .unwrap();

    writeln!(s, "## Overall status\n").unwrap();
    writeln!(s, "| glyph | requirement | satisfied | ring_signed_areas |").unwrap();
    writeln!(s, "|---|---|---|---|").unwrap();
    for g in &oracle.glyphs {
        let requirement = match g.requirement {
            Requirement::BoundedHole => "BoundedHole",
            Requirement::DisjointComponents => "DisjointComponents",
        };
        let areas = g
            .ring_signed_areas
            .iter()
            .map(|a| format!("{a:.3}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            s,
            "| `{}` | {} | **{}** | [{}] |",
            g.name, requirement, g.satisfied, areas
        )
        .unwrap();
    }
    writeln!(
        s,
        "\n`fClef` is `satisfied = true` with zero background points — that is the correct, \
         designed-for outcome for its `DisjointComponents` requirement class, not a shortfall. \
         The `satisfied` column above is the field to read; `background_satisfied` alone would \
         make that correct outcome indistinguishable from a bounded-hole glyph's genuine failure.\n"
    )
    .unwrap();

    if !findings.is_empty() {
        writeln!(s, "## Findings\n").unwrap();
        for f in findings {
            writeln!(s, "- **{f}**").unwrap();
        }
        writeln!(s).unwrap();
    }

    for g in &oracle.glyphs {
        writeln!(s, "## `{}`\n", g.name).unwrap();
        let requirement = match g.requirement {
            Requirement::BoundedHole => "BoundedHole (≥3 ink, ≥3 background inside a bounded hole)",
            Requirement::DisjointComponents => {
                "DisjointComponents (no background requirement; ≥1 ink point per filled subpath)"
            }
        };
        let subpath_note = match g.expected_subpath_count {
            Some(exp) if exp == g.subpath_count => format!("matches recorded {exp}"),
            Some(exp) => format!("**MISMATCH** — recorded {exp}, parsed {}", g.subpath_count),
            None => "no recorded expectation".to_string(),
        };
        let areas = g
            .ring_signed_areas
            .iter()
            .map(|a| format!("{a:.3}"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            s,
            "- Requirement class: {requirement}\n\
             - Overall status: **satisfied = {}**\n\
             - Subpaths: {} ({subpath_note})\n\
             - Ring signed areas (staff-space², shoelace, ring order): `[{areas}]`\n\
             - Bounding box (staff space): `{:?}`\n\
             - Outer contour ring index: {}\n\
             - Transform: scale={}, tx={:.3}, ty={:.3}, target={}x{}\n\
             - background_required={}, background_satisfied={}\n\
             - subpath_coverage_required={}, subpath_coverage_satisfied={}\n\
             - Ink candidates found (>= {}px clearance): {}\n\
             - Background/hole candidates found (>= {}px clearance): {}\n\
             - Ink point spacing relaxed: {}\n\
             - Background point spacing relaxed: {}\n",
            g.satisfied,
            g.subpath_count,
            g.bbox_staff,
            g.outer_contour_ring_index,
            g.transform.scale,
            g.transform.tx,
            g.transform.ty,
            g.transform.target_width as i32,
            g.transform.target_height as i32,
            g.background_required,
            g.background_satisfied,
            g.subpath_coverage_required,
            g.subpath_coverage_satisfied,
            oracle.clearance_floor_device_px,
            g.ink_candidates_found,
            oracle.clearance_floor_device_px,
            g.background_candidates_found,
            g.ink_spacing_relaxed,
            g.background_spacing_relaxed,
        )
        .unwrap();

        writeln!(
            s,
            "| class | subpath_index | staff (x, y) | device (x, y) | clearance (device px) | hole evidence |"
        )
        .unwrap();
        writeln!(s, "|---|---|---|---|---|---|").unwrap();
        for p in &g.points {
            let class = match p.class {
                SampleClass::Ink => "Ink",
                SampleClass::Background => "Background",
            };
            let subpath = match p.subpath_index {
                Some(i) => i.to_string(),
                None => "-".to_string(),
            };
            let evidence = match &p.hole_evidence {
                Some(ev) => format!(
                    "inside_outer_contour={}, even_odd_filled={}, nonzero_filled={} (outer ring {})",
                    ev.inside_outer_contour, ev.even_odd_filled, ev.nonzero_filled, ev.outer_contour_ring_index
                ),
                None => "-".to_string(),
            };
            writeln!(
                s,
                "| {class} | {subpath} | ({:.4}, {:.4}) | ({:.2}, {:.2}) | {:.3} | {evidence} |",
                p.staff.0, p.staff.1, p.device.0, p.device.1, p.clearance_device_px
            )
            .unwrap();
        }
        writeln!(s).unwrap();
    }

    s
}

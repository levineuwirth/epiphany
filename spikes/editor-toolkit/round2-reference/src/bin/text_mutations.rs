//! `text_mutations` — recipe §11's M4, M5 and M6, **executed** against the
//! real frozen fixtures.
//!
//! `round2-diff`'s `selftest` binary covers M1/M2/M3/M3B/M7/M8/M10 on
//! synthetic geometry, because those mutations are geometric and need no
//! fonts. M4, M5 and M6 are not: they are *text* mutations — a ligature
//! unligated, a combining acute omitted, a segment rendered in the wrong face
//! — and each one is only meaningful against shaped glyphs from the declared
//! faces. Recipe revision 2 asserted all three killed D4 and had executed none
//! of them; this binary is the fix, and its exit code is the point.
//!
//! Running it corrected the recipe immediately: **M4 does not kill D4.** An
//! `ff` ligature and two `f` glyphs carry very nearly the same ink (measured:
//! 0.07% of whole-image mass, 1.20% inside the ligature's own region, against
//! a 2% tolerance), so a *mass* rule is simply the wrong instrument for a
//! *shape* substitution. D1 — which asks where the ink is, not how much — sees
//! it at once (221 differing pixels outside the edge band). Recipe revision 3
//! assigns M4 to D1 and records the margin; see `run_m4`.
//!
//! ## Rules of engagement
//!
//! * Every mutation is applied to a **clone of the loaded fixture**, and the
//!   fixture file on disk is never touched.
//! * Every diff is taken against the **unmutated** reference's regions. A
//!   region list recomputed from the mutated glyphs would shrink or move with
//!   the mutation and score it against a box that had already accommodated it
//!   — the same trap `round2-diff`'s `selftest` documents for its literal
//!   region table.
//! * Every substituted glyph id and advance is **measured from the real
//!   faces** through `round2_textkit::shape`, never guessed and never
//!   hard-coded. The measurements are anchor-asserted first (e.g. the probe
//!   for a standalone `f` must not come back as the `ff` ligature id), so a
//!   mutation that silently became a no-op fails here rather than passing as
//!   "did not kill".
//! * A required kill that does not happen is a **blocking failure** and this
//!   binary exits non-zero.

use std::path::PathBuf;

use round2_reference::FacePolicy;
use round2_textkit::faces::{self, FaceResolution, LoadedFace};
use round2_textkit::output;
use round2_textkit::shape::shape_text;
use round2_textkit::types::{SpikePositionedGlyph, SpikeResolvedText};

const WIDTH: u32 = round2_reference::WIDTH;
const HEIGHT: u32 = round2_reference::HEIGHT;

/// F-A's `ff` ligature, recipe §4: "cluster at byte 9 spans `ff` -> gid 234".
const FA_LIGATURE_GID: u32 = 234;
/// F-E's composed `é`, recipe §4: "`e`+U+0301 composes to gid 198".
const FE_COMPOSED_GID: u32 = 198;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures_path = manifest_dir
        .parent()
        .expect("round2-reference has a parent directory")
        .join("round2-textkit")
        .join("fixtures.json");

    let fixtures = output::load_fixtures(&fixtures_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} — run `cargo run -p round2-textkit --bin generate` first",
            fixtures_path.display()
        )
    });

    let resolved = faces::resolve_declared_chain();
    let missing: Vec<&std::path::PathBuf> = resolved
        .iter()
        .filter_map(|r| match r {
            FaceResolution::Missing { path } => Some(path),
            FaceResolution::Loaded(_) => None,
        })
        .collect();
    if !missing.is_empty() {
        for path in &missing {
            println!("NOT RUN: {}", path.display());
        }
        println!(
            "{} of {} declared faces are absent (pin 14: environment absence, not a failure). \
             M4/M5/M6 were not run.",
            missing.len(),
            resolved.len()
        );
        return;
    }
    let loaded: Vec<LoadedFace> = resolved
        .into_iter()
        .map(|r| match r {
            FaceResolution::Loaded(lf) => lf,
            FaceResolution::Missing { .. } => unreachable!("handled above"),
        })
        .collect();

    let fixture = |id: &str| -> SpikeResolvedText {
        fixtures
            .fixtures
            .iter()
            .find(|f| f.id == id)
            .unwrap_or_else(|| panic!("fixture {id} is missing from fixtures.json"))
            .resolved
            .clone()
    };

    println!("=== recipe §11 M4/M5/M6 against the real frozen fixtures ===\n");

    let mut failures: Vec<String> = Vec::new();

    run_m4(&fixture("F-A"), &loaded, &mut failures);
    run_m5(&fixture("F-E"), &loaded, &mut failures);
    run_m6(&fixture("F-B"), &loaded, &mut failures);

    println!("=== failures (blocking) ===");
    if failures.is_empty() {
        println!(
            "  none: M4 killed D1, M5 and M6 killed D4 (and everything else), and M6's emitter \
             refusal fired before any raster existed — each as recipe §11 revision 3 requires."
        );
    } else {
        for f in &failures {
            println!("  FAILURE: {f}");
        }
        eprintln!(
            "\ntext_mutations FAILED: {} blocking failure(s)",
            failures.len()
        );
        std::process::exit(1);
    }
}

/// Builds a fixture's raster and D4 regions, refusing anything the emitter
/// refuses.
fn raster(
    label: &str,
    rt: &SpikeResolvedText,
    faces: &[LoadedFace],
    policy: FacePolicy,
) -> round2_reference::FixtureRasterResult {
    round2_reference::build_fixture_raster(label, rt, faces, WIDTH, HEIGHT, policy)
        .unwrap_or_else(|e| panic!("{label}: {e}"))
}

/// Diffs a mutated raster against the reference, using the **reference's**
/// regions, and reports whether D4 fired.
fn score(
    label: &str,
    reference: &round2_reference::FixtureRasterResult,
    mutated: &[u8],
) -> round2_diff::DiffReport {
    round2_diff::validate_rgba(mutated, WIDTH, HEIGHT)
        .unwrap_or_else(|e| panic!("{label}: mutated buffer is malformed: {e}"));
    let r = round2_diff::diff(&reference.rgba, mutated, WIDTH, HEIGHT, &reference.regions)
        .unwrap_or_else(|e| panic!("{label}: diff call failed: {e}"));
    println!(
        "  D1 outside-band differing px = {} (pass = {})",
        r.d1_pixels_outside_band_differing, r.d1_pass
    );
    println!(
        "  D2 ref_mass = {:.1} cand_mass = {:.1} delta = {:.4}% (pass = {})",
        r.reference_ink_mass,
        r.candidate_ink_mass,
        r.d2_relative_delta * 100.0,
        r.d2_pass
    );
    match (r.d3_delta, r.d3_pass) {
        (Some(d), Some(p)) => println!(
            "  D3 centroid delta = ({:.4}, {:.4}) px (pass = {p})",
            d.0, d.1
        ),
        _ => println!("  D3 undefined (one side has zero ink)"),
    }
    match &r.d4_worst {
        Some(w) => println!(
            "  D4 {} regions; worst = {:?} ref {:.1} -> cand {:.1} ({:.4}%) (pass = {})",
            r.d4_regions.len(),
            w.label,
            w.reference_mass,
            w.candidate_mass,
            w.relative_delta * 100.0,
            r.d4_pass
        ),
        None => println!("  D4 no regions"),
    }
    let failing: Vec<&str> = r
        .d4_regions
        .iter()
        .filter(|x| !x.pass)
        .map(|x| x.label.as_str())
        .collect();
    println!("  D4 failing regions ({}): {:?}", failing.len(), failing);
    // The top regions by relative delta, whether or not any of them failed.
    // When D4 stays silent, "how close did it come" is the measurement that
    // decides whether the rule is blind or merely under-tuned, and a rule
    // that reports only pass/fail cannot answer it.
    let mut ranked: Vec<&round2_diff::RegionMass> = r.d4_regions.iter().collect();
    ranked.sort_by(|a, b| b.relative_delta.partial_cmp(&a.relative_delta).unwrap());
    for reg in ranked.iter().take(3) {
        println!(
            "    region {:?}: ref {:.1} -> cand {:.1} ({:.4}%)",
            reg.label,
            reg.reference_mass,
            reg.candidate_mass,
            reg.relative_delta * 100.0
        );
    }
    println!(
        "  {label}: d1_pass={} d2_pass={} d3_pass={:?} d4_pass={}",
        r.d1_pass, r.d2_pass, r.d3_pass, r.d4_pass
    );
    r
}

/// **M4 — the `ff` ligature replaced by two unligated `f` glyphs**, which is
/// what a consumer that re-shapes the source string with a differently
/// configured shaper produces.
///
/// The two replacement glyphs are placed at the ligature's own origin and one
/// standalone-`f` advance to its right; **every following glyph stays where it
/// was.** A real unligated shaping would also shift the rest of the run, and
/// that shift would fire D4 on every downstream region — which would prove
/// only that D4 notices a global translation, something M1 and M7 already
/// establish. Confining the change to the ligature's own box is the sharper
/// question: does D4 catch an error inside one glyph's bounds while everything
/// around it is pixel-identical?
fn run_m4(reference_rt: &SpikeResolvedText, faces: &[LoadedFace], failures: &mut Vec<String>) {
    println!("--- M4: F-A's `ff` ligature replaced by two unligated `f` glyphs ---");

    // Measure a standalone `f`'s glyph id and advance from the real face.
    // "fa" is used rather than "f" because a single-glyph run carries no
    // advance to read; `fa` is not a ligature in either declared face, which
    // the anchor assertions below establish rather than assume.
    let probe = shape_text("fa", faces);
    assert_eq!(
        probe.segments.len(),
        1,
        "M4 probe: `fa` must itemize to one segment"
    );
    let pg = &probe.segments[0].glyphs;
    assert_eq!(
        pg.len(),
        2,
        "M4 probe: `fa` shaped to {} glyphs — if it ligated, this probe cannot measure a \
         standalone `f` and the mutation would be meaningless",
        pg.len()
    );
    let f_gid = pg[0].glyph_id;
    let f_advance = pg[1].offset.x - pg[0].offset.x;
    assert_ne!(
        f_gid, FA_LIGATURE_GID,
        "M4 probe: the standalone `f` came back as the ligature id — the mutation would be a no-op"
    );
    assert!(
        f_advance > 0.0,
        "M4 probe: measured a non-positive advance for `f` ({f_advance})"
    );
    println!("  measured: standalone `f` = gid {f_gid}, advance {f_advance} staff space");

    let mut mutated = reference_rt.clone();
    let seg = &mut mutated.segments[0];
    let lig_positions: Vec<usize> = seg
        .glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| g.glyph_id == FA_LIGATURE_GID)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        lig_positions.len(),
        1,
        "M4: expected exactly one gid-{FA_LIGATURE_GID} ligature in F-A, found {}",
        lig_positions.len()
    );
    let at = lig_positions[0];
    let lig = seg.glyphs[at].clone();
    let second = SpikePositionedGlyph {
        glyph_id: f_gid,
        offset: round2_textkit::types::SpikePoint::new(
            round2_textkit::quantize::quantize_component(lig.offset.x + f_advance),
            lig.offset.y,
        ),
        transform: lig.transform,
    };
    seg.glyphs[at] = SpikePositionedGlyph {
        glyph_id: f_gid,
        ..lig
    };
    seg.glyphs.insert(at + 1, second);
    println!(
        "  mutated: 1 ligature glyph -> 2 `f` glyphs at x = {} and {}",
        seg.glyphs[at].offset.x,
        seg.glyphs[at + 1].offset.x
    );

    let reference = raster("F-A", reference_rt, faces, FacePolicy::Enforce);
    let m = raster("F-A/M4", &mutated, faces, FacePolicy::Enforce);
    let r = score("M4", &reference, &m.rgba);

    // **Recipe revision 3 requires D1 here, not D4, and the number below is
    // why.** Revision 2 assigned M4 to D4 by analogy with M3 (a dropped
    // glyph), and executing it showed the analogy is false: an `ff` ligature
    // and two `f` glyphs carry very nearly the *same ink*, so a mass rule is
    // blind to the substitution by construction, not by mis-tuning. What
    // differs is *where* the ink sits — the ligature's joined crossbar versus
    // two separate ones — and that is D1's question, which it answers loudly.
    if r.d1_pass {
        failures.push(format!(
            "M4 (unligated `ff`) did not fail D1 — {} differing pixels outside the edge band. A \
             ligature swapped for its components is the single most likely way a re-shaping \
             consumer diverges, and nothing would catch it",
            r.d1_pixels_outside_band_differing
        ));
    }
    if !r.d4_pass {
        failures.push(
            "M4: D4 FIRED, contradicting recipe §11's declared mass-preserving blind spot. That \
             is not a free win — the recipe states D4 cannot see this, and a recipe that is wrong \
             about its own rule must be corrected rather than quietly benefit"
                .to_string(),
        );
    }
    println!();
}

/// **M5 — the composed `é` drawn as a bare `e`, acute omitted.**
///
/// The base glyph keeps its position: a combining mark carries zero advance,
/// so a consumer that dropped it would not reflow anything. The whole error is
/// therefore confined to two glyph boxes.
///
/// **D2 fires anyway, at 2.55%, and an earlier version of this comment said it
/// would not.** The guess was that two acutes are a negligible share of the
/// image's ink; measured, F-E's whole-image ink mass is 16241.0 and dropping
/// the two acutes takes it to 15826.7 — a difference of 414.3, or 2.55%, over
/// D2's 2% tolerance. The lesson is not about
/// this fixture: D2's sensitivity scales inversely with how much ink is on the
/// page, so the same omission in a full score would be invisible to it. D4 is
/// what catches this independently of page content — 13.28% and 13.19% in the
/// two `é` regions — which is exactly why the rule exists.
fn run_m5(reference_rt: &SpikeResolvedText, faces: &[LoadedFace], failures: &mut Vec<String>) {
    println!("--- M5: F-E's composed `é` drawn as `e`, acute omitted ---");

    let probe = shape_text("e", faces);
    assert_eq!(probe.segments.len(), 1, "M5 probe: `e` must be one segment");
    assert_eq!(
        probe.segments[0].glyphs.len(),
        1,
        "M5 probe: `e` must shape to exactly one glyph"
    );
    let e_gid = probe.segments[0].glyphs[0].glyph_id;
    assert_ne!(
        e_gid, FE_COMPOSED_GID,
        "M5 probe: bare `e` came back as the composed `é` id — the mutation would be a no-op"
    );
    println!("  measured: bare `e` = gid {e_gid}, composed `é` = gid {FE_COMPOSED_GID}");

    let mut mutated = reference_rt.clone();
    let mut replaced = 0usize;
    for seg in &mut mutated.segments {
        for g in &mut seg.glyphs {
            if g.glyph_id == FE_COMPOSED_GID {
                g.glyph_id = e_gid;
                replaced += 1;
            }
        }
    }
    assert_eq!(
        replaced, 2,
        "M5: F-E carries the composed glyph twice (recipe §4: at byte 3 and byte 16); replaced {replaced}"
    );
    println!("  mutated: {replaced} composed glyphs -> bare `e`");

    let reference = raster("F-E", reference_rt, faces, FacePolicy::Enforce);
    let m = raster("F-E/M5", &mutated, faces, FacePolicy::Enforce);
    if score("M5", &reference, &m.rgba).d4_pass {
        failures.push(
            "M5 (omitted acute) did not fail D4 — a dropped diacritic is invisible to the \
             differential, which would make check 1 unable to distinguish `resumé` from `resume`"
                .to_string(),
        );
    }
    println!();
}

/// **M6 — F-B's Hebrew segment forced onto face 0**, the host substitution
/// W3 §5 check 2 exists to forbid.
///
/// Two halves, and both are required:
///
/// 1. The **emitter refuses** it. `FacePolicy::Enforce` checks that every
///    segment's declared face has a `cmap` entry for every codepoint in that
///    segment's own source range, so this never reaches a raster. This is the
///    half recipe revision 2 claimed and did not implement.
/// 2. **Forced past the refusal, D4 fires.** Pagella's glyph ids in the range
///    Liberation Serif assigns to Hebrew are either absent or unrelated
///    letterforms, so what gets drawn in the Hebrew segment's boxes is not
///    what belongs there — and the differential must say so, because a future
///    candidate might reach the same state by a route this emitter does not
///    control.
fn run_m6(reference_rt: &SpikeResolvedText, faces: &[LoadedFace], failures: &mut Vec<String>) {
    println!("--- M6: F-B's Hebrew segment forced onto face 0 (host substitution) ---");

    let mut mutated = reference_rt.clone();
    let hebrew = mutated
        .segments
        .iter()
        .position(|s| s.face == Some(1))
        .expect("M6: F-B must have a segment on face 1");
    println!(
        "  segment {hebrew} (source bytes {}..{}) moved from face 1 to face 0",
        mutated.segments[hebrew].source.start, mutated.segments[hebrew].source.end
    );
    mutated.segments[hebrew].face = Some(0);

    // Half 1: the refusal.
    match round2_reference::build_fixture_raster(
        "F-B/M6",
        &mutated,
        faces,
        WIDTH,
        HEIGHT,
        FacePolicy::Enforce,
    ) {
        Err(e) => println!("  emitter REFUSED, as required: {e}"),
        Ok(_) => {
            failures.push(
                "M6: the emitter did NOT refuse a segment whose declared face has no cmap \
                 coverage for its own codepoints — recipe §11's structural safeguard is absent"
                    .to_string(),
            );
            println!("  FAILURE: the emitter accepted the substitution.");
        }
    }

    // Half 2: forced past it.
    let reference = raster("F-B", reference_rt, faces, FacePolicy::Enforce);
    let m = raster(
        "F-B/M6",
        &mutated,
        faces,
        FacePolicy::AllowUncoveredForM6Only,
    );
    println!(
        "  forced: {} glyphs drawn, {} empty (a glyph id with no outline in the substituted face \
         draws nothing at all)",
        m.drawn_glyph_count, m.empty_glyph_count
    );
    if score("M6", &reference, &m.rgba).d4_pass {
        failures.push(
            "M6 (host-substituted face), forced past the refusal, did not fail D4 — the \
             differential cannot see a segment drawn from the wrong face"
                .to_string(),
        );
    }
    println!();
}

//! Loads and validates the candidate-neutral apparatus Packet 2A built:
//! fixtures, the hit-test probe table, and the per-fixture reference raster
//! + regions. Every failure here names the specific file and what was wrong
//! with it — see [`load_all`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use round2_diff::GlyphRegion;
use round2_textkit::hittest::HitTestProbeFile;
use round2_textkit::output::FixtureFile;

/// Pin 4's offscreen target, restated as a literal (the same discipline
/// every other crate in this workspace uses: a loader checks a file against
/// a stated constant, never trusts the file to agree with itself).
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
const EXPECTED_RGBA_LEN: usize = (WIDTH as usize) * (HEIGHT as usize) * 4;

/// The on-disk shape of one entry in `<id>.regions.json`
/// (`round2-reference/output/`), matching the fields `round2-reference`'s
/// own `RegionRecord` writes. Deserialized here rather than depended on
/// directly, because `round2-reference` pulls in `round2-svgref`, which
/// pulls in `resvg`/`usvg`/`tiny-skia` — exactly the rendering dependencies
/// this crate's neutrality boundary forbids. The region *files* are neutral
/// data; the crate that produced them is not.
///
/// **This is an implicit cross-crate schema with no shared type** —
/// `round2-reference`'s own `RegionRecord` and this one are two
/// independent hand-written structs that happen to agree on field names.
/// `deny_unknown_fields` is what turns a future drift between them into a
/// *named parse error at this crate's boundary* rather than a silently
/// ignored field: without it, serde drops unknown fields by default, and a
/// field `round2-reference` starts writing (or renames) would pass through
/// here unnoticed.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionRecord {
    label: String,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl From<RegionRecord> for GlyphRegion {
    fn from(r: RegionRecord) -> Self {
        GlyphRegion {
            label: r.label,
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

/// One fixture's reference apparatus: the rasterized reference image
/// (already length-checked), its D4 regions (already checked non-empty),
/// and the paths they were loaded from (traceability for a `FAIL`).
#[derive(Clone, Debug)]
pub struct ReferenceFixture {
    pub fixture_id: String,
    pub reference_rgba: Vec<u8>,
    pub regions: Vec<GlyphRegion>,
    pub rgba_path: PathBuf,
    pub regions_path: PathBuf,
}

/// Every candidate-neutral input Packet 2A built, loaded and validated in
/// one call ([`load_all`]).
#[derive(Debug)]
pub struct NeutralInputs {
    pub fixtures: FixtureFile,
    pub hittest_probes: HitTestProbeFile,
    /// Keyed by fixture id (`F-A`..`F-E`).
    pub reference: BTreeMap<String, ReferenceFixture>,
}

/// Loads `fixtures.json`, `hittest_probes.json`, and every fixture's
/// reference raster + regions, from the standard Packet 2A layout under
/// `spike_root` (`round2-textkit/fixtures.json`,
/// `round2-textkit/hittest_probes.json`,
/// `round2-reference/output/<id>.rgba`,
/// `round2-reference/output/<id>.regions.json`).
///
/// Every failure names the specific file and what was wrong with it:
///
/// - `fixtures.json` / `hittest_probes.json`: read/parse errors, or a
///   [`round2_textkit::output::FixtureFile::validate`] /
///   [`round2_textkit::hittest::HitTestProbeFile::validate`] failure
///   (digest mismatch, probe-table drift, ...) — propagated verbatim; those
///   loaders already name the path and the specific disagreement.
/// - `<id>.rgba`: refused if its length is not exactly `1920 * 1080 * 4`
///   bytes ([`WIDTH`] x [`HEIGHT`] x 4 RGBA8), naming the file and the
///   actual length.
/// - `<id>.regions.json`: refused if missing, unparsable, or **empty**.
///   This crate refuses an empty region list itself, naming the file,
///   rather than silently handing it to `round2_diff::diff` — which also
///   refuses an empty list (`diff` panics on nothing, it returns an `Err`),
///   but with a message that has no idea which file on disk was empty.
pub fn load_all(spike_root: &Path) -> Result<NeutralInputs, String> {
    let fixtures_path = spike_root.join("round2-textkit/fixtures.json");
    let fixtures = round2_textkit::output::load_fixtures(&fixtures_path)?;

    let hittest_path = spike_root.join("round2-textkit/hittest_probes.json");
    let hittest_probes = round2_textkit::hittest::load_hittest_probes(&hittest_path, &fixtures)?;

    let mut reference = BTreeMap::new();
    for f in &fixtures.fixtures {
        let rgba_path = spike_root
            .join("round2-reference/output")
            .join(format!("{}.rgba", f.id));
        let rgba = std::fs::read(&rgba_path).map_err(|e| {
            format!(
                "{}: failed to read reference raster: {e}",
                rgba_path.display()
            )
        })?;
        if rgba.len() != EXPECTED_RGBA_LEN {
            return Err(format!(
                "{}: reference raster is {} bytes, expected exactly {EXPECTED_RGBA_LEN} \
                 ({WIDTH}x{HEIGHT} RGBA8) — a short or padded buffer cannot be sampled safely",
                rgba_path.display(),
                rgba.len()
            ));
        }

        let regions_path = spike_root
            .join("round2-reference/output")
            .join(format!("{}.regions.json", f.id));
        let regions_text = std::fs::read_to_string(&regions_path).map_err(|e| {
            format!(
                "{}: failed to read region file: {e}",
                regions_path.display()
            )
        })?;
        let records: Vec<RegionRecord> = serde_json::from_str(&regions_text).map_err(|e| {
            format!(
                "{}: failed to parse region file: {e}",
                regions_path.display()
            )
        })?;
        if records.is_empty() {
            return Err(format!(
                "{}: region list is empty — refusing here, before this could reach \
                 round2_diff::diff (which also refuses an empty region list, but with a message \
                 that does not name which file on disk was empty)",
                regions_path.display()
            ));
        }
        let regions: Vec<GlyphRegion> = records.into_iter().map(GlyphRegion::from).collect();

        reference.insert(
            f.id.clone(),
            ReferenceFixture {
                fixture_id: f.id.clone(),
                reference_rgba: rgba,
                regions,
                rgba_path,
                regions_path,
            },
        );
    }

    Ok(NeutralInputs {
        fixtures,
        hittest_probes,
        reference,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real spike workspace root: this crate's manifest directory is
    /// `spikes/editor-toolkit/round2-candidatekit`, one level below root.
    fn real_spike_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn read_real(rel: &str) -> Vec<u8> {
        std::fs::read(real_spike_root().join(rel))
            .unwrap_or_else(|e| panic!("failed to read real {rel}: {e}"))
    }

    /// A fresh, uniquely named directory under the OS temp dir (never under
    /// the repo working tree, so these tests cannot leave stray files for
    /// `git status` to notice), laid out like a spike root's
    /// `round2-textkit/` + `round2-reference/output/` — enough for
    /// `load_all` to be pointed at it.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "round2-candidatekit-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("round2-textkit")).unwrap();
        std::fs::create_dir_all(dir.join("round2-reference/output")).unwrap();
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
    }

    /// Copies the real, committed, valid `fixtures.json` and
    /// `hittest_probes.json` into `dir` — the two files every scenario
    /// below needs unmutated so the failure under test is isolated to the
    /// one file each test actually breaks.
    fn seed_valid_fixtures_and_hittest(dir: &Path) {
        write(
            &dir.join("round2-textkit/fixtures.json"),
            &read_real("round2-textkit/fixtures.json"),
        );
        write(
            &dir.join("round2-textkit/hittest_probes.json"),
            &read_real("round2-textkit/hittest_probes.json"),
        );
    }

    #[test]
    fn load_all_succeeds_against_the_real_committed_apparatus() {
        let inputs = load_all(&real_spike_root()).expect("real apparatus must load");
        assert_eq!(inputs.fixtures.fixtures.len(), 5);
        assert_eq!(inputs.reference.len(), 5);
        for id in ["F-A", "F-B", "F-C", "F-D", "F-E"] {
            assert!(inputs.reference.contains_key(id), "missing {id}");
            let rf = &inputs.reference[id];
            assert_eq!(rf.reference_rgba.len(), EXPECTED_RGBA_LEN);
            assert!(!rf.regions.is_empty());
        }
    }

    /// Required kill: a `.rgba` of the wrong length is refused, naming the
    /// file.
    #[test]
    fn a_wrong_length_rgba_is_refused_and_the_file_is_named() {
        let dir = scratch_dir("wrong-length-rgba");
        seed_valid_fixtures_and_hittest(&dir);
        write(
            &dir.join("round2-reference/output/F-A.rgba"),
            &vec![0u8; 100],
        );
        let err = load_all(&dir).unwrap_err();
        assert!(err.contains("F-A.rgba"), "{err}");
        assert!(err.contains("100 bytes"), "{err}");
        assert!(err.contains("8294400"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: an empty region list is refused here — with a message
    /// naming this file — rather than silently reaching
    /// `round2_diff::diff`.
    #[test]
    fn an_empty_region_list_is_refused_before_it_could_reach_diff() {
        let dir = scratch_dir("empty-regions");
        seed_valid_fixtures_and_hittest(&dir);
        write(
            &dir.join("round2-reference/output/F-A.rgba"),
            &vec![0u8; EXPECTED_RGBA_LEN],
        );
        write(&dir.join("round2-reference/output/F-A.regions.json"), b"[]");
        let err = load_all(&dir).unwrap_err();
        assert!(err.contains("F-A.regions.json"), "{err}");
        assert!(err.contains("empty"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A missing region file (never written at all, as opposed to written
    /// empty) is refused and named — the other half of "missing region
    /// file" in the required API's failure list, distinct from the
    /// empty-but-present case above.
    #[test]
    fn a_missing_region_file_is_refused_and_named() {
        let dir = scratch_dir("missing-regions");
        seed_valid_fixtures_and_hittest(&dir);
        write(
            &dir.join("round2-reference/output/F-A.rgba"),
            &vec![0u8; EXPECTED_RGBA_LEN],
        );
        // F-A.regions.json is deliberately never written.
        let err = load_all(&dir).unwrap_err();
        assert!(err.contains("F-A.regions.json"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: a tampered `fixtures.json` digest is refused. Uses
    /// the same mutation `round2-textkit`'s own
    /// `validate_kills_a_changed_glyph_id` test does (change one glyph id
    /// deep inside a fixture, leaving every named/counted field valid) —
    /// only the whole-artifact digest catches it, which is exactly why this
    /// crate's loader must not skip that check.
    #[test]
    fn a_tampered_fixtures_digest_is_refused() {
        let dir = scratch_dir("tampered-digest");
        let mut tampered = round2_textkit::output::load_fixtures(
            &real_spike_root().join("round2-textkit/fixtures.json"),
        )
        .expect("real fixtures.json must load");
        let g = &mut tampered.fixtures[0].resolved.segments[0].glyphs[3];
        g.glyph_id = 9999;
        let json = serde_json::to_string_pretty(&tampered).unwrap();
        write(&dir.join("round2-textkit/fixtures.json"), json.as_bytes());
        let err = load_all(&dir).unwrap_err();
        assert!(err.contains("digest"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F5: `<id>.regions.json` is an implicit contract between
    /// `round2-reference` (which writes it) and this crate (which reads
    /// it), with no shared type. An extra field must be refused **by
    /// name**, not silently dropped — that is what turns a future schema
    /// drift into a named parse error here instead of quiet data loss.
    #[test]
    fn an_unknown_field_in_a_region_record_is_refused_by_name() {
        let json = serde_json::json!([{
            "label": "x",
            "x0": 0,
            "y0": 0,
            "x1": 1,
            "y1": 1,
            "smuggled_field": 1
        }]);
        let err = serde_json::from_value::<Vec<RegionRecord>>(json)
            .unwrap_err()
            .to_string();
        assert!(err.contains("smuggled_field"), "{err}");
    }
}

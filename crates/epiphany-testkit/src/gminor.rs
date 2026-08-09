//! The `[7f]` conformance gate (G-minor, `spec/PLAN_GMINOR_SCHEMA_MINOR.md`
//! §4, pin 11): an independent oracle over the manifest's carried schema
//! minor, built from **known, decodable, in-tree artifacts**.
//!
//! `epiphany-textproj` deliberately never decodes edit-barrier bytes (pin 8:
//! it has no `epiphany-layout-ir` dependency), so it cannot itself check
//! whether a manifest's carried `SchemaVersion` matches what its declared
//! edit barriers require. This module is the independent check that *can*:
//! `epiphany-testkit` depends on `epiphany-layout-ir`, so it can decode
//! `ExtensionDeclaration::edit_barriers`, walk every barrier's
//! `prohibited_operation_kinds`, and recompute the exact aggregate minor the
//! manifest should carry (`epiphany_layout_ir::barrier::edit_barriers_introduced_minor`).
//!
//! **What this gate is not.** It is not evidence that `epiphany-textproj`
//! validates arbitrary hand-edited documents — pin 11's ruled design (a) is
//! that `textproj` stays a *preserving* producer, carrying whatever
//! `SchemaVersion` a document declares verbatim, and a hand-edited document
//! whose barrier bytes changed while its carried version did not is
//! undetectable at that layer by construction. This gate validates a
//! separate, narrower claim: that *this crate's own fixtures*, built directly
//! against `epiphany-bundle`, are exactly and correctly stamped. An
//! undecodable barrier blob (a foreign extension's bytes, a corrupt encoding)
//! is reported as **not-checkable**, never silently counted as a pass.

use epiphany_bundle::{
    Bundle, DocumentId, ExtensionDeclaration, ExtensionId, FileUuid, Manifest, MemStore,
    SchemaVersion, SemVer,
};
use epiphany_layout_ir::{
    decode_edit_barriers, edit_barriers_introduced_minor, encode_edit_barriers, BarrierCondition,
    BarrierScope, EditBarrier,
};
use epiphany_ops::OperationKindTag;

/// The oracle's verdict for one manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GminorVerdict {
    /// Every barrier blob decoded; `expected` is the recomputed aggregate,
    /// `actual` is the carried superblock value, and `matches` is whether
    /// they are **exactly equal** — not `>=` (pin 11.1: equality alone
    /// catches over-stamping after a contributing barrier is removed).
    Checked {
        expected: SchemaVersion,
        actual: SchemaVersion,
        matches: bool,
    },
    /// At least one barrier blob failed to decode (a foreign extension, or
    /// deliberately corrupt bytes). The exact epoch cannot be established, so
    /// this is reported as not-checkable — **never** as a pass (pin 11.3).
    NotCheckable(String),
}

/// A single edit barrier naming `tags`, with an otherwise-trivial
/// whole-score, unconditional scope — the minimum shape needed to exercise
/// `prohibited_operation_kinds`.
fn barrier(tags: &[OperationKindTag]) -> EditBarrier {
    EditBarrier {
        scope: BarrierScope::WholeScore,
        affected_object_kinds: Vec::new(),
        prohibited_operation_kinds: tags.to_vec(),
        condition: BarrierCondition::Always,
    }
}

fn extension_declaration(id_byte: u8, barriers: &[EditBarrier]) -> ExtensionDeclaration {
    ExtensionDeclaration {
        extension_id: ExtensionId([id_byte; 16]),
        version: SemVer::new(1, 0, 0),
        required: false,
        preserved_chunk_roots: Vec::new(),
        affected_object_kinds: Vec::new(),
        edit_barriers: encode_edit_barriers(barriers),
    }
}

/// Builds a bundle whose manifest declares `extensions` and is stamped at
/// `stamped_version` — the two independent knobs every fixture below varies.
fn build_bundle(
    seed: u8,
    extensions: Vec<ExtensionDeclaration>,
    stamped_version: SchemaVersion,
) -> Bundle<MemStore> {
    let mut manifest = Manifest::empty(DocumentId([seed; 16]));
    manifest.extension_declarations = extensions;
    Bundle::create_versioned(
        MemStore::new(),
        FileUuid([seed; 16]),
        manifest,
        stamped_version,
        crate::production_caps(),
    )
    .expect("fixture manifest is emittable")
}

/// The independent oracle (pin 11): recomputes the exact aggregate minor a
/// bundle's manifest should carry from its decodable edit barriers, and
/// compares it **by equality** to the carried superblock value.
pub fn check(bundle: &Bundle<MemStore>) -> GminorVerdict {
    let mut all_barriers = Vec::new();
    for declaration in &bundle.manifest().extension_declarations {
        match decode_edit_barriers(&declaration.edit_barriers) {
            Ok(mut decoded) => all_barriers.append(&mut decoded),
            Err(error) => {
                return GminorVerdict::NotCheckable(format!(
                    "extension {:?}: edit_barriers did not decode: {error:?}",
                    declaration.extension_id
                ))
            }
        }
    }
    let epoch_max = edit_barriers_introduced_minor(&all_barriers);
    let expected = SchemaVersion::for_major_at_epoch(0, epoch_max);
    let actual = bundle.superblock().manifest_schema_version;
    GminorVerdict::Checked {
        expected,
        actual,
        matches: expected == actual,
    }
}

/// Fixture: a manifest naming barrier tag 31 (`CreateInstrument`, epoch 8),
/// correctly stamped at exactly that epoch. Positive: the oracle must find
/// `matches: true`.
pub fn fixture_correctly_stamped_tag_31() -> Bundle<MemStore> {
    let declarations = vec![extension_declaration(
        1,
        &[barrier(&[OperationKindTag::CreateInstrument])],
    )];
    build_bundle(1, declarations, SchemaVersion::new(0, 8))
}

/// Fixture: a manifest naming only baseline tags, correctly stamped at the
/// baseline `{0, 1}`. Positive.
pub fn fixture_baseline_only() -> Bundle<MemStore> {
    let declarations = vec![extension_declaration(
        2,
        &[barrier(&[
            OperationKindTag::InsertEvent,
            OperationKindTag::DeleteEvent,
        ])],
    )];
    build_bundle(2, declarations, SchemaVersion::V0)
}

/// Negative fixture (pin 11.2, required #1): a blob naming tag 31 (epoch 8),
/// but the manifest carries the **baseline** version — under-stamped. The
/// oracle must find `matches: false`.
pub fn fixture_understamped() -> Bundle<MemStore> {
    let declarations = vec![extension_declaration(
        3,
        &[barrier(&[OperationKindTag::CreateInstrument])],
    )];
    build_bundle(3, declarations, SchemaVersion::V0)
}

/// Negative fixture (pin 11.2, required #2): two barriers, one naming tag 31
/// (epoch 8, the sole max contributor) and one baseline-only. The barrier
/// contributing the maximum is then **removed** (only the baseline barrier
/// remains), but the manifest retains the old aggregate `{0, 8}` — exactly
/// the over-stamp pin 6.5 warns "blindly retaining the previous aggregate"
/// produces. The oracle must find `matches: false`.
pub fn fixture_overstamped_after_barrier_removal() -> Bundle<MemStore> {
    let declarations = vec![extension_declaration(
        4,
        &[barrier(&[OperationKindTag::InsertEvent])],
    )];
    build_bundle(4, declarations, SchemaVersion::new(0, 8))
}

/// Fixture: a manifest declaring an extension whose `edit_barriers` blob is
/// deliberately corrupt (not a valid canonical `EditBarrier` set encoding).
/// The oracle must report `NotCheckable`, never a pass (pin 11.3).
pub fn fixture_undecodable_barrier_blob() -> Bundle<MemStore> {
    let declaration = ExtensionDeclaration {
        extension_id: ExtensionId([5; 16]),
        version: SemVer::new(1, 0, 0),
        required: false,
        preserved_chunk_roots: Vec::new(),
        affected_object_kinds: Vec::new(),
        // Not a canonical edit-barrier-set encoding: garbage bytes that a
        // real foreign/corrupt extension could plausibly carry.
        edit_barriers: vec![0xFF, 0x00, 0x13, 0x37, 0xAB],
    };
    build_bundle(5, vec![declaration], SchemaVersion::V0)
}

/// Runs the whole `[7f]` gate: every fixture, checked against its expected
/// verdict shape. Returns `(checked, not_checkable)` — the two counts the
/// conformance suite reports (pin 11.4: describing the gate's actual reach
/// honestly, not implying it validates arbitrary edits).
pub fn run_gate() -> (usize, usize) {
    let mut checked = 0usize;
    let mut not_checkable = 0usize;

    let positive_correct = check(&fixture_correctly_stamped_tag_31());
    assert_eq!(
        positive_correct,
        GminorVerdict::Checked {
            expected: SchemaVersion::new(0, 8),
            actual: SchemaVersion::new(0, 8),
            matches: true,
        },
        "a manifest naming tag 31, correctly stamped at minor 8, must check as matching"
    );
    checked += 1;

    let positive_baseline = check(&fixture_baseline_only());
    assert_eq!(
        positive_baseline,
        GminorVerdict::Checked {
            expected: SchemaVersion::V0,
            actual: SchemaVersion::V0,
            matches: true,
        },
        "a manifest naming only baseline tags must check as matching its baseline stamp"
    );
    checked += 1;

    let negative_understamped = check(&fixture_understamped());
    assert_eq!(
        negative_understamped,
        GminorVerdict::Checked {
            expected: SchemaVersion::new(0, 8),
            actual: SchemaVersion::V0,
            matches: false,
        },
        "an under-stamped manifest (tag 31 present, baseline carried) must be caught"
    );
    checked += 1;

    let negative_overstamped = check(&fixture_overstamped_after_barrier_removal());
    assert_eq!(
        negative_overstamped,
        GminorVerdict::Checked {
            expected: SchemaVersion::V0,
            actual: SchemaVersion::new(0, 8),
            matches: false,
        },
        "an over-stamped manifest (max contributor removed, old aggregate retained) must be caught"
    );
    checked += 1;

    match check(&fixture_undecodable_barrier_blob()) {
        GminorVerdict::NotCheckable(_) => not_checkable += 1,
        other => panic!("an undecodable barrier blob must report NotCheckable, got {other:?}"),
    }

    (checked, not_checkable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s8_manifest_naming_tag_31_stamps_8_baseline_tags_stamp_baseline() {
        assert_eq!(
            check(&fixture_correctly_stamped_tag_31()),
            GminorVerdict::Checked {
                expected: SchemaVersion::new(0, 8),
                actual: SchemaVersion::new(0, 8),
                matches: true,
            }
        );
        assert_eq!(
            check(&fixture_baseline_only()),
            GminorVerdict::Checked {
                expected: SchemaVersion::V0,
                actual: SchemaVersion::V0,
                matches: true,
            }
        );
    }

    #[test]
    fn s15_the_gate_fails_the_understamped_fixture() {
        let verdict = check(&fixture_understamped());
        assert_eq!(
            verdict,
            GminorVerdict::Checked {
                expected: SchemaVersion::new(0, 8),
                actual: SchemaVersion::V0,
                matches: false,
            }
        );
    }

    #[test]
    fn s16_the_gate_fails_the_overstamped_fixture() {
        let verdict = check(&fixture_overstamped_after_barrier_removal());
        assert_eq!(
            verdict,
            GminorVerdict::Checked {
                expected: SchemaVersion::V0,
                actual: SchemaVersion::new(0, 8),
                matches: false,
            }
        );
    }

    #[test]
    fn s17_an_undecodable_blob_is_reported_not_checkable_never_a_pass() {
        assert!(matches!(
            check(&fixture_undecodable_barrier_blob()),
            GminorVerdict::NotCheckable(_)
        ));
    }

    #[test]
    fn the_gate_reports_four_checked_and_one_not_checkable() {
        assert_eq!(run_gate(), (4, 1));
    }

    #[test]
    fn correctly_stamped_fixtures_match() {
        assert!(matches!(
            check(&fixture_correctly_stamped_tag_31()),
            GminorVerdict::Checked { matches: true, .. }
        ));
        assert!(matches!(
            check(&fixture_baseline_only()),
            GminorVerdict::Checked { matches: true, .. }
        ));
    }

    #[test]
    fn understamped_fixture_is_caught() {
        assert!(matches!(
            check(&fixture_understamped()),
            GminorVerdict::Checked { matches: false, .. }
        ));
    }

    #[test]
    fn overstamped_fixture_is_caught() {
        assert!(matches!(
            check(&fixture_overstamped_after_barrier_removal()),
            GminorVerdict::Checked { matches: false, .. }
        ));
    }

    #[test]
    fn undecodable_barrier_blob_is_not_checkable_not_a_pass() {
        assert!(matches!(
            check(&fixture_undecodable_barrier_blob()),
            GminorVerdict::NotCheckable(_)
        ));
    }
}

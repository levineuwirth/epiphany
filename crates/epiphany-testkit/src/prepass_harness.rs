//! **F4(H) — Agent H's merge gate** (Agent F; `spec/PHASE2_F_WEEK0_WORKLIST.md`
//! item F4, `spec/PHASE2_QUICKSTART.md` §F "Per-agent harnesses → H").
//!
//! Drives Agent H's spelling + notational-decomposition pre-passes
//! ([`epiphany_core::derive_annotations`]) over [`crate::corpus`] and asserts H's
//! stated acceptance criterion, designed for H's specific failure modes:
//!
//! * **Determinism** — the same score derives byte-identically twice, and a
//!   fixture built twice derives identically. Asserted by structural equality
//!   **and** a canonical fingerprint (`DerivedAnnotations` deliberately has no
//!   codec, so the fingerprint is its `Debug` form over canonically-ordered
//!   `BTreeMap`s — the determinism guarantee H makes).
//! * **Eligibility** — every *eligible* embedded `IdentifiedPitch` carries a
//!   spelling that **realizes the pitch's 12-TET class** (the correctness check
//!   the old constant-`C4` stub fails on the first non-`C` pitch); spelling-
//!   unavailable pitches are *not* spelled; every decomposition **reconstructs
//!   its event's sounding duration** (Chapter 3 invariant 15), independently
//!   recomputed from H's components.
//! * **Precedence** — an authored, engraved-layer `RespellPitch`-style override
//!   takes precedence over the inferred spelling; a non-engraved (layer-tagged)
//!   override does not.
//! * **Non-vacuity** — the F discipline: the gate goes red if H were stubbed.
//!   Across the corpus, spellings must span multiple nominals and include
//!   accidentals (a constant stub yields one nominal, none), and decompositions
//!   must use multiple note values and include a tied multi-component split.
//! * **Materialization pipeline** — the derivation stays deterministic when run
//!   on real reduced scores from the criterion-5 convergence path, so criterion
//!   5 keeps passing with non-trivial pre-pass outputs downstream.

use epiphany_core::{
    derive_annotations, AccidentalId, AnalysisLayerId, CmnNominal, DecompositionSource,
    DerivedAnnotations, Event, EventDuration, MusicalDuration, PitchSpelling, PrePassProfile,
    RationalTime, Score, SpellingAttachment, SpellingDirective, SpellingNominal,
    SpellingProvenance, SpellingScope, SpellingSource, SpellingSourceKind,
};

use crate::corpus;

fn profile() -> PrePassProfile {
    PrePassProfile::default()
}

/// A canonical byte fingerprint of derived annotations
/// ([`DerivedAnnotations::canonical_fingerprint`]): the embedded graph values use
/// their ratified `CanonicalValue` bytes, counts/ids are little-endian. Equal
/// fingerprints ⇒ byte-identical derivations — a normative serialization surface,
/// rather than the (non-normative) `Debug` form the gate used previously.
pub fn fingerprint(ann: &DerivedAnnotations) -> Vec<u8> {
    ann.canonical_fingerprint()
}

// ===========================================================================
// Determinism
// ===========================================================================

/// The same score derives identically twice (pure function), by structural
/// equality and by fingerprint.
pub fn assert_derivation_deterministic(score: &Score) {
    let a = derive_annotations(score, &profile())
        .expect("the default pre-pass algorithms are supported");
    let b = derive_annotations(score, &profile())
        .expect("the default pre-pass algorithms are supported");
    assert_eq!(a, b, "derivation is not a pure function of the score");
    assert_eq!(
        fingerprint(&a),
        fingerprint(&b),
        "derivation fingerprint differs across runs"
    );
}

/// Every corpus fixture, built twice from its deterministic builder, derives
/// byte-identical annotations — the "same score reduced twice ⇒ identical
/// pre-pass annotations" property at corpus scale.
pub fn assert_corpus_deterministic() {
    for f in corpus::corpus() {
        let a = derive_annotations(&(f.build)(), &profile())
            .expect("the default pre-pass algorithms are supported");
        let b = derive_annotations(&(f.build)(), &profile())
            .expect("the default pre-pass algorithms are supported");
        assert_eq!(a, b, "fixture `{}` derivation not deterministic", f.name);
        assert_eq!(
            fingerprint(&a),
            fingerprint(&b),
            "fixture `{}` fingerprint differs across builds",
            f.name
        );
    }
}

// ===========================================================================
// Eligibility + correctness
// ===========================================================================

/// The absolute 12-TET semitone a CMN spelling realizes
/// (`nominal + accidentals + 12*octave`), or `None` for a non-CMN nominal.
/// Independent of H's internals — this is how F checks an inferred spelling names
/// the right pitch *in the right register*. The octave is solved by H
/// (`spelling_from_lof`), so a pitch-class-only check would miss an
/// off-by-an-octave or B♯/C♭ enharmonic-wrap regression.
fn spelling_absolute_semitone(s: &PitchSpelling) -> Option<i32> {
    let nominal = match s.nominal {
        SpellingNominal::Cmn(n) => n,
        _ => return None,
    };
    let mut semis = nominal.chromatic() as i32;
    for a in &s.accidentals {
        semis += match a.as_str() {
            "sharp" => 1,
            "flat" => -1,
            "double-sharp" => 2,
            "double-flat" => -2,
            "natural" => 0,
            _ => return None,
        };
    }
    Some(semis + 12 * s.octave as i32)
}

/// The 12-TET pitch class a CMN spelling realizes (`nominal + accidentals`), or
/// `None` for a non-CMN nominal.
fn spelling_pitch_class(s: &PitchSpelling) -> Option<i32> {
    spelling_absolute_semitone(s).map(|semi| semi.rem_euclid(12))
}

/// Every *eligible* embedded pitch carries a non-trivial spelling that realizes
/// its 12-TET pitch class. A spelling-unavailable pitch is never spelled by the
/// *algorithm*, but an authored attachment MAY surface for it
/// (`req:pitch:authored-uninferred`, Pass 12 P12-H7).
pub fn assert_eligible_pitches_spelled(score: &Score, ann: &DerivedAnnotations) {
    for e in score.events.iter() {
        let mut pitches = Vec::new();
        e.collect_identified_pitches(&mut pitches);
        for ip in pitches {
            match ip.pitch.twelve_tet_class() {
                None => {
                    // P12-H7: an authored spelling may surface for an
                    // unavailable pitch; an *inferred* one is still a bug.
                    if let Some(rs) = ann.spellings.get(&ip.id) {
                        assert!(
                            matches!(rs.provenance, SpellingProvenance::Authored(_)),
                            "spelling-unavailable pitch {:?} was inferred-spelled anyway",
                            ip.id
                        );
                    }
                }
                Some(class) => {
                    let rs = ann.spellings.get(&ip.id).unwrap_or_else(|| {
                        panic!(
                            "eligible pitch {:?} (pc {class}) carries no spelling",
                            ip.id
                        )
                    });
                    // The algorithm's own output must *name the right pitch*. (An
                    // authored override is the user's call; only check inferred.)
                    if matches!(rs.provenance, SpellingProvenance::Inferred) {
                        let got = spelling_pitch_class(&rs.spelling).unwrap_or_else(|| {
                            panic!("inferred spelling of {:?} is not a CMN spelling", ip.id)
                        });
                        assert_eq!(
                            got, class as i32,
                            "inferred spelling of {:?} realizes pc {got}, but the pitch is pc {class}",
                            ip.id
                        );
                        // ...and in the right *register*: the absolute semitone
                        // (octave included) must match the pitch's. H solves the
                        // octave in `spelling_from_lof`; the pitch-class check above
                        // would pass an off-by-an-octave or B♯/C♭-wrap regression.
                        let want_semitone = ip.pitch.twelve_tet_semitone().unwrap_or_else(|| {
                            panic!(
                                "eligible pitch {:?} (pc {class}) has no 12-TET semitone",
                                ip.id
                            )
                        });
                        let got_semitone =
                            spelling_absolute_semitone(&rs.spelling).unwrap_or_else(|| {
                                panic!("inferred spelling of {:?} is not a CMN spelling", ip.id)
                            });
                        assert_eq!(
                            got_semitone, want_semitone,
                            "inferred spelling of {:?} realizes semitone {got_semitone}, but the pitch sounds at {want_semitone} (wrong register)",
                            ip.id
                        );
                    }
                }
            }
        }
    }
}

/// Every decomposition H's *algorithm* emits targets a decomposable event, has
/// a consistent tie chain, and its components' **sounding** durations
/// reconstruct the event's musical duration exactly (Chapter 3 invariant 15),
/// recomputed independently. Authored entries (overrides and the P12-H7
/// authored-uninferred surfacings) are exempt from the algorithm-output
/// invariants: P12-H7 deliberately admits inference-ineligible targets
/// (non-decomposable kinds, non-musical durations, ungriddable spans), and
/// authored well-formedness is invariant 15's graph-level jurisdiction, not
/// the pre-pass gate's.
pub fn assert_decompositions_reconstruct(score: &Score, ann: &DerivedAnnotations) {
    for (eid, d) in &ann.decompositions {
        assert_eq!(
            d.target, *eid,
            "decomposition keyed by {eid:?} targets {:?}",
            d.target
        );
        assert!(!d.components.is_empty(), "empty decomposition for {eid:?}");

        let ev = score
            .events
            .get(*eid)
            .unwrap_or_else(|| panic!("decomposition targets non-live event {eid:?}"));
        if !matches!(d.source, DecompositionSource::Inferred) {
            // Authored entry: surfaced under precedence (override) or P12-H7
            // (authored-uninferred). The remaining checks are algorithm-output
            // guarantees and do not apply.
            continue;
        }
        assert!(
            matches!(ev, Event::Pitched(_) | Event::Unpitched(_) | Event::Rest(_)),
            "decomposition targets a non-decomposable event kind ({eid:?})"
        );

        // Tie chain: every component but the last is tied to the next.
        let n = d.components.len();
        for (i, c) in d.components.iter().enumerate() {
            assert_eq!(
                c.tied_to_next,
                i + 1 < n,
                "decomposition of {eid:?}: component {i} tie flag is wrong"
            );
        }

        // Reconstruction: sum of sounding durations == event duration.
        let mut sum = RationalTime::zero();
        for c in &d.components {
            let ratio = c.tuplet.and_then(|tid| {
                score
                    .cross_cutting
                    .tuplets
                    .iter()
                    .find(|t| t.id == tid)
                    .map(|t| t.ratio)
            });
            sum = sum.add(c.sounding_duration(ratio).rational());
        }
        let dur = match ev.duration() {
            EventDuration::Musical(MusicalDuration(rt)) => rt.clone(),
            other => panic!("decomposed event {eid:?} has a non-musical duration {other:?}"),
        };
        assert_eq!(
            sum, dur,
            "decomposition of {eid:?} reconstructs {sum:?}, but the event duration is {dur:?}"
        );
    }

    // Map and taxonomy counts agree: the effective map is exactly the inferred
    // plus authored-override outcomes (an authored `DecompositionAttachment`
    // outranking `Inferred` replaces the derived one and is counted
    // distinctly, mirroring the spelling buckets), plus authored-only
    // surfacings for inference-ineligible events (Pass 12 P12-H7,
    // `req:pitch:authored-uninferred`).
    assert_eq!(
        ann.decompositions.len(),
        ann.taxonomy.decompositions_inferred
            + ann.taxonomy.decompositions_authored
            + ann.taxonomy.decompositions_authored_uninferred,
        "decomposition map size disagrees with the taxonomy counts"
    );
}

// ===========================================================================
// RespellPitch precedence
// ===========================================================================

/// An authored, engraved-layer `UserChosen` override takes precedence over the
/// inferred spelling; a non-engraved (layer-tagged) override is ignored. Then the
/// two remaining decision axes in [`epiphany_core::resolve_spelling`]: among
/// competing engraved overrides the higher **`priority`** wins regardless of
/// attachment order, and an engraved override whose **source does not outrank
/// `Inferred`** in the score's precedence (`Analytical`, by default ranked below
/// `Inferred`) is rejected so the inferred spelling stands.
pub fn assert_respell_precedence() {
    let (score, pid) = corpus::override_probe();
    let replica = score.identity.replica_id;

    // Baseline: no override → the algorithm's inferred spelling stands.
    let base = derive_annotations(&score, &profile())
        .expect("the default pre-pass algorithms are supported");
    let base_rs = base.spellings.get(&pid).expect("probe pitch is spelled");
    assert!(
        matches!(base_rs.provenance, SpellingProvenance::Inferred),
        "baseline provenance should be Inferred, was {:?}",
        base_rs.provenance
    );
    let inferred = base_rs.spelling.clone();

    // A clearly distinct authored spelling (Db4), engraved layer, UserChosen.
    let override_spelling = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::D),
        accidentals: vec![AccidentalId::new("flat")],
        octave: 4,
        render_hints: Default::default(),
    };
    let mut s2 = score.clone();
    s2.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(override_spelling.clone()),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: None,
    });
    let ann2 =
        derive_annotations(&s2, &profile()).expect("the default pre-pass algorithms are supported");
    let rs2 = ann2
        .spellings
        .get(&pid)
        .expect("overridden pitch is spelled");
    assert_eq!(
        rs2.spelling, override_spelling,
        "authored UserChosen override did not take precedence"
    );
    assert_eq!(
        rs2.provenance,
        SpellingProvenance::Authored(SpellingSourceKind::UserChosen),
        "override provenance should be Authored(UserChosen)"
    );
    assert_ne!(
        rs2.spelling, inferred,
        "the override fixture must differ from the inferred spelling to be meaningful"
    );

    // A non-engraved (analysis-layer) override must be ignored: engraved only.
    let mut s3 = score.clone();
    s3.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(override_spelling.clone()),
        source: SpellingSource::UserChosen,
        priority: 0,
        layer: Some(AnalysisLayerId::new(replica, 1)),
    });
    let ann3 =
        derive_annotations(&s3, &profile()).expect("the default pre-pass algorithms are supported");
    let rs3 = ann3.spellings.get(&pid).expect("pitch is spelled");
    assert_eq!(
        rs3.spelling, inferred,
        "a layer-tagged (non-engraved) override must not win"
    );
    assert!(
        matches!(rs3.provenance, SpellingProvenance::Inferred),
        "layer-tagged override should leave provenance Inferred"
    );

    // Two competing engraved overrides on the same pitch resolve by `priority`,
    // not attachment order. Gb4 and F#4 both realize pc 6, so each is a
    // legitimate spelling — only `priority` decides. The lower-priority one is
    // listed *first*, so a naive "first attachment wins" would pick Gb4; the
    // higher-priority F#4 must win instead.
    let gb4 = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::G),
        accidentals: vec![AccidentalId::new("flat")],
        octave: 4,
        render_hints: Default::default(),
    };
    let fs4 = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::F),
        accidentals: vec![AccidentalId::new("sharp")],
        octave: 4,
        render_hints: Default::default(),
    };
    let mut s4 = score.clone();
    s4.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(gb4),
        source: SpellingSource::UserChosen,
        priority: 1, // first, lower priority — must lose
        layer: None,
    });
    s4.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(fs4.clone()),
        source: SpellingSource::UserChosen,
        priority: 9, // second, higher priority — must win
        layer: None,
    });
    let ann4 =
        derive_annotations(&s4, &profile()).expect("the default pre-pass algorithms are supported");
    let rs4 = ann4.spellings.get(&pid).expect("pitch is spelled");
    assert_eq!(
        rs4.spelling, fs4,
        "the higher-priority engraved override must win regardless of attachment order"
    );
    assert_eq!(
        rs4.provenance,
        SpellingProvenance::Authored(SpellingSourceKind::UserChosen),
        "the winning override's provenance should be Authored(UserChosen)"
    );

    // An engraved override whose source does not outrank `Inferred` in the
    // score's precedence (`Analytical`, ranked below `Inferred` by default) is
    // rejected: the inferred spelling stands. This exercises the precedence-rank
    // gate, distinct from the engraved-layer gate above — a high `priority`
    // cannot rescue a source that loses on rank.
    let mut s5 = score.clone();
    s5.spelling_attachments.push(SpellingAttachment {
        scope: SpellingScope::Pitch(pid),
        directive: SpellingDirective::Explicit(override_spelling),
        source: SpellingSource::Analytical,
        priority: 100,
        layer: None,
    });
    let ann5 =
        derive_annotations(&s5, &profile()).expect("the default pre-pass algorithms are supported");
    let rs5 = ann5.spellings.get(&pid).expect("pitch is spelled");
    assert_eq!(
        rs5.spelling, inferred,
        "an engraved override whose source does not outrank Inferred must be ignored"
    );
    assert!(
        matches!(rs5.provenance, SpellingProvenance::Inferred),
        "a non-outranking override should leave provenance Inferred"
    );
}

/// The H↔K seam: a **real reduced** `RespellPitch` (not a hand-pushed
/// attachment) is honored by [`derive_annotations`]. The reducer must surface
/// the override into the materialized graph; otherwise a respelling accepted by
/// `reduce_onto` is lost before the pre-pass runs, silently violating the
/// manual-override precedence requirement.
pub fn assert_reduced_respell_is_honored() {
    use epiphany_core::{check_invariants, OperationId, ReplicaId, WallClockTime};
    use epiphany_ops::{
        AuthorId, CausalContext, HybridLogicalClock, OperationEnvelope, OperationKind,
        OperationPayload, OperationSet, OperationStamp, RespellPitchOp,
    };

    let (score, pid) = corpus::override_probe();

    // Inferred baseline (no override).
    let inferred = derive_annotations(&score, &profile())
        .expect("the default pre-pass algorithms are supported")
        .spellings
        .get(&pid)
        .expect("probe pitch is spelled")
        .spelling
        .clone();
    // A clearly distinct authored spelling (Db4), engraved-layer UserChosen.
    let override_spelling = PitchSpelling {
        nominal: SpellingNominal::Cmn(CmnNominal::D),
        accidentals: vec![AccidentalId::new("flat")],
        octave: 4,
        render_hints: Default::default(),
    };
    assert_ne!(
        override_spelling, inferred,
        "the override fixture must differ from the inferred spelling to be meaningful"
    );

    // Reduce a real RespellPitch onto the score, then derive annotations on the
    // *materialized* graph the reducer produced.
    let id = OperationId::new(ReplicaId(0x5EE11), 0);
    let respell = OperationEnvelope {
        id,
        author: AuthorId(0),
        stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 0), id),
        causal_context: CausalContext::new(),
        transaction: None,
        payload: OperationPayload::Primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch: pid,
            spelling: override_spelling.clone(),
        })),
    };
    let mut set = OperationSet::new();
    set.accept(respell);
    let reduced = set.reduce_onto(&score);

    let ann = derive_annotations(&reduced.score, &profile())
        .expect("the default pre-pass algorithms are supported");
    let rs = ann
        .spellings
        .get(&pid)
        .expect("the respelled pitch is spelled");
    assert_eq!(
        rs.spelling, override_spelling,
        "a reduced RespellPitch must be honored by derive_annotations (not lost before the pre-pass)"
    );
    assert_eq!(
        rs.provenance,
        SpellingProvenance::Authored(SpellingSourceKind::UserChosen),
        "a reduced RespellPitch should resolve with manual-override precedence"
    );
    assert!(
        check_invariants(&reduced.score).is_empty(),
        "the reduced score carrying the surfaced override is invariant-clean"
    );
}

// ===========================================================================
// Spelling correctness vs. published tonal expectations
// ===========================================================================

fn describe(s: &PitchSpelling) -> (CmnNominal, String) {
    let nominal = match s.nominal {
        SpellingNominal::Cmn(n) => n,
        _ => panic!("expected a CMN spelling"),
    };
    let acc = s
        .accidentals
        .iter()
        .map(|a| a.as_str())
        .collect::<Vec<_>>()
        .join("+");
    (nominal, acc)
}

fn build_named(name: &str) -> Score {
    let f = corpus::corpus()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no corpus fixture named `{name}`"));
    (f.build)()
}

/// On a monophonic fixture (one pitch per event, minted in melodic order, so the
/// `BTreeMap` value order is melodic), the inferred spellings match the expected
/// `(nominal, accidental)` sequence.
fn assert_line(name: &str, expected: &[(CmnNominal, &str)]) {
    let ann = derive_annotations(&build_named(name), &profile())
        .expect("the default pre-pass algorithms are supported");
    let got: Vec<(CmnNominal, String)> = ann
        .spellings
        .values()
        .map(|rs| describe(&rs.spelling))
        .collect();
    assert_eq!(
        got.len(),
        expected.len(),
        "{name}: {} spellings, expected {}",
        got.len(),
        expected.len()
    );
    for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            *g,
            (e.0, e.1.to_string()),
            "{name}[{i}]: spelled {g:?}, expected ({:?}, {:?})",
            e.0,
            e.1
        );
    }
}

/// Diatonic scales in C / D / Bb spell to their published key spellings — the
/// "matches published Temperley/Longuet-Higgins expectations on standard cases"
/// criterion, on unambiguous tonal lines.
pub fn assert_spelling_matches_published_expectations() {
    use CmnNominal::*;
    assert_line(
        "c_major_scale",
        &[
            (C, ""),
            (D, ""),
            (E, ""),
            (F, ""),
            (G, ""),
            (A, ""),
            (B, ""),
            (C, ""),
        ],
    );
    assert_line(
        "d_major_scale",
        &[
            (D, ""),
            (E, ""),
            (F, "sharp"),
            (G, ""),
            (A, ""),
            (B, ""),
            (C, "sharp"),
            (D, ""),
        ],
    );
    assert_line(
        "b_flat_major_scale",
        &[
            (B, "flat"),
            (C, ""),
            (D, ""),
            (E, "flat"),
            (F, ""),
            (G, ""),
            (A, ""),
        ],
    );
}

// ===========================================================================
// Non-vacuity (the F tripwire)
// ===========================================================================

/// The gate must go **red if H were stubbed**. Across the corpus: spellings span
/// multiple distinct nominals and include at least one accidental (a constant
/// `C4` stub yields exactly one nominal and no accidentals); decompositions use
/// at least two distinct note values and include at least one tied multi-
/// component split (an empty/no-op decomposition map yields neither).
pub fn assert_non_vacuity() {
    let report = corpus::classify_corpus();

    let mut nominal_seen = [false; 7];
    let mut value_seen = [false; 7];
    let mut any_accidental = false;
    let mut any_tie_chain = false;
    let mut inferred_spellings = 0usize;
    let mut decompositions = 0usize;

    // Per-fixture spread: how many *distinct* fixtures independently exhibit each
    // richness signal. A corpus-wide OR/sum can be satisfied by a single rich
    // fixture, so a *partial* stub (H broken for every input but one) would slip
    // past the aggregate checks below; requiring each signal to recur across ≥2
    // fixtures closes that.
    let mut fixtures_rich_nominals = 0usize; // a fixture spanning ≥5 inferred nominals
    let mut fixtures_with_accidental = 0usize;
    let mut fixtures_multivalue_decomp = 0usize; // a fixture spanning ≥2 note values
    let mut fixtures_with_tie_chain = 0usize;

    for f in &report.fixtures {
        let mut f_nominal_seen = [false; 7];
        let mut f_value_seen = [false; 7];
        let mut f_accidental = false;
        let mut f_tie_chain = false;

        for rs in f.annotations.spellings.values() {
            if matches!(rs.provenance, SpellingProvenance::Inferred) {
                inferred_spellings += 1;
                if let SpellingNominal::Cmn(n) = rs.spelling.nominal {
                    nominal_seen[n as usize] = true;
                    f_nominal_seen[n as usize] = true;
                }
                if !rs.spelling.accidentals.is_empty() {
                    any_accidental = true;
                    f_accidental = true;
                }
            }
        }
        for d in f.annotations.decompositions.values() {
            decompositions += 1;
            if d.components.len() > 1 {
                any_tie_chain = true;
                f_tie_chain = true;
            }
            for c in &d.components {
                value_seen[c.base_value as usize] = true;
                f_value_seen[c.base_value as usize] = true;
            }
        }

        if f_nominal_seen.iter().filter(|x| **x).count() >= 5 {
            fixtures_rich_nominals += 1;
        }
        if f_accidental {
            fixtures_with_accidental += 1;
        }
        if f_value_seen.iter().filter(|x| **x).count() >= 2 {
            fixtures_multivalue_decomp += 1;
        }
        if f_tie_chain {
            fixtures_with_tie_chain += 1;
        }
    }

    let distinct_nominals = nominal_seen.iter().filter(|x| **x).count();
    let distinct_values = value_seen.iter().filter(|x| **x).count();

    assert!(
        inferred_spellings > 0,
        "no inferred spellings at all — H produced nothing"
    );
    assert!(
        distinct_nominals >= 5,
        "inferred spellings collapse to {distinct_nominals} nominal(s); a real \
         line-of-fifths algorithm spans the diatonic letters — looks stubbed"
    );
    assert!(
        any_accidental,
        "no accidental anywhere in the corpus's inferred spellings — looks like \
         the constant middle-C stub"
    );
    assert!(
        decompositions > 0,
        "no decompositions at all — H produced nothing"
    );
    assert!(
        distinct_values >= 2,
        "decompositions use {distinct_values} note value(s); real decomposition \
         spans several — looks stubbed"
    );
    assert!(
        any_tie_chain,
        "no multi-component (tied) decomposition anywhere — syncopation/barline \
         splitting looks stubbed"
    );

    // Spread: no single rich fixture may carry a richness signal for the whole
    // corpus (a partial stub that breaks H for all inputs but one would still
    // satisfy the aggregate ORs above).
    assert!(
        fixtures_rich_nominals >= 2,
        "only {fixtures_rich_nominals} fixture(s) independently span ≥5 nominals; a \
         real algorithm spells many fixtures richly — a partial stub looks like this"
    );
    assert!(
        fixtures_with_accidental >= 2,
        "only {fixtures_with_accidental} fixture(s) produce any accidental; \
         accidentals should arise across several fixtures, not be carried by one"
    );
    assert!(
        fixtures_multivalue_decomp >= 2,
        "only {fixtures_multivalue_decomp} fixture(s) span ≥2 note values; real \
         decomposition varies values across many fixtures, not one"
    );
    assert!(
        fixtures_with_tie_chain >= 2,
        "only {fixtures_with_tie_chain} fixture(s) produce a tied multi-component \
         split; syncopation/barline splitting should recur across fixtures, not one"
    );
}

// ===========================================================================
// Materialization pipeline (criterion-5 path)
// ===========================================================================

/// H's derivation stays deterministic when run on real reduced scores from the
/// criterion-5 convergence path, and is non-vacuous on them — so criterion 5
/// continues to pass with non-trivial pre-pass outputs downstream of
/// materialization.
///
/// This guards **determinism**, **decomposition reconstruction** (invariant 15),
/// and **non-emptiness** on real scores. It does *not* independently exercise
/// inferred-spelling **correctness**: the generators mint authored-CMN pitches,
/// which take the authored-letter path in [`epiphany_core::derive_annotations`]
/// and bypass the line-of-fifths inference. That correctness signal lives in the
/// integer-pitch corpus fixtures (`cmn-12` positions with no authored letter) via
/// [`assert_spelling_matches_published_expectations`] and
/// [`assert_eligible_pitches_spelled`] over the corpus — do not rely on this path
/// to catch a spelling-inference regression.
pub fn assert_deterministic_in_materialization_pipeline(scale: u64) {
    let n = (16 * scale).max(8);
    let mut spelled_anywhere = false;
    for seed in 0..n {
        let (score, _frontier) = crate::convergence::materialized_score(
            seed.wrapping_mul(0x9E37_79B9).wrapping_add(101),
        );
        assert_derivation_deterministic(&score);
        let ann = derive_annotations(&score, &profile())
            .expect("the default pre-pass algorithms are supported");
        if !ann.spellings.is_empty() {
            spelled_anywhere = true;
        }
        // Whatever H produces on a real reduced score must still reconstruct.
        assert_decompositions_reconstruct(&score, &ann);
        assert_eligible_pitches_spelled(&score, &ann);
    }
    assert!(
        spelled_anywhere,
        "the pre-pass produced no spellings on any materialized score — vacuous \
         in the real pipeline"
    );
}

// ===========================================================================
// The whole gate
// ===========================================================================

/// Runs the complete H merge gate. `scale` multiplies the materialization-path
/// iteration count (1 for the unit budget, more for the soak).
pub fn run_all(scale: u64) {
    // Determinism.
    assert_corpus_deterministic();

    // Eligibility + reconstruction over every fixture.
    for f in corpus::corpus() {
        let score = (f.build)();
        let ann = derive_annotations(&score, &profile())
            .expect("the default pre-pass algorithms are supported");
        assert_eligible_pitches_spelled(&score, &ann);
        assert_decompositions_reconstruct(&score, &ann);
    }

    // Authored-override precedence (hand-pushed attachment) ...
    assert_respell_precedence();
    // ... and a real reduced RespellPitch surfaced through the K↔H seam.
    assert_reduced_respell_is_honored();

    // Spelling correctness on published tonal cases.
    assert_spelling_matches_published_expectations();

    // Non-vacuity (the tripwire).
    assert_non_vacuity();

    // Determinism + non-vacuity through the criterion-5 materialization path.
    assert_deterministic_in_materialization_pipeline(scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_merge_gate_passes() {
        run_all(1);
    }

    #[test]
    fn respell_precedence_rule() {
        assert_respell_precedence();
    }

    #[test]
    fn spelling_matches_published_cases() {
        assert_spelling_matches_published_expectations();
    }

    #[test]
    fn non_vacuity_guard() {
        assert_non_vacuity();
    }
}

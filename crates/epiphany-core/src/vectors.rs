//! Decode conformance vectors for the `epiphany-core` score wire
//! (`spec/CONTRACT_CORE_DECODE_VECTORS.md`).
//!
//! See `epiphany_ops::vectors` for the corpus's purpose, its column shape, and
//! why `accept`/`reject` are never collapsed. This module extends the same
//! committed, cross-implementation corpus to the *oldest and most load-bearing*
//! wire format in the repository — the whole-`Score` codec — which had no
//! literal-byte vectors at all before this tranche (only round-trip locking,
//! which cannot see a self-consistent field reordering: see
//! `schema_major_3_tuning_context_wire_bytes_are_frozen` in `codec.rs`, and the
//! contract's account of the tranche-3b-i defect it golden-pins).
//!
//! Two families of surface:
//!
//! * **Leaves** — one per [`CanonicalValue`] type the corpus pins: the five
//!   representative layouts of the Binary Format companion
//!   (§"Representative Complete Layouts") — [`RationalTime`], [`TimeAnchor`],
//!   [`Pitch`], [`Event`], [`Slur`] — plus the four schema-major-3
//!   tuning-context leaves ([`SmuflVersion`], [`SmuflVersionRequirement`],
//!   [`TuningScope`], [`TuningOverride`]) and their container
//!   ([`ScoreTuningContext`]). Every leaf routes through
//!   [`CanonicalValue::decode_canonical`] — the same public, production API a
//!   value-typed operation payload uses — never a decoder reimplemented here.
//! * **Whole `Score`, one per schema major** — `core.score_v0` through
//!   `core.score_v3`, routed through [`Score::decode_canonical_versioned`].
//!   Majors 0–2 are **not** literal-byte-locked at the *current* layout: a
//!   migration deliberately rewrites bytes (that is the point of
//!   default-filling), so injectivity there means the input was already
//!   canonical *at its own major* — `decode_vN_score` re-encodes through the
//!   frozen `encode_vN_score` and rejects a mismatch, so a successful decode
//!   already proves that. Only `core.score_v3` compares
//!   `decoded.canonical_bytes() == bytes`. See the contract's "trap" section;
//!   getting this backwards (comparing v0–v2 against the *current* encoding)
//!   would fail on every vector, and "fixing" it by relaxing the check would
//!   destroy what the vector pins.

use epiphany_determinism::CanonicalF64;

use crate::accidental::{SmuflVersion, SmuflVersionRequirement};
use crate::codec::Codec;
use crate::event::{Event, PitchedEvent, StemConfiguration};
use crate::graph::{ScoreTuningContext, Slur, SlurKind, SpanStyle};
use crate::ids::{EventId, PitchId, ReplicaId, SlurId, VoiceId};
use crate::pitch::{
    AcousticPitch, AcousticRealization, CmnNominal, IdentifiedPitch, Pitch, PitchSpaceId,
    PitchSpacePosition, ScalePosition, TuningReference, TuningSystemId,
};
use crate::time::{
    AnchorOffset, EventDuration, EventPosition, MusicalDuration, MusicalPosition, RationalTime,
    TimeAnchor,
};
use crate::tuning::{TuningOverride, TuningScope};
use crate::{CanonicalValue, Score};

/// One vector: `(surface, verdict, class, name, bytes)`. See
/// `epiphany_ops::vectors::DecodeVector`.
pub type DecodeVector = (&'static str, &'static str, &'static str, String, Vec<u8>);

fn row(
    surface: &'static str,
    verdict: &'static str,
    class: &'static str,
    name: impl Into<String>,
    bytes: Vec<u8>,
) -> DecodeVector {
    (surface, verdict, class, name.into(), bytes)
}

// ===========================================================================
// Shared fixture values.
// ===========================================================================

/// A `cmn-12` pitch with an explicit cents-offset realization, so its
/// canonical bytes end in a length-prefixed [`CanonicalF64`] leaf (the last
/// 8 bytes are the raw IEEE-754 payload) — what the `core.pitch` reject vector
/// corrupts to a non-finite value.
fn pitch_with_cents(cents: f64) -> Pitch {
    Pitch {
        scale_position: ScalePosition {
            space: PitchSpaceId::new("cmn-12"),
            position: PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 0,
                octave: 4,
            },
        },
        acoustic: AcousticPitch {
            tuning: TuningReference::Inherit,
            realization: AcousticRealization::CentsOffset(
                CanonicalF64::new(cents).expect("finite"),
            ),
        },
    }
}

fn simple_event() -> Event {
    Event::Pitched(PitchedEvent {
        id: EventId::new(ReplicaId(1), 1),
        voice: VoiceId::new(ReplicaId(1), 1),
        position: EventPosition::Musical(MusicalPosition(RationalTime::zero())),
        duration: EventDuration::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
        pitches: vec![IdentifiedPitch {
            id: PitchId::new(ReplicaId(1), 1),
            pitch: pitch_with_cents(0.0),
        }],
        articulations: vec![],
        dynamic: None,
        ornaments: vec![],
        stem: StemConfiguration,
        grace: None,
    })
}

fn simple_slur() -> Slur {
    Slur {
        id: SlurId::new(ReplicaId(1), 1),
        start_event: EventId::new(ReplicaId(1), 1),
        end_event: EventId::new(ReplicaId(1), 2),
        kind: SlurKind::default(),
        curvature_override: None,
        style: SpanStyle::default(),
    }
}

/// The one `TuningOverride` embedded in [`loaded_tuning_context`]: a per-voice
/// override that sets `tuning_system` only, leaving `pitch_space` and
/// `reference` inherited — mirrors
/// `schema_major_3_tuning_context_wire_bytes_are_frozen`'s fixture exactly
/// (same field values), so these bytes are known-frozen wire content, not a
/// fresh layout.
fn one_override() -> TuningOverride {
    TuningOverride {
        scope: TuningScope::Voice(VoiceId::new(ReplicaId(1), 7)),
        pitch_space: None,
        tuning_system: Some(TuningSystemId::new("tet-19")),
        reference: None,
    }
}

/// A non-default `ScoreTuningContext`: `smufl` at 1.12/1.18 (not the 1.4/1.4
/// default) and one override, so both major-3 fields are real content, not
/// vacuously-default padding. Same fixture as
/// `schema_major_3_tuning_context_wire_bytes_are_frozen`.
fn loaded_tuning_context() -> ScoreTuningContext {
    let mut ctx = ScoreTuningContext {
        smufl: SmuflVersionRequirement {
            minimum: SmuflVersion::from_decimal(1, "12").unwrap(),
            authored_against: SmuflVersion::from_decimal(1, "18").unwrap(),
        },
        ..ScoreTuningContext::default()
    };
    ctx.overrides.push(one_override());
    ctx
}

/// Encodes `ctx` with `overrides` written *before* `smufl` — the exact
/// regression this tranche exists to catch (Push 4b tranche 3b-i swapped
/// these two fields in both halves of `impl Codec for ScoreTuningContext`,
/// and the whole workspace suite plus 8/8 conformance still passed). The
/// frozen field order is `default_pitch_space` ⌢ `default_tuning_system` ⌢
/// `reference` ⌢ `smufl` ⌢ `overrides`; this swaps the last two.
fn score_tuning_context_bytes_with_fields_swapped(ctx: &ScoreTuningContext) -> Vec<u8> {
    let mut out = Vec::new();
    ctx.default_pitch_space.enc(&mut out);
    ctx.default_tuning_system.enc(&mut out);
    ctx.reference.enc(&mut out);
    ctx.overrides.enc(&mut out);
    ctx.smufl.enc(&mut out);
    out
}

/// Hand-encodes the *unreduced* rational `2/4`: there is no public
/// constructor that skips [`RationalTime`]'s reduce-on-construct invariant
/// (every constructor re-establishes it), so the only way to produce
/// non-canonical bytes for this leaf is to write them by hand, mirroring
/// [`RationalTime`]'s own `CanonicalEncode` (`time.rs`): a sign byte, then a
/// length-prefixed big-endian numerator magnitude, then a length-prefixed
/// big-endian denominator magnitude — wrapped in the outer `u32` leaf-length
/// prefix every embedded leaf carries. Decoding reduces `2/4` to `1/2`, so the
/// re-encoded bytes differ from these: the lenient-leaf-normalization case the
/// fifth representative layout exists to demonstrate (a guard *can* mask a
/// lenient inner codec; here the leaf's own strict check catches it directly).
fn unreduced_two_fourths() -> Vec<u8> {
    let mut inner = Vec::new();
    inner.push(1); // sign: Plus
    inner.extend_from_slice(&1u32.to_le_bytes()); // numerator magnitude length
    inner.push(2); // numerator magnitude: 2
    inner.extend_from_slice(&1u32.to_le_bytes()); // denominator magnitude length
    inner.push(4); // denominator magnitude: 4
    let mut out = Vec::new();
    out.extend_from_slice(&(inner.len() as u32).to_le_bytes()); // outer leaf length prefix
    out.extend_from_slice(&inner);
    out
}

/// Corrupts a tagged union's leading discriminant byte to a value one past
/// every assigned tag, so the decoder's `match` falls through to its
/// `InvalidTag` arm regardless of which union this is.
fn with_invalid_leading_tag(bytes: &[u8], tag: u8) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out[0] = tag;
    out
}

/// Overwrites the trailing 8 bytes of an accept vector's bytes — the raw
/// IEEE-754 payload of a trailing [`CanonicalF64`] leaf (its 4-byte length
/// prefix precedes them) — with a non-finite bit pattern.
fn with_trailing_float_replaced(bytes: &[u8], value: f64) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let n = out.len();
    out[n - 8..].copy_from_slice(&value.to_le_bytes());
    out
}

fn with_trailing_byte(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.push(0);
    out
}

fn truncated(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.pop();
    out
}

// ===========================================================================
// The vectors.
// ===========================================================================

/// Every `epiphany-core` decode vector: the leaf layouts, then the per-major
/// whole-`Score` snapshots.
pub fn decode_vectors() -> Vec<DecodeVector> {
    let mut v: Vec<DecodeVector> = Vec::new();

    // --- RationalTime (the fifth representative layout) --------------------
    const RT: &str = "core.rational_time";
    let eighth = RationalTime::new(1, 8).unwrap();
    v.push(row(
        RT,
        "accept",
        "-",
        "one_eighth",
        eighth.canonical_bytes(),
    ));
    v.push(row(
        RT,
        "reject",
        "unreduced-rational-time",
        "two_fourths_unreduced",
        unreduced_two_fourths(),
    ));

    // --- TimeAnchor ----------------------------------------------------------
    const TA: &str = "core.time_anchor";
    let anchor = TimeAnchor::Event {
        id: EventId::new(ReplicaId(1), 1),
        offset: AnchorOffset::Musical(MusicalDuration(RationalTime::new(1, 4).unwrap())),
    };
    let anchor_bytes = anchor.canonical_bytes();
    v.push(row(TA, "accept", "-", "event_anchor", anchor_bytes.clone()));
    v.push(row(
        TA,
        "reject",
        "out-of-range-discriminant",
        "tag_9_one_past_the_vocabulary",
        with_invalid_leading_tag(&anchor_bytes, 9),
    ));

    // --- Pitch ---------------------------------------------------------------
    const PITCH: &str = "core.pitch";
    let pitch_bytes = pitch_with_cents(1.5).canonical_bytes();
    v.push(row(
        PITCH,
        "accept",
        "-",
        "cents_offset",
        pitch_bytes.clone(),
    ));
    v.push(row(
        PITCH,
        "reject",
        "non-finite-float",
        "cents_offset_nan",
        with_trailing_float_replaced(&pitch_bytes, f64::NAN),
    ));

    // --- Event -----------------------------------------------------------------
    const EVENT: &str = "core.event";
    let event_bytes = simple_event().canonical_bytes();
    v.push(row(
        EVENT,
        "accept",
        "-",
        "pitched_event",
        event_bytes.clone(),
    ));
    v.push(row(
        EVENT,
        "reject",
        "trailing-bytes",
        "pitched_event_trailing",
        with_trailing_byte(&event_bytes),
    ));

    // --- Slur --------------------------------------------------------------
    const SLUR: &str = "core.slur";
    let slur_bytes = simple_slur().canonical_bytes();
    v.push(row(SLUR, "accept", "-", "simple_slur", slur_bytes.clone()));
    v.push(row(
        SLUR,
        "reject",
        "truncated",
        "simple_slur_truncated",
        truncated(&slur_bytes),
    ));

    // --- ScoreTuningContext (schema major 3) --------------------------------
    const STC: &str = "core.score_tuning_context";
    let ctx = loaded_tuning_context();
    let ctx_bytes = ctx.canonical_bytes();
    v.push(row(STC, "accept", "-", "loaded_context", ctx_bytes));
    // THE direct regression vector for the 3b-i defect: bytes with `overrides`
    // written before `smufl` must be rejected by the (correctly-ordered)
    // decoder, even though a self-consistently-reordered codec would accept
    // its own output. This is what a byte-literal corpus catches that
    // round-trip locking cannot (see the module doc).
    v.push(row(
        STC,
        "reject",
        "swapped-major-3-field-order",
        "overrides_before_smufl",
        score_tuning_context_bytes_with_fields_swapped(&ctx),
    ));

    // --- TuningOverride ------------------------------------------------------
    const TO: &str = "core.tuning_override";
    let override_bytes = one_override().canonical_bytes();
    v.push(row(
        TO,
        "accept",
        "-",
        "voice_scoped",
        override_bytes.clone(),
    ));
    v.push(row(
        TO,
        "reject",
        "trailing-bytes",
        "voice_scoped_trailing",
        with_trailing_byte(&override_bytes),
    ));

    // --- TuningScope ---------------------------------------------------------
    const TS: &str = "core.tuning_scope";
    let scope_bytes = TuningScope::Voice(VoiceId::new(ReplicaId(1), 7)).canonical_bytes();
    v.push(row(TS, "accept", "-", "voice", scope_bytes.clone()));
    v.push(row(
        TS,
        "reject",
        "out-of-range-discriminant",
        "tag_9_one_past_the_vocabulary",
        with_invalid_leading_tag(&scope_bytes, 9),
    ));

    // --- SmuflVersionRequirement -----------------------------------------------
    const SVR: &str = "core.smufl_version_requirement";
    let svr_bytes = SmuflVersionRequirement {
        minimum: SmuflVersion::from_decimal(1, "12").unwrap(),
        authored_against: SmuflVersion::from_decimal(1, "18").unwrap(),
    }
    .canonical_bytes();
    v.push(row(
        SVR,
        "accept",
        "-",
        "one_twelve_one_eighteen",
        svr_bytes.clone(),
    ));
    v.push(row(
        SVR,
        "reject",
        "trailing-bytes",
        "one_twelve_one_eighteen_trailing",
        with_trailing_byte(&svr_bytes),
    ));

    // --- SmuflVersion --------------------------------------------------------
    const SV: &str = "core.smufl_version";
    let sv_bytes = SmuflVersion::from_decimal(1, "4")
        .unwrap()
        .canonical_bytes();
    v.push(row(SV, "accept", "-", "one_four", sv_bytes.clone()));
    v.push(row(
        SV,
        "reject",
        "truncated",
        "one_four_truncated",
        truncated(&sv_bytes),
    ));

    // --- Whole Score, one per schema major -----------------------------------
    //
    // A single, real, well-formed `Score` (the positive generator's output —
    // migration-safe: its schema-major-1/2/3 fields all sit at their canonical
    // defaults, exactly what `valid_score`'s existing migration tests already
    // rely on), encoded through each frozen per-major encoder. Majors 0-2 are
    // genuinely *older* wire forms of the same value, synthesized via the
    // pub(crate) `encode_vN_score` mirrors (never a fresh hand-rolled layout);
    // major 3 is the live `canonical_bytes()`.
    let score = crate::generators::valid_score(7);
    let v0 = crate::codec::encode_v0_score(&score);
    let v1 = crate::codec::encode_v1_score(&score);
    let v2 = crate::codec::encode_v2_score(&score);
    let v3 = score.canonical_bytes();

    const SV0: &str = "core.score_v0";
    v.push(row(SV0, "accept", "-", "valid_score_seed_7", v0.clone()));
    v.push(row(
        SV0,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v0),
    ));

    const SV1: &str = "core.score_v1";
    v.push(row(SV1, "accept", "-", "valid_score_seed_7", v1.clone()));
    v.push(row(
        SV1,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v1),
    ));

    const SV2: &str = "core.score_v2";
    v.push(row(SV2, "accept", "-", "valid_score_seed_7", v2.clone()));
    v.push(row(
        SV2,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v2),
    ));

    const SV3: &str = "core.score_v3";
    v.push(row(SV3, "accept", "-", "valid_score_seed_7", v3.clone()));
    v.push(row(
        SV3,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v3),
    ));

    v
}

// ===========================================================================
// Verification.
// ===========================================================================

/// Runs `T`'s [`CanonicalValue::decode_canonical`] — the same public,
/// production API a value-typed operation payload decodes through — never a
/// decoder reimplemented in this module.
fn leaf_check<T: CanonicalValue>(bytes: &[u8]) -> Result<bool, String> {
    match T::decode_canonical(bytes) {
        Ok(v) => Ok(v.canonical_bytes() == bytes),
        Err(e) => Err(format!("{e}")),
    }
}

/// Runs [`Score::decode_canonical_versioned`] at `major`. Majors 0-2 report
/// injectivity as `true` unconditionally on a successful decode: migration
/// deliberately rewrites the bytes (default-filling new fields), so comparing
/// against the *current* `canonical_bytes()` would fail on every vector, and
/// `decode_vN_score`'s own re-encode-through-`encode_vN_score` guard already
/// proved the input canonical at *its own* major before returning `Ok` at all
/// (see the module doc's account of the contract's "trap"). Only major 3
/// compares `decoded.canonical_bytes() == bytes` — the live layout, where that
/// comparison is exactly what injectivity means.
fn score_check(bytes: &[u8], major: u16) -> Result<bool, String> {
    match Score::decode_canonical_versioned(bytes, major) {
        Ok(decoded) => {
            if major == 3 {
                Ok(decoded.canonical_bytes() == bytes)
            } else {
                Ok(true)
            }
        }
        Err(e) => Err(format!("{e}")),
    }
}

/// Applies `surface`'s decoder to `bytes`. See `epiphany_ops::vectors::check`
/// for the exact `Ok`/`Err` semantics. `None` when the surface is not owned by
/// this crate.
pub fn check(surface: &str, bytes: &[u8]) -> Option<Result<bool, String>> {
    match surface {
        "core.rational_time" => Some(leaf_check::<RationalTime>(bytes)),
        "core.time_anchor" => Some(leaf_check::<TimeAnchor>(bytes)),
        "core.pitch" => Some(leaf_check::<Pitch>(bytes)),
        "core.event" => Some(leaf_check::<Event>(bytes)),
        "core.slur" => Some(leaf_check::<Slur>(bytes)),
        "core.score_tuning_context" => Some(leaf_check::<ScoreTuningContext>(bytes)),
        "core.tuning_override" => Some(leaf_check::<TuningOverride>(bytes)),
        "core.tuning_scope" => Some(leaf_check::<TuningScope>(bytes)),
        "core.smufl_version_requirement" => Some(leaf_check::<SmuflVersionRequirement>(bytes)),
        "core.smufl_version" => Some(leaf_check::<SmuflVersion>(bytes)),
        "core.score_v0" => Some(score_check(bytes, 0)),
        "core.score_v1" => Some(score_check(bytes, 1)),
        "core.score_v2" => Some(score_check(bytes, 2)),
        "core.score_v3" => Some(score_check(bytes, 3)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each vector must get the verdict it declares — see
    /// `epiphany_ops::vectors::tests::every_vector_gets_its_declared_verdict`.
    #[test]
    fn every_vector_gets_its_declared_verdict() {
        for (surface, verdict, class, name, bytes) in decode_vectors() {
            let result = check(surface, &bytes).expect("a surface this crate owns");
            match (verdict, &result) {
                ("accept", Ok(true)) => {}
                ("accept", Ok(false)) => {
                    panic!("{surface}/{name}: accepted but does not re-encode to its bytes")
                }
                ("reject", Err(_)) => {}
                _ => panic!("{surface}/{name} ({class}): declared {verdict}, got {result:?}"),
            }
        }
    }

    /// Every surface carries both verdicts, or the corpus pins half a
    /// contract.
    #[test]
    fn every_surface_carries_both_verdicts() {
        use std::collections::BTreeMap;
        let mut seen: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
        for (surface, verdict, ..) in decode_vectors() {
            let e = seen.entry(surface).or_default();
            match verdict {
                "accept" => e.0 = true,
                "reject" => e.1 = true,
                other => panic!("unknown verdict {other}"),
            }
        }
        assert_eq!(seen.len(), 14, "surfaces: {:?}", seen.keys());
        for (surface, (accept, reject)) in seen {
            assert!(accept, "{surface} has no accept vector");
            assert!(reject, "{surface} has no reject vector");
        }
    }

    /// The mandatory regression vector for the 3b-i defect is present: it is
    /// what makes this tranche's existence justified (see the module doc).
    #[test]
    fn the_3b_i_regression_vector_is_present() {
        assert!(decode_vectors()
            .iter()
            .any(|(s, v, c, ..)| *s == "core.score_tuning_context"
                && *v == "reject"
                && *c == "swapped-major-3-field-order"));
    }
}

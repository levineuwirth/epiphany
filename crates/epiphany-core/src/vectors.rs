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
//!   ([`ScoreTuningContext`]), and the schema-major-4 [`Tuplet`]. Every leaf
//!   routes through
//!   [`CanonicalValue::decode_canonical`] — the same public, production API a
//!   value-typed operation payload uses — never a decoder reimplemented here.
//! * **Whole `Score`, one per schema major** — `core.score_v0` through
//!   `core.score_v5`, routed through [`Score::decode_canonical_versioned`].
//!   Majors 0–4 are **not** literal-byte-locked at the *current* layout: a
//!   migration deliberately rewrites bytes (that is the point of
//!   default-filling), so injectivity there means the input was already
//!   canonical *at its own major* — `decode_vN_score` re-encodes through the
//!   frozen `encode_vN_score` and rejects a mismatch, so a successful decode
//!   already proves that. Only `core.score_v5` compares
//!   `decoded.canonical_bytes() == bytes`. See the contract's "trap" section;
//!   getting this backwards (comparing v0–v2 against the *current* encoding)
//!   would fail on every vector, and "fixing" it by relaxing the check would
//!   destroy what the vector pins.

use epiphany_determinism::CanonicalF64;

use crate::accidental::{SmuflVersion, SmuflVersionRequirement};
use crate::codec::Codec;
use crate::event::{
    Event, EventMark, Grace, GraceKind, Ornament, OrnamentKind, PitchedEvent, StemConfiguration,
};
use crate::graph::{
    Dynamic, Lyric, Marker, MarkerKind, ScoreTuningContext, Slur, SlurKind, SpanStyle, Syllabic,
    Tuplet, TupletDisplay, TupletRatio,
};
use crate::ids::{EventId, LyricLineId, MarkerId, PitchId, ReplicaId, SlurId, TupletId, VoiceId};
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
        marks: vec![],
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
    // Schema major 5: a note with marks, an ornament and a grace payload, and
    // the same marks out of order, which no canonical set writes.
    let Event::Pitched(mut marked) = simple_event() else {
        unreachable!("a note")
    };
    marked.marks = vec![
        EventMark::Staccato,
        EventMark::Accent,
        EventMark::Tremolo { strokes: 3 },
    ];
    marked.ornaments = vec![Ornament {
        kind: OrnamentKind::Trill,
        accidental_above: Some(crate::pitch::AccidentalId::new("sharp")),
        accidental_below: None,
    }];
    marked.grace = Some(Grace {
        kind: GraceKind::Acciaccatura,
        value: crate::graph::NoteValue::Eighth,
        dots: 0,
        order: 0,
    });
    let marked_bytes = Event::Pitched(marked.clone()).canonical_bytes();
    let mut unordered = marked;
    unordered.marks.swap(0, 1);
    let unordered_bytes = Event::Pitched(unordered).canonical_bytes();
    v.push(row(
        EVENT,
        "accept",
        "-",
        "pitched_event",
        event_bytes.clone(),
    ));
    v.push(row(
        EVENT,
        "accept",
        "-",
        "with_marks_and_grace",
        marked_bytes,
    ));
    v.push(row(
        EVENT,
        "reject",
        "marks-out-of-order",
        "with_marks_out_of_order",
        unordered_bytes,
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
    // major 4 is the live `canonical_bytes()`. The score holds no tuplet, so
    // its major-3 and major-4 forms are one byte string; the vectors after
    // these carry one.
    let score = crate::generators::valid_score(7);
    let v0 = crate::codec::encode_v0_score(&score);
    let v1 = crate::codec::encode_v1_score(&score);
    let v2 = crate::codec::encode_v2_score(&score);
    let v3 = crate::codec::encode_v3_score(&score);
    let v4 = crate::codec::encode_v4_score(&score);
    let v5 = score.canonical_bytes();

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

    // Schema major 4 appends `display` to `Tuplet`: the same score with a
    // tuplet, in the five-field form majors 0 to 3 share (accepted at major
    // 3, its display then the default) and with a hidden display at major
    // 4; the major-3 bytes are no major-4 encoding. The score decoder has no
    // stamp to read, so it meets them as a major-4 score that ends early;
    // the refusal by name belongs to the envelope decoder, which reads the
    // block's stamp (`ops.operation_envelope`'s `create_tuplet_before_major_4`).
    let mut tupled = score.clone();
    let members: Vec<EventId> = tupled.events.iter().take(2).map(|e| e.id()).collect();
    let tuplet = Tuplet {
        id: TupletId::new(ReplicaId(1), 99),
        ratio: TupletRatio::new(3, 2).expect("not degenerate"),
        members,
        parent: None,
        required_total: MusicalDuration(RationalTime::new(1, 4).expect("a denominator")),
        display: TupletDisplay::HIDDEN,
    };
    tupled.cross_cutting.tuplets.push(tuplet.clone());
    let tupled_v3 = crate::codec::encode_v3_score(&tupled);

    const SV3: &str = "core.score_v3";
    v.push(row(SV3, "accept", "-", "valid_score_seed_7", v3.clone()));
    v.push(row(
        SV3,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v3),
    ));
    v.push(row(SV3, "accept", "-", "with_a_tuplet", tupled_v3.clone()));

    const SV4: &str = "core.score_v4";
    v.push(row(SV4, "accept", "-", "valid_score_seed_7", v4.clone()));
    v.push(row(
        SV4,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v4),
    ));
    v.push(row(
        SV4,
        "accept",
        "-",
        "with_a_hidden_tuplet",
        crate::codec::encode_v4_score(&tupled),
    ));
    v.push(row(
        SV4,
        "reject",
        "truncated",
        "with_a_major_3_tuplet",
        tupled_v3,
    ));

    // Schema major 5 appends `voice_homes` to the score and gives a marker its
    // kind, a lyric its syllable and an event its marks. A major-4 form of a
    // score holding a marker is refused by name: no major before 5 gave a
    // marker a meaning.
    let mut expressive = score.clone();
    let first = expressive.events.iter().next().expect("an event").id();
    expressive.cross_cutting.markers.push(Marker {
        id: MarkerId::new(ReplicaId(1), 77),
        anchor: TimeAnchor::Event {
            id: first,
            offset: AnchorOffset::Zero,
        },
        kind: MarkerKind::Dynamic(Dynamic::Mf),
    });
    expressive.cross_cutting.lyrics.push(Lyric {
        id: LyricLineId::new(ReplicaId(1), 78),
        event: first,
        verse: 1,
        text: crate::Text::new("la"),
        syllabic: Syllabic::Single,
        extension: false,
    });
    let (_, _, voice) = expressive.voices().next().expect("a voice");
    let (voice, staff) = (voice.id, expressive.staves[0].id);
    expressive.voice_homes.insert(voice, staff);
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_a_marker",
        crate::codec::encode_v4_score(&expressive),
    ));
    // Each other value only major 5 can hold, alone in a major-4 form.
    let mut lyric = score.clone();
    lyric.cross_cutting.lyrics = expressive.cross_cutting.lyrics.clone();
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_a_lyric",
        crate::codec::encode_v4_score(&lyric),
    ));
    let line = |kind: crate::graph::SpannerKind, style: crate::graph::LineStyle| {
        let mut lined = score.clone();
        lined.cross_cutting.spanners.push(crate::graph::Spanner {
            id: crate::ids::SpannerId::new(ReplicaId(1), 79),
            start: TimeAnchor::Event {
                id: first,
                offset: AnchorOffset::Zero,
            },
            end: TimeAnchor::Event {
                id: first,
                offset: AnchorOffset::Zero,
            },
            staves: vec![lined.staves[0].id],
            kind,
            style: SpanStyle {
                line: style,
                thickness: None,
            },
        });
        crate::codec::encode_v4_score(&lined)
    };
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_a_wavy_line",
        line(
            crate::graph::SpannerKind::TrillExtension,
            crate::graph::LineStyle::Wavy,
        ),
    ));
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_a_pedal_bracket",
        line(
            crate::graph::SpannerKind::PedalBracket(crate::graph::PedalKind::Sustain),
            crate::graph::LineStyle::Solid,
        ),
    ));
    // A major-4 event's placeholder slots, filled: the frozen encoder writes
    // them empty, so the filled forms are the empty ones with a count or a
    // presence byte set. A note's slots end its bytes: the marks' count, the
    // dynamic's presence, the ornaments' count, the unit stem and the
    // grace's presence (4 + 1 + 4 + 0 + 1 bytes).
    let note = score
        .events
        .iter()
        .find(|e| matches!(e, Event::Pitched(_)))
        .expect("a note")
        .canonical_bytes();
    let tail = note.len() - 10;
    assert_eq!(
        &note[tail..],
        &[0; 10],
        "a note's empty slots end its bytes"
    );
    let refill = |at: usize, value: u8| {
        let mut filled = note.clone();
        filled[at] = value;
        let mut bytes = v4.clone();
        let start = bytes
            .windows(note.len())
            .position(|w| w == note.as_slice())
            .expect("the note is in the score's major-4 bytes");
        bytes.splice(start..start + note.len(), filled);
        bytes
    };
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_an_articulation_placeholder",
        refill(tail, 1),
    ));
    v.push(row(
        SV4,
        "reject",
        "major-5-value",
        "with_a_grace_kind",
        refill(note.len() - 1, 1),
    ));

    const SV5: &str = "core.score_v5";
    v.push(row(SV5, "accept", "-", "valid_score_seed_7", v5.clone()));
    v.push(row(
        SV5,
        "reject",
        "trailing-bytes",
        "valid_score_seed_7_trailing",
        with_trailing_byte(&v5),
    ));
    v.push(row(
        SV5,
        "accept",
        "-",
        "with_expression_and_text",
        expressive.canonical_bytes(),
    ));
    // Text is held to NFC: U+2002 EN SPACE is the composition of U+2000 EN
    // QUAD, both three bytes in UTF-8, so the decomposed form is the same
    // length with its last byte changed.
    let mut spaced = expressive.clone();
    spaced.cross_cutting.lyrics[0].text = crate::Text::new("la\u{2002}la");
    let mut unnormalized = spaced.canonical_bytes();
    let at = unnormalized
        .windows(3)
        .position(|w| w == [0xE2, 0x80, 0x82])
        .expect("the composed space is in the score's bytes");
    unnormalized[at + 2] = 0x80;
    v.push(row(
        SV5,
        "reject",
        "text-not-nfc",
        "with_a_lyric_not_in_nfc",
        unnormalized,
    ));

    // --- Tuplet ----------------------------------------------------------
    const TUPLET: &str = "core.tuplet";
    let tuplet_bytes = tuplet.canonical_bytes();
    v.push(row(TUPLET, "accept", "-", "hidden", tuplet_bytes.clone()));
    v.push(row(
        TUPLET,
        "accept",
        "-",
        "shown",
        Tuplet {
            display: TupletDisplay::default(),
            ..tuplet.clone()
        }
        .canonical_bytes(),
    ));
    // The five-field form of majors 0 to 3: the hidden tuplet without its
    // last two bytes, the display's two tags, so the live decoder runs out.
    v.push(row(
        TUPLET,
        "reject",
        "truncated",
        "major_3_form",
        tuplet_bytes[..tuplet_bytes.len() - 2].to_vec(),
    ));
    v.push(row(
        TUPLET,
        "reject",
        "trailing-bytes",
        "hidden_trailing",
        with_trailing_byte(&tuplet_bytes),
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

/// Runs [`Score::decode_canonical_versioned`] at `major`. Majors 0-3 report
/// injectivity as `true` unconditionally on a successful decode: migration
/// deliberately rewrites the bytes (default-filling new fields), so comparing
/// against the *current* `canonical_bytes()` would fail on every vector, and
/// `decode_vN_score`'s own re-encode-through-`encode_vN_score` guard already
/// proved the input canonical at *its own* major before returning `Ok` at all
/// (see the module doc's account of the contract's "trap"). Only major 5
/// compares `decoded.canonical_bytes() == bytes` — the live layout, where that
/// comparison is exactly what injectivity means.
fn score_check(bytes: &[u8], major: u16) -> Result<bool, String> {
    match Score::decode_canonical_versioned(bytes, major) {
        Ok(decoded) => {
            if major == 5 {
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
        "core.score_v4" => Some(score_check(bytes, 4)),
        "core.score_v5" => Some(score_check(bytes, 5)),
        "core.tuplet" => Some(leaf_check::<Tuplet>(bytes)),
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
        assert_eq!(seen.len(), 17, "surfaces: {:?}", seen.keys());
        for (surface, (accept, reject)) in seen {
            assert!(accept, "{surface} has no accept vector");
            assert!(reject, "{surface} has no reject vector");
        }
    }

    /// The class column is informative (the corpus header), but where this
    /// crate names a mechanical class it names what this decoder does: every
    /// core vector classed `truncated` runs out of bytes and every one classed
    /// `trailing-bytes` has bytes left over. The major-3 tuplet forms are
    /// among the truncated: the core reads no stamp, so it cannot refuse
    /// them by name, as the envelope decoder does.
    #[test]
    fn mechanical_classes_name_the_core_decoders_own_error() {
        use crate::codec::ScoreDecodeError;
        use crate::graph::Tuplet;
        fn error(surface: &str, bytes: &[u8]) -> Option<ScoreDecodeError> {
            let major = surface
                .strip_prefix("core.score_v")
                .map(|m| m.parse::<u16>().expect("a score surface names its major"));
            match (surface, major) {
                (_, Some(major)) => Score::decode_canonical_versioned(bytes, major).err(),
                ("core.tuplet", None) => Tuplet::decode_canonical(bytes).err(),
                ("core.event", None) => Event::decode_canonical(bytes).err(),
                ("core.slur", None) => Slur::decode_canonical(bytes).err(),
                ("core.pitch", None) => Pitch::decode_canonical(bytes).err(),
                ("core.rational_time", None) => RationalTime::decode_canonical(bytes).err(),
                ("core.time_anchor", None) => TimeAnchor::decode_canonical(bytes).err(),
                ("core.score_tuning_context", None) => {
                    ScoreTuningContext::decode_canonical(bytes).err()
                }
                ("core.tuning_override", None) => TuningOverride::decode_canonical(bytes).err(),
                ("core.tuning_scope", None) => TuningScope::decode_canonical(bytes).err(),
                ("core.smufl_version_requirement", None) => {
                    SmuflVersionRequirement::decode_canonical(bytes).err()
                }
                ("core.smufl_version", None) => SmuflVersion::decode_canonical(bytes).err(),
                (other, None) => panic!("{other}: a surface this test does not know"),
            }
        }
        let mut checked = std::collections::BTreeSet::new();
        for (surface, verdict, class, name, bytes) in decode_vectors() {
            let expected = match (verdict, class) {
                ("reject", "truncated") => ScoreDecodeError::UnexpectedEof,
                ("reject", "trailing-bytes") => ScoreDecodeError::TrailingBytes,
                _ => continue,
            };
            assert_eq!(
                error(surface, &bytes),
                Some(expected),
                "{surface}/{name} is classed {class}"
            );
            checked.insert((surface, name.to_string()));
        }
        assert!(checked.contains(&("core.tuplet", "major_3_form".to_string())));
        assert!(checked.contains(&("core.score_v4", "with_a_major_3_tuplet".to_string())));
    }

    /// X4b: each core vector classed `major-5-value` or `text-not-nfc` is
    /// refused by the name its value has, not by some other error a malformed
    /// vector would raise.
    #[test]
    fn major_5_refusals_name_their_value() {
        use crate::codec::ScoreDecodeError;
        let expected = |name: &str| -> &'static str {
            match name {
                "with_a_marker" => "a marker before schema major 5 has no kind",
                "with_a_lyric" => "a lyric line before schema major 5 has no syllable",
                "with_a_wavy_line" => "a wavy line style before schema major 5",
                "with_a_pedal_bracket" => "a pedal line without its sign before schema major 5",
                "with_an_articulation_placeholder" => {
                    "an articulation placeholder before schema major 5"
                }
                "with_a_grace_kind" => "a grace kind before schema major 5",
                "with_a_lyric_not_in_nfc" => "Text: not in Unicode NFC",
                other => panic!("{other}: a refusal this test does not name"),
            }
        };
        let mut checked = 0;
        for (surface, verdict, class, name, bytes) in decode_vectors() {
            if verdict != "reject" || !matches!(class, "major-5-value" | "text-not-nfc") {
                continue;
            }
            let major = surface
                .strip_prefix("core.score_v")
                .and_then(|m| m.parse::<u16>().ok())
                .expect("a score surface");
            assert_eq!(
                Score::decode_canonical_versioned(&bytes, major).err(),
                Some(ScoreDecodeError::InvalidValue(expected(&name))),
                "{surface}/{name}"
            );
            checked += 1;
        }
        assert_eq!(checked, 7, "every major-5 refusal is checked");
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

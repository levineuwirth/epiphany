//! The clipboard fragment projection (Ruling E, `spec/PLAN_EDITOR_APP.md`
//! §Ruling E, GRANTED 2026-07-23): a versioned, values-only s-expression
//! format for [`crate::EditorSession::copy_selection`] /
//! [`crate::EditorSession::paste_at`] / [`crate::EditorSession::paste_over_selection`].
//!
//! # Grammar
//!
//! ```text
//! fragment  ::= "(epiphany-fragment" version voices slurs ties ")"
//! version   ::= "(" u32 " " u32 " " u32 ")"          ; (major minor patch)
//! voices    ::= "(" voice* ")"
//! voice     ::= "(voice (" event* "))"
//! event     ::= "(event" onset duration content ")"
//! onset     ::= <MusicalDuration>                     ; relative to the fragment origin
//! duration  ::= <MusicalDuration>
//! content   ::= "rest" fields | "pitched" fields
//! pitches   ::= "(" pitch-entry* ")"
//! pitch-entry ::= "(pitch-entry" <Pitch> spelling-override ")"
//! spelling-override ::= "()" | "(some" <PitchSpelling> ")"
//! slurs     ::= "(" slur* ")"
//! slur      ::= "(slur" event-ref event-ref <SlurKind> curvature-override <SpanStyle> ")"
//! ties      ::= "(" tie* ")"
//! tie       ::= "(tie" event-ref event-ref <TieClass> <SpanStyle> ")"
//! event-ref ::= "(event-ref" u32{voice} u32{event} ")"
//! ```
//!
//! `<Pitch>`, `<PitchSpelling>`, `<SlurKind>`, `<TieClass>`, `<SpanStyle>`,
//! `<CurvatureOverride>`, `<MusicalDuration>` are the Text Projection's own
//! ratified leaf/value productions — [`epiphany_core::textvalue::TextValue`]
//! impls this module reuses verbatim (`struct_codec!`/`cstyle_enum_codec!` in
//! `epiphany-core`), never re-derived. What is bespoke here is everything
//! *document*-shaped the Text Projection does not have an opinion on: no
//! document id, no envelopes, no causal contexts, and — the load-bearing
//! difference from every id in the rest of the codebase — **no object ids**.
//! An [`EventRef`] names an event by its position in the fragment's own
//! per-voice lanes (`voice` ordinal, `event` ordinal within that lane), never
//! a source `EventId`/`PitchId`/`SlurId`/`TieId`. Paste mints fresh ids from
//! the session's own minters; nothing in this module ever sees a source id.
//!
//! # Closure v1 (Ruling E, fail closed)
//!
//! This module only *represents* closure's outcome — a fragment either
//! carries a slur/tie (both its endpoints resolved to in-fragment
//! [`EventRef`]s) or it does not exist in the fragment at all. The decision
//! of *which* — and the "report dropped" bookkeeping for what closure
//! discarded — is [`crate::EditorSession::copy_selection`]'s job, against the
//! live selection and score; this module has no access to either. A
//! partially-selected tuplet's refusal is likewise a copy-time decision (this
//! module never sees a tuplet).
//!
//! # Untrusted input
//!
//! Fragments arrive from the OS clipboard. [`decode`] enforces three named
//! caps — [`MAX_FRAGMENT_BYTES`], [`MAX_FRAGMENT_EVENTS`],
//! [`MAX_FRAGMENT_NESTING_DEPTH`] — each checked, in that order, *before* the
//! more expensive check after it (byte length is a slice op; nesting depth is
//! one linear scan that never recurses, so it bounds stack depth *before*
//! [`read_sexp`]'s recursive-descent reader ever runs on attacker input; the
//! event count is checked only after a full structural parse, since it needs
//! typed voices to count). An unrecognized major version is rejected
//! immediately after the version triple is read and *before* any attempt to
//! interpret the body as today's voices/slurs/ties grammar — Ruling E: "never
//! partially parsed".

use std::fmt;

use epiphany_core::textvalue::{read_sexp, Sexp, TextError, TextValue};
use epiphany_core::{
    ArticulationMark, CurvatureOverride, DynamicMark, GraceKind, MusicalDuration, OrnamentMark,
    Pitch, PitchSpelling, SlurKind, SpanStyle, StaffPosition, StemConfiguration, TieClass,
};

/// The one fragment major version this build encodes and accepts. Ruling E:
/// starting `(0 1 0)`.
pub const FRAGMENT_VERSION: (u32, u32, u32) = (0, 1, 0);

/// Hard cap on a fragment's encoded byte length. Checked first, on the raw
/// text, before any parsing — the cheapest possible rejection of an
/// unreasonably large clipboard payload.
pub const MAX_FRAGMENT_BYTES: usize = 1 << 20; // 1 MiB

/// Hard cap on the total number of events a fragment may carry, summed over
/// every voice lane. Checked after structural parsing (counting typed events
/// needs the typed voices).
pub const MAX_FRAGMENT_EVENTS: usize = 4096;

/// Hard cap on `(`-nesting depth, checked by one linear, non-recursive scan
/// over the raw text *before* [`read_sexp`] — whose reader recurses one stack
/// frame per open paren — ever sees the input. Comfortably above this
/// grammar's legitimate worst case (a fully-populated pitched event with a
/// spelling override nests on the order of a dozen levels); its job is
/// bounding stack depth against adversarial input, not modeling this
/// grammar's real shape precisely.
pub const MAX_FRAGMENT_NESTING_DEPTH: usize = 64;

/// Why a clipboard fragment could not be decoded. Folded into
/// [`crate::EditorError::InvalidFragment`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FragmentError {
    /// The encoded text is over [`MAX_FRAGMENT_BYTES`].
    TooManyBytes {
        /// The byte cap.
        limit: usize,
        /// The text's actual length.
        found: usize,
    },
    /// The text's `(`-nesting exceeds [`MAX_FRAGMENT_NESTING_DEPTH`].
    NestingTooDeep {
        /// The depth cap.
        limit: usize,
    },
    /// The header named a major version this build does not read (only
    /// `FRAGMENT_VERSION`'s major is accepted). Rejected before any attempt
    /// to parse the body under today's grammar.
    UnsupportedVersion {
        /// The unrecognized major version.
        major: u32,
    },
    /// The text is not a well-formed s-expression, or not a well-formed
    /// fragment of a recognized major version.
    Malformed(TextError),
    /// The fragment's total event count (summed over every voice) is over
    /// [`MAX_FRAGMENT_EVENTS`].
    TooManyEvents {
        /// The event-count cap.
        limit: usize,
        /// The fragment's actual total event count.
        found: usize,
    },
    /// A slur or tie names an `EventRef` outside the fragment's own voices —
    /// corrupt or adversarial input; a well-formed fragment produced by
    /// `encode` never emits one.
    DanglingEventRef,
}

impl fmt::Display for FragmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FragmentError::TooManyBytes { limit, found } => {
                write!(f, "fragment is {found} bytes, over the {limit}-byte cap")
            }
            FragmentError::NestingTooDeep { limit } => {
                write!(f, "fragment nesting exceeds the {limit}-level cap")
            }
            FragmentError::UnsupportedVersion { major } => write!(
                f,
                "fragment major version {major} is not supported (this build reads major {})",
                FRAGMENT_VERSION.0
            ),
            FragmentError::Malformed(err) => write!(f, "malformed fragment: {err}"),
            FragmentError::TooManyEvents { limit, found } => {
                write!(f, "fragment has {found} events, over the {limit}-event cap")
            }
            FragmentError::DanglingEventRef => {
                write!(f, "a slur or tie references an event outside the fragment")
            }
        }
    }
}

impl std::error::Error for FragmentError {}

impl From<TextError> for FragmentError {
    fn from(err: TextError) -> Self {
        FragmentError::Malformed(err)
    }
}

/// The lexical class of `s`, for error messages — mirrors `Sexp`'s own
/// private classifier (duplicated here the same way `epiphany-ops`'
/// `textproj_leaf`/`textvalue_event` each keep their own copy; `Sexp::class`
/// is private to its defining module).
fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

/// A reference to one event **within this fragment**, by its position in a
/// voice lane — never a source `EventId`/`PitchId` (Ruling E: "no object
/// ids"). `voice`/`event` are 0-based indices into [`FragmentDocument::voices`]
/// and that voice's `events`. [`decode`] validates every reference actually
/// resolves before returning a [`FragmentDocument`] — nothing downstream
/// needs to re-check.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct EventRef {
    pub(crate) voice: u32,
    pub(crate) event: u32,
}

impl TextValue for EventRef {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("event-ref"),
            self.voice.project(),
            self.event.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("event-ref", 2)?;
        Ok(EventRef {
            voice: u32::parse(&fields[0])?,
            event: u32::parse(&fields[1])?,
        })
    }
}

/// One note in a chord: its pitch **value** plus an authored spelling
/// override, if the source pitch carried one (an *inferred* spelling never
/// copies — Ruling E: derived state re-derives).
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct FragmentPitch {
    pub(crate) pitch: Pitch,
    pub(crate) spelling_override: Option<PitchSpelling>,
}

impl TextValue for FragmentPitch {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("pitch-entry"),
            self.pitch.project(),
            self.spelling_override.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("pitch-entry", 2)?;
        Ok(FragmentPitch {
            pitch: Pitch::parse(&fields[0])?,
            spelling_override: Option::<PitchSpelling>::parse(&fields[1])?,
        })
    }
}

/// An event's content: a rest, or a chord of one or more pitches with its
/// per-event attachments (Ruling E: "notes/rests with their per-event
/// attachments copy"). Scoped to what [`crate::EditorSession::make_room`]
/// itself already treats as copyable — a note or a rest — mirroring the
/// crate's established make-room boundary rather than inventing a new one;
/// Ruling E names no event-kind scope explicitly (flagged in the W4a
/// report).
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum FragmentEventContent {
    Rest {
        vertical_position: Option<StaffPosition>,
        visible: bool,
    },
    Pitched {
        pitches: Vec<FragmentPitch>,
        articulations: Vec<ArticulationMark>,
        dynamic: Option<DynamicMark>,
        ornaments: Vec<OrnamentMark>,
        stem: StemConfiguration,
        grace: Option<GraceKind>,
    },
}

impl TextValue for FragmentEventContent {
    fn project(&self) -> Sexp {
        match self {
            FragmentEventContent::Rest {
                vertical_position,
                visible,
            } => Sexp::List(vec![
                Sexp::sym("rest"),
                vertical_position.project(),
                visible.project(),
            ]),
            FragmentEventContent::Pitched {
                pitches,
                articulations,
                dynamic,
                ornaments,
                stem,
                grace,
            } => Sexp::List(vec![
                Sexp::sym("pitched"),
                pitches.project(),
                articulations.project(),
                dynamic.project(),
                ornaments.project(),
                stem.project(),
                grace.project(),
            ]),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "FragmentEventContent",
            found: class_of(s),
        })?;
        let head = items
            .first()
            .and_then(Sexp::as_symbol)
            .ok_or(TextError::Syntax(
                "a fragment event content is a list headed by its kind",
            ))?;
        match head {
            "rest" => {
                let fields = s.expect_struct("rest", 2)?;
                Ok(FragmentEventContent::Rest {
                    vertical_position: Option::<StaffPosition>::parse(&fields[0])?,
                    visible: bool::parse(&fields[1])?,
                })
            }
            "pitched" => {
                let fields = s.expect_struct("pitched", 6)?;
                let pitches = Vec::<FragmentPitch>::parse(&fields[0])?;
                if pitches.is_empty() {
                    return Err(TextError::NotCanonical(
                        "a pitched fragment event must have at least one pitch",
                    ));
                }
                Ok(FragmentEventContent::Pitched {
                    pitches,
                    articulations: Vec::<ArticulationMark>::parse(&fields[1])?,
                    dynamic: Option::<DynamicMark>::parse(&fields[2])?,
                    ornaments: Vec::<OrnamentMark>::parse(&fields[3])?,
                    stem: StemConfiguration::parse(&fields[4])?,
                    grace: Option::<GraceKind>::parse(&fields[5])?,
                })
            }
            found => Err(TextError::UnknownConstructor {
                type_name: "FragmentEventContent",
                found: found.to_owned(),
            }),
        }
    }
}

/// One event: a rational onset **relative to the fragment origin**, its
/// written duration, and its content (Ruling E).
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct FragmentEvent {
    pub(crate) onset: MusicalDuration,
    pub(crate) duration: MusicalDuration,
    pub(crate) content: FragmentEventContent,
}

impl TextValue for FragmentEvent {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("event"),
            self.onset.project(),
            self.duration.project(),
            self.content.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("event", 3)?;
        let onset = MusicalDuration::parse(&fields[0])?;
        if onset.0.is_negative() {
            return Err(TextError::NotCanonical(
                "a fragment event's onset must be non-negative",
            ));
        }
        let duration = MusicalDuration::parse(&fields[1])?;
        if !duration.is_positive() {
            return Err(TextError::NotCanonical(
                "a fragment event's duration must be positive",
            ));
        }
        let content = FragmentEventContent::parse(&fields[2])?;
        Ok(FragmentEvent {
            onset,
            duration,
            content,
        })
    }
}

/// One **ordinal-keyed** voice lane (Ruling E: "in per-voice lanes keyed by
/// ordinal (not `VoiceId`)") — its ordinal is this voice's index in
/// [`FragmentDocument::voices`], not a field of this type.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct FragmentVoice {
    pub(crate) events: Vec<FragmentEvent>,
}

impl TextValue for FragmentVoice {
    fn project(&self) -> Sexp {
        Sexp::List(vec![Sexp::sym("voice"), self.events.project()])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("voice", 1)?;
        Ok(FragmentVoice {
            events: Vec::<FragmentEvent>::parse(&fields[0])?,
        })
    }
}

/// A slur fully inside the copied range — closure v1 carried it (both
/// endpoints resolved to in-fragment events); a boundary-cut slur never
/// reaches this type (it is dropped and reported at copy time instead, see
/// the module doc).
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct FragmentSlur {
    pub(crate) start: EventRef,
    pub(crate) end: EventRef,
    pub(crate) kind: SlurKind,
    pub(crate) curvature_override: Option<CurvatureOverride>,
    pub(crate) style: SpanStyle,
}

impl TextValue for FragmentSlur {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("slur"),
            self.start.project(),
            self.end.project(),
            self.kind.project(),
            self.curvature_override.project(),
            self.style.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("slur", 5)?;
        Ok(FragmentSlur {
            start: EventRef::parse(&fields[0])?,
            end: EventRef::parse(&fields[1])?,
            kind: SlurKind::parse(&fields[2])?,
            curvature_override: Option::<CurvatureOverride>::parse(&fields[3])?,
            style: SpanStyle::parse(&fields[4])?,
        })
    }
}

/// A tie fully inside the copied range (see [`FragmentSlur`]'s doc — the
/// same closure rule). The source tie's explicit pitch pairing, if any, is
/// **not** carried (a decision this packet made explicitly, see
/// `DECISIONS.md`): pairing keys on source `PitchId`s, which the fragment
/// deliberately never carries, and remapping it through per-pitch ordinals
/// was judged not worth the added grammar for a v1 whose paste does not yet
/// re-mint cross-cutting structures at all (below). A pasted tie, when
/// pasting is extended to replay one, falls back to `None` — the default
/// enharmonic pairing.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct FragmentTie {
    pub(crate) start: EventRef,
    pub(crate) end: EventRef,
    pub(crate) class: TieClass,
    pub(crate) style: SpanStyle,
}

impl TextValue for FragmentTie {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("tie"),
            self.start.project(),
            self.end.project(),
            self.class.project(),
            self.style.project(),
        ])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("tie", 4)?;
        Ok(FragmentTie {
            start: EventRef::parse(&fields[0])?,
            end: EventRef::parse(&fields[1])?,
            class: TieClass::parse(&fields[2])?,
            style: SpanStyle::parse(&fields[3])?,
        })
    }
}

/// A parsed, validated fragment body (everything after the version header).
/// **Values, never identities** (Ruling E): no document id, no envelopes, no
/// causal contexts, no object ids anywhere in this type or its fields.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct FragmentDocument {
    pub(crate) voices: Vec<FragmentVoice>,
    pub(crate) slurs: Vec<FragmentSlur>,
    pub(crate) ties: Vec<FragmentTie>,
}

fn project_version((major, minor, patch): (u32, u32, u32)) -> Sexp {
    Sexp::List(vec![Sexp::int(major), Sexp::int(minor), Sexp::int(patch)])
}

fn parse_version(s: &Sexp) -> Result<(u32, u32, u32), FragmentError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: "version triple",
        found: class_of(s),
    })?;
    let [major, minor, patch] = items else {
        return Err(TextError::Arity {
            type_name: "version",
            expected: 3,
            found: items.len(),
        }
        .into());
    };
    Ok((u32::parse(major)?, u32::parse(minor)?, u32::parse(patch)?))
}

/// Renders `document` as fragment text: `(epiphany-fragment (0 1 0) …)`.
/// Never fails — it is built from a live, already-valid session selection,
/// not untrusted input (the caps in [`decode`] are a **read**-side
/// discipline; [`crate::EditorSession::copy_selection`] does not need them on
/// the way out).
pub(crate) fn encode(document: &FragmentDocument) -> String {
    Sexp::List(vec![
        Sexp::sym("epiphany-fragment"),
        project_version(FRAGMENT_VERSION),
        document.voices.project(),
        document.slurs.project(),
        document.ties.project(),
    ])
    .render()
}

/// Decodes and validates fragment text — untrusted input (Ruling E). See the
/// module doc for the cap ordering and the "never partially parsed" version
/// discipline.
pub(crate) fn decode(text: &str) -> Result<FragmentDocument, FragmentError> {
    if text.len() > MAX_FRAGMENT_BYTES {
        return Err(FragmentError::TooManyBytes {
            limit: MAX_FRAGMENT_BYTES,
            found: text.len(),
        });
    }
    check_nesting_depth(text)?;

    let sexp = read_sexp(text)?;
    let items = sexp.as_list().ok_or(TextError::Expected {
        expected: "epiphany-fragment",
        found: class_of(&sexp),
    })?;
    let Some((head, rest)) = items.split_first() else {
        return Err(TextError::Syntax("an empty fragment").into());
    };
    if head.as_symbol() != Some("epiphany-fragment") {
        return Err(TextError::Syntax("a fragment begins `(epiphany-fragment …)`").into());
    }
    let [version, voices, slurs, ties] = rest else {
        return Err(TextError::Arity {
            type_name: "epiphany-fragment",
            expected: 4,
            found: rest.len(),
        }
        .into());
    };

    // The version gate: checked, and dispositioned, before any attempt to
    // interpret the body under today's grammar — an unrecognized major is
    // rejected cleanly, never partially parsed (Ruling E).
    let (major, _minor, _patch) = parse_version(version)?;
    if major != FRAGMENT_VERSION.0 {
        return Err(FragmentError::UnsupportedVersion { major });
    }

    let voices = Vec::<FragmentVoice>::parse(voices)?;
    let slurs = Vec::<FragmentSlur>::parse(slurs)?;
    let ties = Vec::<FragmentTie>::parse(ties)?;

    let total_events: usize = voices.iter().map(|v| v.events.len()).sum();
    if total_events > MAX_FRAGMENT_EVENTS {
        return Err(FragmentError::TooManyEvents {
            limit: MAX_FRAGMENT_EVENTS,
            found: total_events,
        });
    }

    let ref_resolves = |r: &EventRef| {
        voices
            .get(r.voice as usize)
            .is_some_and(|v| (r.event as usize) < v.events.len())
    };
    let dangling = slurs
        .iter()
        .any(|s| !ref_resolves(&s.start) || !ref_resolves(&s.end))
        || ties
            .iter()
            .any(|t| !ref_resolves(&t.start) || !ref_resolves(&t.end));
    if dangling {
        return Err(FragmentError::DanglingEventRef);
    }

    Ok(FragmentDocument {
        voices,
        slurs,
        ties,
    })
}

/// A linear, non-recursive scan bounding `(`-nesting depth *before*
/// [`read_sexp`]'s recursive-descent reader runs — see [`MAX_FRAGMENT_NESTING_DEPTH`].
/// Parens inside a quoted string are not counted (a legitimate catalog-id
/// string, the only string-shaped leaf this grammar's leaf productions ever
/// emit, must not spuriously trip the cap); byte strings (`#x…`) never
/// contain parens by grammar, so they need no special handling.
fn check_nesting_depth(text: &str) -> Result<(), FragmentError> {
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;
    for &b in text.as_bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'(' => {
                depth += 1;
                if depth > MAX_FRAGMENT_NESTING_DEPTH {
                    return Err(FragmentError::NestingTooDeep {
                        limit: MAX_FRAGMENT_NESTING_DEPTH,
                    });
                }
            }
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{AcousticPitch, AcousticRealization, TuningReference};
    use epiphany_core::{
        CmnNominal, PitchSpaceId, PitchSpacePosition, RationalTime, ScalePosition,
    };

    fn a_pitch() -> Pitch {
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
                realization: AcousticRealization::Implicit,
            },
        }
    }

    fn a_duration(n: i64, d: i64) -> MusicalDuration {
        MusicalDuration(RationalTime::new(n, d).unwrap())
    }

    fn rest_event(onset: i64) -> FragmentEvent {
        FragmentEvent {
            onset: a_duration(onset, 4),
            duration: a_duration(1, 4),
            content: FragmentEventContent::Rest {
                vertical_position: None,
                visible: true,
            },
        }
    }

    fn pitched_event(onset: i64) -> FragmentEvent {
        FragmentEvent {
            onset: a_duration(onset, 4),
            duration: a_duration(1, 4),
            content: FragmentEventContent::Pitched {
                pitches: vec![FragmentPitch {
                    pitch: a_pitch(),
                    spelling_override: Some(PitchSpelling::cmn(CmnNominal::D, 4)),
                }],
                articulations: vec![],
                dynamic: None,
                ornaments: vec![],
                stem: StemConfiguration,
                grace: None,
            },
        }
    }

    #[test]
    fn a_small_document_round_trips_through_text() {
        let document = FragmentDocument {
            voices: vec![FragmentVoice {
                events: vec![pitched_event(0), rest_event(1)],
            }],
            slurs: vec![FragmentSlur {
                start: EventRef { voice: 0, event: 0 },
                end: EventRef { voice: 0, event: 1 },
                kind: SlurKind::Legato,
                curvature_override: None,
                style: SpanStyle::default(),
            }],
            ties: vec![FragmentTie {
                start: EventRef { voice: 0, event: 0 },
                end: EventRef { voice: 0, event: 1 },
                class: TieClass::Standard,
                style: SpanStyle::default(),
            }],
        };
        let text = encode(&document);
        assert!(text.starts_with("(epiphany-fragment (0 1 0) "));
        let decoded = decode(&text).expect("a well-formed fragment decodes");
        assert_eq!(decoded, document);
    }

    #[test]
    fn decode_rejects_a_fragment_over_the_byte_cap() {
        let text = "a".repeat(MAX_FRAGMENT_BYTES + 1);
        assert_eq!(
            decode(&text),
            Err(FragmentError::TooManyBytes {
                limit: MAX_FRAGMENT_BYTES,
                found: text.len(),
            })
        );
    }

    #[test]
    fn decode_rejects_a_fragment_over_the_event_cap() {
        let document = FragmentDocument {
            voices: vec![FragmentVoice {
                events: (0..=MAX_FRAGMENT_EVENTS as i64).map(rest_event).collect(),
            }],
            slurs: vec![],
            ties: vec![],
        };
        let text = encode(&document);
        let found = MAX_FRAGMENT_EVENTS + 1;
        assert_eq!(
            decode(&text),
            Err(FragmentError::TooManyEvents {
                limit: MAX_FRAGMENT_EVENTS,
                found,
            })
        );
    }

    #[test]
    fn decode_rejects_nesting_over_the_depth_cap_before_parsing() {
        let text = format!(
            "{}{}",
            "(".repeat(MAX_FRAGMENT_NESTING_DEPTH + 1),
            ")".repeat(MAX_FRAGMENT_NESTING_DEPTH + 1)
        );
        assert_eq!(
            decode(&text),
            Err(FragmentError::NestingTooDeep {
                limit: MAX_FRAGMENT_NESTING_DEPTH,
            })
        );
    }

    #[test]
    fn decode_rejects_an_unrecognized_major_cleanly() {
        // A major-1 header over a body that would not even parse under
        // today's grammar (three atoms, not three lists) — proving the
        // version gate rejects before any attempt to interpret the body.
        let text = "(epiphany-fragment (1 0 0) x y z)";
        assert_eq!(
            decode(text),
            Err(FragmentError::UnsupportedVersion { major: 1 })
        );
    }

    #[test]
    fn decode_rejects_a_dangling_event_ref() {
        let document = FragmentDocument {
            voices: vec![FragmentVoice {
                events: vec![rest_event(0)],
            }],
            slurs: vec![FragmentSlur {
                start: EventRef { voice: 0, event: 0 },
                end: EventRef { voice: 0, event: 5 }, // out of range
                kind: SlurKind::Legato,
                curvature_override: None,
                style: SpanStyle::default(),
            }],
            ties: vec![],
        };
        let text = encode(&document);
        assert_eq!(decode(&text), Err(FragmentError::DanglingEventRef));
    }
}

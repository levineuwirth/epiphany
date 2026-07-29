//! The `SpikeResolvedText` type family — a complete, non-canonical mirror of
//! W3 §3E (`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md` lines 369-495).
//!
//! **Not the `.tex` amendment.** Every type here is prefixed `Spike*` (or, for
//! the local stand-ins §3E names but does not yet define anywhere in-tree,
//! documented as a stand-in below) and lives only in this throwaway spike
//! workspace (`spikes/editor-toolkit/`, excluded from the repo-root
//! workspace). Pin 8: mirroring a *subset* of §3E and calling it §3E would
//! test a shape the amendment is not going to have, so every field §3E names
//! is present, under W3's own field names, on every type. Two fields
//! deliberately deviate from §3E's stated Rust types — [`W3_F1`]-driven
//! `version: Option<String>` and the new [`W3_F3`]-driven
//! `face: Option<u32>` — and both deviations are named in
//! `crate::findings` rather than silently taken.
//!
//! [`W3_F1`]: crate::findings::W3_F1
//! [`W3_F3`]: crate::findings::W3_F3
//! [`W3_F6`]: crate::findings::W3_F6
//!
//! ## Named pin-8 deviation: this is a wire mirror, not the runtime type
//!
//! The "reused directly" claim in the table below is true of the values this
//! crate *computes* with — `crate::shape` and `crate::hittest` work in
//! `epiphany_layout_ir::{Point, BoundingBox, StaffSpace, Transform2D,
//! GlyphStyle}` and `epiphany_layout_ir::Provenance` throughout. It is **not**
//! true of what [`SpikeResolvedText`] stores: `epiphany-layout-ir` carries no
//! `serde` dependency at all, so every one of those types is stored here as a
//! `Spike*` serde mirror with an infallible `From` conversion.
//!
//! That is a real deviation from pin 8's "complete §3E mirror", and it is
//! recorded rather than glossed. Its consequences, in order of seriousness:
//!
//! 1. **§3E's `ResolvedText` is not serializable, and the first consumer to
//!    need it wrote a lossy mirror.** That is [`W3_F6`] — a finding about the
//!    amendment, not about this crate.
//! 2. The numeric mirrors ([`SpikePoint`], [`SpikeBoundingBox`],
//!    [`SpikeStaffSpace`]) widen `f32` to `f64`, which is exact, and
//!    [`SpikeTransform2D`] keeps `f32` verbatim. Nothing is lost.
//! 3. [`SpikeProvenance`] used to be genuinely lossy — see its own doc
//!    comment for what it carried and why that was wrong. It now carries
//!    `source` and `dependencies` as canonical byte forms under W3's field
//!    names, so the only surviving gap is `synthesis`, a `Debug` rendering of
//!    a closed enum that is `None` on every fixture here and is labelled as a
//!    rendering at its definition.
//!
//! A consumer that needs the runtime types rather than the wire types builds
//! them from these by the same `From` conversions run backwards — which no
//! packet has needed yet, and which is exactly the work [`W3_F6`] says the
//! amendment should not be leaving to its consumers.
//!
//! ## Real types reused, local stand-ins defined
//!
//! Where `epiphany-layout-ir` already defines a public type §3E also names,
//! this module reuses it directly rather than reproducing it (the same
//! judgment call `round1-candidates/harness` makes for `PathCommand`):
//!
//! | §3E name | reused as |
//! |---|---|
//! | `Provenance` | [`epiphany_layout_ir::Provenance`] |
//! | `Point` | [`epiphany_layout_ir::Point`] |
//! | `BoundingBox` | [`epiphany_layout_ir::BoundingBox`] |
//! | `Transform2D` | [`epiphany_layout_ir::Transform2D`] |
//! | `StaffSpace` | [`epiphany_layout_ir::StaffSpace`] |
//! | `GlyphStyle` | [`epiphany_layout_ir::GlyphStyle`] |
//! | `FontId` (on `TextFaceIdentity::family`) | [`epiphany_layout_ir::FontId`] |
//!
//! §3E names four more types the crate does not define anywhere yet, so this
//! module defines local stand-ins, documented at each definition below:
//! `TextAlign`, `TextDirection`, `ScriptTag`, `LanguageTag`. (`ShaperId`,
//! `UnicodeVersion`, and `FeatureSetting` — named by §3E's
//! `TextShapingIdentity`, not `ResolvedText` itself — are stand-ins defined
//! in [`crate::identity`] instead, next to the rest of the shaping identity.)

use std::ops::Range;

use epiphany_layout_ir::{BoundingBox, GlyphStyle, Point, Provenance, StaffSpace, Transform2D};
use serde::{Deserialize, Serialize};

use crate::identity::SpikeTextShapingIdentity;

/// Stand-in for §3E's `TextAlign`. Every fixture in this recipe uses `Start`
/// (recipe §3): "`align` is `Start`; per W3, `PositionedGlyph::offset` has
/// alignment already applied — a consumer places the run by `origin` alone."
/// The other variants are carried so the type is not vacuously a unit struct
/// masquerading as an enum with one option no one chose.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpikeTextAlign {
    Start,
    Center,
    End,
}

/// Stand-in for §3E's `TextDirection`. Derived from the bidi embedding
/// level's parity (even = `Ltr`, odd = `Rtl`) for every segment this crate
/// itemizes — never guessed from script alone, so a segment's declared
/// direction is always traceable to the bidi algorithm that produced it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpikeTextDirection {
    Ltr,
    Rtl,
}

/// Stand-in for §3E's `ScriptTag`: the 4-letter OpenType/ISO-15924 script
/// tag `rustybuzz` inferred for a segment (e.g. `"Latn"`, `"Hebr"`), read
/// from `rustybuzz::Script::tag` — never from a locale or a caller-supplied
/// guess, so it is reproducible across machines.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeScriptTag(pub String);

/// Stand-in for §3E's `LanguageTag`. This recipe's fixtures set no explicit
/// language (recipe §6 does not name one, and `rustybuzz` does not infer a
/// default from the host locale — see `crate::shape`'s doc comment on why
/// that omission is deliberate and deterministic rather than an oversight).
/// `None` on every segment this crate produces.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeLanguageTag(pub Option<String>);

/// One glyph, positioned (§3E `PositionedGlyph`).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikePositionedGlyph {
    pub glyph_id: u32,
    /// Offset from the run's `origin`, staff-space, y-up, quantized to
    /// `crate::QUANTIZE_GRID`, with alignment already applied (§3E:
    /// "a consumer places the run by `origin` alone and never re-derives
    /// from `align`").
    pub offset: SpikePoint,
    pub transform: Option<SpikeTransform2D>,
}

/// One itemized, shaped run of text within a `SpikeResolvedText` (§3E
/// `ShapedSegment`).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeShapedSegment {
    /// Index into `SpikeTextShapingIdentity::faces` this segment resolved
    /// to, or `None` if resolution walked the whole declared chain and no
    /// face covered this span (`crate::findings::W3_F3`; F-C's uncovered
    /// Arabic letter is the fixture that forces this variant to exist). A
    /// `None`-face segment carries no glyphs — shaping is never attempted
    /// against a face that cannot represent the codepoint, because doing so
    /// would silently draw the `.notdef` glyph, exactly the ambient
    /// substitution pin 9 forbids.
    pub face: Option<u32>,
    pub glyphs: Vec<SpikePositionedGlyph>,
    /// Half-open UTF-8 byte offsets into the owning `SpikeResolvedText::text`.
    pub source: Range<u32>,
    pub direction: SpikeTextDirection,
    pub script: SpikeScriptTag,
    pub language: SpikeLanguageTag,
    /// Em size in staff spaces — `crate::EM_SIZE_STAFF_SPACE` on every
    /// segment in this recipe (recipe §3).
    pub size: SpikeStaffSpace,
}

/// Which side of a possible direction boundary a caret stop's geometric
/// position was read from. Most stops sit inside a single run, where both
/// readings coincide and this crate always records `Downstream`; a stop at
/// an offset that is *also* a direction-run boundary gets two `SpikeCaretStop`s,
/// one per affinity, at two different geometric positions (recipe §7; W3 §5
/// check 4). See `crate::shape` for the construction rule.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpikeCaretAffinity {
    /// Read from the run *ending* at this offset — the visual position
    /// immediately after that run's last glyph.
    Upstream,
    /// Read from the run *starting* at this offset — the visual position
    /// immediately before that run's first glyph. The position every
    /// ordinary (non-boundary) caret stop uses.
    Downstream,
}

/// One caret stop: a grapheme-cluster boundary's geometric position and
/// bidi affinity (recipe §7; W3 §5 check 4).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeCaretStop {
    /// UTF-8 byte offset into `SpikeResolvedText::text` — the boundary this
    /// stop names. Always a grapheme-cluster start, from
    /// `unicode-segmentation`, never a codepoint or glyph boundary.
    pub source_offset: u32,
    pub position: SpikePoint,
    pub affinity: SpikeCaretAffinity,
}

/// One shaping cluster: a maximal span of source bytes shaping treated as
/// indivisible (one HarfBuzz cluster id), together with the caret stops
/// (one per grapheme the span covers) and, for a covered span, the glyph
/// indices that drew it (§3E: "`ClusterMap` carries, per cluster: its source
/// range, its glyph indices, and its caret stops").
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeCluster {
    /// Half-open UTF-8 byte offsets into `SpikeResolvedText::text`.
    pub source: Range<u32>,
    /// Index into `SpikeResolvedText::segments` this cluster's glyphs (if
    /// any) belong to. Always `Some` — even an unresolved cluster belongs
    /// to its `face: None` segment (`crate::findings::W3_F3`).
    pub segment: usize,
    /// Indices into `segments[segment].glyphs`, in that segment's stored
    /// (visual) order. Empty for an unresolved cluster.
    pub glyph_indices: Vec<u32>,
    /// `false` for a codepoint no declared face covers (W3 §5 invariant 4:
    /// "a cluster that shaping could not resolve is represented
    /// diagnostically... never dropped"). F-C is the fixture that exercises
    /// this.
    pub resolved: bool,
    /// How many `unicode-segmentation` extended grapheme clusters this
    /// shaping cluster covers. `1` for an ordinary character, `2` for a
    /// ligature (F-A's `ff`/`fi`) — the denominator the interpolation rule
    /// (recipe §7) divides a ligature's advance by.
    pub grapheme_count: u32,
    pub caret_stops: Vec<SpikeCaretStop>,
}

/// The complete cluster/caret map for one `SpikeResolvedText` (§3E
/// `ClusterMap`): every shaping cluster, in ascending source-byte order.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeClusterMap {
    pub clusters: Vec<SpikeCluster>,
}

/// The shaped text run (§3E `ResolvedText`) — the complete mirror this
/// module's doc comment describes. **Non-canonical; not the `.tex`
/// amendment.**
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeResolvedText {
    pub provenance: SpikeProvenance,
    /// The source string, verbatim — not normalized, not the shaped ink.
    /// F-E's NFD text is exercised precisely because this field must equal
    /// its input byte-for-byte (recipe §8).
    pub text: String,
    pub shaping: SpikeTextShapingIdentity,
    pub segments: Vec<SpikeShapedSegment>,
    pub clusters: SpikeClusterMap,
    pub bounds: SpikeBoundingBox,
    pub reserved_box: SpikeBoundingBox,
    pub origin: SpikePoint,
    pub align: SpikeTextAlign,
    pub style: SpikeGlyphStyle,
    pub layer: i32,
}

// ---------------------------------------------------------------------
// Serde-friendly mirrors of the real epiphany-layout-ir types.
//
// `epiphany_layout_ir::{Point, BoundingBox, Transform2D, StaffSpace,
// GlyphStyle, Provenance}` are the REAL types (see the module doc table
// above) and are what this crate computes with internally throughout
// `crate::shape`. None of them derive `serde::{Serialize, Deserialize}`
// (they are working IR types, not wire types — `epiphany-layout-ir` has no
// serde dependency at all), so `fixtures.json` needs a serializable mirror
// at the boundary. Each mirror is a plain data transcription with an
// infallible `From` conversion; nothing is recomputed or re-derived on the
// way through, so a mirror disagreeing with its source would be a bug in
// the `From` impl, not a second source of truth.
// ---------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeStaffSpace(pub f64);

impl From<StaffSpace> for SpikeStaffSpace {
    fn from(s: StaffSpace) -> Self {
        SpikeStaffSpace(s.0 as f64)
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikePoint {
    pub x: f64,
    pub y: f64,
}

impl From<Point> for SpikePoint {
    fn from(p: Point) -> Self {
        SpikePoint {
            x: p.x.0 as f64,
            y: p.y.0 as f64,
        }
    }
}

impl SpikePoint {
    pub const fn new(x: f64, y: f64) -> Self {
        SpikePoint { x, y }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeBoundingBox {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

impl From<BoundingBox> for SpikeBoundingBox {
    fn from(b: BoundingBox) -> Self {
        SpikeBoundingBox {
            left: b.left.0 as f64,
            bottom: b.bottom.0 as f64,
            right: b.right.0 as f64,
            top: b.top.0 as f64,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeTransform2D {
    pub matrix: [[f32; 3]; 3],
}

impl From<Transform2D> for SpikeTransform2D {
    fn from(t: Transform2D) -> Self {
        SpikeTransform2D { matrix: t.matrix }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeGlyphStyle {
    pub rgba: u32,
}

impl From<GlyphStyle> for SpikeGlyphStyle {
    fn from(s: GlyphStyle) -> Self {
        SpikeGlyphStyle { rgba: s.rgba }
    }
}

/// Serializable mirror of `epiphany_core::ids::TypedObjectId`, carrying the
/// **canonical** identity rather than a rendering of it.
///
/// `TypedObjectId::canonical_bytes()` *is* the identity — Chapter 5 defines
/// equality, ordering and hashing over exactly those bytes — so mirroring
/// them (plus the variant discriminant, which they already contain, restated
/// for readability) is lossless for every variant, present and future,
/// without a 28-arm match that would silently miss the next one.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeTypedObjectId {
    /// `TypedObjectId::discriminant()` — the 16-bit variant tag.
    pub discriminant: u16,
    /// `TypedObjectId::canonical_bytes()`, lowercase hex. The identity.
    pub canonical_bytes_hex: String,
}

impl From<&epiphany_core::TypedObjectId> for SpikeTypedObjectId {
    fn from(id: &epiphany_core::TypedObjectId) -> Self {
        SpikeTypedObjectId {
            discriminant: id.discriminant(),
            canonical_bytes_hex: hex_lower(&id.canonical_bytes()),
        }
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Serializable mirror of `epiphany_layout_ir::Provenance`, **under W3's own
/// field names, carrying every field**.
///
/// Revision 2 of this file carried `source_debug: String` and
/// `dependency_count: usize` instead of `source` and `dependencies`. That was
/// a real deviation from pin 8 wearing the clothes of a faithful mirror: the
/// module doc claimed "every field §3E names is present, under W3's own field
/// names, on every type" while this type renamed one field into a `Debug`
/// rendering and replaced another with its length. `Debug` output is not an
/// encoding — it has no stability contract and cannot be parsed back — and a
/// dependency *count* discards the invalidation set that is the field's entire
/// purpose (Chapter 7 §"Invalidation Rules"). It happened to lose nothing
/// measurable here only because these fixtures' dependency lists are empty,
/// which is an accident of the fixtures, not a property of the mirror.
///
/// These fixtures are synthetic (recipe §5: "Bidi and fallback are exercised
/// in the spike through synthetic `ResolvedText` fixtures, which need no model
/// work"), so there is no real score-graph object behind them; `source`
/// records the placeholder `TypedObjectId` `crate::fixtures` constructs (an
/// `Event` id derived from the fixture's own ordinal) purely for traceability,
/// not as a claim that a score-graph object exists.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeProvenance {
    pub source: SpikeTypedObjectId,
    /// `SynthesisKind` is a closed C-like enum with one payload-carrying
    /// variant (`Registered(SynthesisRegistryId)`) and no canonical byte form
    /// of its own, so this is its `Debug` rendering — named as such rather
    /// than presented as an encoding. `None` on every fixture in this recipe.
    pub synthesis: Option<String>,
    pub dependencies: Vec<SpikeTypedObjectId>,
    /// **Serialized as a decimal string, not a JSON number, and that is not
    /// cosmetic.** A `u128` stable id renders as up to 39 digits, which
    /// exceeds every numeric type a generic JSON parser offers: round-tripping
    /// this file through `serde_json::Value` — or Python's `json`, or any
    /// JavaScript consumer — silently converts it to an `f64` and destroys the
    /// low bits. Measured before this fix:
    /// `82875741697311382809239399464544864365` came back as
    /// `8.287574169731139e+37`.
    ///
    /// A provenance id that changes when a tool merely reads and rewrites the
    /// file is not an identity. The canonical wire format is binary and has no
    /// such problem, so this is a constraint on **JSON artifacts** — this
    /// file, and any debug or fixture dump carrying an id of this width.
    #[serde(with = "u128_as_string")]
    pub stable_id: u128,
}

/// Serializes a `u128` losslessly as a decimal string. See
/// [`SpikeProvenance::stable_id`] for why a bare JSON number is not safe.
mod u128_as_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &u128, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u128, D::Error> {
        let s = String::deserialize(d)?;
        s.parse::<u128>().map_err(serde::de::Error::custom)
    }
}

impl From<&Provenance> for SpikeProvenance {
    fn from(p: &Provenance) -> Self {
        SpikeProvenance {
            source: (&p.source).into(),
            synthesis: p.synthesis.map(|s| format!("{s:?}")),
            dependencies: p
                .dependencies
                .iter()
                .map(SpikeTypedObjectId::from)
                .collect(),
            stable_id: p.stable_id.0,
        }
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::*;
    use epiphany_core::{EventId, PitchId, TypedObjectId};

    /// The mirror must carry the dependency *set*, not its length. Revision 2
    /// carried `dependency_count: usize`; this is the test that would have
    /// failed then, and it is here so the field cannot quietly become a count
    /// again.
    #[test]
    fn the_mirror_carries_every_dependency_not_a_count() {
        let source = TypedObjectId::Event(EventId::from_raw(7));
        let deps = vec![
            TypedObjectId::Pitch(PitchId::from_raw(11)),
            TypedObjectId::Event(EventId::from_raw(13)),
        ];
        let real = Provenance::projected(source, deps.clone());
        let mirror = SpikeProvenance::from(&real);
        assert_eq!(mirror.dependencies.len(), 2);
        for (m, d) in mirror.dependencies.iter().zip(deps.iter()) {
            assert_eq!(m, &SpikeTypedObjectId::from(d));
        }
        // Two ids that differ ONLY in variant must mirror differently — a
        // count, or a payload-only mirror, could not tell these apart.
        let a = SpikeTypedObjectId::from(&TypedObjectId::Event(EventId::from_raw(11)));
        let b = SpikeTypedObjectId::from(&TypedObjectId::Pitch(PitchId::from_raw(11)));
        assert_ne!(a, b);
    }

    /// The mirrored bytes must be `canonical_bytes()` itself, not a rendering
    /// that merely looks like it.
    #[test]
    fn the_mirrored_id_is_the_canonical_byte_form() {
        let id = TypedObjectId::Event(EventId::from_raw(0xF00D_0000));
        let m = SpikeTypedObjectId::from(&id);
        assert_eq!(m.discriminant, id.discriminant());
        assert_eq!(m.canonical_bytes_hex, hex_lower(&id.canonical_bytes()));
        assert_eq!(m.canonical_bytes_hex.len(), id.canonical_bytes().len() * 2);
    }
}

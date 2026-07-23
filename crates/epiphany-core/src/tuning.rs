//! Chapter 4 tuning-resolution vocabulary and the resolver
//! (`core_spec.tex` §"Tuning Systems", `sec:tuning:system`, `:3279` onward),
//! Push 4b tranche 2 (`spec/CONTRACT_PUSH4B_RESOLVER.md`).
//!
//! Like [`crate::pitch_space`] this is a **vertical slice that stays in
//! memory**: [`TuningSystem`], [`TuningResolution`], [`TuningOverride`],
//! [`TuningScope`], the built-in catalog, and [`resolve_pitch_frequency`]
//! (the resolver) land together, with a behavioural proof of life (real
//! frequencies asserted, not `is_ok()`) rather than as three separate
//! passes. **No `Codec` impl exists, or may be added, for anything in this
//! module** (Ruling C, `spec/PLAN_PUSH4B_TUNING.md`): these types are
//! referenced only by id from canonical score state (`ScoreTuningContext`'s
//! wire form stays the three fields it always had — see the hand-written
//! codec in `codec.rs`), so they stay free to change once a later tranche
//! discovers they are wrong.
//!
//! ## What resolves, and what still does not
//!
//! [`TuningResolution`] is a **six**-variant enum in the specification
//! (`core_spec.tex:3309`); this module defines three —
//! [`TuningResolution::EqualTemperament`], [`TuningResolution::PerPositionRatios`],
//! and, since Push 4b tranche 2b, [`TuningResolution::Function`] (plus
//! [`PositionRatio`] and the marker [`TuningParameters`]). The other three are
//! transcribed only when a built-in needs them, so their unconstructed payload
//! subtrees (`ImportedTuningData`, `AdaptiveTuningParameters`, …) never become
//! an unconsumed type surface (the `NOTEHEAD_ANCHORS` failure): `Adaptive`
//! waits on `HarmonicContext`, which does not exist in Rust and whose
//! completion `core_spec.tex` puts out of scope; `Overlay` and `Imported` wait
//! on a built-in that needs a split-accidental keyboard or an imported
//! `.scl`/MTS tuning respectively — nothing in the twenty-item catalog
//! constructs either.
//!
//! [`built_in_tuning_system`] resolves **nineteen** of the twenty catalog
//! identifiers (`req:tuning:builtin-tuning-catalog`): the six `tet-*` equal
//! temperaments, the three `ji-static-5limit-*` just-intonation systems, and,
//! since tranche 2b, the ten historical temperaments (`pythagorean`, the three
//! `meantone-*`, `werckmeister-iii`/`-iv`, `vallotti`, `kirnberger-ii`/`-iii`,
//! `young-ii`) — each built from its ratified fifth-tempering construction
//! (`core_spec.tex` §"Temperament Constructions", `:3696`-`4011`) by
//! `temperament_ratios`, never from a pasted cents table. Only
//! `ji-adaptive-5limit` is still a real catalog entry whose resolution this
//! module defers ([`TuningCatalogEntry::Deferred`]) — never a guessed
//! frequency — because it needs `HarmonicContext`, which does not exist in
//! Rust.
//!
//! ## The compatibility check, narrowed the same way tranche 1 narrowed it
//!
//! `req:tuning:tuning-system-compatibility` (`:3581`) allows a resolved tuning
//! system's `pitch_space` to differ from the resolved pitch space when a
//! *registered compatibility mapping* declares them compatible. **No such
//! registry exists** in this tranche (matching how tranche 1 left the
//! pitch-space-mapping registry unbuilt, `spec/PLAN_PUSH4B_TUNING.md` Ruling
//! C): [`resolve_pitch_frequency`] accepts only exact `pitch_space` equality
//! and fails closed on any mismatch, a deliberate deferral, not an oversight.

use core::num::NonZeroU32;

use crate::graph::{Score, ScoreTuningContext};
use crate::ids::{RegionId, StaffId, VoiceId};
use crate::pitch::{
    AcousticRealization, Pitch, PitchSpaceId, PitchSpacePosition, ReferencePitch, TuningFunctionId,
    TuningReference, TuningSystemId, VoiceSelector,
};
use crate::pitch_space::{built_in_position_structure, JiRatio, PositionStructure};
use crate::time::TimeAnchor;

// ===========================================================================
// Types (Chapter 4 §"Tuning Systems" / §"Score Tuning Context and
// Hierarchical Resolution").
// ===========================================================================

/// One entry of a [`TuningResolution::PerPositionRatios`] catalog
/// (`core_spec.tex:3314-3316`): "Explicit per-position ratios. Each entry is
/// a ratio relative to the reference position." The specification's own
/// listing writes only `PerPositionRatios(Vec<PositionRatio>)` and never
/// spells out `PositionRatio`'s fields — nothing in `core_spec.tex`
/// constructs one but the three `ji-static-5limit-*` built-ins — so this
/// tranche defines it as narrowly as those three need: a chromatic position
/// plus the exact ratio-to-anchor at that position, reusing
/// [`JiRatio`](crate::pitch_space::JiRatio) rather than inventing a second
/// rational type.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PositionRatio {
    /// The chromatic position this ratio governs: `0..divisions_per_octave`
    /// of the enclosing pitch space's chromatic layer (for the built-ins
    /// below, always `0..12`, `cmn-12`'s chromatic layer).
    pub position: i32,
    /// The exact frequency ratio of `position` relative to the tuning's own
    /// 1/1 (its anchor), octave-reduced into `[1, 2)`.
    pub ratio: JiRatio,
}

/// Placeholder for [`TuningResolution::Function`]'s parameter schema
/// (`core_spec.tex:3318-3324`). The specification never gives this type's
/// shape — Chapter 4's own forward-reference list (`:4083`) singles out the
/// sibling `AdaptiveTuningParameters` as "likewise undefined," for the same
/// reason: no built-in in the twenty-item catalog parameterizes a `Function`
/// resolution, since each of the ten historical temperaments this module
/// builds is fixed entirely by its [`TuningFunctionId`] alone. The same
/// discipline as [`SpellingParameters`](crate::pitch_space::SpellingParameters),
/// one level down: a documented zero-field marker so
/// `TuningResolution::Function`'s field list matches the specification's own
/// listing, carrying no state and inventing no schema.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct TuningParameters;

/// How a tuning system resolves pitch-space positions to frequencies
/// (`core_spec.tex:3309-3348`). **Deliberately partial**: the specification
/// names six variants; this module defines three. See the module doc for
/// which tranche completes each of the other three.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TuningResolution {
    /// N-tone equal temperament: each step is the Nth root of the octave
    /// ratio. Includes 12-TET when `divisions_per_octave == 12`.
    EqualTemperament { divisions_per_octave: u16 },
    /// Explicit per-position ratios, each relative to the reference
    /// position.
    PerPositionRatios(Vec<PositionRatio>),
    /// Procedural definition: a registered tuning function that computes
    /// frequencies from a reference (`core_spec.tex:3318-3324`: "Historical
    /// temperaments (Werckmeister, Vallotti, meantone variants) live here").
    /// The ten historical temperaments are reserved built-in
    /// [`TuningFunctionId`]s, each re-derived from its ratified
    /// fifth-tempering construction (`core_spec.tex` §"Temperament
    /// Constructions", `:3696`-`4011`) by `temperament_ratios` rather than
    /// transcribed from a cents table. No parameter schema exists yet
    /// ([`TuningParameters`] is a documented marker); an unreserved
    /// `TuningFunctionId` has no registry to resolve against and fails
    /// closed (`coordinate_ratio` returns `None`) — `Function` is an
    /// extension point, not a registry this module builds.
    Function {
        function: TuningFunctionId,
        parameters: TuningParameters,
    },
}

/// A tuning system: a map from pitch-space positions to frequencies, given a
/// reference pitch (`core_spec.tex:3287-3303`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TuningSystem {
    pub id: TuningSystemId,
    /// Human-readable name.
    pub name: String,
    /// The pitch space whose positions this tuning resolves.
    pub pitch_space: PitchSpaceId,
    /// How the tuning resolves positions to frequencies, given a reference
    /// pitch.
    pub resolution: TuningResolution,
    /// Optional historical or provenance notes.
    pub description: Option<String>,
}

/// The scope a [`TuningOverride`] applies to (`core_spec.tex:3534-3539`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TuningScope {
    Voice(VoiceId),
    Staff(StaffId),
    Region(RegionId),
    Range {
        start: TimeAnchor,
        end: TimeAnchor,
        voices: VoiceSelector,
    },
}

/// A per-scope override of one or more tuning components
/// (`core_spec.tex:3527-3532`). `None` fields inherit from the next-outer
/// scope, per `req:tuning:tuning-resolution-order`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TuningOverride {
    pub scope: TuningScope,
    pub pitch_space: Option<PitchSpaceId>,
    pub tuning_system: Option<TuningSystemId>,
    pub reference: Option<ReferencePitch>,
}

// ===========================================================================
// The built-in catalog, as data (partial, honestly) — item 2.
// ===========================================================================

/// A built-in catalog lookup result: a real, resolved [`TuningSystem`], or a
/// real catalog identifier (`req:tuning:builtin-tuning-catalog` still
/// requires it to resolve *eventually*) whose resolution this tranche
/// defers. Distinguishing this from "not a built-in identifier at all"
/// ([`built_in_tuning_system`] returning `None`) is what lets
/// [`resolve_pitch_frequency`] report a genuinely unknown identifier and a
/// known-but-deferred one differently, per the contract's "a clear 'not yet
/// supported' error, never a fallback frequency."
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TuningCatalogEntry {
    Resolved(TuningSystem),
    /// Why this identifier's resolution is deferred, and to what.
    Deferred(&'static str),
}

/// Looks up a built-in [`TuningSystem`] (Chapter 4 §"Built-in Catalog",
/// `core_spec.tex:3656-3694`, `req:tuning:builtin-tuning-catalog`).
///
/// Nineteen of the twenty resolve:
///
/// * the six `tet-*` — [`TuningResolution::EqualTemperament`] from the
///   identifier's divisions. `tet-12` pairs with `cmn-12` (the default
///   pairing, `core_spec.tex:4058-4068`); `tet-19/22/31/53/72` pair with the
///   built-in `edo-19/22/31/53/72` pitch spaces — the only built-in pitch
///   spaces whose [`PositionStructure::Chromatic`] cardinality matches, so
///   the pairing is forced by the catalog rather than chosen;
/// * the three `ji-static-5limit-{C,G,D}` — [`TuningResolution::PerPositionRatios`]
///   computed by `ji_static_5limit_ratios` from the lattice block
///   (`req:tuning:ji-static-construction`), over `cmn-12`'s twelve chromatic
///   positions (`core_spec.tex:4019-4021`: "assigned in ascending order to
///   the twelve chromatic positions of `cmn-12`");
/// * the ten historical temperaments (`pythagorean`, three `meantone-*`,
///   `werckmeister-iii`/`-iv`, `vallotti`, `kirnberger-ii`/`-iii`,
///   `young-ii`, Push 4b tranche 2b) — [`TuningResolution::Function`], each
///   built by `temperament_ratios` from its ratified fifth-tempering
///   construction (`core_spec.tex` §"Temperament Constructions",
///   `:3696`-`4011`), over `cmn-12`'s twelve chromatic positions exactly as
///   the static-JI systems are.
///
/// The remaining one is a real catalog entry whose resolution is deferred,
/// not guessed ([`TuningCatalogEntry::Deferred`]):
///
/// * `ji-adaptive-5limit` — needs `HarmonicContext`
///   (`req:tuning:adaptive-default-version`), which does not exist in Rust.
///
/// `None` for any other identifier: not one of the twenty at all.
pub fn built_in_tuning_system(id: &TuningSystemId) -> Option<TuningCatalogEntry> {
    fn tet(
        name: &'static str,
        pitch_space: &'static str,
        divisions: u16,
        desc: &'static str,
    ) -> TuningCatalogEntry {
        TuningCatalogEntry::Resolved(TuningSystem {
            id: TuningSystemId::new(name),
            name: name.to_owned(),
            pitch_space: PitchSpaceId::new(pitch_space),
            resolution: TuningResolution::EqualTemperament {
                divisions_per_octave: divisions,
            },
            description: Some(desc.to_owned()),
        })
    }
    fn ji_static(
        name: &'static str,
        anchor_chromatic_degree: i32,
        tonic: &'static str,
    ) -> TuningCatalogEntry {
        TuningCatalogEntry::Resolved(TuningSystem {
            id: TuningSystemId::new(name),
            name: name.to_owned(),
            pitch_space: PitchSpaceId::new("cmn-12"),
            resolution: TuningResolution::PerPositionRatios(ji_static_5limit_ratios(
                anchor_chromatic_degree,
            )),
            description: Some(format!(
                "Static 5-limit just intonation anchored to {tonic} tonic."
            )),
        })
    }
    /// A historical temperament's catalog entry: [`TuningResolution::Function`]
    /// naming `name` as its own [`TuningFunctionId`] (item 3 of
    /// `spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`: "`pitch_space` = `cmn-12`").
    /// `desc` is transcribed from the built-in catalog table
    /// (`core_spec.tex:3669-3679`).
    fn temperament(name: &'static str, desc: &'static str) -> TuningCatalogEntry {
        TuningCatalogEntry::Resolved(TuningSystem {
            id: TuningSystemId::new(name),
            name: name.to_owned(),
            pitch_space: PitchSpaceId::new("cmn-12"),
            resolution: TuningResolution::Function {
                function: TuningFunctionId::new(name),
                parameters: TuningParameters,
            },
            description: Some(desc.to_owned()),
        })
    }
    const DEFERRED_ADAPTIVE: &str =
        "adaptive tuning needs HarmonicContext, which does not exist in Rust (out of scope this tranche)";
    match id.as_str() {
        "tet-12" => Some(tet(
            "tet-12",
            "cmn-12",
            12,
            "12-tone equal temperament. The default.",
        )),
        "tet-19" => Some(tet("tet-19", "edo-19", 19, "19-tone equal temperament.")),
        "tet-22" => Some(tet("tet-22", "edo-22", 22, "22-tone equal temperament.")),
        "tet-31" => Some(tet("tet-31", "edo-31", 31, "31-tone equal temperament.")),
        "tet-53" => Some(tet("tet-53", "edo-53", 53, "53-tone equal temperament.")),
        "tet-72" => Some(tet("tet-72", "edo-72", 72, "72-tone equal temperament.")),
        "ji-static-5limit-C" => Some(ji_static("ji-static-5limit-C", 0, "C")),
        "ji-static-5limit-G" => Some(ji_static("ji-static-5limit-G", 7, "G")),
        "ji-static-5limit-D" => Some(ji_static("ji-static-5limit-D", 2, "D")),
        "pythagorean" => Some(temperament(
            "pythagorean",
            "Pure-fifth (3:2) Pythagorean tuning.",
        )),
        "meantone-1/4-comma" => Some(temperament(
            "meantone-1/4-comma",
            "Quarter-comma meantone (pure major thirds).",
        )),
        "meantone-1/6-comma" => Some(temperament("meantone-1/6-comma", "Sixth-comma meantone.")),
        "meantone-1/5-comma" => Some(temperament("meantone-1/5-comma", "Fifth-comma meantone.")),
        "werckmeister-iii" => Some(temperament(
            "werckmeister-iii",
            "Werckmeister III well temperament.",
        )),
        "werckmeister-iv" => Some(temperament(
            "werckmeister-iv",
            "Werckmeister IV well temperament.",
        )),
        "vallotti" => Some(temperament("vallotti", "Vallotti well temperament.")),
        "kirnberger-ii" => Some(temperament(
            "kirnberger-ii",
            "Kirnberger II well temperament.",
        )),
        "kirnberger-iii" => Some(temperament(
            "kirnberger-iii",
            "Kirnberger III well temperament.",
        )),
        "young-ii" => Some(temperament(
            "young-ii",
            "Thomas Young's second temperament.",
        )),
        "ji-adaptive-5limit" => Some(TuningCatalogEntry::Deferred(DEFERRED_ADAPTIVE)),
        _ => None,
    }
}

/// The greatest common divisor of two positive integers (Euclid's
/// algorithm), used to keep [`ji_static_5limit_ratios`]'s fractions in
/// lowest terms.
fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Computes the twelve, anchor-relative ratios of the static 5-limit
/// construction (`req:tuning:ji-static-construction`, `core_spec.tex:4015-4024`,
/// read and verified before citing): the lattice block
/// $\{3^a 5^b \mid a \in [-1,2],\ b \in [-1,1]\}$ — twelve cells, generated
/// by its bounds, nothing selected or discarded — octave-reduced into
/// `[1, 2)` and assigned in ascending order starting from the anchor, which
/// takes the role of `1/1`. Computed here in code, in exact integer
/// arithmetic (never a pasted cents or ratio table), exactly the same
/// construction the specification states in prose.
///
/// `anchor_chromatic_degree` is the `cmn-12` chromatic position (0 = C,
/// 7 = G, 2 = D, …) playing the role of `1/1`; the returned table's
/// `position` fields are `(anchor_chromatic_degree + step) mod 12` for the
/// construction's ascending step order, so indexing the result by
/// *chromatic position* (not by lattice step) gives each position's
/// ratio-to-anchor directly.
fn ji_static_5limit_ratios(anchor_chromatic_degree: i32) -> Vec<PositionRatio> {
    let mut cells: Vec<(i64, i64)> = Vec::with_capacity(12);
    for a in -1..=2i32 {
        for b in -1..=1i32 {
            let mut num: i64 = 1;
            let mut den: i64 = 1;
            if a >= 0 {
                num *= 3i64.pow(a as u32);
            } else {
                den *= 3i64.pow((-a) as u32);
            }
            if b >= 0 {
                num *= 5i64.pow(b as u32);
            } else {
                den *= 5i64.pow((-b) as u32);
            }
            // Octave-reduce into [1, 2) by exact integer doubling/halving —
            // never a float comparison, so the reduction cannot introduce
            // rounding error of its own.
            while num >= 2 * den {
                den *= 2;
            }
            while num < den {
                num *= 2;
            }
            let g = gcd(num, den);
            cells.push((num / g, den / g));
        }
    }
    // Sort ascending by value, comparing by cross-multiplication so the
    // ordering is exact (no float division).
    cells.sort_by(|(n1, d1), (n2, d2)| (n1 * d2).cmp(&(n2 * d1)));
    cells
        .into_iter()
        .enumerate()
        .map(|(step, (num, den))| PositionRatio {
            position: (anchor_chromatic_degree + step as i32).rem_euclid(12),
            ratio: JiRatio {
                numerator: num as i32,
                denominator: NonZeroU32::new(den as u32)
                    .expect("an octave-reduced denominator is a positive power of two times an odd factor, never zero"),
            },
        })
        .collect()
}

// ===========================================================================
// The ten historical temperaments (item 2 of
// `spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`): the circle-of-fifths walk and the
// ten ratified constructions it is walked over.
// ===========================================================================

/// The pure 3/2 fifth, the Pythagorean comma, the syntonic comma, and the
/// schisma, each as `1200 · log2(exact ratio)` — never a hardcoded rounded
/// cents value (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 2 and "Do not":
/// "Hardcode a rounded comma value"). `f64` throughout is correct here:
/// these feed only non-canonical frequencies
/// (`req:determinism:canonical-floating-point` binds *stored* canonical
/// floats, not values computed and discarded in memory — read and confirmed
/// before citing), and several fifths in the walk below (meantone's 1/5- and
/// 1/6-comma variants) are irrational by construction, so there is no
/// exact-rational alternative to begin with.
fn pure_fifth_cents() -> f64 {
    1200.0 * (3.0_f64 / 2.0).log2()
}
/// The Pythagorean comma, `531441/524288 ≈ 23.460` c (`core_spec.tex:3713`).
fn pythagorean_comma_cents() -> f64 {
    1200.0 * (531441.0_f64 / 524288.0).log2()
}
/// The syntonic comma, `81/80 ≈ 21.506` c (`core_spec.tex:3765`).
fn syntonic_comma_cents() -> f64 {
    1200.0 * (81.0_f64 / 80.0).log2()
}
/// The schisma, `32805/32768 ≈ 1.954` c (`core_spec.tex:3896`) — the
/// Pythagorean comma minus the syntonic comma exactly: `syntonic + schisma =
/// pythagorean`, which is why a Kirnberger construction's regular
/// syntonic-tempered fifths plus its one schisma-tempered closing fifth
/// reach exactly one Pythagorean comma of total tempering. Computed here
/// directly from `32805/32768`, independently of [`pythagorean_comma_cents`]
/// and [`syntonic_comma_cents`], so the closure tests below *prove* that
/// identity rather than assume it.
fn schisma_cents() -> f64 {
    1200.0 * (32805.0_f64 / 32768.0).log2()
}

/// How one of the twelve fifths in the circle-of-fifths chain
/// `C–G–D–A–E–B–F♯–C♯–G♯–E♭–B♭–F–(C)` is tempered by a historical
/// construction (`core_spec.tex` §"Temperament Constructions",
/// `:3696`-`4011`). This — which fifths, by what fraction of which comma —
/// *is* the construction; the derived cents tables the specification also
/// gives are checked against the walk below, never pasted in as data
/// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 2).
#[derive(Copy, Clone, Debug)]
enum FifthTempering {
    /// The untempered 3/2 ratio.
    Pure,
    /// Narrowed by `fraction` of the *Pythagorean* comma (Werckmeister
    /// III/IV, Vallotti, Young II — the well temperaments).
    NarrowPythagorean(f64),
    /// Widened by `fraction` of the Pythagorean comma (Werckmeister IV's two
    /// wide fifths, `G♯–E♭` and `E♭–B♭`).
    WidePythagorean(f64),
    /// Narrowed by `fraction` of the *syntonic* comma (meantone; Kirnberger's
    /// regular tempered fifths).
    NarrowSyntonic(f64),
    /// Narrowed by exactly one schisma — Kirnberger's closing `F♯–D♭` fifth,
    /// "the single most commonly omitted element of these ... constructions"
    /// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`'s schisma-fifth trap).
    NarrowSchisma,
    /// The closing/residual fifth: given no fraction of its own, but
    /// whatever cents value brings the twelve-fifth chain to exactly seven
    /// octaves (8400 c) — the wolf, for the four non-circulating
    /// constructions (`pythagorean`, the three `meantone-*`).
    Residual,
}

/// A temperament's construction: the twelve fifths of the circle
/// `C–G–D–A–E–B–F♯–C♯–G♯–E♭–B♭–F–(C)`
/// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 2), in that fixed arc order —
/// index 0 is `C–G`, index 1 is `G–D`, …, index 8 is `G♯–E♭` (the
/// conventional wolf/closing position), index 11 is the final `F–C`.
type Construction = [FifthTempering; 12];

/// The `cmn-12` chromatic degree reached after each of the twelve chain
/// arcs, in chain order starting from `C` itself: `C`(0), `G`(1), `D`(2),
/// `A`(3), `E`(4), `B`(5), `F♯`(6), `C♯`(7), `G♯`(8), `E♭`(9), `B♭`(10),
/// `F`(11) — i.e. `CHAIN_CHROMATIC_DEGREE[k]` is chain position `k`'s
/// `cmn-12` degree (`C=0, C♯=1, D=2, …, B=11`).
const CHAIN_CHROMATIC_DEGREE: [usize; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];

/// The result of walking a construction: the deliverable the closure tests
/// below recompute from, never a hardcoded constant
/// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`'s "closure invariant, recomputed
/// in code").
struct TemperamentWalk {
    /// The twelve ratios relative to `C` (`1/1`), indexed by `cmn-12`
    /// chromatic degree (`0..12`). The one field production code
    /// ([`temperament_ratios`]) consumes.
    ratios: [f64; 12],
    /// The twelve individual fifths' actual cents, in chain order (index 0
    /// is `C–G`, …, index 8 is `G♯–E♭`, index 11 is `F–C`) — read only by
    /// the closure/wolf tests below, hence `#[cfg(test)]`: no production
    /// code needs a single fifth's cents in isolation.
    #[cfg(test)]
    fifth_cents: [f64; 12],
    /// The *raw* (unreduced) cumulative cents after walking all twelve
    /// fifths forward from `C` — exactly `8400.0` (seven octaves) if and
    /// only if the construction closes. Also test-only, for the same reason.
    #[cfg(test)]
    raw_closure_cents: f64,
}

/// Walks the circle-of-fifths chain under `construction`
/// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 2: "one walk that places the
/// twelve notes and derives their ratios"). At most one arc may be
/// [`FifthTempering::Residual`] (the four non-circulating constructions);
/// its cents are computed as whatever value brings the other eleven
/// arcs' sum to seven octaves (`core_spec.tex:3749-3759`'s own framing:
/// "any assignment of twelve distinct pitch classes must sum to
/// [8400 c] ... by construction").
fn walk_temperament(construction: &Construction) -> TemperamentWalk {
    let pure = pure_fifth_cents();

    // Pass 1: every fixed (non-residual) arc's actual cents, plus the index
    // of the one residual (wolf) arc, if any.
    let mut fifth_cents = [0.0f64; 12];
    let mut residual_index: Option<usize> = None;
    for (i, tempering) in construction.iter().enumerate() {
        let deviation = match *tempering {
            FifthTempering::Pure => 0.0,
            FifthTempering::NarrowPythagorean(fraction) => fraction * pythagorean_comma_cents(),
            FifthTempering::WidePythagorean(fraction) => -fraction * pythagorean_comma_cents(),
            FifthTempering::NarrowSyntonic(fraction) => fraction * syntonic_comma_cents(),
            FifthTempering::NarrowSchisma => schisma_cents(),
            FifthTempering::Residual => {
                debug_assert!(
                    residual_index.is_none(),
                    "a construction may have at most one residual (wolf) fifth"
                );
                residual_index = Some(i);
                continue;
            }
        };
        fifth_cents[i] = pure - deviation;
    }
    // Pass 2: the residual (wolf) arc, if any, closes the chain to exactly
    // seven octaves.
    if let Some(i) = residual_index {
        let sum_of_others: f64 = fifth_cents
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, c)| *c)
            .sum();
        fifth_cents[i] = 8400.0 - sum_of_others;
    }

    // Walk forward from C, accumulating *raw* (unreduced) cents; reducing
    // only when reading off each chain note's pitch class below is what
    // lets the wolf (for the non-circulating four) fall out of the same
    // walk as everything else, rather than needing separate handling.
    let mut cumulative = [0.0f64; 13];
    for i in 0..12 {
        cumulative[i + 1] = cumulative[i] + fifth_cents[i];
    }

    let mut ratios = [0.0f64; 12];
    for (chain_pos, &degree) in CHAIN_CHROMATIC_DEGREE.iter().enumerate() {
        let pitch_class_cents = cumulative[chain_pos].rem_euclid(1200.0);
        ratios[degree] = 2f64.powf(pitch_class_cents / 1200.0);
    }

    TemperamentWalk {
        ratios,
        #[cfg(test)]
        fifth_cents,
        #[cfg(test)]
        raw_closure_cents: cumulative[12],
    }
}

/// `pythagorean` (`core_spec.tex:3709-3759`): eleven pure fifths; the
/// conventional `E♭–G♯` cut leaves the wolf at `G♯–E♭` (chain arc 8).
fn pythagorean_construction() -> Construction {
    use FifthTempering::{Pure, Residual};
    [
        Pure, Pure, Pure, Pure, Pure, Pure, Pure, Pure, Residual, Pure, Pure, Pure,
    ]
}

/// `meantone-1/n-comma` (`core_spec.tex:3761-3812`): all eleven non-wolf
/// fifths narrowed uniformly by `1/n` of the *syntonic* comma; same
/// `E♭–G♯` cut as `pythagorean`, so the wolf is again chain arc 8.
fn meantone_construction(comma_fraction: f64) -> Construction {
    use FifthTempering::{NarrowSyntonic, Residual};
    let n = NarrowSyntonic(comma_fraction);
    [n, n, n, n, n, n, n, n, Residual, n, n, n]
}

/// `werckmeister-iii` (`core_spec.tex:3814-3838`): `C–G, G–D, D–A, B–F♯`
/// narrowed 1/4 Pythagorean comma; the other eight pure. Circulating — no
/// residual arc.
fn werckmeister_iii_construction() -> Construction {
    use FifthTempering::{NarrowPythagorean, Pure};
    let quarter = NarrowPythagorean(0.25);
    [
        quarter, // 0: C-G
        quarter, // 1: G-D
        quarter, // 2: D-A
        Pure,    // 3: A-E
        Pure,    // 4: E-B
        quarter, // 5: B-F#
        Pure,    // 6: F#-C#
        Pure,    // 7: C#-G#
        Pure,    // 8: G#-Eb
        Pure,    // 9: Eb-Bb
        Pure,    // 10: Bb-F
        Pure,    // 11: F-C
    ]
}

/// `werckmeister-iv` (`core_spec.tex:3840-3866`): `C–G, D–A, E–B, F♯–C♯,
/// B♭–F` narrowed 1/3 Pythagorean comma; `G♯–E♭, E♭–B♭` widened 1/3
/// Pythagorean comma; the other five pure. Circulating.
fn werckmeister_iv_construction() -> Construction {
    use FifthTempering::{NarrowPythagorean, Pure, WidePythagorean};
    let narrow = NarrowPythagorean(1.0 / 3.0);
    let wide = WidePythagorean(1.0 / 3.0);
    [
        narrow, // 0: C-G
        Pure,   // 1: G-D
        narrow, // 2: D-A
        Pure,   // 3: A-E
        narrow, // 4: E-B
        Pure,   // 5: B-F#
        narrow, // 6: F#-C#
        Pure,   // 7: C#-G#
        wide,   // 8: G#-Eb
        wide,   // 9: Eb-Bb
        narrow, // 10: Bb-F
        Pure,   // 11: F-C
    ]
}

/// `vallotti` (`core_spec.tex:3868-3888`): `F–C, C–G, G–D, D–A, A–E, E–B`
/// (six consecutive) narrowed 1/6 Pythagorean comma; the other six pure.
/// Circulating.
fn vallotti_construction() -> Construction {
    use FifthTempering::{NarrowPythagorean, Pure};
    let sixth = NarrowPythagorean(1.0 / 6.0);
    [
        sixth, // 0: C-G
        sixth, // 1: G-D
        sixth, // 2: D-A
        sixth, // 3: A-E
        sixth, // 4: E-B
        Pure,  // 5: B-F#
        Pure,  // 6: F#-C#
        Pure,  // 7: C#-G#
        Pure,  // 8: G#-Eb
        Pure,  // 9: Eb-Bb
        Pure,  // 10: Bb-F
        sixth, // 11: F-C
    ]
}

/// `young-ii` (`core_spec.tex:3985-4010`): `C–G, G–D, D–A, A–E, E–B, B–F♯`
/// (six consecutive) narrowed 1/6 Pythagorean comma; the other six pure —
/// the same construction as `vallotti`, rotated to start at `C` instead of
/// `F` (compare this construction's tempered run, arcs 0-5, against
/// `vallotti`'s, arcs 11,0-4). Circulating.
fn young_ii_construction() -> Construction {
    use FifthTempering::{NarrowPythagorean, Pure};
    let sixth = NarrowPythagorean(1.0 / 6.0);
    [
        sixth, // 0: C-G
        sixth, // 1: G-D
        sixth, // 2: D-A
        sixth, // 3: A-E
        sixth, // 4: E-B
        sixth, // 5: B-F#
        Pure,  // 6: F#-C#
        Pure,  // 7: C#-G#
        Pure,  // 8: G#-Eb
        Pure,  // 9: Eb-Bb
        Pure,  // 10: Bb-F
        Pure,  // 11: F-C
    ]
}

/// `kirnberger-ii` (`core_spec.tex:3890-3937`): `D–A, A–E` narrowed 1/2
/// syntonic comma; the closing fifth (named `F♯–D♭` in the specification,
/// since Kirnberger's own chain is built outward from `D♭` — the *same
/// physical arc* as chain position 6, `F♯–C♯`, `D♭` and `C♯` being one
/// enharmonic pitch class) narrowed one schisma — the trap
/// `spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` calls out by name; the other nine
/// pure. Circulating.
fn kirnberger_ii_construction() -> Construction {
    use FifthTempering::{NarrowSchisma, NarrowSyntonic, Pure};
    let half = NarrowSyntonic(0.5);
    [
        Pure,          // 0: C-G
        Pure,          // 1: G-D
        half,          // 2: D-A
        half,          // 3: A-E
        Pure,          // 4: E-B
        Pure,          // 5: B-F#
        NarrowSchisma, // 6: F#-C# (the closing F#-Db fifth)
        Pure,          // 7: C#-G#
        Pure,          // 8: G#-Eb
        Pure,          // 9: Eb-Bb
        Pure,          // 10: Bb-F
        Pure,          // 11: F-C
    ]
}

/// `kirnberger-iii` (`core_spec.tex:3939-3983`): `C–G, G–D, D–A, A–E` (four
/// consecutive) narrowed 1/4 syntonic comma; the same closing `F♯–D♭`
/// (chain position 6) narrowed one schisma as `kirnberger-ii`; the other
/// seven pure. Circulating.
fn kirnberger_iii_construction() -> Construction {
    use FifthTempering::{NarrowSchisma, NarrowSyntonic, Pure};
    let quarter = NarrowSyntonic(0.25);
    [
        quarter,       // 0: C-G
        quarter,       // 1: G-D
        quarter,       // 2: D-A
        quarter,       // 3: A-E
        Pure,          // 4: E-B
        Pure,          // 5: B-F#
        NarrowSchisma, // 6: F#-C# (the closing F#-Db fifth)
        Pure,          // 7: C#-G#
        Pure,          // 8: G#-Eb
        Pure,          // 9: Eb-Bb
        Pure,          // 10: Bb-F
        Pure,          // 11: F-C
    ]
}

/// The twelve ratios (indexed by `cmn-12` chromatic degree) of the
/// reserved-built-in temperament named by `function`, or `None` for any
/// other id — the extension point's fail-closed path
/// (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 1: "An unknown
/// `TuningFunctionId` returns `None` ... `Function` is an extension point and
/// no registry exists"). Never memoized: each of the ten constructions is
/// cheap arithmetic on twelve `f64`s, so there is no cache to keep coherent.
fn temperament_ratios(function: &TuningFunctionId) -> Option<[f64; 12]> {
    let construction = match function.as_str() {
        "pythagorean" => pythagorean_construction(),
        "meantone-1/4-comma" => meantone_construction(0.25),
        "meantone-1/5-comma" => meantone_construction(0.2),
        "meantone-1/6-comma" => meantone_construction(1.0 / 6.0),
        "werckmeister-iii" => werckmeister_iii_construction(),
        "werckmeister-iv" => werckmeister_iv_construction(),
        "vallotti" => vallotti_construction(),
        "kirnberger-ii" => kirnberger_ii_construction(),
        "kirnberger-iii" => kirnberger_iii_construction(),
        "young-ii" => young_ii_construction(),
        _ => return None,
    };
    Some(walk_temperament(&construction).ratios)
}

// ===========================================================================
// The frequency resolver (item 3): (position, TuningSystem, ReferencePitch)
// -> Hz.
// ===========================================================================

/// Why a tuning could not be resolved to a frequency. Every variant is a
/// closed failure, never a fallback frequency
/// (`req:tuning:tuning-resolution-determinism`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TuningResolutionError {
    /// `voice` is not the id of any voice reachable from the score's
    /// `canvas.regions` — a caller error (an orphaned or foreign
    /// [`VoiceId`]), not a tuning failure.
    VoiceNotFound(VoiceId),
    /// The resolved tuning-system identifier is not in the built-in catalog
    /// at all (no registry for score-defined tuning systems exists, Ruling C
    /// of `spec/PLAN_PUSH4B_TUNING.md`).
    UnknownTuningSystem(TuningSystemId),
    /// The resolved tuning-system identifier is a real catalog entry whose
    /// resolution this tranche defers (see [`built_in_tuning_system`]); the
    /// reason names which tranche completes it.
    NotYetSupported {
        id: TuningSystemId,
        reason: &'static str,
    },
    /// The resolved pitch space does not resolve structurally at all
    /// ([`built_in_position_structure`] returned `None`) — an unknown
    /// identifier or one of the six built-in spaces the specification names
    /// but does not structurally determine (`crate::pitch_space`).
    UnresolvedPitchSpace(PitchSpaceId),
    /// `req:tuning:tuning-system-compatibility`: the resolved tuning
    /// system's declared `pitch_space` differs from the resolved pitch
    /// space, and no compatibility-mapping registry exists this tranche —
    /// a deliberate deferral (see the module doc), not a guess.
    IncompatiblePitchSpace {
        pitch_space: PitchSpaceId,
        tuning_system_pitch_space: PitchSpaceId,
    },
    /// The pitch's (or the reference's) position could not be placed on the
    /// tuning system's coordinate frame: a structural mismatch between the
    /// resolved pitch space's [`PositionStructure`] and the
    /// [`PitchSpacePosition`] variant in play, or between its chromatic
    /// cardinality and the tuning system's own divisions/table length.
    PositionUnavailable,
}

impl core::fmt::Display for TuningResolutionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::VoiceNotFound(v) => write!(f, "voice {v:?} is not reachable from the score graph"),
            Self::UnknownTuningSystem(id) => write!(f, "'{id}' is not a built-in tuning system"),
            Self::NotYetSupported { id, reason } => {
                write!(f, "'{id}' is not yet supported: {reason}")
            }
            Self::UnresolvedPitchSpace(id) => {
                write!(f, "'{id}' does not resolve to a structurally determined pitch space")
            }
            Self::IncompatiblePitchSpace {
                pitch_space,
                tuning_system_pitch_space,
            } => write!(
                f,
                "resolved pitch space '{pitch_space}' is incompatible with the tuning system's declared \
                 pitch space '{tuning_system_pitch_space}' (no compatibility mapping registry exists)"
            ),
            Self::PositionUnavailable => {
                f.write_str("the pitch space position could not be placed on the tuning system's coordinate frame")
            }
        }
    }
}

impl std::error::Error for TuningResolutionError {}

/// The absolute tuning-coordinate of `position`, in the coordinate frame a
/// tuning system with `divisions` positions-per-octave uses — the same
/// absolute chromatic-coordinate idea [`Pitch::twelve_tet_semitone`]
/// (`crate::pitch`) uses for `cmn-12`, generalized from a fixed 12 to any
/// `N` and to [`PitchSpacePosition::Integer`] (the EDO pitch spaces'
/// position kind) as well as [`PitchSpacePosition::Cmn`].
fn absolute_coordinate(
    position: &PitchSpacePosition,
    structure: &PositionStructure,
    divisions: u32,
) -> Option<i64> {
    match (position, structure) {
        (
            PitchSpacePosition::Cmn {
                nominal,
                alteration,
                octave,
            },
            PositionStructure::DiatonicOverChromatic {
                chromatic_positions_per_octave,
                nominal_to_chromatic,
                ..
            },
        ) if u32::from(*chromatic_positions_per_octave) == divisions => {
            let degree = i64::from(nominal_to_chromatic[*nominal as usize]);
            Some(i64::from(*octave) * i64::from(divisions) + degree + i64::from(*alteration))
        }
        (
            PitchSpacePosition::Integer { space_size, index },
            PositionStructure::Chromatic {
                positions_per_octave,
            },
        ) if u32::from(*space_size) == divisions
            && u32::from(*positions_per_octave) == divisions =>
        {
            Some(i64::from(*index))
        }
        _ => None,
    }
}

/// The chromatic cardinality of `structure` — the "positions per octave"
/// figure [`TuningResolution::Function`] borrows from the pitch space
/// rather than carrying itself (the variant has no division-count field,
/// `core_spec.tex:3318-3324`). Both `Chromatic` and `DiatonicOverChromatic`
/// name one; `JiLattice` and `Registered` do not (a lattice or a grammar
/// plugin has no single "positions per octave" scalar), so `None` there is
/// a structural refusal, not an oversight.
fn chromatic_cardinality(structure: &PositionStructure) -> Option<u32> {
    match structure {
        PositionStructure::Chromatic {
            positions_per_octave,
        } => Some(u32::from(*positions_per_octave)),
        PositionStructure::DiatonicOverChromatic {
            chromatic_positions_per_octave,
            ..
        } => Some(u32::from(*chromatic_positions_per_octave)),
        PositionStructure::JiLattice { .. } | PositionStructure::Registered(_) => None,
    }
}

/// The full-register frequency ratio of absolute coordinate `s`, relative to
/// coordinate `0` under `resolution`.
fn coordinate_ratio(resolution: &TuningResolution, s: i64) -> Option<f64> {
    match resolution {
        TuningResolution::EqualTemperament {
            divisions_per_octave,
        } => {
            if *divisions_per_octave == 0 {
                return None;
            }
            Some(2f64.powf(s as f64 / f64::from(*divisions_per_octave)))
        }
        TuningResolution::PerPositionRatios(table) => {
            let n = i64::try_from(table.len()).ok().filter(|n| *n > 0)?;
            let degree = i32::try_from(s.rem_euclid(n)).ok()?;
            let octave = i32::try_from(s.div_euclid(n)).ok()?;
            let entry = table.iter().find(|pr| pr.position == degree)?;
            let base = f64::from(entry.ratio.numerator) / f64::from(entry.ratio.denominator.get());
            Some(base * 2f64.powi(octave))
        }
        TuningResolution::Function { function, .. } => {
            // `degree = s.rem_euclid(12)`, `octave = s.div_euclid(12)`,
            // `ratio = temperament_ratios[degree] · 2^octave`
            // (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 1). An unknown
            // `TuningFunctionId` has no registry to consult, so
            // `temperament_ratios` returns `None` and this fails closed —
            // never a fallback frequency.
            let ratios = temperament_ratios(function)?;
            let degree = i32::try_from(s.rem_euclid(12)).ok()?;
            let octave = i32::try_from(s.div_euclid(12)).ok()?;
            Some(ratios[degree as usize] * 2f64.powi(octave))
        }
    }
}

/// Computes the frequency in Hz at which `position` sounds under `system`,
/// anchored by `reference` (Chapter 4 §"Tuning Systems" / §"Reference
/// Pitch"; `req:tuning:tuning-resolution-determinism`).
///
/// **Anchoring** — the one subtlety `spec/CONTRACT_PUSH4B_RESOLVER.md`
/// singles out: a [`TuningResolution`]'s ratios are relative to the
/// tuning's own 1/1 (its anchor), but `reference` fixes a *different*
/// position's absolute frequency. Both `position` and `reference.position`
/// are placed on the same absolute coordinate frame
/// (`absolute_coordinate`), and the frequency is
/// `reference.frequency_hz() * ratio(position) / ratio(reference.position)`.
/// The arbitrary choice of which position the construction calls "1/1"
/// cancels out of that quotient, so this is correct regardless of anchor —
/// for [`TuningResolution::EqualTemperament`] it reduces exactly to
/// `ref_freq · 2^((position − ref_position)/N)`.
pub fn frequency_for_position(
    position: &PitchSpacePosition,
    system: &TuningSystem,
    reference: &ReferencePitch,
) -> Result<f64, TuningResolutionError> {
    let structure = built_in_position_structure(&system.pitch_space)
        .ok_or_else(|| TuningResolutionError::UnresolvedPitchSpace(system.pitch_space.clone()))?;
    let divisions =
        match &system.resolution {
            TuningResolution::EqualTemperament {
                divisions_per_octave,
            } => u32::from(*divisions_per_octave),
            TuningResolution::PerPositionRatios(table) => u32::try_from(table.len())
                .map_err(|_| TuningResolutionError::PositionUnavailable)?,
            // `Function` carries no division count of its own (unlike the other
            // two variants); divisions comes from the *pitch space*'s chromatic
            // cardinality instead — 12 for `cmn-12`
            // (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 1).
            TuningResolution::Function { .. } => chromatic_cardinality(&structure)
                .ok_or(TuningResolutionError::PositionUnavailable)?,
        };
    let s = absolute_coordinate(position, &structure, divisions)
        .ok_or(TuningResolutionError::PositionUnavailable)?;
    let s_ref = absolute_coordinate(&reference.position, &structure, divisions)
        .ok_or(TuningResolutionError::PositionUnavailable)?;
    let ratio_p = coordinate_ratio(&system.resolution, s)
        .ok_or(TuningResolutionError::PositionUnavailable)?;
    let ratio_ref = coordinate_ratio(&system.resolution, s_ref)
        .ok_or(TuningResolutionError::PositionUnavailable)?;
    Ok(reference.frequency_hz() * ratio_p / ratio_ref)
}

// ===========================================================================
// The five-scope resolution walk (item 4).
// ===========================================================================

/// The independently-resolved pitch space, tuning system, and reference
/// pitch that govern a pitch at a given location (`req:tuning:tuning-resolution-order`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedTuning {
    pub pitch_space: PitchSpaceId,
    pub tuning_system: TuningSystemId,
    pub reference: ReferencePitch,
}

/// The first value `field` supplies from an override whose scope matches
/// `voice`, else `staff`, else `region`, in that priority order (scopes 2-4
/// of `req:tuning:tuning-resolution-order`) — the same lookup used
/// identically for all three components the walk resolves. Within a tier,
/// overrides are consulted in `context.overrides`' own declared order ("in
/// the order they apply", `core_spec.tex:3523`), skipping past a
/// scope-matching override that leaves this particular component `None`
/// rather than stopping at it, so a later override at the same scope can
/// still supply the value this one didn't.
fn override_value<T: Clone>(
    context: &ScoreTuningContext,
    voice: VoiceId,
    staff: StaffId,
    region: RegionId,
    field: impl Fn(&TuningOverride) -> &Option<T>,
) -> Option<T> {
    let tier = |matches_scope: &dyn Fn(&TuningScope) -> bool| -> Option<T> {
        context
            .overrides
            .iter()
            .filter(|o| matches_scope(&o.scope))
            .find_map(|o| field(o).clone())
    };
    tier(&|s| matches!(s, TuningScope::Voice(v) if *v == voice))
        .or_else(|| tier(&|s| matches!(s, TuningScope::Staff(st) if *st == staff)))
        .or_else(|| {
            // Step 4: "each region enclosing the pitch, innermost to
            // outermost". In this data model a `Voice` is owned by exactly
            // one `StaffInstance`, owned by exactly one `Region`
            // (containment, not a derived time-range query) — there is no
            // nested-region concept, so "innermost to outermost" is exactly
            // this one region.
            //
            // `TuningScope::Range` is part of the type (Chapter 4 defines
            // it) but `req:tuning:tuning-resolution-order` enumerates
            // exactly five steps and does not include it; this tranche's
            // walk never matches it (see the module doc's scope note).
            tier(&|s| matches!(s, TuningScope::Region(r) if *r == region))
        })
}

/// The five-scope walk of `req:tuning:tuning-resolution-order`
/// (`core_spec.tex:3549`, read and verified before citing): resolves each of
/// `pitch_space`, `tuning_system`, and `reference` independently, walking
/// from the pitch's own [`crate::pitch::AcousticPitch`] outward through
/// voice, staff, and region overrides to the score default.
///
/// Step 1 supplies a value only for `tuning_system` (an explicit
/// [`TuningReference::Explicit`] short-circuits) — [`AcousticPitch`] carries
/// no pitch-space or reference field of its own, so those two components
/// always proceed to step 2. (A pitch's [`AcousticRealization::AbsoluteHz`]
/// short-circuits the *whole* frequency, bypassing this walk entirely; see
/// [`resolve_pitch_frequency`].)
///
/// [`AcousticPitch`]: crate::pitch::AcousticPitch
pub fn resolve_tuning_scope(
    pitch: &Pitch,
    voice: VoiceId,
    staff: StaffId,
    region: RegionId,
    context: &ScoreTuningContext,
) -> ResolvedTuning {
    let tuning_system = match &pitch.acoustic.tuning {
        TuningReference::Explicit(id) => id.clone(),
        TuningReference::Inherit => {
            override_value(context, voice, staff, region, |o| &o.tuning_system)
                .unwrap_or_else(|| context.default_tuning_system.clone())
        }
    };
    let pitch_space = override_value(context, voice, staff, region, |o| &o.pitch_space)
        .unwrap_or_else(|| context.default_pitch_space.clone());
    let reference = override_value(context, voice, staff, region, |o| &o.reference)
        .unwrap_or_else(|| context.reference.clone());
    ResolvedTuning {
        pitch_space,
        tuning_system,
        reference,
    }
}

// ===========================================================================
// The top-level pipeline: walk, catalog lookup, compatibility check (item
// 5), frequency.
// ===========================================================================

/// Locates the region and staff that structurally own `voice`: a `Voice`
/// belongs to exactly one `StaffInstance`, which belongs to exactly one
/// `Region` (Chapter 5's containment tree — ownership, not a derived
/// time-range query). `None` if no region in `score.canvas.regions` owns a
/// voice with this id.
fn locate_voice(score: &Score, voice: VoiceId) -> Option<(RegionId, StaffId)> {
    for region in &score.canvas.regions {
        for instance in region.staff_instances() {
            if instance.voices.iter().any(|v| v.id == voice) {
                return Some((region.id, instance.staff));
            }
        }
    }
    None
}

/// The full resolver: walks the five scopes, checks compatibility, and
/// computes the frequency in Hz at which `pitch` sounds, given its location
/// (`voice`) in `score`.
///
/// Step 1's other short-circuit — [`AcousticRealization::AbsoluteHz`] —
/// bypasses everything else: the frequency is already fixed, so neither the
/// scope walk nor the built-in catalog is consulted at all.
/// [`AcousticRealization::CentsOffset`] is applied multiplicatively on top
/// of the resolved base frequency, per its own documented semantics ("an
/// explicit offset in cents from the tuning system's result").
pub fn resolve_pitch_frequency(
    score: &Score,
    pitch: &Pitch,
    voice: VoiceId,
) -> Result<f64, TuningResolutionError> {
    if let AcousticRealization::AbsoluteHz(hz) = pitch.acoustic.realization {
        return Ok(hz.get());
    }
    let (region, staff) =
        locate_voice(score, voice).ok_or(TuningResolutionError::VoiceNotFound(voice))?;
    let resolved = resolve_tuning_scope(pitch, voice, staff, region, &score.tuning_context);
    let entry = built_in_tuning_system(&resolved.tuning_system).ok_or_else(|| {
        TuningResolutionError::UnknownTuningSystem(resolved.tuning_system.clone())
    })?;
    let system = match entry {
        TuningCatalogEntry::Resolved(system) => system,
        TuningCatalogEntry::Deferred(reason) => {
            return Err(TuningResolutionError::NotYetSupported {
                id: resolved.tuning_system,
                reason,
            });
        }
    };
    // req:tuning:tuning-system-compatibility (`:3581`): accept only exact
    // equality this tranche — see the module doc's compatibility note.
    if system.pitch_space != resolved.pitch_space {
        return Err(TuningResolutionError::IncompatiblePitchSpace {
            pitch_space: resolved.pitch_space,
            tuning_system_pitch_space: system.pitch_space,
        });
    }
    let base =
        frequency_for_position(&pitch.scale_position.position, &system, &resolved.reference)?;
    match pitch.acoustic.realization {
        AcousticRealization::Implicit => Ok(base),
        AcousticRealization::CentsOffset(c) => Ok(base * 2f64.powf(c.get() / 1200.0)),
        AcousticRealization::AbsoluteHz(_) => unreachable!("handled by the early return above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        Canvas, MetricTimeModel, Region, RegionContent, RegionTimeModel, StaffBasedContent,
        StaffExtent, StaffInstance, TimeExtent, Voice,
    };
    use crate::ids::{IdentityContext, ReplicaId, StaffInstanceId};
    use crate::pitch::{AcousticPitch, CmnNominal, ScalePosition};
    use crate::time::WallClockTime;
    use epiphany_determinism::{Tolerance, ToleranceClass, ToleranceGovernance};

    fn cents(c: f64) -> Tolerance {
        Tolerance::absolute(
            ToleranceClass::AcousticCents,
            c,
            ToleranceGovernance::Validation,
        )
        .unwrap()
    }

    /// Absolute cents between two frequencies — the metric the proof-of-life
    /// tests assert against, per the contract's own instruction ("a
    /// cents-level check is right").
    fn cents_between(a: f64, b: f64) -> f64 {
        1200.0 * (a / b).log2().abs()
    }

    fn cmn_pitch(space: &str, nominal: CmnNominal, alteration: i8, octave: i8) -> Pitch {
        Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new(space),
                position: PitchSpacePosition::Cmn {
                    nominal,
                    alteration,
                    octave,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        }
    }

    fn wc_extent(a: i64, b: i64) -> TimeExtent {
        TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(a),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(b),
            },
        }
    }

    /// A minimal score: one region, one staff instance, two voices — enough
    /// for `locate_voice` and the scope walk, nothing more.
    struct Fixture {
        score: Score,
        voice_a: VoiceId,
        voice_b: VoiceId,
    }

    fn fixture() -> Fixture {
        let r = ReplicaId(1);
        let voice_a = VoiceId::new(r, 1);
        let voice_b = VoiceId::new(r, 2);
        let staff = StaffId::new(r, 1);
        let mut instance = StaffInstance::new(StaffInstanceId::new(r, 1), staff);
        instance.voices = vec![Voice::user(voice_a), Voice::user(voice_b)];
        let region = Region {
            id: RegionId::new(r, 1),
            time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
            content: RegionContent::StaffBased(StaffBasedContent {
                staff_instances: vec![instance],
                ..Default::default()
            }),
            time_extent: wc_extent(0, 1000),
            staff_extent: StaffExtent {
                staves: vec![staff],
            },
            local_tempo_map: None,
            permits_spanning_slurs: false,
        };
        let mut score = Score::empty(IdentityContext::new(r));
        score.canvas = Canvas {
            regions: vec![region],
            ..Default::default()
        };
        Fixture {
            score,
            voice_a,
            voice_b,
        }
    }

    // -- Proof of life 1: tet-12, A4 = 440 Hz -> C5. --------------------------

    #[test]
    fn tet12_a4_440_resolves_c5_to_523_2511_hz() {
        let f = fixture();
        let c5 = cmn_pitch("cmn-12", CmnNominal::C, 0, 5);
        let freq = resolve_pitch_frequency(&f.score, &c5, f.voice_a).expect("tet-12 resolves");
        assert!(
            cents(0.01).within(cents_between(freq, 523.2511), 0.0),
            "expected ~523.2511 Hz, got {freq}"
        );
    }

    // -- Proof of life 2: ji-static-5limit-C's major third differs from --
    // -- tet-12's by the syntonic comma (~13.7 c).                      --

    #[test]
    fn ji_static_5limit_c_major_third_distinct_from_tet12() {
        // Anchor the reference *at the tonic itself* (C4), so both systems'
        // "1/1" and the comparison point coincide: the ratio this measures
        // is then exactly each system's C-to-E interval, with no confound
        // from how the two systems otherwise retune A differently (both
        // being referenced against the *same* absolute A4 would silently
        // mix "how A retunes" into "how E retunes" — a trap this test's
        // first draft fell into).
        let f = fixture();
        let c4_ref = ReferencePitch::new(
            PitchSpacePosition::Cmn {
                nominal: CmnNominal::C,
                alteration: 0,
                octave: 4,
            },
            264.0,
        )
        .unwrap();
        let mut ji_score = f.score.clone();
        ji_score.tuning_context.default_tuning_system = TuningSystemId::new("ji-static-5limit-C");
        ji_score.tuning_context.reference = c4_ref.clone();
        let mut tet_score = f.score.clone();
        tet_score.tuning_context.reference = c4_ref;

        let e4 = cmn_pitch("cmn-12", CmnNominal::E, 0, 4);
        let ji_freq = resolve_pitch_frequency(&ji_score, &e4, f.voice_a)
            .expect("ji-static-5limit-C resolves");
        let tet_freq =
            resolve_pitch_frequency(&tet_score, &e4, f.voice_a).expect("tet-12 resolves");
        // Just major third 5/4 (386.31 c) vs equal-tempered (400 c): the just
        // third is *flatter*, by the syntonic comma (~13.7 c).
        assert!(
            ji_freq < tet_freq,
            "the just major third ({ji_freq} Hz) must be flatter than the equal-tempered one ({tet_freq} Hz)"
        );
        let diff = cents_between(ji_freq, tet_freq);
        assert!(
            cents(0.05).within(diff, 13.6864),
            "expected a ~13.6864 c syntonic-comma difference, got {diff}"
        );
    }

    #[test]
    fn ji_static_5limit_lattice_matches_the_published_construction() {
        // `core_spec.tex:4034-4046`'s table, spot-checked at all three
        // anchors: this proves the *code's* lattice-block construction
        // reproduces the specification's own worked values, rather than the
        // production code and the spec merely agreeing to look similar.
        let ratio_at = |system: &str, degree: i32| -> (i32, u32) {
            let TuningCatalogEntry::Resolved(sys) =
                built_in_tuning_system(&TuningSystemId::new(system)).unwrap()
            else {
                panic!("{system} must resolve")
            };
            let TuningResolution::PerPositionRatios(table) = sys.resolution else {
                panic!("{system} must be PerPositionRatios")
            };
            let entry = table.iter().find(|pr| pr.position == degree).unwrap();
            (entry.ratio.numerator, entry.ratio.denominator.get())
        };
        // ji-static-5limit-C: anchor at C (chromatic 0). C=1/1, E=5/4 (step 4), G=3/2 (step 7).
        assert_eq!(ratio_at("ji-static-5limit-C", 0), (1, 1));
        assert_eq!(ratio_at("ji-static-5limit-C", 4), (5, 4));
        assert_eq!(ratio_at("ji-static-5limit-C", 7), (3, 2));
        // ji-static-5limit-G: anchor at G (chromatic 7), so G itself is 1/1.
        assert_eq!(ratio_at("ji-static-5limit-G", 7), (1, 1));
        // ji-static-5limit-D: anchor at D (chromatic 2), so D itself is 1/1.
        assert_eq!(ratio_at("ji-static-5limit-D", 2), (1, 1));
    }

    // -- Proof of life 3: scope precedence. -----------------------------------

    #[test]
    fn voice_scope_override_changes_resolution_for_its_voice_but_not_others() {
        let mut f = fixture();
        f.score.tuning_context.overrides.push(TuningOverride {
            scope: TuningScope::Voice(f.voice_a),
            pitch_space: None,
            tuning_system: None,
            reference: Some(
                ReferencePitch::new(
                    PitchSpacePosition::Cmn {
                        nominal: CmnNominal::A,
                        alteration: 0,
                        octave: 4,
                    },
                    415.0,
                )
                .unwrap(),
            ),
        });
        let a4 = cmn_pitch("cmn-12", CmnNominal::A, 0, 4);
        let in_voice_a =
            resolve_pitch_frequency(&f.score, &a4, f.voice_a).expect("resolves under the override");
        let in_voice_b =
            resolve_pitch_frequency(&f.score, &a4, f.voice_b).expect("resolves under the default");
        assert!(
            cents(0.01).within(cents_between(in_voice_a, 415.0), 0.0),
            "voice A's own reference override must apply: got {in_voice_a}"
        );
        assert!(
            cents(0.01).within(cents_between(in_voice_b, 440.0), 0.0),
            "voice B must still see the score default (440 Hz): got {in_voice_b}"
        );
    }

    // -- Proof of life 4: compatibility rejects a mismatch. -------------------

    #[test]
    fn compatibility_check_rejects_a_pitch_space_mismatch() {
        let mut f = fixture();
        // tet-19's declared pitch_space is edo-19; the score's default pitch
        // space stays cmn-12 (unchanged) — a genuine, catchable mismatch.
        f.score.tuning_context.default_tuning_system = TuningSystemId::new("tet-19");
        let c5 = cmn_pitch("cmn-12", CmnNominal::C, 0, 5);
        let err = resolve_pitch_frequency(&f.score, &c5, f.voice_a)
            .expect_err("must reject the mismatch");
        assert!(
            matches!(err, TuningResolutionError::IncompatiblePitchSpace { .. }),
            "expected IncompatiblePitchSpace, got {err:?}"
        );
    }

    // -- Proof of life 5: a deferred system fails closed. ---------------------

    #[test]
    fn deferred_ji_adaptive_fails_closed() {
        // As of Push 4b tranche 2b, `pythagorean` (and the other nine
        // historical temperaments) resolve — see the temperament tests
        // below. `ji-adaptive-5limit` is the one remaining catalog entry
        // whose resolution is still deferred (it needs `HarmonicContext`,
        // which does not exist in Rust).
        let f = fixture();
        let c5 = cmn_pitch("cmn-12", CmnNominal::C, 0, 5);
        let mut score = f.score.clone();
        score.tuning_context.default_tuning_system = TuningSystemId::new("ji-adaptive-5limit");
        let err = resolve_pitch_frequency(&score, &c5, f.voice_a)
            .expect_err("ji-adaptive-5limit must not resolve to a frequency");
        assert!(
            matches!(err, TuningResolutionError::NotYetSupported { .. }),
            "ji-adaptive-5limit must report NotYetSupported (a known-but-deferred identifier), got {err:?}"
        );
        // A genuinely unknown identifier reports differently, so the two
        // failure modes never blur together.
        let mut score = f.score.clone();
        score.tuning_context.default_tuning_system = TuningSystemId::new("not-a-built-in-system");
        let err = resolve_pitch_frequency(&score, &c5, f.voice_a)
            .expect_err("unknown id must not resolve");
        assert!(matches!(err, TuningResolutionError::UnknownTuningSystem(_)));
    }

    // -- Extra: the whole-walk AbsoluteHz short-circuit. ----------------------

    #[test]
    fn absolute_hz_realization_short_circuits_the_whole_walk() {
        let mut f = fixture();
        // An unresolvable tuning system: if the shortcut were skipped, this
        // would return an error, not 500.0.
        f.score.tuning_context.default_tuning_system = TuningSystemId::new("not-a-built-in-system");
        let mut pinned = cmn_pitch("cmn-12", CmnNominal::C, 0, 5);
        pinned.acoustic.realization = AcousticRealization::absolute_hz(500.0).unwrap();
        let freq = resolve_pitch_frequency(&f.score, &pinned, f.voice_a)
            .expect("AbsoluteHz must resolve without consulting the tuning system at all");
        assert_eq!(freq, 500.0);
    }

    // -- Extra: an EDO built-in resolves through Integer positions. -----------

    #[test]
    fn tet_19_resolves_edo_integer_positions() {
        let mut f = fixture();
        f.score.tuning_context.default_pitch_space = PitchSpaceId::new("edo-19");
        f.score.tuning_context.default_tuning_system = TuningSystemId::new("tet-19");
        f.score.tuning_context.reference = ReferencePitch::new(
            PitchSpacePosition::Integer {
                space_size: 19,
                index: 0,
            },
            440.0,
        )
        .unwrap();
        let one_step = Pitch {
            scale_position: ScalePosition {
                space: PitchSpaceId::new("edo-19"),
                position: PitchSpacePosition::Integer {
                    space_size: 19,
                    index: 1,
                },
            },
            acoustic: AcousticPitch {
                tuning: TuningReference::Inherit,
                realization: AcousticRealization::Implicit,
            },
        };
        let freq =
            resolve_pitch_frequency(&f.score, &one_step, f.voice_a).expect("tet-19 resolves");
        let expected = 440.0 * 2f64.powf(1.0 / 19.0);
        assert!(
            cents(0.01).within(cents_between(freq, expected), 0.0),
            "expected ~{expected} Hz, got {freq}"
        );
    }

    // =========================================================================
    // Push 4b tranche 2b: the ten historical temperaments. This is the
    // tranche's reason to exist, per `spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`'s
    // "Proof of life" section — every assertion below recomputes its expected
    // value from the walk (or from an exact comma ratio), never from a
    // hardcoded cents constant copied out of the spec's tables.
    // =========================================================================

    /// `1200 · log2(ratio)` for chromatic `degree` in `walk` — the cents this
    /// module's own walk assigns that degree, relative to C.
    fn cents_of(walk: &TemperamentWalk, degree: usize) -> f64 {
        1200.0 * walk.ratios[degree].log2()
    }

    // -- Closure 1: the six circulating temperaments sum to one Pythagorean --
    // -- comma, and none of their twelve fifths is a wolf. -------------------

    #[test]
    fn circulating_temperaments_close_to_one_pythagorean_comma_with_no_wolf() {
        let comma = pythagorean_comma_cents();
        let cases: [(&str, Construction); 6] = [
            ("werckmeister-iii", werckmeister_iii_construction()),
            ("werckmeister-iv", werckmeister_iv_construction()),
            ("vallotti", vallotti_construction()),
            ("kirnberger-ii", kirnberger_ii_construction()),
            ("kirnberger-iii", kirnberger_iii_construction()),
            ("young-ii", young_ii_construction()),
        ];
        for (name, construction) in cases {
            let walk = walk_temperament(&construction);
            // The sum of the twelve fifths' deviations from pure equals one
            // Pythagorean comma (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md`'s
            // closure invariant for the circulating six), recomputed from
            // the walk's raw (unreduced) closing cents rather than asserted
            // directly against a constant.
            let total_deviation = 12.0 * pure_fifth_cents() - walk.raw_closure_cents;
            assert!(
                (total_deviation - comma).abs() < 1e-9,
                "{name}: twelve fifths should deviate from pure by exactly one \
                 Pythagorean comma ({comma} c), computed {total_deviation} c"
            );
            // No fifth is a wolf: every one of the twelve stays within 15 c
            // of a pure fifth. 15 c comfortably exceeds this construction's
            // largest single tempering (kirnberger-ii's half-syntonic-comma
            // fifths, ~10.75 c) while comfortably staying below any real
            // historical wolf (pythagorean's is ~23.5 c narrow; meantone's
            // are 16-36 c wide).
            for (i, &fifth) in walk.fifth_cents.iter().enumerate() {
                let deviation = (fifth - pure_fifth_cents()).abs();
                assert!(
                    deviation < 15.0,
                    "{name}: chain arc {i} deviates from pure by {deviation} c -- too large, looks like a wolf"
                );
            }
        }
    }

    // -- Closure 2: the four non-circulating temperaments' residual wolf ------
    // -- matches the spec's ratified value.                                --

    #[test]
    fn noncirculating_temperaments_wolf_matches_the_ratified_spec_value() {
        // Chain arc 8 (`G♯–E♭`) is the one `Residual` arc in each of these
        // four constructions -- see each `*_construction` function's own
        // comment.
        const WOLF_ARC: usize = 8;
        let cases: [(&str, Construction, f64); 4] = [
            ("pythagorean", pythagorean_construction(), 678.495),
            ("meantone-1/4-comma", meantone_construction(0.25), 737.637),
            ("meantone-1/5-comma", meantone_construction(0.2), 725.809),
            (
                "meantone-1/6-comma",
                meantone_construction(1.0 / 6.0),
                717.923,
            ),
        ];
        for (name, construction, expected_wolf) in cases {
            let walk = walk_temperament(&construction);
            let wolf = walk.fifth_cents[WOLF_ARC];
            assert!(
                (wolf - expected_wolf).abs() < 0.001,
                "{name}: expected the wolf at {expected_wolf} c (`core_spec.tex`'s ratified value), computed {wolf} c"
            );
        }
    }

    // -- Discriminators: spot values the walk must reproduce against the -----
    // -- spec's own independently-derived cents tables.                   ----

    #[test]
    fn pythagorean_e_and_fsharp_match_the_published_cents() {
        let walk = walk_temperament(&pythagorean_construction());
        let e = cents_of(&walk, CmnNominal::E.chromatic() as usize);
        let fsharp = cents_of(&walk, 6); // F# = chromatic degree 6, no plain CmnNominal for it
        assert!(
            (e - 407.820).abs() < 0.001,
            "pythagorean E: expected 407.820 c, got {e}"
        );
        assert!(
            (fsharp - 611.730).abs() < 0.001,
            "pythagorean F#: expected 611.730 c, got {fsharp}"
        );
    }

    #[test]
    fn meantone_quarter_comma_major_third_c_to_e_is_the_just_5_4() {
        let walk = walk_temperament(&meantone_construction(0.25));
        let c = cents_of(&walk, CmnNominal::C.chromatic() as usize);
        let e = cents_of(&walk, CmnNominal::E.chromatic() as usize);
        let third = e - c;
        assert!(
            (third - 386.31).abs() < 0.01,
            "expected the just 5/4 major third (386.31 c), got {third} c"
        );
    }

    #[test]
    fn kirnberger_ii_and_iii_d_differ_though_the_chain_skeleton_is_the_same() {
        // Same chain skeleton (D-A/A-E tempered, F#-Db schisma-tempered),
        // different comma fraction -- a test that passes both proves the two
        // are not secretly the same construction.
        let ii = walk_temperament(&kirnberger_ii_construction());
        let iii = walk_temperament(&kirnberger_iii_construction());
        let d = CmnNominal::D.chromatic() as usize;
        let d_ii = cents_of(&ii, d);
        let d_iii = cents_of(&iii, d);
        assert!(
            (d_ii - 203.910).abs() < 0.001,
            "kirnberger-ii D: expected 203.910 c, got {d_ii}"
        );
        assert!(
            (d_iii - 193.157).abs() < 0.001,
            "kirnberger-iii D: expected 193.157 c, got {d_iii}"
        );
        assert!(
            (d_ii - d_iii).abs() > 1.0,
            "kirnberger-ii and -iii should disagree audibly on D, got {d_ii} vs {d_iii}"
        );
    }

    // -- Resolver-level: all ten resolve to a real frequency. -----------------

    #[test]
    fn all_ten_temperaments_resolve_and_produce_a_real_frequency() {
        const TEN: [&str; 10] = [
            "pythagorean",
            "meantone-1/4-comma",
            "meantone-1/5-comma",
            "meantone-1/6-comma",
            "werckmeister-iii",
            "werckmeister-iv",
            "vallotti",
            "kirnberger-ii",
            "kirnberger-iii",
            "young-ii",
        ];
        let f = fixture();
        let c5 = cmn_pitch("cmn-12", CmnNominal::C, 0, 5);
        for id in TEN {
            let mut score = f.score.clone();
            score.tuning_context.default_tuning_system = TuningSystemId::new(id);
            let freq = resolve_pitch_frequency(&score, &c5, f.voice_a)
                .unwrap_or_else(|e| panic!("{id} must resolve to a frequency, got error: {e}"));
            assert!(
                freq.is_finite() && freq > 0.0,
                "{id}: expected a real, positive frequency, got {freq}"
            );
        }
    }

    #[test]
    fn werckmeister_iii_c_sharp_resolves_distinctly_from_tet_12_c_sharp() {
        let f = fixture();
        let c_sharp = cmn_pitch("cmn-12", CmnNominal::C, 1, 5);
        let mut wm_score = f.score.clone();
        wm_score.tuning_context.default_tuning_system = TuningSystemId::new("werckmeister-iii");
        // `f.score` already defaults to tet-12.
        let wm_freq = resolve_pitch_frequency(&wm_score, &c_sharp, f.voice_a)
            .expect("werckmeister-iii resolves");
        let tet_freq =
            resolve_pitch_frequency(&f.score, &c_sharp, f.voice_a).expect("tet-12 resolves");
        let diff = cents_between(wm_freq, tet_freq);
        assert!(
            diff > 0.5,
            "expected werckmeister-iii's C# to differ audibly from tet-12's, diff was only {diff} c"
        );
    }

    // -- Fail-closed: an unreserved TuningFunctionId never yields a frequency. -

    #[test]
    fn unknown_tuning_function_id_fails_closed() {
        // Unit level: `coordinate_ratio` returns `None` directly -- the
        // extension point has no registry
        // (`spec/CONTRACT_PUSH4B_TEMPERAMENTS.md` item 1).
        let bogus_resolution = TuningResolution::Function {
            function: TuningFunctionId::new("not-a-real-temperament"),
            parameters: TuningParameters,
        };
        assert_eq!(coordinate_ratio(&bogus_resolution, 0), None);

        // Full-pipeline level: a `TuningSystem` carrying that resolution
        // never produces a frequency, only an error -- `PositionUnavailable`,
        // since `coordinate_ratio`'s `None` is exactly what
        // `frequency_for_position` reports that way for.
        let bogus_system = TuningSystem {
            id: TuningSystemId::new("bogus"),
            name: "bogus".to_owned(),
            pitch_space: PitchSpaceId::new("cmn-12"),
            resolution: bogus_resolution,
            description: None,
        };
        let c5_position = PitchSpacePosition::Cmn {
            nominal: CmnNominal::C,
            alteration: 0,
            octave: 5,
        };
        let err = frequency_for_position(&c5_position, &bogus_system, &ReferencePitch::a440())
            .expect_err("an unknown TuningFunctionId must never resolve to a frequency");
        assert!(
            matches!(err, TuningResolutionError::PositionUnavailable),
            "expected PositionUnavailable, got {err:?}"
        );
    }
}

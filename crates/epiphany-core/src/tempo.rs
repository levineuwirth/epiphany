//! The tempo map: the function mapping musical positions to wall-clock
//! positions (Chapter 3 §"Tempo and the Tempo Map").
//!
//! This is the data model plus the **sanctioned closed-form conversion stub**
//! (QUICKSTART "Don't do these": *"Stub `wallclock_to_musical` with linear
//! segments only. For curve segments, return … a diagnostic noting that curve
//! integration is not yet implemented."*). Conversion integrates the piecewise
//! tempo map over its [`TempoSegment`]s: [`TempoShape::Constant`],
//! [`TempoShape::Linear`], and [`TempoShape::Exponential`] segments have a
//! closed-form solution (Chapter 3 §"Conversion") and are integrated here;
//! [`TempoShape::Curve`] needs the deferred numerical-integration algorithm
//! (one of the four open canonical algorithms, QUICKSTART intro) and is
//! reported as [`TempoError::CurveIntegrationUnsupported`], never computed
//! wrongly.
//!
//! Segment boundaries are symbolic [`TimeAnchor`]s; resolving them to musical
//! positions in general needs the score graph, so the conversion takes a
//! resolver closure ([`TempoMap::musical_to_wallclock_with`]). The no-argument
//! [`TempoMap::musical_to_wallclock`] uses a self-contained resolver that
//! places `Region`-start-relative segment anchors (the natural anchoring for a
//! region-local tempo map); anchors it cannot place make the conversion report
//! [`TempoError::PiecewiseIntegrationUnsupported`] rather than guess.
//!
//! **Determinism and tolerance.** The inverse [`TempoMap::wallclock_to_musical`]
//! uses a deterministic continued-fraction rational approximation with
//! documented bounds ([`INVERSION_MAX_ITERATIONS`], [`INVERSION_MAX_DENOMINATOR`],
//! [`INVERSION_TOLERANCE_WHOLE_NOTES`]) — the documented tolerance and iteration
//! bounds Chapter 3 §"Conversion" requires of any numerical inversion. The
//! residual is governed by [`epiphany_determinism::ToleranceClass::TempoIntegration`].
//!
//! Conversion here is **advisory** (it uses `f64`), not canonical state:
//! musical time is the exact rational and wall-clock is exact nanoseconds
//! (Appendix D §"Exact and Quantized Representations").

use epiphany_determinism::CanonicalF64;

use crate::time::{
    MusicalDuration, MusicalPosition, RationalTime, RegionEdge, TimeAnchor, WallClockTime,
};

/// A tempo: beats per minute at a given beat unit (Chapter 3). A quarter-note
/// BPM and a dotted-quarter BPM at the same numeric value differ in rate, so the
/// beat unit is part of the tempo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Tempo {
    bpm: CanonicalF64,
    beat_unit: MusicalDuration,
}

impl Tempo {
    /// Builds a tempo, rejecting a non-finite or non-positive BPM and a
    /// non-positive beat unit.
    pub fn new(bpm: f64, beat_unit: MusicalDuration) -> Option<Self> {
        if bpm > 0.0 && beat_unit.is_positive() {
            CanonicalF64::new(bpm).map(|bpm| Tempo { bpm, beat_unit })
        } else {
            None
        }
    }

    /// Quarter-note BPM (the common case): `bpm` beats, each a quarter note.
    pub fn quarter(bpm: f64) -> Option<Self> {
        Tempo::new(bpm, MusicalDuration(crate::time::RationalTime::new(1, 4)?))
    }

    /// Beats per minute.
    pub fn bpm(&self) -> f64 {
        self.bpm.get()
    }

    /// The beat unit (in whole notes).
    pub fn beat_unit(&self) -> &MusicalDuration {
        &self.beat_unit
    }

    /// Seconds per whole note at this tempo: one beat (`beat_unit` whole notes)
    /// lasts `60/bpm` seconds, so a whole note lasts
    /// `(60/bpm) / beat_unit` seconds.
    pub fn seconds_per_whole_note(&self) -> f64 {
        let beat_whole_notes = self.beat_unit.rational().to_f64();
        (60.0 / self.bpm.get()) / beat_whole_notes
    }

    /// Speed in whole notes per second: the reciprocal of
    /// [`Tempo::seconds_per_whole_note`]. Strictly positive (the constructor
    /// rejects a non-positive bpm or beat unit), which is what keeps the
    /// musical→wall-clock mapping monotonically increasing (Chapter 3
    /// §"Conversion": musical time always advances).
    pub fn whole_notes_per_second(&self) -> f64 {
        let beat_whole_notes = self.beat_unit.rational().to_f64();
        (self.bpm.get() * beat_whole_notes) / 60.0
    }
}

/// The shape of a tempo change over a segment (Chapter 3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TempoShape {
    Constant,
    Linear,
    Exponential,
    /// Arbitrary curve (control points carried by the deferred companion).
    Curve,
}

/// A piecewise tempo segment (Chapter 3). Boundaries are [`TimeAnchor`]s; the
/// `end`/`end_tempo` are `None` for an open constant segment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TempoSegment {
    pub start: TimeAnchor,
    pub end: Option<TimeAnchor>,
    pub start_tempo: Tempo,
    pub end_tempo: Option<Tempo>,
    pub shape: TempoShape,
}

/// The tempo map (Chapter 3). `initial` applies before the first segment.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TempoMap {
    pub initial: Option<Tempo>,
    pub segments: Vec<TempoSegment>,
}

/// Why a tempo conversion could not be performed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TempoError {
    /// No tempo is defined (empty map with no `initial`).
    NoTempo,
    /// Curve integration is not implemented in v0 (deferred open algorithm,
    /// QUICKSTART; Appendix D §"Open Algorithm Hooks"). [`TempoShape::Curve`]
    /// segments need the deferred numerical integrator.
    CurveIntegrationUnsupported,
    /// A segment boundary [`TimeAnchor`] could not be resolved to a musical
    /// position with the available resolver, so the piecewise map could not be
    /// placed on the musical timeline. Resolution in general needs the score
    /// graph (Chapter 3 §"Time Anchors"); this is reported, never guessed.
    PiecewiseIntegrationUnsupported,
    /// The tempo map's segments are not well-formed: out of order, overlapping,
    /// a non-`Constant` segment missing its `end_tempo`, or an open
    /// (`end == None`) non-`Constant` segment (Chapter 3 §"Tempo and the Tempo
    /// Map"). The graph-invariant checker reports the same conditions
    /// structurally (invariant on the tempo map).
    MalformedTempoMap,
    /// A conversion result fell outside the representable range (a non-finite
    /// or out-of-`i64` nanosecond value), so it is reported rather than
    /// silently saturated (Appendix D §"Floating-Point Values").
    ConversionOverflow,
}

/// Maximum continued-fraction iterations in [`TempoMap::wallclock_to_musical`]
/// (the documented iteration bound Chapter 3 §"Conversion" requires).
pub const INVERSION_MAX_ITERATIONS: u32 = 64;
/// Maximum denominator the inverse will introduce, in whole-note units. Keeps
/// recovered rhythms simple (`1/3`, `1/12`, …) instead of spurious large ratios.
pub const INVERSION_MAX_DENOMINATOR: u64 = 1_000_000;
/// Absolute residual tolerance of the inverse, in whole notes
/// ([`epiphany_determinism::ToleranceClass::TempoIntegration`]). Comfortably
/// larger than the half-nanosecond rounding of the forward direction, so an
/// ordinary rhythm round-trips, yet far smaller than any musical distinction.
pub const INVERSION_TOLERANCE_WHOLE_NOTES: f64 = 1e-6;

/// The closed-form integration model of one stretch of musical time: a
/// half-open interval `[start, end)` in whole notes (`end == None` is open) plus
/// how the speed (whole notes per second) behaves across it. Built from the
/// resolved segments, the gaps between them, and the `initial`/leading and
/// trailing regions (Chapter 3 §"Tempo and the Tempo Map", gap rule).
struct TempoPiece {
    start: f64,
    end: Option<f64>,
    model: SpeedModel,
}

/// How speed varies across a [`TempoPiece`]. Speeds are whole notes per second,
/// always strictly positive.
enum SpeedModel {
    /// Constant speed.
    Const(f64),
    /// Linear interpolation of speed from `s0` (at the piece start) to `s1` (at
    /// the piece end). Equivalent to linear bpm interpolation when the two
    /// tempos share a beat unit (Chapter 3: "Linear interpolation from
    /// start_tempo to end_tempo").
    Linear { s0: f64, s1: f64 },
    /// Exponential (continuous-rate) interpolation of speed from `s0` to `s1`.
    Exponential { s0: f64, s1: f64 },
}

impl SpeedModel {
    /// Wall-clock seconds elapsed across the sub-interval `[u0, u1]` of the
    /// piece, where `u` is the fractional position within the piece (`0` at the
    /// piece start, `1` at its end) and `len` is the piece's musical length in
    /// whole notes. Closed-form per shape.
    fn seconds(&self, u0: f64, u1: f64, len: f64) -> f64 {
        match self {
            SpeedModel::Const(s) => len * (u1 - u0) / s,
            SpeedModel::Linear { s0, s1 } => {
                let d = s1 - s0;
                if d.abs() < f64::EPSILON {
                    len * (u1 - u0) / s0
                } else {
                    // ∫ du/(s0 + d·u) = (1/d) ln(s(u)); seconds scale by `len`.
                    let su1 = s0 + d * u1;
                    let su0 = s0 + d * u0;
                    len / d * (su1 / su0).ln()
                }
            }
            SpeedModel::Exponential { s0, s1 } => {
                let l = (s1 / s0).ln();
                if l.abs() < f64::EPSILON {
                    len * (u1 - u0) / s0
                } else {
                    // s(u) = s0·e^{l·u}; ∫ du/s(u) = (e^{-l·u0} - e^{-l·u1})/(s0·l).
                    len * ((-l * u0).exp() - (-l * u1).exp()) / (s0 * l)
                }
            }
        }
    }

    /// The inverse of [`SpeedModel::seconds`] from `u0 = 0`: the fractional
    /// position `u` within the piece reached after `secs` wall-clock seconds.
    fn invert(&self, secs: f64, len: f64) -> f64 {
        match self {
            SpeedModel::Const(s) => secs * s / len,
            SpeedModel::Linear { s0, s1 } => {
                let d = s1 - s0;
                if d.abs() < f64::EPSILON {
                    secs * s0 / len
                } else {
                    // secs = len/d · ln(s(u)/s0) ⇒ s(u) = s0·e^{secs·d/len}.
                    let su = s0 * (secs * d / len).exp();
                    (su - s0) / d
                }
            }
            SpeedModel::Exponential { s0, s1 } => {
                let l = (s1 / s0).ln();
                if l.abs() < f64::EPSILON {
                    secs * s0 / len
                } else {
                    // secs = len/(s0·l)·(1 - e^{-l·u}) ⇒ u = -ln(1 - secs·s0·l/len)/l.
                    -(1.0 - secs * s0 * l / len).ln() / l
                }
            }
        }
    }
}

/// The self-contained segment-anchor resolver used by the no-argument
/// conversions: it places a `Region`-start-relative anchor (the natural
/// anchoring for a region-local tempo map, where the region origin is musical
/// zero) and declines everything else (which needs the score graph). Returning
/// `None` makes the conversion report
/// [`TempoError::PiecewiseIntegrationUnsupported`] rather than guess.
fn region_relative_resolve(anchor: &TimeAnchor) -> Option<MusicalPosition> {
    match anchor {
        TimeAnchor::Region {
            edge: RegionEdge::Start,
            offset,
            ..
        } => match offset {
            crate::time::AnchorOffset::Zero => Some(MusicalPosition::origin()),
            crate::time::AnchorOffset::Musical(d) => Some(MusicalPosition(d.rational().clone())),
            crate::time::AnchorOffset::WallClock(_) => None,
        },
        _ => None,
    }
}

impl TempoMap {
    /// A constant-tempo map (the common case): `initial` tempo, no segments.
    pub fn constant(tempo: Tempo) -> Self {
        TempoMap {
            initial: Some(tempo),
            segments: Vec::new(),
        }
    }

    /// Converts a musical position to wall-clock time (Chapter 3 §"Conversion"),
    /// using `region_relative_resolve` for segment boundaries. Exact for a
    /// constant map; integrates closed-form over `Constant`/`Linear`/
    /// `Exponential` segments; reports a [`TempoError`] (never a wrong answer)
    /// for curves or unresolvable boundaries. Result rounds to the nearest
    /// nanosecond.
    pub fn musical_to_wallclock(&self, pos: &MusicalPosition) -> Result<WallClockTime, TempoError> {
        self.musical_to_wallclock_with(pos, region_relative_resolve)
    }

    /// As [`TempoMap::musical_to_wallclock`], but resolves segment-boundary
    /// [`TimeAnchor`]s through `resolve` (e.g. against the score graph), so
    /// event- and measure-anchored segments can be placed too.
    pub fn musical_to_wallclock_with(
        &self,
        pos: &MusicalPosition,
        resolve: impl Fn(&TimeAnchor) -> Option<MusicalPosition>,
    ) -> Result<WallClockTime, TempoError> {
        let pieces = self.build_pieces(&resolve)?;
        let target = pos.rational().to_f64();
        let seconds = elapsed_seconds(&pieces, target)?;
        let ns = (seconds * 1e9).round();
        checked_i64(ns).map(WallClockTime)
    }

    /// Converts a wall-clock time to a musical position (Chapter 3
    /// §"Conversion"), using `region_relative_resolve` for segment boundaries.
    /// The inverse uses a deterministic continued-fraction rational
    /// approximation with the documented bounds [`INVERSION_MAX_ITERATIONS`] /
    /// [`INVERSION_MAX_DENOMINATOR`] / [`INVERSION_TOLERANCE_WHOLE_NOTES`], so an
    /// ordinary rhythm (a triplet `1/12`, a dotted `3/8`) round-trips exactly
    /// rather than being quantized to a fixed grid.
    pub fn wallclock_to_musical(&self, time: WallClockTime) -> Result<MusicalPosition, TempoError> {
        self.wallclock_to_musical_with(time, region_relative_resolve)
    }

    /// As [`TempoMap::wallclock_to_musical`], but resolves segment-boundary
    /// [`TimeAnchor`]s through `resolve`.
    pub fn wallclock_to_musical_with(
        &self,
        time: WallClockTime,
        resolve: impl Fn(&TimeAnchor) -> Option<MusicalPosition>,
    ) -> Result<MusicalPosition, TempoError> {
        let pieces = self.build_pieces(&resolve)?;
        let seconds = time.0 as f64 / 1e9;
        let whole_notes = whole_notes_at(&pieces, seconds)?;
        rational_from_f64(
            whole_notes,
            INVERSION_MAX_DENOMINATOR,
            INVERSION_TOLERANCE_WHOLE_NOTES,
        )
        .map(MusicalPosition)
        .ok_or(TempoError::ConversionOverflow)
    }

    /// Resolves and validates the segments, then builds the contiguous list of
    /// [`TempoPiece`]s covering musical time from zero. Returns a
    /// [`TempoError`] for an empty map, an unresolvable boundary, a curve
    /// segment, or a malformed segment sequence.
    fn build_pieces(
        &self,
        resolve: &impl Fn(&TimeAnchor) -> Option<MusicalPosition>,
    ) -> Result<Vec<TempoPiece>, TempoError> {
        // The constant common case: one open piece at `initial`.
        if self.segments.is_empty() {
            let t = self.initial.as_ref().ok_or(TempoError::NoTempo)?;
            return Ok(vec![TempoPiece {
                start: 0.0,
                end: None,
                model: SpeedModel::Const(t.whole_notes_per_second()),
            }]);
        }

        // Resolve every segment's start (required) to a musical whole-note
        // value; the end is resolved if present, else taken lazily as the next
        // segment's start (or open for the final segment).
        let mut starts: Vec<f64> = Vec::with_capacity(self.segments.len());
        for seg in &self.segments {
            let p = resolve(&seg.start).ok_or(TempoError::PiecewiseIntegrationUnsupported)?;
            starts.push(p.rational().to_f64());
        }
        // Segments MUST be in monotonically increasing start order (Chapter 3).
        for w in starts.windows(2) {
            if w[1] < w[0] {
                return Err(TempoError::MalformedTempoMap);
            }
        }

        let n = self.segments.len();
        let mut pieces: Vec<TempoPiece> = Vec::new();
        // Leading region before the first segment: the gap rule gives it
        // `initial` (or, absent that, the first segment's start_tempo).
        let first_start = starts[0];
        if first_start > 0.0 {
            let lead = self
                .initial
                .as_ref()
                .unwrap_or(&self.segments[0].start_tempo);
            pieces.push(TempoPiece {
                start: 0.0,
                end: Some(first_start),
                model: SpeedModel::Const(lead.whole_notes_per_second()),
            });
        }

        for (i, seg) in self.segments.iter().enumerate() {
            let start = starts[i];
            // Effective end: the segment's own end if given, else the next
            // segment's start, else open.
            let explicit_end = match &seg.end {
                Some(a) => Some(
                    resolve(a)
                        .ok_or(TempoError::PiecewiseIntegrationUnsupported)?
                        .rational()
                        .to_f64(),
                ),
                None => None,
            };
            let next_start = (i + 1 < n).then(|| starts[i + 1]);
            let eff_end = explicit_end.or(next_start);
            if let Some(e) = eff_end {
                if e < start {
                    return Err(TempoError::MalformedTempoMap);
                }
                // Non-overlap with the next segment.
                if let Some(ns) = next_start {
                    if e > ns {
                        return Err(TempoError::MalformedTempoMap);
                    }
                }
            }
            let s0 = seg.start_tempo.whole_notes_per_second();
            let model = match seg.shape {
                TempoShape::Curve => return Err(TempoError::CurveIntegrationUnsupported),
                TempoShape::Constant => {
                    // A Constant segment's end_tempo MUST be absent or equal to
                    // start_tempo (Chapter 3).
                    if let Some(et) = &seg.end_tempo {
                        if et != &seg.start_tempo {
                            return Err(TempoError::MalformedTempoMap);
                        }
                    }
                    SpeedModel::Const(s0)
                }
                TempoShape::Linear | TempoShape::Exponential => {
                    // A non-Constant segment needs a finite end and an end_tempo.
                    let et = seg
                        .end_tempo
                        .as_ref()
                        .ok_or(TempoError::MalformedTempoMap)?;
                    if eff_end.is_none() {
                        return Err(TempoError::MalformedTempoMap);
                    }
                    let s1 = et.whole_notes_per_second();
                    if seg.shape == TempoShape::Linear {
                        SpeedModel::Linear { s0, s1 }
                    } else {
                        SpeedModel::Exponential { s0, s1 }
                    }
                }
            };
            pieces.push(TempoPiece {
                start,
                end: eff_end,
                model,
            });

            // Gap between this segment's end and the next segment's start: the
            // gap rule holds the most-recent terminating tempo (Chapter 3).
            if let (Some(e), Some(ns)) = (eff_end, next_start) {
                if e < ns {
                    let term = seg.end_tempo.as_ref().unwrap_or(&seg.start_tempo);
                    pieces.push(TempoPiece {
                        start: e,
                        end: Some(ns),
                        model: SpeedModel::Const(term.whole_notes_per_second()),
                    });
                }
            }
        }

        // Trailing region after a final segment with a finite end: the gap rule
        // holds its terminating tempo to infinity.
        if let Some(last) = self.segments.last() {
            if let Some(TempoPiece { end: Some(e), .. }) = pieces.last() {
                let e = *e;
                let term = last.end_tempo.as_ref().unwrap_or(&last.start_tempo);
                pieces.push(TempoPiece {
                    start: e,
                    end: None,
                    model: SpeedModel::Const(term.whole_notes_per_second()),
                });
            }
        }

        Ok(pieces)
    }
}

/// Integrates the pieces to find the wall-clock seconds elapsed from musical
/// zero to `target` whole notes.
fn elapsed_seconds(pieces: &[TempoPiece], target: f64) -> Result<f64, TempoError> {
    if !target.is_finite() {
        return Err(TempoError::ConversionOverflow);
    }
    let mut total = 0.0;
    for p in pieces {
        if target <= p.start {
            break;
        }
        let piece_end = p.end.unwrap_or(target).min(target);
        if piece_end <= p.start {
            continue;
        }
        let len = match p.end {
            // Length used to normalise `u`; an open piece is constant, so its
            // length cancels — use the covered span itself.
            Some(e) => e - p.start,
            None => piece_end - p.start,
        };
        if len <= 0.0 {
            continue;
        }
        let u0 = 0.0;
        let u1 = (piece_end - p.start) / len;
        total += p.model.seconds(u0, u1, len);
    }
    if total.is_finite() {
        Ok(total)
    } else {
        Err(TempoError::ConversionOverflow)
    }
}

/// Inverts [`elapsed_seconds`]: the musical whole-note position reached after
/// `target_secs` wall-clock seconds from musical zero.
fn whole_notes_at(pieces: &[TempoPiece], target_secs: f64) -> Result<f64, TempoError> {
    if !target_secs.is_finite() {
        return Err(TempoError::ConversionOverflow);
    }
    if target_secs <= 0.0 {
        return Ok(0.0);
    }
    let mut acc = 0.0;
    for p in pieces {
        match p.end {
            Some(e) => {
                let len = e - p.start;
                if len <= 0.0 {
                    continue;
                }
                let full = p.model.seconds(0.0, 1.0, len);
                if acc + full >= target_secs {
                    let u = p.model.invert(target_secs - acc, len);
                    return Ok(p.start + u * len);
                }
                acc += full;
            }
            None => {
                // Open final piece (always Const): solve directly.
                let remaining = target_secs - acc;
                // Use a unit length so `u` maps straight to whole notes.
                let u = p.model.invert(remaining, 1.0);
                return Ok(p.start + u);
            }
        }
    }
    // No open tail (should not happen — build_pieces always ends open); clamp to
    // the final piece end as a sound fallback.
    Ok(pieces.last().and_then(|p| p.end).unwrap_or(0.0))
}

/// Rounds an `f64` nanosecond/whole-note quantity to `i64`, returning
/// [`TempoError::ConversionOverflow`] for a non-finite or out-of-range value
/// instead of the saturating `as i64` cast (Appendix D §"Floating-Point Values").
fn checked_i64(value: f64) -> Result<i64, TempoError> {
    if value.is_finite() && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        Ok(value as i64)
    } else {
        Err(TempoError::ConversionOverflow)
    }
}

/// The deterministic continued-fraction rational approximation behind
/// [`TempoMap::wallclock_to_musical`]: the simplest `numerator/denominator`
/// (denominator ≤ `max_den`) within `tol` whole notes of `x`, or the best
/// convergent reached within [`INVERSION_MAX_ITERATIONS`]. Returns `None` only
/// if the result does not fit the inline rational range.
fn rational_from_f64(x: f64, max_den: u64, tol: f64) -> Option<RationalTime> {
    if !x.is_finite() {
        return None;
    }
    let neg = x < 0.0;
    let x_abs = x.abs();
    // Convergent recurrence h/k, seeded with h_{-1}=1,h_{-2}=0 / k_{-1}=0,k_{-2}=1.
    let (mut h_prev2, mut h_prev1) = (0i128, 1i128);
    let (mut k_prev2, mut k_prev1) = (1i128, 0i128);
    let mut value = x_abs;
    let mut best = (0i128, 1i128); // 0/1 until the first convergent lands.
    for _ in 0..INVERSION_MAX_ITERATIONS {
        let a = value.floor();
        if !a.is_finite() {
            break;
        }
        let ai = a as i128;
        let h = ai * h_prev1 + h_prev2;
        let k = ai * k_prev1 + k_prev2;
        if k <= 0 || (k as u128) > max_den as u128 {
            break;
        }
        h_prev2 = h_prev1;
        h_prev1 = h;
        k_prev2 = k_prev1;
        k_prev1 = k;
        best = (h, k);
        if (h as f64 / k as f64 - x_abs).abs() <= tol {
            break;
        }
        let frac = value - a;
        if frac.abs() < f64::EPSILON {
            break;
        }
        value = 1.0 / frac;
    }
    let (num, den) = best;
    let num = if neg { -num } else { num };
    let n = i64::try_from(num).ok()?;
    let d = i64::try_from(den).ok()?;
    RationalTime::new(n, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{RegionId, ReplicaId};
    use crate::time::{AnchorOffset, RationalTime};

    /// A `Region`-start anchor offset by `whole_notes` of musical time — the
    /// natural, self-contained anchoring for a region-local tempo segment.
    fn region_at(whole_notes: RationalTime) -> TimeAnchor {
        TimeAnchor::Region {
            id: RegionId::new(ReplicaId(1), 0),
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(whole_notes)),
        }
    }

    #[test]
    fn constant_tempo_round_trips_musical_and_wallclock() {
        // 120 quarter-note BPM: a quarter note = 0.5 s, a whole note = 2 s.
        let map = TempoMap::constant(Tempo::quarter(120.0).unwrap());
        let one_whole = MusicalPosition(RationalTime::from_int(1));
        let t = map.musical_to_wallclock(&one_whole).unwrap();
        assert_eq!(t, WallClockTime(2_000_000_000)); // 2 s in ns
                                                     // And back.
        assert_eq!(map.wallclock_to_musical(t).unwrap(), one_whole);
        // Half note at 120 q-bpm = 1 s.
        let half = MusicalPosition(RationalTime::new(1, 2).unwrap());
        assert_eq!(
            map.musical_to_wallclock(&half).unwrap(),
            WallClockTime(1_000_000_000)
        );
    }

    #[test]
    fn ordinary_rhythms_round_trip_exactly() {
        // The 1/1024-grid quantization used to lose a triplet 1/12; the
        // continued-fraction inverse recovers it (and other ordinary rhythms).
        let map = TempoMap::constant(Tempo::quarter(120.0).unwrap());
        for (n, d) in [(1, 12), (1, 3), (3, 8), (5, 6), (7, 16), (1, 7)] {
            let pos = MusicalPosition(RationalTime::new(n, d).unwrap());
            let t = map.musical_to_wallclock(&pos).unwrap();
            assert_eq!(
                map.wallclock_to_musical(t).unwrap(),
                pos,
                "rhythm {n}/{d} did not round-trip"
            );
        }
    }

    #[test]
    fn tempo_rejects_bad_values() {
        assert!(Tempo::quarter(0.0).is_none());
        assert!(Tempo::quarter(-60.0).is_none());
        assert!(Tempo::quarter(f64::NAN).is_none());
        assert!(Tempo::new(120.0, MusicalDuration::zero()).is_none());
    }

    #[test]
    fn linear_segment_is_integrated_not_rejected() {
        // A single linear ramp over [0, 1] whole notes from 60 to 120 q-bpm.
        // Speeds: 0.25 -> 0.5 whole notes/s. Closed form for one whole note is
        // (1/Δs)·ln(s1/s0) = (1/0.25)·ln(2) = 4·ln 2 ≈ 2.77259 s.
        let map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_at(RationalTime::zero()),
                end: Some(region_at(RationalTime::from_int(1))),
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: Some(Tempo::quarter(120.0).unwrap()),
                shape: TempoShape::Linear,
            }],
        };
        let one = MusicalPosition(RationalTime::from_int(1));
        let t = map
            .musical_to_wallclock(&one)
            .expect("linear is implemented");
        let secs = t.0 as f64 / 1e9;
        assert!((secs - 4.0 * 2f64.ln()).abs() < 1e-6, "got {secs}s");
        // The ramp inverts and ordinary positions round-trip.
        assert_eq!(map.wallclock_to_musical(t).unwrap(), one);
        let half = MusicalPosition(RationalTime::new(1, 2).unwrap());
        let th = map.musical_to_wallclock(&half).unwrap();
        assert_eq!(map.wallclock_to_musical(th).unwrap(), half);
    }

    #[test]
    fn exponential_segment_is_integrated() {
        let map = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_at(RationalTime::zero()),
                end: Some(region_at(RationalTime::from_int(2))),
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: Some(Tempo::quarter(120.0).unwrap()),
                shape: TempoShape::Exponential,
            }],
        };
        let p = MusicalPosition(RationalTime::from_int(1));
        let t = map
            .musical_to_wallclock(&p)
            .expect("exponential is implemented");
        // Round-trips through the closed-form inverse.
        assert_eq!(map.wallclock_to_musical(t).unwrap(), p);
    }

    #[test]
    fn deferred_and_malformed_cases_report_errors_not_wrong_answers() {
        let empty = TempoMap::default();
        assert_eq!(
            empty.musical_to_wallclock(&MusicalPosition(RationalTime::from_int(1))),
            Err(TempoError::NoTempo)
        );

        // A curve segment (resolvable boundary) defers to the numerical algorithm.
        let curved = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_at(RationalTime::zero()),
                end: Some(region_at(RationalTime::from_int(1))),
                start_tempo: Tempo::quarter(120.0).unwrap(),
                end_tempo: Some(Tempo::quarter(60.0).unwrap()),
                shape: TempoShape::Curve,
            }],
        };
        assert_eq!(
            curved.musical_to_wallclock(&MusicalPosition(RationalTime::from_int(1))),
            Err(TempoError::CurveIntegrationUnsupported)
        );

        // A wall-clock-anchored boundary cannot be placed by the self-contained
        // resolver, so the conversion declines rather than guesses.
        let unplaceable = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: TimeAnchor::WallClock {
                    time: WallClockTime(0),
                },
                end: None,
                start_tempo: Tempo::quarter(120.0).unwrap(),
                end_tempo: None,
                shape: TempoShape::Constant,
            }],
        };
        assert_eq!(
            unplaceable.musical_to_wallclock(&MusicalPosition(RationalTime::from_int(1))),
            Err(TempoError::PiecewiseIntegrationUnsupported)
        );

        // A non-Constant segment without an end_tempo is malformed.
        let no_end_tempo = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_at(RationalTime::zero()),
                end: Some(region_at(RationalTime::from_int(1))),
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: None,
                shape: TempoShape::Linear,
            }],
        };
        assert_eq!(
            no_end_tempo.musical_to_wallclock(&MusicalPosition(RationalTime::from_int(1))),
            Err(TempoError::MalformedTempoMap)
        );

        // A Constant segment whose end_tempo disagrees with start_tempo is
        // malformed.
        let bad_constant = TempoMap {
            initial: None,
            segments: vec![TempoSegment {
                start: region_at(RationalTime::zero()),
                end: None,
                start_tempo: Tempo::quarter(60.0).unwrap(),
                end_tempo: Some(Tempo::quarter(120.0).unwrap()),
                shape: TempoShape::Constant,
            }],
        };
        assert_eq!(
            bad_constant.musical_to_wallclock(&MusicalPosition(RationalTime::from_int(1))),
            Err(TempoError::MalformedTempoMap)
        );
    }

    #[test]
    fn out_of_range_conversion_reports_overflow_not_saturation() {
        let map = TempoMap::constant(Tempo::quarter(120.0).unwrap());
        let huge = MusicalPosition(RationalTime::new(i64::MAX, 1).unwrap());
        assert_eq!(
            map.musical_to_wallclock(&huge),
            Err(TempoError::ConversionOverflow)
        );
    }
}

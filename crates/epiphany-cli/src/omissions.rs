//! What the engraver left out of an imported score: the third column of the
//! corpus report.
//!
//! Every check reads the resolved layout's provenance, so it counts what was
//! drawn for each object of the score, never what the engraver is believed to
//! draw. A zero-length stroke is a traced anchor, the projection's placeholder
//! for an object it does not engrave, and does not count as ink. The checks:
//!
//! - an event, pitch, tie or slur with no primitive at all;
//! - a notehead or rest drawn at a value other than its duration's, a flag or
//!   an augmentation dot the duration needs and the event lacks (an eighth or
//!   shorter needs a flag or a beam), for every
//!   event whose duration is one notated value (a duration that needs a tie or
//!   a tuplet to notate is counted as not checked);
//! - a clef, and for a staff with a key, a key signature, missing where a
//!   system starts; a clef change after the start with no clef drawn for it;
//! - a time signature missing from the measure where a meter takes effect;
//! - on a layout of several pages, ink no system owns, which is on no page;
//! - each diagnostic the projection raised, by kind.
//!
//! Not checked, and counted as such: whether an accidental is the one the
//! key and the measure's earlier notes call for; an unpitched note's value;
//! and a key change after the start.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{
    AnchorOffset, Event, EventDuration, EventPosition, RationalTime, Score, StaffId,
    StaffInstanceId, TimeAnchor, TypedObjectId,
};
use epiphany_layout_ir::constrained::{LayoutDiagnostic, LayoutDiagnosticKind};
use epiphany_layout_ir::{is_beam_stroke, ResolvedLayoutIR};

/// Engraving omissions by kind, with counts; and what was not checked.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Omissions {
    pub kinds: BTreeMap<String, usize>,
    /// Checks that could not be made, by reason.
    pub unchecked: BTreeMap<String, usize>,
}

impl Omissions {
    fn add(&mut self, kind: &str) {
        *self.kinds.entry(kind.to_owned()).or_default() += 1;
    }

    fn skip(&mut self, reason: &str) {
        *self.unchecked.entry(reason.to_owned()).or_default() += 1;
    }
}

/// A duration as one notated value: the base value in whole notes as a
/// power of two (`2` a breve, `1/8` an eighth) and its dots; `None` when the
/// duration needs a tie or a tuplet.
pub fn notated(duration: &RationalTime) -> Option<(i32, u8)> {
    let RationalTime::Small(small) = duration else {
        return None;
    };
    let (n, d) = (i64::from(small.numerator()), i64::from(small.denominator()));
    if n <= 0 {
        return None;
    }
    // duration = base * (2 - 2^-dots) = base * (2^(dots+1) - 1) / 2^dots
    for dots in 0..=3u8 {
        let factor = (1i64 << (dots + 1)) - 1;
        if n % factor != 0 {
            continue;
        }
        // base = (n / factor) * 2^dots / d, which must be 2^k.
        let num = (n / factor) << dots;
        let (num, den) = reduce(num, d);
        if num == 1 && den.count_ones() == 1 {
            return Some((-(den.trailing_zeros() as i32), dots));
        }
        if den == 1 && num.count_ones() == 1 {
            return Some((num.trailing_zeros() as i32, dots));
        }
    }
    None
}

fn reduce(a: i64, b: i64) -> (i64, i64) {
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    (a / x, b / x)
}

/// The notehead a base value draws with (`exp` is its power of two).
fn notehead_for(exp: i32) -> &'static str {
    match exp {
        e if e >= 1 => "noteheadDoubleWhole",
        0 => "noteheadWhole",
        -1 => "noteheadHalf",
        _ => "noteheadBlack",
    }
}

/// The rest glyph a base value draws with.
fn rest_for(exp: i32) -> String {
    match exp {
        e if e >= 1 => String::from("restDoubleWhole"),
        0 => String::from("restWhole"),
        -1 => String::from("restHalf"),
        -2 => String::from("restQuarter"),
        e => {
            let n = 1u64 << (-e);
            let suffix = match n {
                8 => "8th",
                16 => "16th",
                32 => "32nd",
                64 => "64th",
                128 => "128th",
                256 => "256th",
                _ => "1024th",
            };
            format!("rest{suffix}")
        }
    }
}

fn offset(anchor: &TimeAnchor) -> Option<RationalTime> {
    match anchor {
        TimeAnchor::Region {
            offset: AnchorOffset::Musical(d),
            ..
        } => Some(d.0.clone()),
        TimeAnchor::Region {
            offset: AnchorOffset::Zero,
            ..
        } => Some(RationalTime::zero()),
        _ => None,
    }
}

/// Counts what the engraver left out of `score` in `layout`.
pub fn omissions(
    score: &Score,
    layout: &ResolvedLayoutIR,
    diagnostics: &[LayoutDiagnostic],
) -> Omissions {
    let mut out = Omissions::default();
    let mut glyphs: BTreeMap<TypedObjectId, Vec<&str>> = BTreeMap::new();
    let mut inked: BTreeSet<TypedObjectId> = BTreeSet::new();
    for g in &layout.glyphs {
        glyphs
            .entry(g.provenance.source)
            .or_default()
            .push(g.glyph.as_str());
        inked.insert(g.provenance.source);
    }
    // A zero-length stroke is a traced anchor: the projection's placeholder
    // for an object it does not engrave, carrying provenance and no ink.
    for s in &layout.strokes {
        if s.from != s.to {
            inked.insert(s.provenance.source);
        }
    }
    for c in &layout.curves {
        inked.insert(c.provenance.source);
    }
    // The notes a beam joins, named among its dependencies.
    let beamed: BTreeSet<TypedObjectId> = layout
        .strokes
        .iter()
        .filter(|s| is_beam_stroke(s))
        .flat_map(|s| s.provenance.dependencies.iter().copied())
        .collect();
    let names = |source: TypedObjectId| glyphs.get(&source).cloned().unwrap_or_default();

    let instances: Vec<&epiphany_core::StaffInstance> = score
        .canvas
        .regions
        .iter()
        .flat_map(|r| r.staff_instances())
        .collect();

    // The length of a measure under each meter, by the meter's onset.
    let mut meter_lengths: Vec<(RationalTime, RationalTime)> = score
        .canvas
        .regions
        .iter()
        .filter_map(|r| r.content.staff_based())
        .filter_map(|c| c.default_metric_grid.as_ref())
        .flat_map(|g| g.meter_sequence.iter())
        .filter_map(|m| {
            let signature = score
                .time_signatures
                .iter()
                .find(|t| t.id == m.time_signature)?;
            Some((offset(&m.anchor)?, signature.measure_duration().0.clone()))
        })
        .collect();
    meter_lengths.sort();

    // A tuplet member's written value is its sounding span scaled by the
    // ratio of every tuplet holding it, its own and their parents'.
    let tuplet_of: BTreeMap<epiphany_core::TupletId, &epiphany_core::Tuplet> = score
        .cross_cutting
        .tuplets
        .iter()
        .map(|t| (t.id, t))
        .collect();
    let mut written_scale: BTreeMap<epiphany_core::EventId, RationalTime> = BTreeMap::new();
    for tuplet in &score.cross_cutting.tuplets {
        let mut scale = RationalTime::from_int(1);
        let mut next = Some(tuplet);
        let mut seen = BTreeSet::new();
        while let Some(t) = next.filter(|t| seen.insert(t.id)) {
            let ratio =
                RationalTime::new(i64::from(t.ratio.actual()), i64::from(t.ratio.notated()))
                    .expect("a tuplet ratio has no zero term");
            scale = scale.mul(&ratio);
            next = t.parent.and_then(|p| tuplet_of.get(&p).copied());
        }
        for member in &tuplet.members {
            written_scale.insert(*member, scale.clone());
        }
    }

    // Events, by what their durations and contents call for.
    for instance in &instances {
        let mut measures: Vec<RationalTime> = instance
            .measures
            .iter()
            .filter_map(|m| offset(&m.start))
            .collect();
        measures.sort();
        // The measure holding an onset: its start, its length, and whether
        // it is a full bar. A first measure shorter than its bar is a pickup,
        // whose rests keep their values.
        let measure_span = |onset: &RationalTime| -> Option<(RationalTime, RationalTime, bool)> {
            let i = measures.partition_point(|m| m <= onset).checked_sub(1)?;
            let start = measures[i].clone();
            let bar = meter_lengths
                .partition_point(|(o, _)| o <= &start)
                .checked_sub(1)
                .map(|k| meter_lengths[k].1.clone());
            let length = match measures.get(i + 1) {
                Some(next) => next.sub(&start),
                None => bar.clone()?,
            };
            let pickup = i == 0 && bar.as_ref().is_some_and(|bar| &length < bar);
            Some((start, length, !pickup))
        };
        for voice in &instance.voices {
            for id in &voice.events {
                let Some(event) = score.events.get(*id) else {
                    continue;
                };
                let (EventPosition::Musical(onset), EventDuration::Musical(duration)) =
                    (event.position(), event.duration())
                else {
                    out.skip("event with a non-metric time");
                    continue;
                };
                let own = names(TypedObjectId::Event(*id));
                let mut sources = vec![TypedObjectId::Event(*id)];
                match event {
                    Event::Pitched(p) => {
                        sources.extend(p.pitches.iter().map(|ip| TypedObjectId::Pitch(ip.id)))
                    }
                    Event::Rest(r) if !r.visible => continue,
                    _ => {}
                }
                if !sources.iter().any(|s| inked.contains(s)) {
                    out.add(match event {
                        Event::Pitched(_) => "note not drawn",
                        Event::Rest(_) => "rest not drawn",
                        Event::Unpitched(_) => "unpitched note not drawn",
                        _ => "event not drawn",
                    });
                    continue;
                }
                let whole_measure = measure_span(&onset.0).is_some_and(|(start, length, full)| {
                    full && start == onset.0 && length == duration.0
                });
                let value = notated(&match written_scale.get(id) {
                    Some(scale) => duration.0.mul(scale),
                    None => duration.0.clone(),
                });
                match event {
                    Event::Rest(_) => {
                        let drawn: Vec<&str> = own
                            .iter()
                            .copied()
                            .filter(|g| g.starts_with("rest"))
                            .collect();
                        let expected = if whole_measure {
                            Some(String::from("restWhole"))
                        } else {
                            value.map(|(exp, _)| rest_for(exp))
                        };
                        match expected {
                            None => out.skip("rest: duration not one notated value"),
                            Some(glyph) if !drawn.contains(&glyph.as_str()) => {
                                out.add("rest drawn at another value")
                            }
                            Some(_) => {}
                        }
                        if let Some((_, dots)) = value {
                            if !whole_measure && dots > 0 && !own.contains(&"augmentationDot") {
                                out.add("augmentation dot (rest)");
                            }
                        }
                    }
                    Event::Pitched(p) => {
                        for _ in &p.pitches {
                            out.skip("accidental: not checked against the key and the measure");
                        }
                        let Some((exp, dots)) = value else {
                            out.skip("note: duration not one notated value");
                            continue;
                        };
                        for ip in &p.pitches {
                            let heads: Vec<&str> = names(TypedObjectId::Pitch(ip.id))
                                .into_iter()
                                .filter(|g| g.starts_with("notehead"))
                                .collect();
                            if heads.is_empty() {
                                out.add("notehead not drawn");
                            } else if !heads.contains(&notehead_for(exp)) {
                                out.add("notehead of another value");
                            }
                        }
                        if exp <= -3
                            && !own.iter().any(|g| g.starts_with("flag"))
                            && !beamed.contains(&TypedObjectId::Event(*id))
                        {
                            out.add("flag or beam");
                        }
                        let dotted = own.contains(&"augmentationDot")
                            || p.pitches.iter().any(|ip| {
                                names(TypedObjectId::Pitch(ip.id)).contains(&"augmentationDot")
                            });
                        if dots > 0 && !dotted {
                            out.add("augmentation dot");
                        }
                    }
                    Event::Unpitched(_) => out.skip("unpitched note: value not checked"),
                    _ => {}
                }
            }
        }
    }

    for tie in &score.cross_cutting.ties {
        if !inked.contains(&TypedObjectId::Tie(tie.id)) {
            out.add("tie not drawn");
        }
    }
    for slur in &score.cross_cutting.slurs {
        if !inked.contains(&TypedObjectId::Slur(slur.id)) {
            out.add("slur not drawn");
        }
    }

    // Clefs and key signatures where systems start, and clef changes. A
    // system starts where its earliest measure does; what it should show is
    // the clef and key in effect there.
    let instance_of: BTreeMap<StaffId, &epiphany_core::StaffInstance> =
        instances.iter().map(|i| (i.staff, *i)).collect();
    let measure_start: BTreeMap<epiphany_core::MeasureId, RationalTime> = instances
        .iter()
        .flat_map(|i| i.measures.iter())
        .filter_map(|m| Some((m.id, offset(&m.start)?)))
        .collect();
    let mut clefs_drawn: BTreeMap<StaffInstanceId, usize> = BTreeMap::new();
    let mut systems_with: BTreeMap<StaffInstanceId, usize> = BTreeMap::new();
    for system in layout.systems() {
        let owned: Vec<&epiphany_layout_ir::ResolvedGlyph> = system
            .primitives
            .glyphs
            .iter()
            .filter_map(|&i| layout.glyphs.get(i as usize))
            .collect();
        let start = system
            .measures
            .iter()
            .filter_map(|m| measure_start.get(&m.measure))
            .min()
            .cloned()
            .unwrap_or_else(RationalTime::zero);
        for staff in &system.staves {
            let Some(instance) = instance_of.get(&staff.staff) else {
                continue;
            };
            let source = TypedObjectId::StaffInstance(instance.id);
            let mine: Vec<&str> = owned
                .iter()
                .filter(|g| g.provenance.source == source)
                .map(|g| g.glyph.as_str())
                .collect();
            // The system's clefs from the left, its lead's first.
            let mut placed: Vec<(f32, &str)> = owned
                .iter()
                .filter(|g| g.provenance.source == source && g.glyph.as_str().contains("Clef"))
                .map(|g| (g.position.x.0, g.glyph.as_str()))
                .collect();
            placed.sort_by(|a, b| a.0.total_cmp(&b.0));
            let clefs: Vec<&str> = placed.into_iter().map(|(_, name)| name).collect();
            *clefs_drawn.entry(instance.id).or_default() += clefs.len();
            *systems_with.entry(instance.id).or_default() += 1;
            let in_effect = |anchors: Vec<(RationalTime, usize)>| {
                anchors
                    .into_iter()
                    .filter(|(o, _)| o <= &start)
                    .max()
                    .map(|(_, i)| i)
            };
            let clef = in_effect(
                instance
                    .clef_sequence
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| Some((offset(&c.anchor)?, i)))
                    .collect(),
            )
            .map(|i| instance.clef_sequence[i].clef);
            if let Some(clef) = clef {
                match clefs.first() {
                    None => out.add("clef at a system start"),
                    Some(drawn) => {
                        let shape = match clef.shape {
                            epiphany_core::ClefShape::G => "gClef",
                            epiphany_core::ClefShape::F => "fClef",
                            epiphany_core::ClefShape::C => "cClef",
                            epiphany_core::ClefShape::Percussion => "ercussionClef",
                        };
                        if !drawn.contains(shape) {
                            out.add("clef of another shape at a system start");
                        } else if clef.octave_shift != 0
                            && epiphany_layout_ir::clef_glyph_for(&clef) != Some(*drawn)
                        {
                            out.add("clef octave mark");
                        }
                    }
                }
            }
            let fifths = in_effect(
                instance
                    .key_sequence
                    .iter()
                    .enumerate()
                    .filter_map(|(i, k)| Some((offset(&k.anchor)?, i)))
                    .collect(),
            )
            .map_or(0, |i| instance.key_sequence[i].key.fifths());
            if fifths != 0 && !mine.iter().any(|g| g.starts_with("accidental")) {
                out.add("key signature at a system start");
            }
        }
    }
    for instance in &instances {
        // A change is a clef other than the one in force before it.
        let mut sequence: Vec<(RationalTime, epiphany_core::Clef)> = instance
            .clef_sequence
            .iter()
            .filter_map(|c| Some((offset(&c.anchor)?, c.clef)))
            .collect();
        sequence.sort_by(|a, b| a.0.cmp(&b.0));
        let changes = sequence
            .windows(2)
            .filter(|w| w[1].0 != RationalTime::zero() && w[1].1 != w[0].1)
            .count();
        let drawn = clefs_drawn.get(&instance.id).copied().unwrap_or(0);
        let at_starts = systems_with.get(&instance.id).copied().unwrap_or(0);
        let for_changes = drawn.saturating_sub(at_starts.min(drawn));
        for _ in for_changes..changes {
            out.add("clef change");
        }
        let key_changes = instance
            .key_sequence
            .iter()
            .filter(|k| offset(&k.anchor).is_some_and(|o| o != RationalTime::zero()))
            .count();
        for _ in 0..key_changes {
            out.skip("key change: not checked");
        }
    }

    // Time signatures where meters take effect.
    let meter_onsets: BTreeSet<RationalTime> = score
        .canvas
        .regions
        .iter()
        .filter_map(|r| r.content.staff_based())
        .filter_map(|c| c.default_metric_grid.as_ref())
        .flat_map(|g| g.meter_sequence.iter().filter_map(|m| offset(&m.anchor)))
        .collect();
    for instance in &instances {
        for measure in &instance.measures {
            if offset(&measure.start).is_some_and(|o| meter_onsets.contains(&o))
                && !names(TypedObjectId::Measure(measure.id))
                    .iter()
                    .any(|g| g.starts_with("timeSig"))
            {
                out.add("time signature");
            }
        }
    }

    // Ink no system owns is drawn only on a layout of one page, which
    // `page` keeps it on; on several, no page shows it.
    if layout.pages.len() > 1 {
        let unowned = &layout.unowned;
        let strokes = unowned
            .strokes
            .iter()
            .filter_map(|&i| layout.strokes.get(i as usize))
            .filter(|s| s.from != s.to)
            .count();
        for _ in 0..unowned.glyphs.len() + strokes + unowned.curves.len() {
            out.add("ink on no page");
        }
    }

    for diagnostic in diagnostics {
        let kind = match &diagnostic.kind {
            LayoutDiagnosticKind::MissingSpelling => String::from("diagnostic: missing spelling"),
            LayoutDiagnosticKind::UnbundledGlyph(glyph) => {
                format!("diagnostic: unbundled glyph {}", glyph.as_str())
            }
        };
        out.add(&kind);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: i64, d: i64) -> RationalTime {
        RationalTime::new(n, d).expect("a rational")
    }

    #[test]
    fn notated_values_name_their_base_and_dots() {
        assert_eq!(notated(&t(1, 1)), Some((0, 0)));
        assert_eq!(notated(&t(2, 1)), Some((1, 0)));
        assert_eq!(notated(&t(1, 4)), Some((-2, 0)));
        assert_eq!(notated(&t(3, 8)), Some((-2, 1)));
        assert_eq!(notated(&t(3, 4)), Some((-1, 1)));
        assert_eq!(notated(&t(7, 16)), Some((-2, 2)));
        assert_eq!(notated(&t(3, 32)), Some((-4, 1)));
        assert_eq!(notated(&t(5, 8)), None, "needs a tie");
        assert_eq!(notated(&t(1, 12)), None, "needs a tuplet");
    }

    #[test]
    fn rest_glyphs_follow_the_smufl_names() {
        assert_eq!(rest_for(0), "restWhole");
        assert_eq!(rest_for(-2), "restQuarter");
        assert_eq!(rest_for(-3), "rest8th");
        assert_eq!(rest_for(-5), "rest32nd");
    }
}

//! A faithful **stub** of `epiphany-layout-ir` (Agent E, Chapter 7 "Layout
//! Intermediate Representation" and Chapter 9 "Constraint-Solver Interface").
//!
//! Agent E has not landed; per the QUICKSTART, Agent F *"builds against A and
//! stubs for the others."* This module is that stub and drives v0 acceptance
//! criterion 6 (Layout round-trip):
//!
//! > A score graph → LogicalLayoutIR → stub-solved ResolvedLayoutIR → RenderIR
//! > interface call completes without panic and without losing provenance
//! > back-references.
//!
//! It is a minimal but spec-faithful model: the four IR stages
//! ([`LogicalLayoutIR`] → [`ConstrainedLayoutIR`] → [`ResolvedLayoutIR`] →
//! [`RenderIR`]), the [`TimeAxisModel`] tagged enum (Metric / Proportional /
//! Aleatoric / Registered — *not* a trait object, Chapter 7), the
//! [`Provenance`] back-references that every layout object MUST carry
//! (Chapter 7 §"Provenance"), the [`GlyphCatalogIdentity`] (Chapter 7 §7.3.2),
//! and the stub constraint solver ([`StubSolver`]) that returns
//! [`SolveStatus::Solved`] with the input geometry **verbatim** (the QUICKSTART
//! direction: *"the stub returns `SolveStatus::Solved` with the input geometry
//! verbatim"*). Quality metrics are not implemented — only the interface, as the
//! spec requires. Resolved positions are quantized to the Appendix D
//! `1/1024` staff-space grid via [`QuantizedCoord`].
//!
//! When `epiphany-layout-ir` lands, [`round_trip`] re-points at the real IR
//! types; the provenance-preservation contract it asserts is the one the real
//! crate must also satisfy.

use std::collections::BTreeSet;

use epiphany_core::{Region, RegionTimeModel, Score, TypedObjectId};
use epiphany_determinism::{blake3_256, trunc128, DomainTag, Preimage, QuantizedCoord};

use crate::rng::Rng;

/// A layout object's stable identifier across re-layouts where its source is
/// unchanged (Chapter 7 §"Provenance"). Carried unchanged through every stage.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LayoutObjectId(pub u128);

/// A generated layout-object identifier value.
pub fn gen_layout_object_id(rng: &mut Rng) -> LayoutObjectId {
    LayoutObjectId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

/// Derives the stable layout id of an object from its score-graph **source**
/// alone — never from its traversal position. This is what makes the id stable
/// across relayouts (Chapter 7 §"Provenance": stable across re-layouts where the
/// source is unchanged): inserting, removing, or reordering other objects cannot
/// change any object's stable id, because each depends solely on its own source.
pub fn stable_layout_id(source: &TypedObjectId) -> LayoutObjectId {
    LayoutObjectId(trunc128(&blake3_256(&source.canonical_bytes())))
}

/// Why an IR object exists without a direct score-graph source (Chapter 7
/// §"Provenance"). Engraver-synthesized objects MUST declare one. The variant set
/// mirrors the spec's normative `SynthesisKind`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SynthesisKind {
    CancellationAccidental,
    KeySignatureNatural,
    GeneratedRest,
    EngravedBreak,
    MultimeasureRest,
    Cautionary,
    /// Extension-defined synthesis kind, identified by a registry id.
    Registered(u128),
}

/// The provenance back-reference every layout object carries (Chapter 7
/// §"Provenance"). This is what makes incremental layout possible and what the
/// round-trip harness proves is preserved end to end.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Provenance {
    /// The score-graph object this IR object represents or derives from.
    pub source: TypedObjectId,
    /// For engraver-synthesized objects, the synthesis kind; `None` for objects
    /// with a direct score-graph source.
    pub synthesis: Option<SynthesisKind>,
    /// Every score-graph object whose change should invalidate this layout
    /// object (the incremental-layout dependency set).
    pub dependencies: Vec<TypedObjectId>,
    /// Stable across re-layouts where the source is unchanged.
    pub stable_id: LayoutObjectId,
}

/// The per-region time axis, a tagged enum over the four kinds (Chapter 7): the
/// enum form is canonical, *not* `Box<dyn TimeAxis>`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TimeAxisModel {
    Metric,
    Proportional {
        duration_ns: i64,
    },
    Aleatoric,
    /// Extension-defined axis kind, identified by a registry id.
    Registered(u128),
}

/// Glyph catalog identity for layout conformance (Chapter 7 §7.3.2): the font
/// and metric data the solve consumed. Stubbed to a fixed Bravura-like identity.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphCatalogIdentity {
    pub font_id: &'static str,
    pub smufl_version: (u16, u16),
    pub metrics_hash: [u8; 32],
}

impl Default for GlyphCatalogIdentity {
    fn default() -> Self {
        GlyphCatalogIdentity {
            font_id: "Bravura",
            smufl_version: (1, 4),
            metrics_hash: stub_metrics_hash(),
        }
    }
}

/// A small representative stub of the per-glyph SMuFL metrics a real solve would
/// consult: `(glyph name, advance width, bounding box [l, b, r, t])` in
/// `1/1024`-staff-space units. Agent E bundles the real Bravura metrics in-tree;
/// this stands in until then.
const STUB_GLYPH_METRICS: &[(&str, i32, [i32; 4])] = &[
    ("noteheadBlack", 1180, [0, -512, 1180, 512]),
    ("noteheadHalf", 1180, [0, -512, 1180, 512]),
    ("gClef", 2684, [0, -2048, 2600, 4660]),
    ("fClef", 2776, [0, -1024, 2776, 1024]),
    ("accidentalSharp", 994, [0, -1392, 994, 1392]),
    ("accidentalFlat", 821, [0, -703, 821, 1751]),
    ("restQuarter", 1024, [0, -1536, 1024, 1536]),
];

/// The catalog metrics identity: a **domain-tagged** (`MUSCFNTM`) BLAKE3 hash
/// over the canonical serialization of the (stub) glyph metrics actually
/// available to the solve (Chapter 7 §7.3.2 / Appendix D §"Domain-Separated
/// Preimages"), rather than a raw hash of a descriptive string.
pub fn stub_metrics_hash() -> [u8; 32] {
    stub_metrics_hash_for(STUB_GLYPH_METRICS.iter().map(|metric| metric.0))
}

fn stub_metrics_hash_for<'a>(names: impl IntoIterator<Item = &'a str>) -> [u8; 32] {
    let names: BTreeSet<&str> = names.into_iter().collect();
    let mut p = Preimage::new(DomainTag::FONT_METRICS);
    p.push_u64_le(names.len() as u64);
    for name in names {
        let (_, advance, bbox) = STUB_GLYPH_METRICS
            .iter()
            .find(|metric| metric.0 == name)
            .expect("every constrained glyph must name bundled stub metrics");
        p.push_u64_le(name.len() as u64);
        p.push_bytes(name.as_bytes());
        p.push_u64_le(*advance as u64);
        for coord in bbox {
            p.push_u64_le(*coord as u64);
        }
    }
    *p.finish().as_bytes()
}

/// A canonical 2-D point on the `1/1024` staff-space grid (Appendix D
/// §"Quantized Layout Coordinates").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Point {
    pub x: QuantizedCoord,
    pub y: QuantizedCoord,
}

// --- Stage 1: LogicalLayoutIR ----------------------------------------------

/// A structural layout object before spacing — engraving decisions notionally
/// made, positions unresolved (Chapter 7 §7.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LayoutObject {
    pub provenance: Provenance,
}

/// A region projected into layout space, carrying its time axis (Chapter 7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LayoutRegion {
    pub provenance: Provenance,
    pub time_axis: TimeAxisModel,
    pub objects: Vec<LayoutObject>,
}

/// The logical IR: the structural projection of the score graph (Chapter 7 §7.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogicalLayoutIR {
    pub regions: Vec<LayoutRegion>,
}

// --- Stage 2: ConstrainedLayoutIR ------------------------------------------

/// A glyph with a baseline anchor and bounding extent, the input to the solver
/// (Chapter 7 §7.2). The `baseline` is the geometry the stub solver returns
/// verbatim.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphObject {
    pub provenance: Provenance,
    /// The SMuFL glyph whose metrics the solver consults.
    pub glyph_name: &'static str,
    pub baseline: Point,
}

/// The constrained IR: composite objects flattened to glyphs (Chapter 7 §7.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConstrainedLayoutIR {
    pub glyphs: Vec<GlyphObject>,
    pub catalog: GlyphCatalogIdentity,
}

// --- Stage 3: ResolvedLayoutIR ---------------------------------------------

/// A glyph with a definitive, quantized position (Chapter 7 §7.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedGlyph {
    pub provenance: Provenance,
    pub position: Point,
}

/// The resolved IR: every glyph positioned (Chapter 7 §7.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResolvedLayoutIR {
    pub glyphs: Vec<ResolvedGlyph>,
    pub catalog: GlyphCatalogIdentity,
}

// --- Stage 4: RenderIR (interface only) ------------------------------------

/// A single renderer primitive (Chapter 7 §7.4). Interface only — no actual
/// rendering. Every primitive is traceable to its source (Chapter 7: *"every
/// renderer primitive MUST be traceable to its originating ResolvedGlyph"*).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RenderPrimitive {
    pub provenance: Provenance,
    pub position: Point,
}

/// The render IR interface output (Chapter 7 §7.4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RenderIR {
    pub primitives: Vec<RenderPrimitive>,
}

// --- Chapter 9: the constraint-solver interface ----------------------------

/// The solver status (Chapter 9 §9.1). Variants quoted from the spec.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SolveStatus {
    /// All hard constraints satisfied, target quality reached.
    Solved,
    /// All hard constraints satisfied, but warnings were generated.
    SolvedWithWarnings,
    /// Deterministic budget exhausted before reaching target quality.
    PartialBudgetExhausted,
    /// Hard constraints cannot be simultaneously satisfied.
    Unsatisfiable,
    /// Solver bug or unexpected error.
    InternalError,
}

/// The solver report (Chapter 9 §9.1). The `layout` is always present; its
/// authority depends on `status`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SolveReport {
    pub status: SolveStatus,
    pub satisfied_hard_constraints: bool,
    pub layout: ResolvedLayoutIR,
}

/// The constraint-solver interface (Chapter 9 §9.1), reduced to the single
/// from-scratch entry point the round-trip needs.
pub trait ConstraintSolver {
    fn solve(&self, input: &ConstrainedLayoutIR) -> SolveReport;
}

/// The v0 stub solver (QUICKSTART, Agent E: *"the stub returns
/// `SolveStatus::Solved` with the input geometry verbatim"*). It copies each
/// glyph's baseline anchor into its resolved position unchanged, preserves
/// provenance, and reports all hard constraints satisfied.
pub struct StubSolver;

impl ConstraintSolver for StubSolver {
    fn solve(&self, input: &ConstrainedLayoutIR) -> SolveReport {
        let metrics_available = input
            .glyphs
            .iter()
            .all(|glyph| STUB_GLYPH_METRICS.iter().any(|m| m.0 == glyph.glyph_name));
        let expected_hash = stub_metrics_hash_for(input.glyphs.iter().map(|g| g.glyph_name));
        let catalog_matches = input.catalog.metrics_hash == expected_hash;
        let glyphs = input
            .glyphs
            .iter()
            .map(|g| ResolvedGlyph {
                provenance: g.provenance.clone(),
                position: g.baseline, // input geometry, verbatim
            })
            .collect();
        SolveReport {
            status: if metrics_available && catalog_matches {
                SolveStatus::Solved
            } else {
                SolveStatus::InternalError
            },
            satisfied_hard_constraints: metrics_available && catalog_matches,
            layout: ResolvedLayoutIR {
                glyphs,
                catalog: input.catalog.clone(),
            },
        }
    }
}

// --- The pipeline ----------------------------------------------------------

/// Projects a score graph into [`LogicalLayoutIR`]. Every layout object carries
/// a [`Provenance`] whose `source` is the score-graph object it represents, with
/// dependency back-references for incremental layout. One [`LayoutRegion`] per
/// score region carries that region's [`TimeAxisModel`].
pub fn to_logical(score: &Score) -> LogicalLayoutIR {
    // Stable ids are a pure function of the source identifier (not traversal
    // position), so inserting or reordering objects never changes another
    // object's stable id across relayouts.
    let stable = stable_layout_id;

    // Staves and instruments live at score scope; project them into the first
    // region (or a region-less bucket if there are none) as cross-cutting
    // objects. For the round-trip we attach every non-region object to the
    // region(s) so provenance flows through the region pipeline.
    let mut regions = Vec::new();
    for region in &score.canvas.regions {
        let mut objects = Vec::new();

        // Staves manifested in this region (via the staff extent).
        for staff_id in &region.staff_extent.staves {
            let source = TypedObjectId::Staff(*staff_id);
            objects.push(LayoutObject {
                provenance: Provenance {
                    source,
                    synthesis: None,
                    dependencies: vec![],
                    stable_id: stable(&source),
                },
            });
        }

        // Staff instances, voices, and their events + pitches.
        for si in region.staff_instances() {
            let si_src = TypedObjectId::StaffInstance(si.id);
            objects.push(LayoutObject {
                provenance: Provenance {
                    source: si_src,
                    synthesis: None,
                    dependencies: vec![TypedObjectId::Staff(si.staff)],
                    stable_id: stable(&si_src),
                },
            });
            for voice in &si.voices {
                let v_src = TypedObjectId::Voice(voice.id);
                objects.push(LayoutObject {
                    provenance: Provenance {
                        source: v_src,
                        synthesis: None,
                        dependencies: vec![si_src],
                        stable_id: stable(&v_src),
                    },
                });
                for eid in &voice.events {
                    let e_src = TypedObjectId::Event(*eid);
                    // Event's pitches become its invalidation dependencies.
                    let mut deps = vec![v_src];
                    if let Some(event) = score.events.get(*eid) {
                        let mut buf = Vec::new();
                        event.collect_identified_pitches(&mut buf);
                        for p in &buf {
                            deps.push(TypedObjectId::Pitch(p.id));
                        }
                    }
                    objects.push(LayoutObject {
                        provenance: Provenance {
                            source: e_src,
                            synthesis: None,
                            dependencies: deps,
                            stable_id: stable(&e_src),
                        },
                    });
                    // And the pitches themselves as their own objects.
                    if let Some(event) = score.events.get(*eid) {
                        let mut buf = Vec::new();
                        event.collect_identified_pitches(&mut buf);
                        for p in &buf {
                            let p_src = TypedObjectId::Pitch(p.id);
                            objects.push(LayoutObject {
                                provenance: Provenance {
                                    source: p_src,
                                    synthesis: None,
                                    dependencies: vec![e_src],
                                    stable_id: stable(&p_src),
                                },
                            });
                        }
                    }
                }
            }
        }

        // Measures, per staff instance (Chapter 5 §"Measures").
        for si in region.staff_instances() {
            for measure in &si.measures {
                let m_src = TypedObjectId::Measure(measure.id);
                objects.push(LayoutObject {
                    provenance: Provenance {
                        source: m_src,
                        synthesis: None,
                        dependencies: vec![TypedObjectId::StaffInstance(si.id)],
                        stable_id: stable(&m_src),
                    },
                });
            }
        }

        let r_src = TypedObjectId::Region(region.id);
        regions.push(LayoutRegion {
            provenance: Provenance {
                source: r_src,
                synthesis: None,
                dependencies: region
                    .staff_extent
                    .staves
                    .iter()
                    .map(|s| TypedObjectId::Staff(*s))
                    .collect(),
                stable_id: stable(&r_src),
            },
            time_axis: time_axis_of(region),
            objects,
        });
    }

    // Score-level cross-cutting structures (ties, slurs, beams, tuplets,
    // spanners, markers, chord symbols). They are score-wide, so they flow
    // through the first region's object list; their `source` ids are recovered by
    // [`laid_out_object_ids`] all the same.
    if let Some(first) = regions.first_mut() {
        let cc = &score.cross_cutting;
        let mut push = |src: TypedObjectId, deps: Vec<TypedObjectId>| {
            first.objects.push(LayoutObject {
                provenance: Provenance {
                    source: src,
                    synthesis: None,
                    dependencies: deps,
                    stable_id: stable(&src),
                },
            });
        };
        for t in &cc.ties {
            push(
                TypedObjectId::Tie(t.id),
                vec![
                    TypedObjectId::Event(t.start_event),
                    TypedObjectId::Event(t.end_event),
                ],
            );
        }
        for s in &cc.slurs {
            push(
                TypedObjectId::Slur(s.id),
                vec![
                    TypedObjectId::Event(s.start_event),
                    TypedObjectId::Event(s.end_event),
                ],
            );
        }
        for b in &cc.beams {
            push(TypedObjectId::Beam(b.id), vec![]);
        }
        for tu in &cc.tuplets {
            push(
                TypedObjectId::Tuplet(tu.id),
                tu.members
                    .iter()
                    .map(|e| TypedObjectId::Event(*e))
                    .collect(),
            );
        }
        for sp in &cc.spanners {
            push(TypedObjectId::Spanner(sp.id), vec![]);
        }
        for mk in &cc.markers {
            push(TypedObjectId::Marker(mk.id), vec![]);
        }
        for ch in &cc.chord_symbols {
            push(TypedObjectId::ChordSymbol(ch.id), vec![]);
        }
    }

    LogicalLayoutIR { regions }
}

/// Maps a score region's time model to the layout [`TimeAxisModel`] (Chapter 7).
fn time_axis_of(region: &Region) -> TimeAxisModel {
    match &region.time_model {
        RegionTimeModel::Metric(_) => TimeAxisModel::Metric,
        RegionTimeModel::Proportional(p) => TimeAxisModel::Proportional {
            duration_ns: p.duration.0,
        },
        RegionTimeModel::Aleatoric(_) => TimeAxisModel::Aleatoric,
    }
}

/// Flattens [`LogicalLayoutIR`] into [`ConstrainedLayoutIR`]: one glyph per
/// layout object (including the region object itself), each with a baseline
/// anchor laid out left-to-right on the `1/1024` grid. Provenance is preserved.
pub fn to_constrained(logical: &LogicalLayoutIR) -> ConstrainedLayoutIR {
    let mut glyphs = Vec::new();
    let mut column: i64 = 0;
    for region in &logical.regions {
        // The region's own object first, then its contents.
        glyphs.push(GlyphObject {
            provenance: region.provenance.clone(),
            glyph_name: glyph_name_for(&region.provenance.source),
            baseline: Point {
                x: QuantizedCoord::from_units(column * 1024),
                y: QuantizedCoord::from_units(0),
            },
        });
        column += 1;
        for object in &region.objects {
            glyphs.push(GlyphObject {
                provenance: object.provenance.clone(),
                glyph_name: glyph_name_for(&object.provenance.source),
                baseline: Point {
                    x: QuantizedCoord::from_units(column * 1024),
                    y: QuantizedCoord::from_units(0),
                },
            });
            column += 1;
        }
    }
    let metrics_hash = stub_metrics_hash_for(glyphs.iter().map(|glyph| glyph.glyph_name));
    ConstrainedLayoutIR {
        glyphs,
        catalog: GlyphCatalogIdentity {
            metrics_hash,
            ..GlyphCatalogIdentity::default()
        },
    }
}

fn glyph_name_for(source: &TypedObjectId) -> &'static str {
    STUB_GLYPH_METRICS[source.discriminant() as usize % STUB_GLYPH_METRICS.len()].0
}

/// The RenderIR interface call (Chapter 7 §7.4): one primitive per resolved
/// glyph, provenance and position preserved. Interface only.
pub fn to_render(resolved: &ResolvedLayoutIR) -> RenderIR {
    RenderIR {
        primitives: resolved
            .glyphs
            .iter()
            .map(|g| RenderPrimitive {
                provenance: g.provenance.clone(),
                position: g.position,
            })
            .collect(),
    }
}

/// The set of score-graph objects this stub lays out (the objects whose
/// [`TypedObjectId`]s the round-trip expects to recover from the RenderIR).
pub fn laid_out_object_ids(score: &Score) -> BTreeSet<TypedObjectId> {
    let mut ids = BTreeSet::new();
    for region in &score.canvas.regions {
        ids.insert(TypedObjectId::Region(region.id));
        for staff_id in &region.staff_extent.staves {
            ids.insert(TypedObjectId::Staff(*staff_id));
        }
        for si in region.staff_instances() {
            ids.insert(TypedObjectId::StaffInstance(si.id));
            for voice in &si.voices {
                ids.insert(TypedObjectId::Voice(voice.id));
                for eid in &voice.events {
                    ids.insert(TypedObjectId::Event(*eid));
                    if let Some(event) = score.events.get(*eid) {
                        let mut buf = Vec::new();
                        event.collect_identified_pitches(&mut buf);
                        for p in &buf {
                            ids.insert(TypedObjectId::Pitch(p.id));
                        }
                    }
                }
            }
            for measure in &si.measures {
                ids.insert(TypedObjectId::Measure(measure.id));
            }
        }
    }
    // Score-level cross-cutting structures.
    let cc = &score.cross_cutting;
    ids.extend(cc.ties.iter().map(|t| TypedObjectId::Tie(t.id)));
    ids.extend(cc.slurs.iter().map(|s| TypedObjectId::Slur(s.id)));
    ids.extend(cc.beams.iter().map(|b| TypedObjectId::Beam(b.id)));
    ids.extend(cc.tuplets.iter().map(|t| TypedObjectId::Tuplet(t.id)));
    ids.extend(cc.spanners.iter().map(|s| TypedObjectId::Spanner(s.id)));
    ids.extend(cc.markers.iter().map(|m| TypedObjectId::Marker(m.id)));
    ids.extend(
        cc.chord_symbols
            .iter()
            .map(|c| TypedObjectId::ChordSymbol(c.id)),
    );
    ids
}

/// What the round-trip recovered, for inspection by tests.
#[derive(Clone, Debug)]
pub struct RoundTripReport {
    pub status: SolveStatus,
    pub logical_objects: usize,
    pub glyphs: usize,
    pub render_primitives: usize,
    /// Every score-graph source recovered from the RenderIR.
    pub recovered_sources: BTreeSet<TypedObjectId>,
}

/// Collects `(stable_id -> Provenance)` for a stage's objects, asserting no two
/// objects share a stable id (which would let set comparisons hide duplication).
fn provenance_map<'a>(
    label: &str,
    provenances: impl Iterator<Item = &'a Provenance>,
) -> std::collections::BTreeMap<LayoutObjectId, Provenance> {
    let mut map = std::collections::BTreeMap::new();
    for p in provenances {
        let prev = map.insert(p.stable_id, p.clone());
        assert!(
            prev.is_none(),
            "{label}: duplicate stable id {:?} (provenance duplication)",
            p.stable_id
        );
    }
    map
}

/// Runs the full pipeline (acceptance criterion 6): graph → LogicalLayoutIR →
/// ConstrainedLayoutIR → stub-solved ResolvedLayoutIR → RenderIR, asserting that
/// it completes without panic and **without losing provenance back-references**.
/// Specifically:
///
/// * the stub solver returns [`SolveStatus::Solved`] with all hard constraints
///   satisfied;
/// * the **complete** [`Provenance`] of every object — `source`, `synthesis`,
///   `dependencies`, and `stable_id` — survives every stage unchanged (compared
///   as `stable_id -> Provenance` maps, so a dropped/corrupted dependency or
///   synthesis kind fails, not just a changed id);
/// * no two objects ever share a `stable_id` (duplication cannot hide);
/// * the set of sources recovered from the RenderIR equals the set the stub laid
///   out — a bijection back to graph identity.
pub fn round_trip(score: &Score) -> RoundTripReport {
    let logical = to_logical(score);
    let constrained = to_constrained(&logical);

    // Full-provenance maps at each stage (duplication is caught while building).
    let logical_map = provenance_map(
        "logical",
        logical.regions.iter().flat_map(|r| {
            std::iter::once(&r.provenance).chain(r.objects.iter().map(|o| &o.provenance))
        }),
    );
    let constrained_map = provenance_map(
        "constrained",
        constrained.glyphs.iter().map(|g| &g.provenance),
    );
    assert_eq!(
        logical_map, constrained_map,
        "provenance not preserved logical -> constrained"
    );

    let report = StubSolver.solve(&constrained);
    assert_eq!(
        report.status,
        SolveStatus::Solved,
        "the stub solver must report Solved"
    );
    assert!(
        report.satisfied_hard_constraints,
        "the stub solver must satisfy all hard constraints"
    );

    // The stub solver's geometry contract: it returns the input geometry
    // *verbatim* — each resolved glyph's position is exactly its constrained
    // baseline (Chapter 9 / QUICKSTART: "the input geometry verbatim"). A future
    // solver that altered geometry would fail this. The solver preserves order,
    // so glyphs line up by index.
    assert_eq!(
        report.layout.glyphs.len(),
        constrained.glyphs.len(),
        "the solver must not add or drop glyphs"
    );
    for (constrained_glyph, resolved_glyph) in constrained.glyphs.iter().zip(&report.layout.glyphs)
    {
        assert_eq!(
            resolved_glyph.position, constrained_glyph.baseline,
            "stub solver must return the input geometry verbatim"
        );
    }

    let resolved_map = provenance_map(
        "resolved",
        report.layout.glyphs.iter().map(|g| &g.provenance),
    );
    assert_eq!(
        constrained_map, resolved_map,
        "provenance not preserved constrained -> resolved"
    );

    let render = to_render(&report.layout);
    let render_map = provenance_map("render", render.primitives.iter().map(|p| &p.provenance));
    assert_eq!(
        resolved_map, render_map,
        "provenance not preserved resolved -> render"
    );

    // Provenance back to graph identity: the recovered sources are exactly the
    // set the stub laid out (a bijection), and the primitive count matches the
    // distinct-stable-id count (no duplication, no loss).
    let expected = laid_out_object_ids(score);
    let recovered: BTreeSet<TypedObjectId> = render
        .primitives
        .iter()
        .map(|p| p.provenance.source)
        .collect();
    assert_eq!(
        expected, recovered,
        "RenderIR provenance does not bijection back to the laid-out graph objects"
    );
    assert_eq!(
        render.primitives.len(),
        render_map.len(),
        "render produced duplicate objects"
    );

    RoundTripReport {
        status: report.status,
        logical_objects: logical.regions.iter().map(|r| 1 + r.objects.len()).sum(),
        glyphs: constrained.glyphs.len(),
        render_primitives: render.primitives.len(),
        recovered_sources: recovered,
    }
}

// --- Generators for the stub IR types (Agent E's surface, generated here) ---

/// A synthesis kind (every variant, including the registered form).
pub fn gen_synthesis_kind(rng: &mut Rng) -> SynthesisKind {
    match rng.below(7) {
        0 => SynthesisKind::CancellationAccidental,
        1 => SynthesisKind::KeySignatureNatural,
        2 => SynthesisKind::GeneratedRest,
        3 => SynthesisKind::EngravedBreak,
        4 => SynthesisKind::MultimeasureRest,
        5 => SynthesisKind::Cautionary,
        _ => SynthesisKind::Registered(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128),
    }
}

/// A time-axis model (every variant of the tagged enum).
pub fn gen_time_axis_model(rng: &mut Rng) -> TimeAxisModel {
    match rng.below(4) {
        0 => TimeAxisModel::Metric,
        1 => TimeAxisModel::Proportional {
            duration_ns: rng.range(0, 1 << 40) as i64,
        },
        2 => TimeAxisModel::Aleatoric,
        _ => TimeAxisModel::Registered(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128),
    }
}

/// A provenance record with a random source, optional synthesis kind, a few
/// dependency back-references, and a source-derived [`stable_layout_id`].
pub fn gen_provenance(rng: &mut Rng) -> Provenance {
    let source = crate::generators::typed_object_id(rng);
    let mut dependencies = Vec::new();
    for _ in 0..rng.range_usize(0, 3) {
        dependencies.push(crate::generators::typed_object_id(rng));
    }
    Provenance {
        stable_id: stable_layout_id(&source),
        source,
        synthesis: if rng.boolean() {
            Some(gen_synthesis_kind(rng))
        } else {
            None
        },
        dependencies,
    }
}

/// A solver status (every variant).
pub fn gen_solve_status(rng: &mut Rng) -> SolveStatus {
    *rng.choose(&[
        SolveStatus::Solved,
        SolveStatus::SolvedWithWarnings,
        SolveStatus::PartialBudgetExhausted,
        SolveStatus::Unsatisfiable,
        SolveStatus::InternalError,
    ])
}

/// A glyph-catalog identity (the stub Bravura-like identity).
pub fn gen_glyph_catalog_identity(_rng: &mut Rng) -> GlyphCatalogIdentity {
    GlyphCatalogIdentity::default()
}

/// A canonical quantized 2-D point.
pub fn gen_point(rng: &mut Rng) -> Point {
    Point {
        x: QuantizedCoord::from_units(rng.next_u64() as i64),
        y: QuantizedCoord::from_units(rng.next_u64() as i64),
    }
}

/// A logical layout object (carrying a generated [`Provenance`]).
pub fn gen_layout_object(rng: &mut Rng) -> LayoutObject {
    LayoutObject {
        provenance: gen_provenance(rng),
    }
}

/// A constrained glyph object (provenance + baseline anchor).
pub fn gen_glyph_object(rng: &mut Rng) -> GlyphObject {
    let glyph_name = STUB_GLYPH_METRICS[rng.below(STUB_GLYPH_METRICS.len() as u64) as usize].0;
    GlyphObject {
        provenance: gen_provenance(rng),
        glyph_name,
        baseline: gen_point(rng),
    }
}

/// A resolved glyph (provenance + definitive position).
pub fn gen_resolved_glyph(rng: &mut Rng) -> ResolvedGlyph {
    ResolvedGlyph {
        provenance: gen_provenance(rng),
        position: gen_point(rng),
    }
}

/// A logical layout region (provenance + time axis + a few objects).
pub fn gen_layout_region(rng: &mut Rng) -> LayoutRegion {
    let n = rng.range_usize(0, 4);
    LayoutRegion {
        provenance: gen_provenance(rng),
        time_axis: gen_time_axis_model(rng),
        objects: (0..n).map(|_| gen_layout_object(rng)).collect(),
    }
}

/// A logical layout IR (a few regions).
pub fn gen_logical_layout_ir(rng: &mut Rng) -> LogicalLayoutIR {
    let n = rng.range_usize(0, 3);
    LogicalLayoutIR {
        regions: (0..n).map(|_| gen_layout_region(rng)).collect(),
    }
}

/// A constrained layout IR whose catalog hash covers exactly its glyph metrics.
pub fn gen_constrained_layout_ir(rng: &mut Rng) -> ConstrainedLayoutIR {
    let n = rng.range_usize(0, 8);
    let glyphs: Vec<_> = (0..n).map(|_| gen_glyph_object(rng)).collect();
    let metrics_hash = stub_metrics_hash_for(glyphs.iter().map(|glyph| glyph.glyph_name));
    ConstrainedLayoutIR {
        glyphs,
        catalog: GlyphCatalogIdentity {
            metrics_hash,
            ..GlyphCatalogIdentity::default()
        },
    }
}

/// A resolved layout IR produced by the real stub-solver interface.
pub fn gen_resolved_layout_ir(rng: &mut Rng) -> ResolvedLayoutIR {
    StubSolver.solve(&gen_constrained_layout_ir(rng)).layout
}

/// A render primitive with generated provenance and geometry.
pub fn gen_render_primitive(rng: &mut Rng) -> RenderPrimitive {
    RenderPrimitive {
        provenance: gen_provenance(rng),
        position: gen_point(rng),
    }
}

/// A RenderIR generated through the resolved-to-render projection.
pub fn gen_render_ir(rng: &mut Rng) -> RenderIR {
    to_render(&gen_resolved_layout_ir(rng))
}

/// A complete solver report generated through the solver interface.
pub fn gen_solve_report(rng: &mut Rng) -> SolveReport {
    StubSolver.solve(&gen_constrained_layout_ir(rng))
}

/// A self-consistent round-trip report value.
pub fn gen_round_trip_report(rng: &mut Rng) -> RoundTripReport {
    let render = gen_render_ir(rng);
    let recovered_sources = render
        .primitives
        .iter()
        .map(|primitive| primitive.provenance.source)
        .collect();
    RoundTripReport {
        status: SolveStatus::Solved,
        logical_objects: render.primitives.len(),
        glyphs: render.primitives.len(),
        render_primitives: render.primitives.len(),
        recovered_sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixtures, generators};
    use epiphany_core::TypedObjectId;

    #[test]
    fn metrics_hash_is_domain_tagged_and_nonzero() {
        // Not an all-zero placeholder, and domain-separated (a different domain
        // over the same bytes would differ).
        let h = stub_metrics_hash();
        assert_ne!(h, [0u8; 32]);
        assert_eq!(GlyphCatalogIdentity::default().metrics_hash, h);

        let mut rng = Rng::new(8);
        let input = gen_constrained_layout_ir(&mut rng);
        assert_eq!(StubSolver.solve(&input).status, SolveStatus::Solved);
        let mut wrong_catalog = input;
        wrong_catalog.catalog.metrics_hash[0] ^= 1;
        assert_eq!(
            StubSolver.solve(&wrong_catalog).status,
            SolveStatus::InternalError,
            "solver must reject a catalog hash that does not cover consulted metrics"
        );
    }

    #[test]
    fn ir_generators_are_deterministic_and_well_formed() {
        let mut a = Rng::new(13);
        let mut b = Rng::new(13);
        // Deterministic from the seed.
        assert_eq!(gen_logical_layout_ir(&mut a), gen_logical_layout_ir(&mut b));
        // Provenance stable_id is always the source-derived id.
        let mut rng = Rng::new(99);
        for _ in 0..64 {
            let p = gen_provenance(&mut rng);
            assert_eq!(p.stable_id, stable_layout_id(&p.source));
            let _ = gen_glyph_object(&mut rng);
            let _ = gen_resolved_glyph(&mut rng);
            let _ = gen_time_axis_model(&mut rng);
            let _ = gen_synthesis_kind(&mut rng);
            let _ = gen_solve_status(&mut rng);
            let _ = gen_glyph_catalog_identity(&mut rng);
            let _ = gen_layout_object_id(&mut rng);
            let _ = gen_constrained_layout_ir(&mut rng);
            let _ = gen_resolved_layout_ir(&mut rng);
            let _ = gen_render_primitive(&mut rng);
            let _ = gen_render_ir(&mut rng);
            let _ = gen_solve_report(&mut rng);
            let _ = gen_round_trip_report(&mut rng);
        }
    }

    #[test]
    fn ten_measure_single_staff_round_trips() {
        // The QUICKSTART's headline case for Agent E's hand-off: a real
        // 10-measure single-staff score (with measures and cross-cutting objects).
        let score = fixtures::ten_measure_single_staff(0xA11CE);
        let report = round_trip(&score);
        assert!(report.glyphs > 0);
        assert_eq!(report.glyphs, report.render_primitives);

        // The projection genuinely covers measures and the cross-cutting objects
        // (so their omission could not pass unseen).
        let measures = report
            .recovered_sources
            .iter()
            .filter(|s| matches!(s, TypedObjectId::Measure(_)))
            .count();
        assert_eq!(measures, 10, "all ten measures must be laid out");
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::Tie(_))));
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::Spanner(_))));
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::Marker(_))));
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::ChordSymbol(_))));
    }

    #[test]
    fn rich_and_varied_scores_round_trip() {
        for seed in 0..128u64 {
            round_trip(&fixtures::ten_measure_single_staff(seed));
            round_trip(&generators::graph::valid_score(
                seed.wrapping_mul(0x9E37_79B9),
            ));
            // The rich generator has metric, proportional, and aleatoric regions
            // plus a tuplet, tie, spanner, marker, and chord symbol.
            let rich = generators::graph::valid_score_rich(seed);
            let report = round_trip(&rich);
            assert!(report.glyphs >= 3);
            assert!(report
                .recovered_sources
                .iter()
                .any(|s| matches!(s, TypedObjectId::Tuplet(_))));
        }
    }

    /// Every object's stable id is exactly `stable_layout_id(source)` — a pure
    /// function of the source, with no dependence on traversal position. This is
    /// the property that makes ids stable across relayouts.
    #[test]
    fn stable_ids_are_a_pure_function_of_source() {
        let score = fixtures::ten_measure_single_staff(5);
        let logical = to_logical(&score);
        for region in &logical.regions {
            assert_eq!(
                region.provenance.stable_id,
                stable_layout_id(&region.provenance.source)
            );
            for obj in &region.objects {
                assert_eq!(
                    obj.provenance.stable_id,
                    stable_layout_id(&obj.provenance.source)
                );
            }
        }
    }

    /// Reordering the regions of a score (a relayout where the *sources* are
    /// unchanged) does not change any object's stable id. The pre-fix
    /// traversal-counter scheme would have failed this.
    #[test]
    fn reordering_regions_preserves_stable_ids() {
        let score = generators::graph::valid_score_rich(9);
        let source_to_stable = |s: &Score| {
            to_logical(s)
                .regions
                .iter()
                .flat_map(|r| {
                    std::iter::once((r.provenance.source, r.provenance.stable_id)).chain(
                        r.objects
                            .iter()
                            .map(|o| (o.provenance.source, o.provenance.stable_id)),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let before = source_to_stable(&score);
        let mut reordered = score.clone();
        reordered.canvas.regions.reverse();
        let after = source_to_stable(&reordered);
        assert_eq!(
            before, after,
            "reordering regions changed stable ids (they are not position-independent)"
        );
    }
}

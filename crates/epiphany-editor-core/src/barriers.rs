//! The edit-barrier gate (Chapter 8 §"Forward Compatibility and Edit Barriers"
//! / §"Behavior Under Unknown Extensions").
//!
//! The spec's MUST: *"Edits MUST be checked against every active edit barrier.
//! An edit matching a barrier's scope, affected object kinds, and operation
//! kinds is prohibited unless the user explicitly performs an unsafe edit."*
//! The session holds the active extensions' decoded barriers
//! ([`ActiveExtension`]); before minting an envelope, [`crate::EditorSession`]
//! derives the candidate edit's **subjects** — the objects the operation's
//! payload names, each with its structural containment ([`EditContext`]) — and
//! evaluates every barrier via [`EditBarrier::prohibits_edit`] against a
//! [`ScoreOracle`] over the session's materialized score.
//!
//! Matching policy (documented, deliberate):
//!
//! * **Object-kind matching is on the payload's named targets** (the object an
//!   op mints, tombstones, or overwrites — plus, for a pitch insert, the host
//!   event whose pitch list it mutates). Indirect containers (the voice an
//!   event sits in, say) are *not* treated as edited objects; protecting a
//!   container is what the barrier's **scope** is for, and scope is matched
//!   against the target's real containment, precisely.
//! * **Score-level operations** (`SetMetadata`, the transaction descriptor)
//!   name no graph object: only a score-wide barrier (empty
//!   `affected_object_kinds`, `WholeScore`/`TuningContext`/`Registered` scope)
//!   can match them.
//! * **Extension-defined operations** (`OperationKind::Registered`) carry a
//!   payload the core cannot read, so their targets are unknowable: a barrier
//!   prohibiting that registered kind matches **conservatively** (scope and
//!   object kinds are treated as matching — Chapter 8's "never silently drop a
//!   barrier you cannot evaluate").

use epiphany_core::{
    EventId, PitchId, PitchSpaceId, RegionId, Score, StaffInstanceId, TypedObjectId, VoiceId,
};
use epiphany_layout_ir::{BarrierScope, EditBarrier, EditContext, EditOracle, ExtensionRef};
use epiphany_ops::{OperationKind, OperationKindTag};

/// One active extension declaration's barrier view: the declaring extension
/// (named when its barrier refuses an edit, and recorded for tombstoning when
/// an unsafe edit crosses it) plus its decoded edit barriers.
///
/// The session opens on a bare [`Score`], not a bundle, so it cannot read the
/// manifest itself: whoever opened the bundle decodes each
/// `ExtensionDeclaration.edit_barriers` blob
/// ([`epiphany_layout_ir::decode_edit_barriers`]) and injects the result via
/// [`crate::EditorSession::set_active_extensions`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ActiveExtension {
    /// The declaring extension (the manifest `ExtensionDeclaration`'s
    /// `extension_id`, as the barrier layer's opaque 128-bit reference).
    pub extension: ExtensionRef,
    /// The extension's decoded edit barriers.
    pub barriers: Vec<EditBarrier>,
}

/// What a candidate operation edits, for barrier matching.
pub(crate) enum BarrierSubjects {
    /// The graph objects the payload names, each with its containment.
    Objects(Vec<(TypedObjectId, EditContext)>),
    /// The operation edits score-level state and names no graph object
    /// (`SetMetadata`, a transaction descriptor): only a score-wide barrier
    /// can match.
    ScoreWide,
    /// An extension-defined operation whose payload the core cannot read:
    /// scope and object kinds are treated conservatively as matching.
    Unknown,
}

/// The [`EditOracle`] over the session's materialized score: **known** barrier
/// conditions are evaluated *precisely* against the graph, so a barrier
/// narrowed to `ObjectExists` really deactivates when the object is gone.
///
/// `has_extension_data` is `false` for every object: the v0 materialized score
/// carries no extension payloads on graph objects, so "carries data declared by
/// the extension" is precisely (not conservatively) false.
pub(crate) struct ScoreOracle<'a>(pub &'a Score);

impl EditOracle for ScoreOracle<'_> {
    fn object_exists(&self, object: &TypedObjectId) -> bool {
        let score = self.0;
        match object {
            TypedObjectId::Event(id) => score.events.get(*id).is_some(),
            TypedObjectId::Pitch(id) => score.live_pitch_ids().contains(id),
            TypedObjectId::Voice(id) => score.voices().any(|(_, _, v)| v.id == *id),
            TypedObjectId::Staff(id) => score.staves.iter().any(|s| s.id == *id),
            TypedObjectId::StaffInstance(id) => score.staff_instances().any(|(_, si)| si.id == *id),
            TypedObjectId::StaffGroup(id) => score.staff_groups.iter().any(|g| g.id == *id),
            TypedObjectId::Region(id) => score.canvas.regions.iter().any(|r| r.id == *id),
            TypedObjectId::Instrument(id) => score.instruments.iter().any(|i| i.id == *id),
            TypedObjectId::PartDefinition(id) => score.parts.iter().any(|p| p.id == *id),
            TypedObjectId::BarlineAlignmentGroup(id) => score
                .canvas
                .regions
                .iter()
                .flat_map(|r| r.content.barline_alignment_groups())
                .any(|g| g.id == *id),
            TypedObjectId::Slur(id) => score.cross_cutting.slurs.iter().any(|s| s.id == *id),
            TypedObjectId::Tie(id) => score.cross_cutting.ties.iter().any(|t| t.id == *id),
            TypedObjectId::Beam(id) => score.cross_cutting.beams.iter().any(|b| b.id == *id),
            TypedObjectId::Spanner(id) => score.cross_cutting.spanners.iter().any(|s| s.id == *id),
            TypedObjectId::Tuplet(id) => score.cross_cutting.tuplets.iter().any(|t| t.id == *id),
            TypedObjectId::Marker(id) => score.cross_cutting.markers.iter().any(|m| m.id == *id),
            TypedObjectId::RepeatStructure(id) => {
                score.cross_cutting.repeats.iter().any(|r| r.id == *id)
            }
            TypedObjectId::AnalyticalAnnotation(id) => {
                score.cross_cutting.analytical.iter().any(|a| a.id == *id)
            }
            TypedObjectId::Comment(id) => score.cross_cutting.comments.iter().any(|c| c.id == *id),
            TypedObjectId::GraphicGesture(id) => score
                .cross_cutting
                .graphic_gestures
                .iter()
                .any(|g| g.id == *id),
            TypedObjectId::LyricLine(id) => score.cross_cutting.lyrics.iter().any(|l| l.id == *id),
            TypedObjectId::ChordSymbol(id) => score
                .cross_cutting
                .chord_symbols
                .iter()
                .any(|c| c.id == *id),
            TypedObjectId::GraphicObject(id) => score
                .canvas
                .regions
                .iter()
                .flat_map(|r| r.content.graphic_objects())
                .any(|g| g.id == *id),
            TypedObjectId::TimeSignature(id) => score.time_signatures.iter().any(|t| t.id == *id),
            TypedObjectId::AnalysisLayer(id) => score.analysis_layers.iter().any(|l| l.id == *id),
            TypedObjectId::View(id) => score.views.iter().any(|v| v.id == *id),
            // The v0 graph hosts no measure objects (measures are derived) and
            // no extension-registered objects, so these precisely do not exist.
            TypedObjectId::Measure(_) | TypedObjectId::Registered(..) => false,
        }
    }

    fn has_extension_data(&self, _object: &TypedObjectId, _extension: ExtensionRef) -> bool {
        false
    }
}

fn ctx(region: Option<RegionId>, staff_instance: Option<StaffInstanceId>) -> EditContext {
    EditContext {
        region,
        staff_instance,
        analysis_layer: None,
        pitch_space: None,
    }
}

/// The region a staff instance manifests in, if it is in the graph.
fn region_of_staff_instance(score: &Score, instance: StaffInstanceId) -> Option<RegionId> {
    score
        .staff_instances()
        .find(|(_, si)| si.id == instance)
        .map(|(region, _)| region)
}

/// The (region, staff instance) a voice lives in, if it is in the graph.
fn voice_location(score: &Score, voice: VoiceId) -> Option<(RegionId, StaffInstanceId)> {
    score
        .voices()
        .find(|(_, _, v)| v.id == voice)
        .map(|(region, si, _)| (region, si))
}

/// The (region, staff instance) an event lives in, via the voice listing it.
fn event_location(score: &Score, event: EventId) -> Option<(RegionId, StaffInstanceId)> {
    score
        .voices()
        .find(|(_, _, v)| v.events.contains(&event))
        .map(|(region, si, _)| (region, si))
}

/// The containment and pitch space of a live pitch, via the event embedding it.
fn pitch_context(score: &Score, pitch: PitchId) -> EditContext {
    let mut buf: Vec<&epiphany_core::IdentifiedPitch> = Vec::new();
    for event in score.events.iter() {
        buf.clear();
        event.collect_identified_pitches(&mut buf);
        if let Some(ip) = buf.iter().find(|ip| ip.id == pitch) {
            let location = event_location(score, event.id());
            return EditContext {
                region: location.map(|(r, _)| r),
                staff_instance: location.map(|(_, si)| si),
                analysis_layer: None,
                pitch_space: Some(ip.pitch.scale_position.space.clone()),
            };
        }
    }
    EditContext::default()
}

/// A context for a pitch value the operation itself carries (an insert's new
/// pitch), whose space is read from the value rather than the graph.
fn carried_pitch_context(
    location: Option<(RegionId, StaffInstanceId)>,
    space: PitchSpaceId,
) -> EditContext {
    EditContext {
        region: location.map(|(r, _)| r),
        staff_instance: location.map(|(_, si)| si),
        analysis_layer: None,
        pitch_space: Some(space),
    }
}

/// The context of a cross-cutting structure: the containment of its first
/// locatable endpoint event (a slur/tie/beam/spanner lives where its anchors
/// live), or the default context when none is locatable.
fn structure_context(score: &Score, endpoints: &[TypedObjectId]) -> EditContext {
    let location = endpoints.iter().find_map(|endpoint| match endpoint {
        TypedObjectId::Event(id) => event_location(score, *id),
        _ => None,
    });
    ctx(location.map(|(r, _)| r), location.map(|(_, si)| si))
}

/// The containment of a live measure: the staff instance whose measure list
/// carries it, with its region.
fn measure_location(
    score: &Score,
    measure: epiphany_core::MeasureId,
) -> Option<(RegionId, StaffInstanceId)> {
    score.canvas.regions.iter().find_map(|region| {
        region.staff_instances().iter().find_map(|si| {
            si.measures
                .iter()
                .any(|m| m.id == measure)
                .then_some((region.id, si.id))
        })
    })
}

/// The containment context a repeat structure's anchor sites bind: the first
/// object-referencing site (in [`epiphany_core::RepeatStructure::anchor_sites`]
/// order — start, end, jump targets, volta spans) that resolves. An event or
/// measure site binds its (region, staff instance); a bare region anchor
/// binds the region alone. Region- and measure-anchored repeats must derive
/// real containment here — an event-only walk would let a repeat anchored
/// solely to a protected region bypass a region-scoped barrier.
fn repeat_context(score: &Score, repeat: &epiphany_core::RepeatStructure) -> EditContext {
    for site in repeat.anchor_sites() {
        match site {
            epiphany_core::TimeAnchor::Event { id, .. } => {
                if let Some((region, instance)) = event_location(score, *id) {
                    return ctx(Some(region), Some(instance));
                }
            }
            epiphany_core::TimeAnchor::Measure { id, .. } => {
                if let Some((region, instance)) = measure_location(score, *id) {
                    return ctx(Some(region), Some(instance));
                }
            }
            epiphany_core::TimeAnchor::Region { id, .. } => return ctx(Some(*id), None),
            epiphany_core::TimeAnchor::WallClock { .. } => {}
        }
    }
    ctx(None, None)
}

/// The endpoints of a cross-cutting structure already in the graph, by id.
fn graph_structure_endpoints(score: &Score, structure: &TypedObjectId) -> Vec<TypedObjectId> {
    let events = |ids: Vec<EventId>| ids.into_iter().map(TypedObjectId::Event).collect();
    match structure {
        TypedObjectId::Slur(id) => score
            .cross_cutting
            .slurs
            .iter()
            .find(|s| s.id == *id)
            .map(|s| events(vec![s.start_event, s.end_event]))
            .unwrap_or_default(),
        TypedObjectId::Tie(id) => score
            .cross_cutting
            .ties
            .iter()
            .find(|t| t.id == *id)
            .map(|t| events(vec![t.start_event, t.end_event]))
            .unwrap_or_default(),
        TypedObjectId::Beam(id) => score
            .cross_cutting
            .beams
            .iter()
            .find(|b| b.id == *id)
            .map(|b| events(b.events.clone()))
            .unwrap_or_default(),
        TypedObjectId::Spanner(id) => score
            .cross_cutting
            .spanners
            .iter()
            .find(|s| s.id == *id)
            .map(|s| {
                [&s.start, &s.end]
                    .into_iter()
                    .filter_map(|anchor| match anchor {
                        epiphany_core::TimeAnchor::Event { id, .. } => {
                            Some(TypedObjectId::Event(*id))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Derives the barrier subjects of `kind`: the objects its payload names, each
/// with its containment in `score` (see the module docs for the matching
/// policy). Containment that cannot be resolved (the target is not in the
/// graph — reduction's invariant preconditions own that case) yields a context
/// with the unresolved fields `None`.
pub(crate) fn subjects_of(kind: &OperationKind, score: &Score) -> BarrierSubjects {
    let one = |object: TypedObjectId, context: EditContext| {
        BarrierSubjects::Objects(vec![(object, context)])
    };
    match kind {
        OperationKind::InsertEvent(op) => {
            let region = region_of_staff_instance(score, op.staff_instance);
            one(
                TypedObjectId::Event(op.event_id()),
                ctx(region, Some(op.staff_instance)),
            )
        }
        OperationKind::DeleteEvent(op) => {
            let location = event_location(score, op.event);
            one(
                TypedObjectId::Event(op.event),
                ctx(location.map(|(r, _)| r), location.map(|(_, si)| si)),
            )
        }
        OperationKind::ModifyEvent(op) => {
            let location = event_location(score, op.event_id());
            one(
                TypedObjectId::Event(op.event_id()),
                ctx(location.map(|(r, _)| r), location.map(|(_, si)| si)),
            )
        }
        OperationKind::RespellPitch(op) => one(
            TypedObjectId::Pitch(op.pitch),
            pitch_context(score, op.pitch),
        ),
        OperationKind::Transpose(op) => BarrierSubjects::Objects(
            op.targets
                .iter()
                .map(|pitch| (TypedObjectId::Pitch(*pitch), pitch_context(score, *pitch)))
                .collect(),
        ),
        OperationKind::InsertIdentifiedPitch(op) => {
            // The op mutates the host event's pitch list *and* mints the pitch:
            // both are named targets.
            let location = event_location(score, op.event);
            let event_ctx = ctx(location.map(|(r, _)| r), location.map(|(_, si)| si));
            let pitch_ctx =
                carried_pitch_context(location, op.pitch.pitch.scale_position.space.clone());
            BarrierSubjects::Objects(vec![
                (TypedObjectId::Event(op.event), event_ctx),
                (TypedObjectId::Pitch(op.pitch_id()), pitch_ctx),
            ])
        }
        OperationKind::DeleteIdentifiedPitch(op) => one(
            TypedObjectId::Pitch(op.pitch),
            pitch_context(score, op.pitch),
        ),
        OperationKind::ModifyIdentifiedPitch(op) => one(
            TypedObjectId::Pitch(op.pitch),
            pitch_context(score, op.pitch),
        ),
        OperationKind::CreateCrossCutting(op) => one(
            op.structure.id(),
            structure_context(score, &op.structure.endpoints()),
        ),
        OperationKind::ModifyCrossCutting(op) => one(
            op.structure.id(),
            structure_context(score, &op.structure.endpoints()),
        ),
        OperationKind::DeleteCrossCutting(op) => one(
            op.structure,
            structure_context(score, &graph_structure_endpoints(score, &op.structure)),
        ),
        OperationKind::ChangeRegionTimeModel(op) => {
            one(TypedObjectId::Region(op.region), ctx(Some(op.region), None))
        }
        OperationKind::SetUserSystemBreak(op) => {
            one(TypedObjectId::Region(op.region), ctx(Some(op.region), None))
        }
        OperationKind::SetUserPageBreak(op) => {
            one(TypedObjectId::Region(op.region), ctx(Some(op.region), None))
        }
        OperationKind::SetMetricGrid(op) => {
            one(TypedObjectId::Region(op.region), ctx(Some(op.region), None))
        }
        OperationKind::CreateRegion(op) => one(
            TypedObjectId::Region(op.region_id()),
            ctx(Some(op.region_id()), None),
        ),
        OperationKind::DeleteRegion(op) => {
            one(TypedObjectId::Region(op.region), ctx(Some(op.region), None))
        }
        OperationKind::CreateStaffInstance(op) => one(
            TypedObjectId::StaffInstance(op.instance_id()),
            ctx(Some(op.region), Some(op.instance_id())),
        ),
        OperationKind::DeleteStaffInstance(op) => one(
            TypedObjectId::StaffInstance(op.staff_instance),
            ctx(
                region_of_staff_instance(score, op.staff_instance),
                Some(op.staff_instance),
            ),
        ),
        OperationKind::CreateVoice(op) => one(
            TypedObjectId::Voice(op.voice_id()),
            ctx(
                region_of_staff_instance(score, op.staff_instance),
                Some(op.staff_instance),
            ),
        ),
        OperationKind::DeleteVoice(op) => {
            let location = voice_location(score, op.voice);
            one(
                TypedObjectId::Voice(op.voice),
                ctx(location.map(|(r, _)| r), location.map(|(_, si)| si)),
            )
        }
        // Phase-3 first tranche. A staff is a global (score-root) object with
        // no regional containment; a meter overwrite edits its region's grid
        // slot and names the carried signature it mints; a tempo overwrite
        // names its region scope (the score-level map is score-wide state); a
        // layout overwrite names its staff instance.
        OperationKind::CreateStaff(op) => {
            one(TypedObjectId::Staff(op.staff_id()), EditContext::default())
        }
        OperationKind::SetTimeSignature(op) => {
            let mut objects = vec![(TypedObjectId::Region(op.region), ctx(Some(op.region), None))];
            if let Some(signature) = &op.time_signature {
                objects.push((
                    TypedObjectId::TimeSignature(signature.id),
                    ctx(Some(op.region), None),
                ));
            }
            BarrierSubjects::Objects(objects)
        }
        OperationKind::SetTempoSegment(op) => match op.region {
            Some(region) => one(TypedObjectId::Region(region), ctx(Some(region), None)),
            None => BarrierSubjects::ScoreWide,
        },
        OperationKind::SetStaffLayout(op) => one(
            TypedObjectId::StaffInstance(op.staff_instance),
            ctx(
                region_of_staff_instance(score, op.staff_instance),
                Some(op.staff_instance),
            ),
        ),
        OperationKind::SetMetadata(_) | OperationKind::DeclareTransaction(_) => {
            BarrierSubjects::ScoreWide
        }
        OperationKind::CreateRepeatStructure(op) => one(
            TypedObjectId::RepeatStructure(op.repeat_structure_id()),
            repeat_context(score, &op.repeat),
        ),
        OperationKind::DeleteRepeatStructure(op) => {
            let context = score
                .cross_cutting
                .repeats
                .iter()
                .find(|r| r.id == op.repeat)
                .map(|r| repeat_context(score, r))
                .unwrap_or_else(|| ctx(None, None));
            one(TypedObjectId::RepeatStructure(op.repeat), context)
        }
        OperationKind::Registered(..) => BarrierSubjects::Unknown,
    }
}

/// Every active extension with a barrier prohibiting the candidate edit, in
/// declaration order, deduplicated. Empty means the edit is permitted.
pub(crate) fn prohibiting_extensions(
    extensions: &[ActiveExtension],
    tag: OperationKindTag,
    subjects: &BarrierSubjects,
    oracle: &dyn EditOracle,
) -> Vec<ExtensionRef> {
    let mut crossed = Vec::new();
    for ext in extensions {
        let hit = ext.barriers.iter().any(|barrier| match subjects {
            BarrierSubjects::Objects(objects) => objects
                .iter()
                .any(|(object, context)| barrier.prohibits_edit(tag, object, context, oracle)),
            BarrierSubjects::ScoreWide => {
                barrier.prohibited_operation_kinds.contains(&tag)
                    && barrier.affected_object_kinds.is_empty()
                    && matches!(
                        barrier.scope,
                        BarrierScope::WholeScore
                            | BarrierScope::TuningContext
                            | BarrierScope::Registered(_)
                    )
                    && barrier.condition.is_active(oracle)
            }
            BarrierSubjects::Unknown => {
                barrier.prohibited_operation_kinds.contains(&tag)
                    && barrier.condition.is_active(oracle)
            }
        });
        if hit && !crossed.contains(&ext.extension) {
            crossed.push(ext.extension);
        }
    }
    crossed
}

//! v0 → v1 operation-payload migration (QUICKSTART Agent K, "v0 → v1 payload
//! migration", *option 2*: a one-time migration, not parallel dialects forever).
//!
//! A v0 envelope carried identifier-only payloads; a v1 envelope carries the
//! real value-typed payloads ([`crate::payload`]). [`migrate_v0_envelope`] lifts
//! a v0 envelope to v1, **using the score graph as context** to reconstruct the
//! values the v0 projection dropped. It is:
//!
//! * **Deterministic** — the same `(v0, context)` always yields byte-identical
//!   v1, and
//! * **Equivalence-preserving** — a v0 envelope and its v1 migration reduce to
//!   byte-identical canonical [`MaterializedState`](crate::MaterializedState).
//!
//! [`project_v1_to_v0`] is the total inverse direction (v1 → the v0 wire shape),
//! used to drive the regression guard: `reduce(migrate(project(v1))) ==
//! reduce(v1)` proves the migration loses no reduction-relevant content. See
//! `crates/epiphany-testkit` (the migration harness).
//!
//! ## Irreversible case (P12-K1)
//!
//! A v0 `RespellPitch` carried only a [`ContentHash`] *fingerprint* of the new
//! spelling, not the spelling itself. Migration recovers the [`PitchSpelling`]
//! from the context — an explicit per-pitch spelling attachment whose canonical
//! bytes hash to the fingerprint. If the context does not contain it (a fresh
//! context that never materialized the respell), the spelling cannot be
//! reconstructed and migration returns [`MigrationError::Irreversible`]; the
//! quickstart's contract is that such a bundle opens read-only. This is the one
//! representative payload that is not self-contained, recorded as Pass-12
//! candidate P12-K1.

use epiphany_core::{
    CanonicalValue, PitchSpelling, ReplicaId, Score, SpellingDirective, SpellingScope, VoiceId,
};
use epiphany_determinism::ContentHash;

use crate::payload::{
    ChangeRegionTimeModelOp, CreateCrossCuttingOp, CrossCuttingValue, DeleteEventOp, InsertEventOp,
    OperationKind, OperationPayload, PositionRemapping, RespellPitchOp, SetUserSystemBreakOp,
    TupletCompensation,
};
use crate::v0::{
    V0CreateCrossCuttingOp, V0DeleteEventOp, V0InsertEventOp, V0OperationEnvelope, V0OperationKind,
    V0OperationPayload, V0PositionRemapping, V0RegionTimeModelTag, V0RespellPitchOp,
    V0SetUserSystemBreakOp, V0TupletCompensation,
};
use crate::{valuegen, OperationEnvelope};

/// Why a v0 envelope could not be migrated to v1.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MigrationError {
    /// A value the v1 payload requires cannot be reconstructed from the v0
    /// projection plus the provided context (P12-K1). The bundle opens
    /// read-only.
    Irreversible(&'static str),
}

impl core::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MigrationError::Irreversible(what) => {
                write!(f, "v0 envelope is not migratable: {what}")
            }
        }
    }
}

impl std::error::Error for MigrationError {}

/// The deterministic fingerprint a v0 `RespellPitch` carried for a spelling.
fn spelling_fingerprint(spelling: &PitchSpelling) -> ContentHash {
    ContentHash::of_blob(&spelling.canonical_bytes())
}

// ===========================================================================
// Projection: v1 → v0 (total).
// ===========================================================================

/// Projects a v1 envelope down to its v0 wire shape (dropping the value content
/// to identifiers, scalars, and a spelling fingerprint). Total — every v1
/// envelope has a v0 projection. The inverse of [`migrate_v0_envelope`] on the
/// reduction-relevant content.
pub fn project_v1_to_v0(env: &OperationEnvelope) -> V0OperationEnvelope {
    V0OperationEnvelope {
        id: env.id,
        author: env.author,
        stamp: env.stamp,
        causal_context: env.causal_context.clone(),
        transaction: env.transaction,
        payload: project_payload(&env.payload),
    }
}

fn project_payload(p: &OperationPayload) -> V0OperationPayload {
    match p {
        OperationPayload::Primitive(kind) => V0OperationPayload::Primitive(project_kind(kind)),
        OperationPayload::ResolveConflict(rc) => V0OperationPayload::ResolveConflict(*rc),
        OperationPayload::UndoTransaction(u) => V0OperationPayload::UndoTransaction(*u),
        // v1-native (no v0 predecessor): projected verbatim, like Group 1–4.
        OperationPayload::ResolveEquivocation(re) => V0OperationPayload::ResolveEquivocation(*re),
    }
}

fn project_kind(kind: &OperationKind) -> V0OperationKind {
    match kind {
        OperationKind::InsertEvent(op) => V0OperationKind::InsertEvent(V0InsertEventOp {
            voice: op.voice(),
            staff_instance: op.staff_instance,
            event: op.event_id(),
            position: op.musical_position(),
            duration: op.musical_duration(),
            pitches: op.pitch_ids(),
        }),
        OperationKind::DeleteEvent(op) => V0OperationKind::DeleteEvent(V0DeleteEventOp {
            event: op.event,
            tuplet_compensation: project_tuplet(&op.tuplet_compensation),
        }),
        OperationKind::RespellPitch(op) => V0OperationKind::RespellPitch(V0RespellPitchOp {
            pitch: op.pitch,
            spelling: spelling_fingerprint(&op.spelling),
        }),
        OperationKind::CreateCrossCutting(op) => {
            V0OperationKind::CreateCrossCutting(V0CreateCrossCuttingOp {
                id: op.structure.id(),
                endpoints: op.structure.endpoints(),
            })
        }
        OperationKind::ChangeRegionTimeModel(op) => {
            V0OperationKind::ChangeRegionTimeModel(crate::v0::V0ChangeRegionTimeModelOp {
                region: op.region,
                new_time_model: project_time_model(&op.new_time_model),
                declared_incompatible: op.declared_incompatible.clone(),
                remapping: project_remapping(&op.remapping),
            })
        }
        OperationKind::SetUserSystemBreak(op) => {
            V0OperationKind::SetUserSystemBreak(V0SetUserSystemBreakOp {
                region: op.region,
                anchor: op.resolved_position(),
                present: op.present,
            })
        }
        OperationKind::DeclareTransaction(desc) => {
            V0OperationKind::DeclareTransaction(desc.clone())
        }
        OperationKind::Registered(id, bytes) => V0OperationKind::Registered(*id, bytes.clone()),
        // v1-native (Group 1): no lossy v0 form — projected verbatim.
        OperationKind::ModifyEvent(op) => V0OperationKind::ModifyEvent(op.clone()),
        OperationKind::Transpose(op) => V0OperationKind::Transpose(op.clone()),
        OperationKind::InsertIdentifiedPitch(op) => {
            V0OperationKind::InsertIdentifiedPitch(op.clone())
        }
        OperationKind::DeleteIdentifiedPitch(op) => V0OperationKind::DeleteIdentifiedPitch(*op),
        OperationKind::ModifyIdentifiedPitch(op) => {
            V0OperationKind::ModifyIdentifiedPitch(op.clone())
        }
        // v1-native (Group 2): projected verbatim.
        OperationKind::DeleteCrossCutting(op) => V0OperationKind::DeleteCrossCutting(*op),
        OperationKind::ModifyCrossCutting(op) => V0OperationKind::ModifyCrossCutting(op.clone()),
        // v1-native (Group 3): projected verbatim.
        OperationKind::CreateRegion(op) => V0OperationKind::CreateRegion(op.clone()),
        OperationKind::DeleteRegion(op) => V0OperationKind::DeleteRegion(*op),
        OperationKind::CreateStaffInstance(op) => V0OperationKind::CreateStaffInstance(op.clone()),
        OperationKind::DeleteStaffInstance(op) => V0OperationKind::DeleteStaffInstance(*op),
        OperationKind::CreateVoice(op) => V0OperationKind::CreateVoice(op.clone()),
        OperationKind::DeleteVoice(op) => V0OperationKind::DeleteVoice(*op),
        // v1-native (Group 4): projected verbatim.
        OperationKind::SetMetadata(op) => V0OperationKind::SetMetadata(op.clone()),
        OperationKind::SetMetricGrid(op) => V0OperationKind::SetMetricGrid(op.clone()),
        OperationKind::SetUserPageBreak(op) => V0OperationKind::SetUserPageBreak(op.clone()),
    }
}

fn project_tuplet(t: &TupletCompensation) -> V0TupletCompensation {
    match t {
        TupletCompensation::NotInTuplet => V0TupletCompensation::NotInTuplet,
        TupletCompensation::ReplaceWithRest { rest } => V0TupletCompensation::ReplaceWithRest {
            new_rest: rest.id,
            duration: match &rest.duration {
                epiphany_core::EventDuration::Musical(d) => d.clone(),
                _ => epiphany_core::MusicalDuration::zero(),
            },
        },
        TupletCompensation::RewriteTuplets { tuplets } => V0TupletCompensation::RewriteTuplets {
            tuplets: tuplets.clone(),
        },
        TupletCompensation::CascadeDeleteTuplets { tuplets } => {
            V0TupletCompensation::CascadeDeleteTuplets {
                tuplets: tuplets.clone(),
            }
        }
    }
}

fn project_time_model(m: &epiphany_core::RegionTimeModel) -> V0RegionTimeModelTag {
    match m {
        epiphany_core::RegionTimeModel::Metric(_) => V0RegionTimeModelTag::Metric,
        epiphany_core::RegionTimeModel::Proportional(_) => V0RegionTimeModelTag::Proportional,
        epiphany_core::RegionTimeModel::Aleatoric(_) => V0RegionTimeModelTag::Aleatoric,
    }
}

fn project_remapping(r: &PositionRemapping) -> V0PositionRemapping {
    match r {
        PositionRemapping::PreserveTime => V0PositionRemapping::PreserveTime,
        PositionRemapping::Reassign(v) => V0PositionRemapping::Reassign(v.clone()),
    }
}

// ===========================================================================
// Migration: v0 → v1 (uses context to reconstruct values).
// ===========================================================================

/// Lifts a v0 envelope to v1, reconstructing value payloads from `context`.
/// Deterministic and equivalence-preserving (see module docs). Returns
/// [`MigrationError::Irreversible`] for the one representative payload whose
/// value the v0 projection cannot reconstruct without the context (a respell's
/// spelling, P12-K1).
pub fn migrate_v0_envelope(
    v0: V0OperationEnvelope,
    context: &Score,
) -> Result<OperationEnvelope, MigrationError> {
    let payload = migrate_payload(&v0.payload, context)?;
    Ok(v0.rewrap(payload))
}

fn migrate_payload(
    p: &V0OperationPayload,
    context: &Score,
) -> Result<OperationPayload, MigrationError> {
    Ok(match p {
        V0OperationPayload::Primitive(kind) => {
            OperationPayload::Primitive(migrate_kind(kind, context)?)
        }
        V0OperationPayload::ResolveConflict(rc) => OperationPayload::ResolveConflict(*rc),
        V0OperationPayload::UndoTransaction(u) => OperationPayload::UndoTransaction(*u),
        // v1-native: round-trips by identity (deterministic and trivially
        // equivalence-preserving; no context needed).
        V0OperationPayload::ResolveEquivocation(re) => OperationPayload::ResolveEquivocation(*re),
    })
}

fn migrate_kind(kind: &V0OperationKind, context: &Score) -> Result<OperationKind, MigrationError> {
    Ok(match kind {
        V0OperationKind::InsertEvent(op) => OperationKind::InsertEvent(InsertEventOp {
            staff_instance: op.staff_instance,
            // The v0 op carried every reduction-relevant scalar, so the event
            // value is reconstructed self-contained (no context needed).
            event: valuegen::insert_event_value(
                op.event,
                op.voice,
                op.position.clone(),
                op.duration.clone(),
                &op.pitches,
            ),
        }),
        V0OperationKind::DeleteEvent(op) => OperationKind::DeleteEvent(DeleteEventOp {
            event: op.event,
            tuplet_compensation: migrate_tuplet(&op.tuplet_compensation),
        }),
        V0OperationKind::RespellPitch(op) => OperationKind::RespellPitch(RespellPitchOp {
            pitch: op.pitch,
            spelling: recover_spelling(op, context)?,
        }),
        V0OperationKind::CreateCrossCutting(op) => {
            OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                structure: reconstruct_structure(op)?,
            })
        }
        V0OperationKind::ChangeRegionTimeModel(op) => {
            OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
                region: op.region,
                new_time_model: migrate_time_model(op.new_time_model),
                declared_incompatible: op.declared_incompatible.clone(),
                remapping: migrate_remapping(&op.remapping),
            })
        }
        V0OperationKind::SetUserSystemBreak(op) => {
            OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                region: op.region,
                anchor: valuegen::region_start_anchor(op.region, op.anchor.clone()),
                present: op.present,
            })
        }
        V0OperationKind::DeclareTransaction(desc) => {
            OperationKind::DeclareTransaction(desc.clone())
        }
        V0OperationKind::Registered(id, bytes) => OperationKind::Registered(*id, bytes.clone()),
        // v1-native (Group 1): identity round-trip (no lossy reconstruction).
        V0OperationKind::ModifyEvent(op) => OperationKind::ModifyEvent(op.clone()),
        V0OperationKind::Transpose(op) => OperationKind::Transpose(op.clone()),
        V0OperationKind::InsertIdentifiedPitch(op) => {
            OperationKind::InsertIdentifiedPitch(op.clone())
        }
        V0OperationKind::DeleteIdentifiedPitch(op) => OperationKind::DeleteIdentifiedPitch(*op),
        V0OperationKind::ModifyIdentifiedPitch(op) => {
            OperationKind::ModifyIdentifiedPitch(op.clone())
        }
        // v1-native (Group 2): identity round-trip.
        V0OperationKind::DeleteCrossCutting(op) => OperationKind::DeleteCrossCutting(*op),
        V0OperationKind::ModifyCrossCutting(op) => OperationKind::ModifyCrossCutting(op.clone()),
        // v1-native (Group 3): identity round-trip.
        V0OperationKind::CreateRegion(op) => OperationKind::CreateRegion(op.clone()),
        V0OperationKind::DeleteRegion(op) => OperationKind::DeleteRegion(*op),
        V0OperationKind::CreateStaffInstance(op) => OperationKind::CreateStaffInstance(op.clone()),
        V0OperationKind::DeleteStaffInstance(op) => OperationKind::DeleteStaffInstance(*op),
        V0OperationKind::CreateVoice(op) => OperationKind::CreateVoice(op.clone()),
        V0OperationKind::DeleteVoice(op) => OperationKind::DeleteVoice(*op),
        // v1-native (Group 4): identity round-trip.
        V0OperationKind::SetMetadata(op) => OperationKind::SetMetadata(op.clone()),
        V0OperationKind::SetMetricGrid(op) => OperationKind::SetMetricGrid(op.clone()),
        V0OperationKind::SetUserPageBreak(op) => OperationKind::SetUserPageBreak(op.clone()),
    })
}

fn migrate_tuplet(t: &V0TupletCompensation) -> TupletCompensation {
    match t {
        V0TupletCompensation::NotInTuplet => TupletCompensation::NotInTuplet,
        V0TupletCompensation::ReplaceWithRest { new_rest, duration } => {
            // The v0 op dropped the rest's voice/position; they are recovered
            // from the deleted event's placement at reduction, so a placeholder
            // voice is faithful for the rest value's own field.
            TupletCompensation::ReplaceWithRest {
                rest: valuegen::rest_value(
                    *new_rest,
                    VoiceId::new(ReplicaId(1), 0),
                    duration.clone(),
                ),
            }
        }
        V0TupletCompensation::RewriteTuplets { tuplets } => TupletCompensation::RewriteTuplets {
            tuplets: tuplets.clone(),
        },
        V0TupletCompensation::CascadeDeleteTuplets { tuplets } => {
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: tuplets.clone(),
            }
        }
    }
}

fn migrate_time_model(tag: V0RegionTimeModelTag) -> epiphany_core::RegionTimeModel {
    match tag {
        V0RegionTimeModelTag::Metric => valuegen::metric_model(),
        V0RegionTimeModelTag::Proportional => valuegen::proportional_model(),
        V0RegionTimeModelTag::Aleatoric => valuegen::aleatoric_model(),
    }
}

fn migrate_remapping(r: &V0PositionRemapping) -> PositionRemapping {
    match r {
        V0PositionRemapping::PreserveTime => PositionRemapping::PreserveTime,
        V0PositionRemapping::Reassign(v) => PositionRemapping::Reassign(v.clone()),
    }
}

/// Reconstructs a cross-cutting structure value from its reference-level v0
/// projection. The id's kind selects the structure; the rich per-kind fields
/// (a tie's class, a beam's level) are not recoverable from the reference and
/// are rebuilt with defaults — they do not affect canonical reduction state
/// (only the graph), so equivalence holds.
fn reconstruct_structure(op: &V0CreateCrossCuttingOp) -> Result<CrossCuttingValue, MigrationError> {
    use epiphany_core::TypedObjectId;
    let events: Vec<_> = op
        .endpoints
        .iter()
        .filter_map(|e| match e {
            TypedObjectId::Event(id) => Some(*id),
            _ => None,
        })
        .collect();
    Ok(match op.id {
        TypedObjectId::Slur(id) if events.len() == 2 => {
            CrossCuttingValue::Slur(valuegen::slur(id, events[0], events[1]))
        }
        TypedObjectId::Tie(id) if events.len() == 2 => {
            CrossCuttingValue::Tie(valuegen::tie(id, events[0], events[1]))
        }
        TypedObjectId::Beam(id) if events.len() >= 2 => {
            CrossCuttingValue::Beam(valuegen::beam(id, events))
        }
        _ => {
            return Err(MigrationError::Irreversible(
                "cross-cutting reference does not name a representative event-anchored structure",
            ))
        }
    })
}

/// Recovers a respell's [`PitchSpelling`] from the context: an explicit
/// per-pitch spelling attachment whose canonical bytes hash to the v0
/// fingerprint (P12-K1).
fn recover_spelling(
    op: &V0RespellPitchOp,
    context: &Score,
) -> Result<PitchSpelling, MigrationError> {
    for att in &context.spelling_attachments {
        if let (SpellingScope::Pitch(pitch), SpellingDirective::Explicit(spelling)) =
            (&att.scope, &att.directive)
        {
            if *pitch == op.pitch && spelling_fingerprint(spelling) == op.spelling {
                return Ok(spelling.clone());
            }
        }
    }
    Err(MigrationError::Irreversible(
        "respell spelling fingerprint has no matching explicit spelling in context (P12-K1)",
    ))
}

#[cfg(test)]
mod tests {
    //! Direct coverage of the reconstruction branches the corpus-driven
    //! equivalence gate (`epiphany-testkit::migration`) does not reach: its
    //! generator only emits `NotInTuplet` and a `Slur` create, so the Tie/Beam,
    //! tuplet-compensation, Spanner-irreversible, and respell paths are tested
    //! here against `project_v1_to_v0` + `migrate_v0_envelope` directly.
    use super::*;
    use crate::stamp::HybridLogicalClock;
    use crate::{CausalContext, OperationStamp};
    use epiphany_core::{
        BeamId, EventId, IdentityContext, MusicalDuration, PitchId, ReplicaId, Score, SlurId,
        Spanner, SpannerId, StaffId, TieId, TimeAnchor, VoiceId,
    };

    fn env(payload: OperationPayload) -> OperationEnvelope {
        let id = epiphany_core::OperationId::new(ReplicaId(3), 0);
        OperationEnvelope {
            id,
            author: crate::support::AuthorId(0),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(epiphany_core::WallClockTime(1), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload,
        }
    }

    fn ev(n: u64) -> EventId {
        EventId::new(ReplicaId(3), n)
    }

    /// Round-trip `migrate(project(env))` against an empty context.
    fn round_trip(env: &OperationEnvelope) -> Result<OperationEnvelope, MigrationError> {
        let ctx = Score::empty(IdentityContext::new(ReplicaId(1)));
        migrate_v0_envelope(project_v1_to_v0(env), &ctx)
    }

    fn primitive(kind: OperationKind) -> OperationPayload {
        OperationPayload::Primitive(kind)
    }

    #[test]
    fn tie_and_beam_creates_round_trip_exactly() {
        for kind in [
            CrossCuttingValue::Tie(valuegen::tie(TieId::new(ReplicaId(3), 1), ev(1), ev(2))),
            CrossCuttingValue::Beam(valuegen::beam(
                BeamId::new(ReplicaId(3), 1),
                vec![ev(1), ev(2), ev(3)],
            )),
            CrossCuttingValue::Slur(valuegen::slur(SlurId::new(ReplicaId(3), 1), ev(1), ev(2))),
        ] {
            let e = env(primitive(OperationKind::CreateCrossCutting(
                CreateCrossCuttingOp { structure: kind },
            )));
            assert_eq!(round_trip(&e), Ok(e.clone()));
        }
    }

    #[test]
    fn spanner_create_is_irreversible() {
        // A spanner is anchored by TimeAnchors, not a fixed event pair, so its
        // value is not reconstructable from the v0 event-reference projection
        // (documented in operation_catalog.tex §CreateCrossCutting).
        let spanner = Spanner {
            id: SpannerId::new(ReplicaId(3), 1),
            start: TimeAnchor::Event {
                id: ev(1),
                offset: epiphany_core::AnchorOffset::Zero,
            },
            end: TimeAnchor::Event {
                id: ev(2),
                offset: epiphany_core::AnchorOffset::Zero,
            },
            staves: vec![StaffId::new(ReplicaId(3), 0)],
        };
        let e = env(primitive(OperationKind::CreateCrossCutting(
            CreateCrossCuttingOp {
                structure: CrossCuttingValue::Spanner(spanner),
            },
        )));
        assert!(matches!(
            round_trip(&e),
            Err(MigrationError::Irreversible(_))
        ));
    }

    #[test]
    fn tuplet_compensation_variants_migrate() {
        let voice = VoiceId::new(ReplicaId(3), 0);
        // NotInTuplet, RewriteTuplets, CascadeDeleteTuplets round-trip exactly.
        for comp in [
            TupletCompensation::NotInTuplet,
            TupletCompensation::RewriteTuplets {
                tuplets: vec![epiphany_core::TupletId::new(ReplicaId(3), 1)],
            },
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![epiphany_core::TupletId::new(ReplicaId(3), 2)],
            },
        ] {
            let e = env(primitive(OperationKind::DeleteEvent(DeleteEventOp {
                event: ev(1),
                tuplet_compensation: comp,
            })));
            assert_eq!(round_trip(&e), Ok(e.clone()));
        }

        // ReplaceWithRest preserves the rest's id and duration; its voice is the
        // one v0 dropped (recovered from the deleted event's placement at
        // reduction, not from the rest value), so the round-trip is not exact —
        // it is reduction-faithful (finding #3).
        let dur = MusicalDuration::whole();
        let e = env(primitive(OperationKind::DeleteEvent(DeleteEventOp {
            event: ev(1),
            tuplet_compensation: TupletCompensation::ReplaceWithRest {
                rest: valuegen::rest_value(ev(9), voice, dur.clone()),
            },
        })));
        let migrated = round_trip(&e).expect("ReplaceWithRest migrates");
        let OperationPayload::Primitive(OperationKind::DeleteEvent(op)) = &migrated.payload else {
            panic!("expected DeleteEvent");
        };
        let TupletCompensation::ReplaceWithRest { rest } = &op.tuplet_compensation else {
            panic!("expected ReplaceWithRest");
        };
        assert_eq!(rest.id, ev(9));
        assert_eq!(rest.duration, epiphany_core::EventDuration::Musical(dur));
    }

    #[test]
    fn respell_recovers_from_context_else_irreversible() {
        let pitch = PitchId::new(ReplicaId(3), 1);
        let spelling = valuegen::spelling(42);
        let e = env(primitive(OperationKind::RespellPitch(RespellPitchOp {
            pitch,
            spelling: spelling.clone(),
        })));

        // With the spelling attached to the context, the respell round-trips.
        let mut ctx = Score::empty(IdentityContext::new(ReplicaId(1)));
        ctx.spelling_attachments
            .push(valuegen::explicit_spelling_attachment(pitch, spelling));
        assert_eq!(
            migrate_v0_envelope(project_v1_to_v0(&e), &ctx),
            Ok(e.clone())
        );

        // Without it, the fingerprint cannot be inverted: Irreversible (P12-K1).
        let empty = Score::empty(IdentityContext::new(ReplicaId(1)));
        assert!(matches!(
            migrate_v0_envelope(project_v1_to_v0(&e), &empty),
            Err(MigrationError::Irreversible(_))
        ));
    }

    #[test]
    fn group1_and_group2_kinds_round_trip_by_identity() {
        // The Group-1/2 (M2) kinds are v1-native: they had no lossy v0 form, so
        // project+migrate is the identity (no context needed), and the round-trip
        // is exact for every one of them.
        let voice = VoiceId::new(ReplicaId(3), 0);
        let kinds = [
            OperationKind::ModifyEvent(crate::payload::ModifyEventOp {
                event: valuegen::insert_event_value(
                    ev(1),
                    voice,
                    epiphany_core::MusicalPosition::origin(),
                    MusicalDuration::whole(),
                    &[PitchId::new(ReplicaId(3), 1)],
                ),
            }),
            OperationKind::Transpose(crate::payload::TransposeOp {
                targets: vec![PitchId::new(ReplicaId(3), 1)],
                chromatic_steps: -2,
            }),
            OperationKind::InsertIdentifiedPitch(crate::payload::InsertIdentifiedPitchOp {
                event: ev(1),
                pitch: valuegen::identified_pitch(PitchId::new(ReplicaId(3), 2)),
            }),
            OperationKind::DeleteIdentifiedPitch(crate::payload::DeleteIdentifiedPitchOp {
                pitch: PitchId::new(ReplicaId(3), 2),
            }),
            OperationKind::ModifyIdentifiedPitch(crate::payload::ModifyIdentifiedPitchOp {
                pitch: PitchId::new(ReplicaId(3), 1),
                value: valuegen::pitch_value_nth(3),
            }),
            OperationKind::DeleteCrossCutting(crate::payload::DeleteCrossCuttingOp {
                structure: epiphany_core::TypedObjectId::Slur(SlurId::new(ReplicaId(3), 5)),
            }),
            OperationKind::ModifyCrossCutting(crate::payload::ModifyCrossCuttingOp {
                structure: CrossCuttingValue::Slur(valuegen::slur(
                    SlurId::new(ReplicaId(3), 5),
                    ev(1),
                    ev(2),
                )),
            }),
            OperationKind::CreateRegion(crate::payload::CreateRegionOp {
                region: valuegen::region(epiphany_core::RegionId::new(ReplicaId(3), 7)),
            }),
            OperationKind::DeleteRegion(crate::payload::DeleteRegionOp {
                region: epiphany_core::RegionId::new(ReplicaId(3), 7),
            }),
            OperationKind::CreateStaffInstance(crate::payload::CreateStaffInstanceOp {
                region: epiphany_core::RegionId::new(ReplicaId(3), 7),
                instance: valuegen::staff_instance(
                    epiphany_core::StaffInstanceId::new(ReplicaId(3), 8),
                    StaffId::new(ReplicaId(3), 1),
                ),
            }),
            OperationKind::DeleteStaffInstance(crate::payload::DeleteStaffInstanceOp {
                staff_instance: epiphany_core::StaffInstanceId::new(ReplicaId(3), 8),
            }),
            OperationKind::CreateVoice(crate::payload::CreateVoiceOp {
                staff_instance: epiphany_core::StaffInstanceId::new(ReplicaId(3), 8),
                voice: valuegen::voice(VoiceId::new(ReplicaId(3), 9)),
            }),
            OperationKind::DeleteVoice(crate::payload::DeleteVoiceOp {
                voice: VoiceId::new(ReplicaId(3), 9),
            }),
            OperationKind::SetMetadata(crate::payload::SetMetadataOp {
                metadata: valuegen::score_metadata(2),
            }),
            OperationKind::SetMetricGrid(crate::payload::SetMetricGridOp {
                region: epiphany_core::RegionId::new(ReplicaId(3), 7),
                grid: Some(valuegen::metric_grid()),
            }),
            OperationKind::SetUserPageBreak(crate::payload::SetUserPageBreakOp {
                region: epiphany_core::RegionId::new(ReplicaId(3), 7),
                anchor: valuegen::region_start_anchor(
                    epiphany_core::RegionId::new(ReplicaId(3), 7),
                    epiphany_core::MusicalPosition::origin(),
                ),
                present: true,
            }),
        ];
        for kind in kinds {
            let e = env(primitive(kind));
            assert_eq!(round_trip(&e), Ok(e.clone()));
        }
    }
}

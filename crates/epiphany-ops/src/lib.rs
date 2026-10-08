#![forbid(unsafe_code)]
//! # epiphany-ops
//!
//! The Epiphany **concurrent semantics**: the operations through which the
//! score graph becomes a *live* model, and the deterministic reduction by
//! which a set of operations becomes a materialized score state. This crate
//! implements the normative requirements of **Chapter 6 (Semantic Operations
//! and Concurrent Reduction)** of the core specification. It is Agent C's crate
//! per `spec/QUICKSTART.md`; it depends on [`epiphany_determinism`] (Agent A)
//! and [`epiphany_core`] (Agent B), and on nothing else.
//!
//! ## The thesis in one paragraph
//!
//! A score's canonical state is the set of operations committed to it; the
//! materialized graph is a *deterministic reduction* of that set (Chapter 6
//! §"Design Principles"). The replicated operation set is a grow-only CRDT;
//! the materialized graph is not. Replicas accumulate [envelopes](OperationEnvelope)
//! and converge on the same set, then reduce it — *in a single canonical order*
//! — to byte-identical materialized state. The canonical reduction order
//! ([`canonical_reduction_order`]) is the determinism heart of the
//! architecture: any permutation of the same input envelopes reduces to the
//! same bytes (Appendix D §"Canonical score determinism"). If that does not
//! hold, nothing else matters.
//!
//! ## What lives here
//!
//! * `stamp` — [`OperationStamp`] and the [`HybridLogicalClock`], with the
//!   per-replica monotonicity tuple `(physical, logical, counter)` that the
//!   canonical order and anomaly detection both consume (Chapter 6 §6.6).
//! * `causal` — [`CausalContext`] as a dotted version vector, and the
//!   happens-before closure used for transaction ordering and the
//!   missing-predecessor rule (Chapter 6 §6.2).
//! * `payload` — [`OperationKind`], the discriminator-only [`OperationKindTag`],
//!   [`OperationPayload`], and the representative operation payloads the chapter
//!   specifies reduction rules for (Chapter 6 §6.10).
//! * `envelope` — [`OperationEnvelope`], its canonical serialization, the
//!   [`EnvelopeHash`] (`MUSCENVH`), and the well-formedness contract including
//!   the `stamp.id == id` invariant (Chapter 6 §6.4).
//! * `slot` — the order-independent [`OperationSlot`] model: `Single` or
//!   `Equivocated`, with the Pass-10 transition rules (Chapter 6 §6.5).
//! * `anomaly` — [`AnomalousReplicaSegment`] and the [`IntegrityAnomaly`]
//!   register, kept separate from ordinary conflicts (Chapter 6 §6.6,
//!   Chapter 5 §"System-Derived Counter Collisions").
//! * `effect` — [`OperationEffect`], [`NoOpReason`], the typed
//!   [`PreconditionFailureReason`], and the [`RepairRecord`] / [`RepairKind`]
//!   re-anchoring vocabulary (Chapter 6 §6.2.3, §6.7).
//! * `conflict` — [`ConflictRecord`], [`ConflictKind`], the content-derived
//!   [`ConflictId`] ([`derive_conflict_id`]), and the conflict registry
//!   (Chapter 6 §6.4).
//! * `transaction` / `undo` — [`TransactionDescriptor`] with the
//!   causal-prior-descriptor rule, and [`UndoTransactionPayload`] with its
//!   [`UndoPolicy`] (Chapter 6 §6.6, §6.8).
//! * `opset` — [`OperationSet`]: the slot map plus the acceptance pipeline
//!   (well-formedness → slot transition → causal validation).
//! * `reduce` — [`canonical_reduction_order`], [`MaterializedState`], and the
//!   reduction driver (Chapter 6 §6.3). [`OperationSet::reduce_onto`] also
//!   materializes the representative mutations into an Agent B
//!   [`epiphany_core::Score`].
//!
//! ## Scope (per QUICKSTART and Chapter 6 §6.11)
//!
//! Chapter 6 specifies the *framework* and a *representative selection* of
//! operations; the full catalog of ~60–80 primitives is an explicit open
//! question (§6.11) deferred to the Operation Catalog companion. This crate
//! mirrors that: it implements the framework in full and the representative
//! operations the chapter gives reduction rules for, which is sufficient to
//! exercise every reduction *discipline* (position-keyed insert with voice
//! promotion, delete-wins with tombstones and re-anchoring, field-overwrite
//! with conflict records, set-union, LWW-advisory, structural-migration
//! conflict, and atomic transactions). See `DECISIONS.md` for the boundary and
//! the batched Pass 11 candidates.
//!
//! ## Implementation decisions (per QUICKSTART "Decisions you'll need to make")
//!
//! Fully sync, no async (decision 4); current stable Rust, MSRV the
//! workspace's `rust-version` (decision 5); `unsafe` forbidden crate-wide.
//! Canonical iteration is enforced structurally with `BTreeMap`/`BTreeSet`
//! and sorted projections (Appendix D §"Ordered Iteration").

mod anomaly;
mod causal;
mod conflict;
mod decode;
mod effect;
mod encode;
mod envdecode;
mod envelope;
mod migrate;
mod opset;
mod payload;
mod reduce;
mod slot;
mod stamp;
mod support;
#[cfg(test)]
mod textproj_conformance;
mod textproj_envelope;
mod textproj_kind;
mod textproj_leaf;
mod v0;
mod validate;
pub mod valuegen;

pub mod fuzz;
pub mod vectors;

/// The reduction semantics **this build implements**, as a bare number.
///
/// `core_spec.tex` §"Canonical Document Identity" is normative: *snapshots
/// produced under an earlier algorithm version cannot be used as canonical
/// bases under a later one without rebuilding*. Enforcing that needs a value
/// naming what the running implementation actually does — and before P13-S27
/// no such value existed anywhere. `ReductionAlgorithmVersion`
/// (`epiphany-bundle`) was a wire field whose reader compared it only against
/// the superblock that same value had seeded, so the check was a tautology for
/// every conformingly-written document.
///
/// # The bump discipline — this is the whole guarantee
///
/// **Any change to a canonical reduction verdict, or to canonical reduced
/// state, MUST bump this constant and record the change in the list below.**
///
/// **No mechanism can detect a semantics change.** A golden test over reduction
/// outputs can *prompt* the question — outputs moved, did semantics? — but it
/// can never answer it: a deliberate semantics change and an accidental
/// regression look identical from outside. **The discipline is the guarantee;
/// there is no backstop.**
///
/// # Why the series *started* at `0`, and why that was a decision
///
/// Bundles written before the authority check carry `0` when they have no
/// canonical base, and bases self-report whatever they were stamped with.
/// Starting the series anywhere but `0` would have made every existing
/// base-bearing document fail to open **without any semantics having changed** —
/// the check would have manufactured the breakage it exists to detect. `0` was
/// therefore a decision, **not "unset"**.
///
/// This is a fact about the *baseline*, not about the current value: read the
/// declaration below for that, and the `Bumps` list for how the series got
/// there.
///
/// # Bumps
///
/// * `0` — the baseline. The semantics `canonical_reduction_order` and
///   `reduce_onto` implement as of P13-S27 (2026-08-08). No earlier version
///   exists; nothing predates this constant.
/// * `1` — **P13-S16** (2026-08-09, `spec/CONTRACT_P13S16_PROJECTION.md`), the
///   first real bump. It carries **one change of each kind**, and the two are
///   not interchangeable:
///   - a **reduction verdict** change — `CreateStaffGroup` carrying a non-empty
///     `members` now reduces to a `ContainerNotEmpty` no-op where version `0`
///     applied it. The effect recorded for that operation differs.
///   - a **canonical reduced state** change — `CreateStaff` carrying
///     `group: Some(g)` now appends the staff to `g`'s `members`. Its verdict is
///     unchanged (it still applies); the graph the reduction produces is what
///     differs.
///
///   Either alone would require this bump. A base materialized under `0` holds
///   state this version would not have computed, so it must be rebuilt rather
///   than reused.
/// * `2` — **X3** (2026-10-03). X3.1 adds `CreateTuplet` (operation_catalog
///   §CreateTuplet) and keeps tuplet membership in the referent index in both
///   reduction modes; X3.5 admits a pickup (operation_catalog
///   §CreateMeasure). On histories of the operation kinds that existed before
///   it, eight **reduction verdicts** change, each intended:
///   - a base-free `DeleteEvent` declaring `RewriteTuplets`, or
///     `CascadeDeleteTuplets` not naming exactly the live tuplets that hold
///     the event, is refused `TupletCompensationInvalid`, as graph-aware
///     reduction refuses it, where version `1` applied it base-free with a
///     `TupletCompensated` repair against a tuplet no operation had minted
///     (and, for the cascade, that id tombstoned), on any history at all;
///   - a base-free `DeleteEvent` whose `ReplaceWithRest` rest differs in
///     duration from the event, read from `voice_occupancy`, is refused
///     `TupletCompensationInvalid`, as graph-aware reduction refuses it,
///     where version `1` applied it base-free. `CreateTuplet` makes the
///     declaration ordinary base-free and a concurrent trim makes it stale,
///     so without this check one valid history reduced differently in the
///     two modes. Whether the rest's id is fresh stays a graph-aware,
///     referential check;
///   - a `ModifyEvent` changing a tuplet member's duration is refused
///     `EventDurationInvalid`, where version `1` applied it over a base
///     holding the tuplet and broke invariant 16;
///   - a `CreateMeasure` whose predecessor is its instance's only live
///     measure, and which starts less than a full bar after it, applies (the
///     first measure is a pickup), where version `1` refused it
///     `MeasureMeterMismatch`;
///   - a base-free `ChangeRegionTimeModel` conflicts
///     `TimeModelMigrationFailure`, naming them, when its region holds events
///     with a metric placement that its target does not admit (a proportional
///     target) or that a `Reassign` leaves unmapped, as graph-aware reduction
///     conflicts, where version `1` applied it base-free. Both modes find the
///     region's events from the indices they keep: `voice_occupancy`, the
///     ledger's instances and voices, and a promoted voice's instance through
///     the insert it was promoted for;
///   - an applied `Reassign` moves `voice_occupancy` in both modes, where
///     version `1` moved it only with a graph, so base-free reduction reads
///     the remapped placements in a later `InsertEvent`'s overlap check, a
///     `ModifyEvent`'s placement verdict, a replacement rest's placement and
///     the re-anchoring "nearest" ordering, as graph-aware reduction does;
///   - a base-free `InsertEvent` into a region that is not metric, created so
///     or made so by an applied `ChangeRegionTimeModel`, is refused
///     `WrongRegionTimeModel`, as graph-aware reduction refuses it, where
///     version `1` applied it base-free, or refused it for another reason:
///     an overlap with an indexed event (`EventDurationInvalid`) or a
///     carried pitch id already in canonical state (`TargetTombstoned`).
///     Each region's coordinate discipline
///     is now held in both modes (`region_disciplines`), moved by an applied
///     migration and rolled back with a failed transaction;
///   - graph-aware, a `ChangeRegionTimeModel` judges each event the occupancy
///     index holds by its indexed placement, and judges from the graph only
///     the events the index does not hold (a base's events of another
///     coordinate kind). The two readings differ only for an event an
///     `InsertEvent` carried at a wall-clock position into a metric region,
///     which breaks invariant 4 and which the index holds at the region's
///     origin: a metric target now admits it, where version `1` conflicted.
///
///   The last four are older than X3: through `ChangeRegionTimeModel` the
///   two modes disagreed, on valid histories for the first three, and
///   version 2 makes them agree.
///
///   And two operations produce different **canonical reduced state** over a
///   base that already holds a tuplet, each intended:
///   - a `DeleteEvent` whose `ReplaceWithRest` compensation replaces a
///     member puts the rest in the member's place in the index, so its
///     effect no longer carries the `AttachmentTombstoned` repair the stale
///     index used to record against the tuplet;
///   - a member tombstoned with no compensation to declare, as a cue event
///     cascaded out from under the tuplet is, or as the rest is when an undo
///     removes the transaction that replaced a member with it,
///     cascade-deletes the tuplet: a `CascadeDeleted` repair, and the tuplet
///     and any decomposition attachment naming it removed from the graph,
///     where version `1` left the tuplet naming a dead member (for the cue,
///     with an `AttachmentTombstoned` repair).
///
///   Locked by `version_2_verdicts_on_histories_that_make_no_tuplet` (the
///   first and third verdicts), the `reduction_modes` tests of
///   `epiphany-musicxml`, which reduce one history both ways and compare
///   (the second), `g3b_create_measure_pickup_successor_applies_end_to_end`
///   (the fourth) and `a_base_tuplet_follows_its_members_replacement_and_cascade`
///   (the state, the undone replacement among it). The same rule cascades a
///   tuplet whose member an undo removes on histories that create the
///   tuplet, which version `1` could not reduce
///   (`undo_cascades_a_tuplet_whose_members_it_removes`). The last four
///   verdicts are locked by the `reduction_modes` tests
///   `a_migration_finds_its_regions_events_in_both_modes`,
///   `a_reassigned_measure_is_read_at_its_new_placements_in_both_modes`,
///   `an_insert_reads_its_regions_time_model_in_both_modes` and
///   `a_migration_judges_an_indexed_event_by_its_placement_in_both_modes`,
///   with `migration_finds_a_promoted_voices_event_in_its_region` and
///   `migration_judges_a_bases_wall_clock_events_from_the_graph` holding
///   graph-aware reduction's own verdicts over a base.
///
/// * `3` — **X4a** (2026-10-07). The two reduction modes are held to each
///   other by a fuzz over editors' concurrent histories (`fuzz::modes`), and
///   each split it finds is closed here, in the mode that disagreed. Each
///   change is a **reduction verdict** change, and with it the state the
///   verdict produces:
///   - the promotion pre-pass buckets every concurrent `InsertEvent` into a
///     voice, whatever its preconditions, as the catalog's rule reads, where
///     graph-aware reduction took only inserts whose voice its graph held
///     before anything applied. Over the importer's empty base it promoted
///     nothing, so the later of two overlapping concurrent inserts into a
///     voice the history made was refused `EventDurationInvalid` (or applied
///     where its winner failed) graph-aware and promoted base-free; now both
///     modes promote it. Base-free reduction is unchanged.
///   - a `TransposeInterval` of a pitch in `cmn-24` with an authored spelling
///     applies, the spelling moved by quarter-tones and its accidental kept to
///     its kind (`PitchSpelling::transposed_by_quarter_tones`), where
///     graph-aware reduction refused it `TranspositionOutOfRange`, unable to
///     rewrite a 24-chromatic spelling, and base-free reduction, which holds no
///     spelling, applied it. Every quarter-tone the importer reads carries
///     such a spelling. The graph's pitch, spelling and value chain change with
///     the verdict.
///   - base-free reduction reads a referent its set mints as the set leaves
///     it (`req:catalog:base-free-referents`): one an envelope mints that no
///     operation made live, or that one tombstoned, is missing, so a
///     `CreateStaffInstance`, `CreateStaff`, `CreatePartDefinition`,
///     `CreateView`, `CreateMeasure`, `SetStaffLayout` or
///     `ChangeRegionTimeModel` naming it is refused `TargetMissing`, and an
///     `InsertEvent` into such a voice `VoiceMissing`, as graph-aware
///     reduction refuses each; before, base-free reduction checked none of
///     these referents, and created a voice it had never seen on first use.
///     So is a system-promoted voice no promotion of the reduction made, and
///     a migration of a region an undo tombstoned, which the graph keeps, is
///     refused graph-aware as well, where it applied.
///     An object no envelope mints is still taken as live. The pinned digest
///     of a seeded `gen_envelope_set` reduction moves with these verdicts.
///   - base-free reduction keeps each pitch it minted at its current value
///     (`pitch_values`, written wherever the graph writes one), so a
///     `TransposeInterval` resolves its targets and records its write in the
///     pitch's chain in both modes: a concurrent `ModifyIdentifiedPitch` of a
///     transposed pitch conflicts base-free as graph-aware, where it applied,
///     and an undo of a transpose restores the pitch base-free, where it was
///     refused `TargetMissing` for having nothing to restore. A pitch from a
///     base is still unknown base-free.
///   - a `TransposeInterval` whose transposed value no well-formed accidental
///     stack writes (past a triple accidental, or in `cmn-24` past five
///     quarter-tones) is refused `TranspositionOutOfRange` in both modes, read
///     from the value; an authored spelling it cannot move is dropped, the
///     pitch taking the propagated one, where the operation was refused
///     graph-aware alone, and the graph wrote a repeated accidental for a
///     value past a triple one. A deleted pitch takes every spelling
///     attachment scoped to it, where a propagated one outlived it
///     (`SpellingScopeResolves`).
///   - an undo reads the same history in both modes: base-free reduction seeds
///     the score-level settings chains (metadata, canvas layout defaults,
///     spelling precedence, tuning context) with an empty score's values, as
///     graph-aware reduction onto an empty base does, so a second undo of a
///     settings transaction conflicts base-free as graph-aware, where it
///     applied; and it records each write to a pitch's spelling set, so an
///     undo of a respelling a later transpose superseded conflicts base-free,
///     where it was undone.
///   - a `ChangeRegionTimeModel` whose `Reassign` would leave two events of
///     one voice overlapping conflicts `TimeModelMigrationFailure`, naming
///     both, read from the occupancy index both modes keep, where it applied
///     and broke invariant 3.
///   - a `CreateStaffInstance` for a staff a live instance of its region
///     already manifests is refused `ContainerNotEmpty`, read from the
///     region's instances and their staves both modes keep, where two
///     concurrent creates both applied (`StaffInstanceResolves`). The seeded
///     digest moves again, its stream's instances all naming one staff, and
///     test fixtures that made two instances of a staff in a region now give
///     the second a staff of its own.
///   - a `DeleteRegion` of a region a live tempo segment of another map
///     anchors to is refused `ContainerNotEmpty`, read from the tempo chains
///     both modes keep, where it applied and left the anchor naming nothing
///     (`CrossCuttingRefsResolve`).
///   - a `ChangeRegionTimeModel` to a model admitting no musical offset
///     (`CoordinateDiscipline::admits_musical_offsets`) conflicts, naming the
///     region's live measures beside its events, where it left them anchored
///     in musical time; applied, it drops the region's system and page breaks;
///     and a `SetUserSystemBreak` or `SetUserPageBreak` in musical time into
///     such a region is refused `WrongRegionTimeModel` (`AnchorOffsetModel`).
///   - an undo that would tombstone a measure with a live later measure in its
///     instance is blocked by it, as by the measure guard's other surfaces
///     (strict: conflicted; best effort: the measure kept), where it removed
///     the measure and left the next two bars from its predecessor
///     (`MeasureMeterConsistency`).
///   - a pitch an undo tombstones leaves its surviving event in the graph, as
///     a deleted pitch does, where it stayed both live and tombstoned
///     (`UniqueIdentifiers`); a graph-state change only.
///   - an undo that would tombstone a region, a staff instance or a voice with
///     a live child the same undo leaves (an instance, a voice, an event) is
///     blocked by it, as a measure is, where it removed the container and left
///     the child naming it; and an instance or region an undo tombstones
///     leaves the graph, as a deleted one does, where the graph kept it
///     (`StaffInstanceResolves`).
///   - an undo whose restored cross-cutting value names an endpoint deleted
///     since is superseded by that delete, where it restored the dangling
///     reference (`CrossCuttingRefsResolve`).
///   - a `SetTimeSignature`, and a `SetMetricGrid` that sets a grid, into a
///     region admitting no musical offset is refused `WrongRegionTimeModel`,
///     and a `ChangeRegionTimeModel` into such a model drops the region's
///     default and local metric grids, where each kept a meter in musical
///     time (`AnchorOffsetModel`).
///   - an `InsertEvent` carrying a wall-clock position is refused
///     `WrongRegionTimeModel` in both modes, read from the value, where it was
///     admitted into a metric region and indexed at the region's origin
///     (`EventCoordinateModel`).
///   - a `SetTempoSegment` whose segment is anchored to a region that is not
///     live is refused `TargetMissing`, the anchor read as a referent in both
///     modes, where a score-level segment's anchor was not read
///     (`CrossCuttingRefsResolve`); one anchored by a musical offset to a
///     region admitting none is refused `WrongRegionTimeModel`; and a live
///     segment so anchored strands a migration of its region to such a model,
///     which conflicts naming the region, where each applied
///     (`AnchorOffsetModel`).
///   - an applied `ChangeRegionTimeModel` whose `Reassign` reorders a voice's
///     events leaves the voice in position order in the graph, as a move
///     does, where it kept the old order (`VoiceEventsSortedNonOverlap`); a
///     graph-state change only.
///   - a tie gives way (D48, `req:opcat:tie-gives-way`): after an operation
///     applies, a live tie it broke, its pairing or its class's placement no
///     longer holding, is removed and the operation records a
///     `CascadeDeleted` repair for it (a conflicted operation's effect
///     carries none; the tie's tombstone names it), read from the indices both
///     modes keep. An insert between a tie's ends, a move, a pitch edited,
///     transposed, deleted or added to an implicitly paired end, a migration's
///     remapping, an undo's restoration, and a tie created or rewritten over
///     such a change each so remove it, where the tie stayed (`TiePairing`).
///   - a `ModifyEvent` keeps a live pitch of its event that its value does not
///     carry and whose insert its author never saw, at its current value and
///     with its attachments (add wins, D48); and an undo restoring an event's
///     value keeps every live pitch of the event the value does not carry, the
///     undone transaction's own tombstoned first. Each dropped the pitch from
///     the graph and left it live, its spelling naming nothing
///     (`SpellingScopeResolves`). Graph state only; no verdict moves.
///   - a `SetClef` or `SetKeySignature` into an instance whose region admits
///     no musical offset is refused `WrongRegionTimeModel`, as is a
///     `CreateStaffInstance` carrying a clef or key change anchored by a
///     musical offset into such a region; and a live instance of a region
///     holding a clef or key change strands the region's migration to such a
///     model, which conflicts naming the instance. Each applied and left a
///     change anchored by a musical offset the region no longer admits
///     (`AnchorOffsetModel`).
///   - an undo that would tombstone a staff a live part definition or spanner
///     names is blocked by it, as by a live staff instance (strict:
///     conflicted; best effort: the staff kept); and a `CreateCrossCutting` or
///     `ModifyCrossCutting` of a spanner naming a dead staff is refused
///     `TargetMissing`, its staves read as referents in both modes. Each left
///     a reference to a staff the score does not declare
///     (`CrossCuttingRefsResolve`).
///
///   Locked by the committed histories of `tests/two_modes/` (each declares
///   whether it reduces alike) and, for the promotion,
///   `two_replacements_of_one_quarter_promote_alike_in_both_modes`
///   (`epiphany-musicxml`'s `reduction_modes`); for the quarter-tone,
///   `an_imported_quarter_tone_transposes_alike_in_both_modes` and
///   `cmn_24_with_an_authored_spelling_moves_it`; for referents,
///   `a_region_the_history_made_and_deleted_is_missing_in_both_modes` and
///   `an_instrument_minted_by_a_failed_transaction_is_missing_in_both_modes`;
///   for pitch values,
///   `a_transpose_and_a_concurrent_pitch_edit_conflict_in_both_modes` and
///   `an_undone_transpose_restores_its_pitch_in_both_modes`; for spellings,
///   `a_transpose_past_a_triple_accidental_refuses` and
///   `an_unfollowable_authored_spelling_is_dropped_and_the_pitch_moves`
///   (which replaces `an_untransposable_authored_spelling_refuses_the_whole_operation`);
///   for undo, `a_second_undo_of_a_settings_transaction_conflicts_in_both_modes`
///   and `an_undo_of_a_respelling_a_transpose_superseded_conflicts_in_both_modes`;
///   for the reassignment,
///   `a_reassignment_that_overlaps_a_voice_conflicts_in_both_modes`; for
///   anchors in musical time,
///   `a_region_out_of_musical_time_keeps_no_musical_break_in_both_modes` and
///   `a_migration_finds_its_regions_events_in_both_modes`, whose proportional
///   target now names the measure too; for the measure undo,
///   `an_undo_of_a_measure_with_a_later_one_conflicts_in_both_modes`; for the
///   containers, `an_undo_of_a_container_another_author_filled_conflicts_in_both_modes`
///   and `an_undone_region_leaves_the_graph`; for meters,
///   `a_meter_in_a_region_out_of_musical_time_is_refused_in_both_modes`; for
///   the wall-clock insert, `an_insert_at_a_wall_clock_position_is_refused_in_both_modes`
///   and `a_migration_judges_an_indexed_event_by_its_placement_in_both_modes`,
///   which held its admission and now holds its refusal; for tempo,
///   `a_tempo_in_a_region_out_of_musical_time_is_refused_in_both_modes`; for
///   the reordering,
///   `a_reassignment_that_reorders_a_voice_keeps_it_sorted_in_both_modes`; for
///   ties, `a_tie_gives_way_to_an_edit_that_breaks_it_in_both_modes`; for
///   kept pitches, `a_modify_keeps_a_pitch_its_author_never_saw_in_both_modes`
///   and `an_undo_of_a_modify_keeps_a_pitch_added_since_in_both_modes`; for
///   clefs and keys,
///   `a_clef_or_key_in_a_region_out_of_musical_time_is_refused_in_both_modes`
///   and `a_migration_finds_its_regions_events_in_both_modes`, whose
///   proportional target now names the instance too; for staves,
///   `an_undo_of_a_staff_a_part_or_spanner_names_conflicts_in_both_modes`.
///
/// A bump without its entry above leaves a number nobody can account for: this
/// list is the only record of *why* each version exists.
///
/// **No mechanism detects a missed bump.** The authority check compares
/// *declared* versions, so it catches a base stamped with a version other than
/// this one — it cannot notice that the semantics changed while the constant
/// stood still. Every future change to a canonical reduction verdict **or to
/// canonical reduced state** must move this constant and add its entry here.
/// Both classes are named because a change that leaves every verdict intact
/// while altering the reduced graph is the easier one to overlook, and it
/// invalidates a base just as completely. That discipline is the whole
/// guarantee.
///
/// # Layering
///
/// This is a plain `u32`, and `epiphany-ops` **MUST NOT** gain a dependency on
/// `epiphany-bundle` in order to use that crate's `ReductionAlgorithmVersion`
/// wrapper. The wrapper is constructed at the composition boundary by whoever
/// depends on both (P13-S27 pin 1, §0.3).
pub const CURRENT_REDUCTION_ALGORITHM_VERSION: u32 = 3;

pub use anomaly::{
    AnomalousReplicaSegment, IntegrityAnomaly, IntegrityAnomalyKind, ReplicaAnomalyReason,
};
pub use causal::CausalContext;
pub use conflict::{
    derive_conflict_id, ConflictId, ConflictKind, ConflictRecord, ConflictRegistry,
    ConflictResolutionState, FieldPath, ResolutionAction,
};
pub use decode::MaterializedDecodeError;
pub use effect::{
    NoOpReason, OperationEffect, PreconditionFailureReason, ReanchorReason, ReanchorResult,
    RepairKind, RepairRecord, TupletCompensationKind,
};
pub use envdecode::{decode_envelope, EnvelopeDecodeError};
pub use envelope::{
    peek_operation_id, well_formed, EnvelopeHash, OperationEnvelope, WellFormednessError,
};
pub use migrate::{migrate_v0_envelope, project_v1_to_v0, MigrationError};
pub use opset::{AcceptOutcome, OperationSet};
pub use payload::{
    operation_block_introduced_minor, ChangeRegionTimeModelOp, CreateAnalysisLayerOp,
    CreateCrossCuttingOp, CreateInstrumentOp, CreateMeasureOp, CreatePartDefinitionOp,
    CreateRegionOp, CreateRepeatStructureOp, CreateStaffGroupOp, CreateStaffInstanceOp,
    CreateStaffOp, CreateTupletOp, CreateViewOp, CreateVoiceOp, CrossCuttingValue,
    DeleteCrossCuttingOp, DeleteEventOp, DeleteIdentifiedPitchOp, DeleteRegionOp,
    DeleteRepeatStructureOp, DeleteStaffInstanceOp, DeleteVoiceOp, InsertEventOp,
    InsertIdentifiedPitchOp, ModifyCrossCuttingOp, ModifyEventOp, ModifyIdentifiedPitchOp,
    OperationKind, OperationKindTag, OperationPayload, PositionRemapping, ResolveConflictPayload,
    ResolveEquivocationPayload, RespellPitchOp, SetCanvasLayoutDefaultsOp, SetClefOp,
    SetKeySignatureOp, SetMetadataOp, SetMetricGridOp, SetSpellingPrecedenceOp, SetStaffLayoutOp,
    SetTempoSegmentOp, SetTimeSignatureOp, SetTuningContextOp, SetUserPageBreakOp,
    SetUserSystemBreakOp, TransactionCategory, TransactionDescriptor, TransposeIntervalOp,
    TransposeOp, TupletCompensation,
};
pub use reduce::{
    canonical_reduction_order, measure_anchor_relation_for_agreement_test, GraphMaterialization,
    MaterializedState, ObjectState, PendingReason,
};
pub use slot::OperationSlot;
pub use stamp::{HybridLogicalClock, OperationStamp, StampTuple};
pub use support::{
    AuthorId, ConflictKindRegistryId, ExtensionPreconditionId, IntegrityAnomalyRegistryId,
    ObjectKind, OperationKindRegistryId, PreconditionFailureRegistryId, ReanchorReasonRegistryId,
    RepairKindRegistryId, ReplicaAnomalyRegistryId, ResolutionRegistryId,
    SerializedCanonicalInputs,
};
pub use textproj_envelope::{parse_envelope, project_envelope};

pub use undo::{UndoPolicy, UndoTransactionPayload};
pub use v0::V0OperationEnvelope;
pub use validate::{advisory_violations, AdvisoryViolation, ValidationMode};

mod undo;

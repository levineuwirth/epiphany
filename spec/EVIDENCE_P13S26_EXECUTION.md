# Evidence — P13-S26 execution

**Not part of the candidate's normative content.** Touch row 8 of
`spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`. This file is the destination
gates 6 and 7 require: every mutation transcript and boundary-gate output,
recorded verbatim rather than summarised.

Contract ratified at `01c621d`; amendment 1 at `f9170b0`; amendment 2 at
`86bf7c6`. Executed 2026-08-11.

---

## §1. Structural baseline

After pins 3, 4, 5, 6, 7, 8 and 10, before any mutation:

```
suites=43 passed=1586 failed=0 ignored=0
```

The pre-rung baseline was 42 suites / 1583. This rung adds one suite
(`invariant_ten_surface`) carrying three tests. Pin 8 narrowed and renamed
`t12`; it was not deleted, so no test was removed.

**Pin 4's count movement was measured, never predicted.** First run after the
requirement was added:

```
assertion `left == right` failed
  left: 286
 right: 285
```

`CORE_REQUIREMENT_COUNT` 214 → **215**; `SUITE_REQUIREMENT_COUNT` 285 → **286**;
`SUITE_LABEL_COUNT` 285 → **286**.

---

## §2. Expected-versus-observed matrix

Every mutation ran against the **full workspace** with `--no-fail-fast`, was
restored by writing back the captured original bytes (never git), and the
baseline was re-verified. §3's "Must fail" cells are exhaustive; a mismatch is a
finding.

| M | # | Expected | Observed | Verdict |
|---|---|---|---|---|
| M1-A | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1-B | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1-C | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1-D | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1-E | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1-F | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1b | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1c | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M1d | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M2 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M7 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M8 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M9 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M14 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M18 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M19 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M20 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M21 | 1 | `specification_item_ten_names_exactly_the_derived_surface` | `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M4-A | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4-B | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4-C | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4-D | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4-E | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4-F | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4b | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4c | 2 | `implementation_doc_names_exactly_the_derived_surface`, `invariants::g3a_tests::t12_invariant_10_doc_block_slices_and_is_non_empty` | `implementation_doc_names_exactly_the_derived_surface`, `invariants::g3a_tests::t12_invariant_10_doc_block_slices_and_is_non_empty` | **MATCH** |
| M4d | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M4e | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M10 | 1 | `implementation_doc_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface` | **MATCH** |
| M5 | 5 | `aleatoric_reference_locality_states_both_referents_and_locality`, `every_requirement_block_has_one_label`, `every_requirement_citation_is_defined`, `requirement_labels_are_unique_across_the_suite`, `requirement_labels_follow_the_grammar` | `aleatoric_reference_locality_states_both_referents_and_locality`, `every_requirement_block_has_one_label`, `every_requirement_citation_is_defined`, `requirement_labels_are_unique_across_the_suite`, `requirement_labels_follow_the_grammar` | **MATCH** |
| M6 | 3 | `aleatoric_reference_locality_states_both_referents_and_locality`, `every_requirement_citation_is_defined`, `requirement_label_areas_match_their_chapters` | `aleatoric_reference_locality_states_both_referents_and_locality`, `every_requirement_citation_is_defined`, `requirement_label_areas_match_their_chapters` | **MATCH** |
| M11 | 1 | `aleatoric_reference_locality_states_both_referents_and_locality` | `aleatoric_reference_locality_states_both_referents_and_locality` | **MATCH** |
| M12 | 1 | `aleatoric_reference_locality_states_both_referents_and_locality` | `aleatoric_reference_locality_states_both_referents_and_locality` | **MATCH** |
| M13 | 1 | `aleatoric_reference_locality_states_both_referents_and_locality` | `aleatoric_reference_locality_states_both_referents_and_locality` | **MATCH** |
| M16 | 1 | `aleatoric_reference_locality_states_both_referents_and_locality` | `aleatoric_reference_locality_states_both_referents_and_locality` | **MATCH** |
| M15 | 2 | `implementation_doc_names_exactly_the_derived_surface`, `specification_item_ten_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface`, `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| M17 | 2 | `implementation_doc_names_exactly_the_derived_surface`, `specification_item_ten_names_exactly_the_derived_surface` | `implementation_doc_names_exactly_the_derived_surface`, `specification_item_ten_names_exactly_the_derived_surface` | **MATCH** |
| **M3** | — | *passing control:* M1-B stops failing | no failures; 1586 passed, 0 failed | **MATCH** |

**38 mutations, 38 matches.** No listed test passed unexpectedly; no unlisted
test failed.

---

## §3. Mutation transcripts

Each entry carries the complete `--no-fail-fast` failure set and the failing
assertion verbatim, untruncated.

### M1-A

*Mutation.* delete `RepeatStructure.voltas` from item 10's nested itemize (group A)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1834491) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1-B

*Mutation.* delete `StaffInstance.instrument_override` (group B)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1836196) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1-C

*Mutation.* delete `StaffBasedContent.default_metric_grid` (group C)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1837920) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1-D

*Mutation.* delete `NotatedComponent.tuplet` (group D)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1839618) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1-E

*Mutation.* delete `IndeterminacyHints.alternatives` (group E)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1844537) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1-F

*Mutation.* delete `TempoSegment.end` (group F)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1846695) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1b

*Mutation.* add a token not in `INVARIANT_TEN_SURFACE`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1848415) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("Bogus.token", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1c

*Mutation.* change one target to a different vocabulary term (`Slur.start_event` → declared staff)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1850117) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "declared staff"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M1d

*Mutation.* change one target to a term outside pin 1a's vocabulary

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1852414) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:187:13:
core_spec.tex item 10: target for Slur.start_event uses "nonexistent target", outside pin 1a's vocabulary. Ordering cannot catch this -- a bad term sorts fine
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M2

*Mutation.* restore item 10 to its pre-rung sentence in full

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1854242) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:229:5:
assertion `left == right` failed: item 10's opening sentence must occur exactly once inside req:graph:score-graph-invariants; found 0
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M7

*Mutation.* move one row into a **second** nested itemize inside item 10

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1855925) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M8

*Mutation.* narrow test 1's slice so it drops the final nested `\item`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1857779) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:267:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M9

*Mutation.* duplicate one `\item` inside the nested itemize

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1859584) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:174:5:
core_spec.tex item 10 lists ["Marker.anchor"] more than once; set comparison cannot see a duplicate, so it is checked here
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M14

*Mutation.* permute a complete existing target (set unchanged, order wrong)

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1861255) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:181:9:
assertion `left == right` failed: core_spec.tex item 10: target for AnalyticalAnnotation.anchor is not in pin 1a's canonical order
  left: "live event, extant region, anchor target"
 right: "anchor target, extant region, live event"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M18

*Mutation.* add `\label {tmp:m18}` — spaced, non-`req:` — inside item 10's outer slice

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1862920) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:244:5:
item 10 must contain no \label: pin 3 adds no label and no requirement block.
Slice was:
\item Except where the re-anchoring rules of Chapter~\ref{ch:semops} explicitly permit transient dangling states during edits, every graph reference resolves to an extant object. \label {tmp:m18} Every cross-cutting structure's references resolve to extant objects in the graph, except where explicit re-anchoring rules permit transient dangling states during edits (see Chapter~\ref{ch:semops}). Tempo-map conditions that are not reference resolution --- segment ordering and non-overlap --- are governed by Requirement~\ref{req:time:tempo-segment-order}, not by this invariant. The complete reference surface is: \begin{itemize} \item \texttt{Slur.start\_event} --- live event. \item \texttt{Slur.end\_event} --- live event. \item \texttt{Tie.start\_event} --- live event. \item \texttt{Tie.end\_event} --- live event. \item \texttt{Beam.events} --- live event. \item \texttt{SubBeam.events} --- live event. \item \texttt{Tuplet.members} --- live event. \item \texttt{Tuplet.parent} --- extant tuplet. \item \texttt{Spanner.staves} --- declared staff. \item \texttt{Spanner.start} --- anchor target. \item \texttt{Spanner.end} --- anchor target. \item \texttt{Marker.anchor} --- anchor target. \item \texttt{RepeatStructure.start} --- anchor target. \item \texttt{RepeatStructure.end} --- anchor target. \item \texttt{RepeatStructure.kind} --- anchor target. \item \texttt{RepeatStructure.voltas} --- anchor target. \item \texttt{ChordSymbol.anchor} --- anchor target. \item \texttt{AnalyticalAnnotation.anchor} --- anchor target, extant region, live event. \item \texttt{AnalyticalAnnotation.layer} --- declared analysis layer. \item \texttt{Comment.anchor} --- anchor target, extant region, live event. \item \texttt{GraphicGesture.objects} --- stored graphic object. \item \texttt{GraphicGesture.anchoring} --- anchor target, declared staff, live event. \item \texttt{LyricLine.events} --- live event. \item \texttt{Staff.instrument} --- declared instrument. \item \texttt{StaffInstance.instrument\_override} --- declared instrument. \item \texttt{Staff.group} --- declared staff group. \item \texttt{StaffGroup.members} --- declared staff. \item \texttt{PartDefinition.staves} --- declared staff. \item \texttt{ViewDefinition.active\_layers} --- declared analysis layer. \item \texttt{MetricTimeModel.meters} --- declared time signature. \item \texttt{StaffBasedContent.default\_metric\_grid} --- declared time signature. \item \texttt{Measure.time\_signature} --- declared time signature. \item \texttt{StaffInstance.local\_metric\_grid} --- declared time signature. \item \texttt{NotatedComponent.tuplet} --- extant tuplet. \item \texttt{IndeterminacyHints.alternatives} --- live event. \item \texttt{TrajectoryEvent.start} --- live pitch. \item \texttt{TrajectoryEvent.end} --- live pitch. \item \texttt{GraphicEvent.graphics} --- stored graphic object. \item \texttt{CueEvent.source} --- live event. \item \texttt{TempoSegment.start} --- anchor target. \item \texttt{TempoSegment.end} --- anchor target. \end{itemize}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M19

*Mutation.* truncate the real opening **and** place one complete anchor after the nested list

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1864612) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:229:5:
assertion `left == right` failed: item 10's opening sentence must occur exactly once inside req:graph:score-graph-invariants; found 0
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M20

*Mutation.* add a spaced, additive, label-free `\begin {requirement}` inside item 10

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1866299) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:249:5:
item 10 must contain no requirement block.
Slice was:
\item Except where the re-anchoring rules of Chapter~\ref{ch:semops} explicitly permit transient dangling states during edits, every graph reference resolves to an extant object. \begin {requirement} Probe. \end {requirement} Every cross-cutting structure's references resolve to extant objects in the graph, except where explicit re-anchoring rules permit transient dangling states during edits (see Chapter~\ref{ch:semops}). Tempo-map conditions that are not reference resolution --- segment ordering and non-overlap --- are governed by Requirement~\ref{req:time:tempo-segment-order}, not by this invariant. The complete reference surface is: \begin{itemize} \item \texttt{Slur.start\_event} --- live event. \item \texttt{Slur.end\_event} --- live event. \item \texttt{Tie.start\_event} --- live event. \item \texttt{Tie.end\_event} --- live event. \item \texttt{Beam.events} --- live event. \item \texttt{SubBeam.events} --- live event. \item \texttt{Tuplet.members} --- live event. \item \texttt{Tuplet.parent} --- extant tuplet. \item \texttt{Spanner.staves} --- declared staff. \item \texttt{Spanner.start} --- anchor target. \item \texttt{Spanner.end} --- anchor target. \item \texttt{Marker.anchor} --- anchor target. \item \texttt{RepeatStructure.start} --- anchor target. \item \texttt{RepeatStructure.end} --- anchor target. \item \texttt{RepeatStructure.kind} --- anchor target. \item \texttt{RepeatStructure.voltas} --- anchor target. \item \texttt{ChordSymbol.anchor} --- anchor target. \item \texttt{AnalyticalAnnotation.anchor} --- anchor target, extant region, live event. \item \texttt{AnalyticalAnnotation.layer} --- declared analysis layer. \item \texttt{Comment.anchor} --- anchor target, extant region, live event. \item \texttt{GraphicGesture.objects} --- stored graphic object. \item \texttt{GraphicGesture.anchoring} --- anchor target, declared staff, live event. \item \texttt{LyricLine.events} --- live event. \item \texttt{Staff.instrument} --- declared instrument. \item \texttt{StaffInstance.instrument\_override} --- declared instrument. \item \texttt{Staff.group} --- declared staff group. \item \texttt{StaffGroup.members} --- declared staff. \item \texttt{PartDefinition.staves} --- declared staff. \item \texttt{ViewDefinition.active\_layers} --- declared analysis layer. \item \texttt{MetricTimeModel.meters} --- declared time signature. \item \texttt{StaffBasedContent.default\_metric\_grid} --- declared time signature. \item \texttt{Measure.time\_signature} --- declared time signature. \item \texttt{StaffInstance.local\_metric\_grid} --- declared time signature. \item \texttt{NotatedComponent.tuplet} --- extant tuplet. \item \texttt{IndeterminacyHints.alternatives} --- live event. \item \texttt{TrajectoryEvent.start} --- live pitch. \item \texttt{TrajectoryEvent.end} --- live pitch. \item \texttt{GraphicEvent.graphics} --- stored graphic object. \item \texttt{CueEvent.source} --- live event. \item \texttt{TempoSegment.start} --- anchor target. \item \texttt{TempoSegment.end} --- anchor target. \end{itemize}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M21

*Mutation.* duplicate the complete anchor elsewhere inside `req:graph:score-graph-invariants`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1867965) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:229:5:
assertion `left == right` failed: item 10's opening sentence must occur exactly once inside req:graph:score-graph-invariants; found 2
  left: 2
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-A

*Mutation.* delete `RepeatStructure.voltas` from the `/// 10.` block

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1874775) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-B

*Mutation.* delete `StaffInstance.instrument_override`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1881551) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-C

*Mutation.* delete `StaffBasedContent.default_metric_grid`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1888357) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-D

*Mutation.* delete `NotatedComponent.tuplet`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1895125) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-E

*Mutation.* delete `IndeterminacyHints.alternatives`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1901908) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4-F

*Mutation.* delete `TempoSegment.end`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1908718) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4b

*Mutation.* add a token not in the inventory to the doc block

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1915525) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("Bogus.token", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4c

*Mutation.* destroy the `/// 10.` block entirely

*Complete `--no-fail-fast` failure set* (full workspace, 1584 passed / 2 failed):

```
  implementation_doc_names_exactly_the_derived_surface
  invariants::g3a_tests::t12_invariant_10_doc_block_slices_and_is_non_empty
```

*Failing assertion, verbatim:*

```
---- invariants::g3a_tests::t12_invariant_10_doc_block_slices_and_is_non_empty stdout ----

thread 'invariants::g3a_tests::t12_invariant_10_doc_block_slices_and_is_non_empty' (1921013) panicked at crates/epiphany-core/src/invariants.rs:4646:14:
invariant 10's doc comment is present

---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1922314) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4d

*Mutation.* change one doc-block target to a different vocabulary term

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1929083) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:382:5:
assertion `left == right` failed
  left: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "declared staff"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
 right: {("AnalyticalAnnotation.anchor", "anchor target, extant region, live event"), ("AnalyticalAnnotation.layer", "declared analysis layer"), ("Beam.events", "live event"), ("ChordSymbol.anchor", "anchor target"), ("Comment.anchor", "anchor target, extant region, live event"), ("CueEvent.source", "live event"), ("GraphicEvent.graphics", "stored graphic object"), ("GraphicGesture.anchoring", "anchor target, declared staff, live event"), ("GraphicGesture.objects", "stored graphic object"), ("IndeterminacyHints.alternatives", "live event"), ("LyricLine.events", "live event"), ("Marker.anchor", "anchor target"), ("Measure.time_signature", "declared time signature"), ("MetricTimeModel.meters", "declared time signature"), ("NotatedComponent.tuplet", "extant tuplet"), ("PartDefinition.staves", "declared staff"), ("RepeatStructure.end", "anchor target"), ("RepeatStructure.kind", "anchor target"), ("RepeatStructure.start", "anchor target"), ("RepeatStructure.voltas", "anchor target"), ("Slur.end_event", "live event"), ("Slur.start_event", "live event"), ("Spanner.end", "anchor target"), ("Spanner.start", "anchor target"), ("Spanner.staves", "declared staff"), ("Staff.group", "declared staff group"), ("Staff.instrument", "declared instrument"), ("StaffBasedContent.default_metric_grid", "declared time signature"), ("StaffGroup.members", "declared staff"), ("StaffInstance.instrument_override", "declared instrument"), ("StaffInstance.local_metric_grid", "declared time signature"), ("SubBeam.events", "live event"), ("TempoSegment.end", "anchor target"), ("TempoSegment.start", "anchor target"), ("Tie.end_event", "live event"), ("Tie.start_event", "live event"), ("TrajectoryEvent.end", "live pitch"), ("TrajectoryEvent.start", "live pitch"), ("Tuplet.members", "live event"), ("Tuplet.parent", "extant tuplet"), ("ViewDefinition.active_layers", "declared analysis layer")}
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M4e

*Mutation.* change one doc-block target to a term outside the vocabulary

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1935871) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:187:13:
invariants.rs invariant-10 doc block: target for Slur.start_event uses "nonexistent target", outside pin 1a's vocabulary. Ordering cannot catch this -- a bad term sorts fine
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M10

*Mutation.* duplicate one line in the doc block

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  implementation_doc_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1942674) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:174:5:
invariants.rs invariant-10 doc block lists ["Marker.anchor"] more than once; set comparison cannot see a duplicate, so it is checked here
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M5

*Mutation.* delete pin 4's requirement from `core_spec.tex`

*Complete `--no-fail-fast` failure set* (full workspace, 1581 passed / 5 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
  every_requirement_block_has_one_label
  every_requirement_citation_is_defined
  requirement_labels_are_unique_across_the_suite
  requirement_labels_follow_the_grammar
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1953135) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:395:10:
core_spec.tex declares req:time:aleatoric-reference-locality
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- every_requirement_block_has_one_label stdout ----

thread 'every_requirement_block_has_one_label' (1953158) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:264:5:
assertion `left == right` failed
  left: 214
 right: 215
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- requirement_labels_follow_the_grammar stdout ----

thread 'requirement_labels_follow_the_grammar' (1953163) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:299:5:
assertion `left == right` failed
  left: 285
 right: 286

---- every_requirement_citation_is_defined stdout ----

thread 'every_requirement_citation_is_defined' (1953159) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:459:5:
assertion `left == right` failed
  left: 285
 right: 286

---- requirement_labels_are_unique_across_the_suite stdout ----

thread 'requirement_labels_are_unique_across_the_suite' (1953162) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:369:5:
assertion `left == right` failed
  left: 285
 right: 286
```
### M6

*Mutation.* relabel pin 4's requirement into the `req:graph:` area

*Complete `--no-fail-fast` failure set* (full workspace, 1583 passed / 3 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
  every_requirement_citation_is_defined
  requirement_label_areas_match_their_chapters
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1954840) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:395:10:
core_spec.tex declares req:time:aleatoric-reference-locality
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- requirement_label_areas_match_their_chapters stdout ----

thread 'requirement_label_areas_match_their_chapters' (1954867) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:345:5:
core_spec.tex:2761 chapter "Time and Duration" requires area "time", found req:graph:aleatoric-reference-locality
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- every_requirement_citation_is_defined stdout ----

thread 'every_requirement_citation_is_defined' (1954865) panicked at crates/epiphany-testkit/tests/requirement_labels.rs:500:5:
req:time:aleatoric-reference-locality: crates/epiphany-core/src/invariants.rs, crates/epiphany-testkit/tests/invariant_ten_surface.rs, crates/epiphany-testkit/tests/requirement_labels.rs, spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md, spec/core_spec.tex
```
### M11

*Mutation.* replace “same region” with “any region”

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1956581) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:406:9:
req:time:aleatoric-reference-locality must state "same region"; block was:
\begin{requirement} \label{req:time:aleatoric-reference-locality} Every event referenced by an aleatoric region's \texttt{ordering} DAG, and every event used as a key in its \texttt{bounds} map, \MUST{} be an event of any region. Naming an event that does not exist is a dangling reference, governed by graph invariant~10; naming an event that exists in a \emph{different} region is a distinct defect, and this requirement is what forbids it. Neither the ordering DAG nor the bounds map may reach outside the region whose time model declares them.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M12

*Mutation.* delete the `ordering` referent from pin 4's requirement

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1959790) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:406:9:
req:time:aleatoric-reference-locality must state "ordering"; block was:
\begin{requirement} \label{req:time:aleatoric-reference-locality} Every event used as a key in its \texttt{bounds} map, \MUST{} be an event of that same region. Naming an event that does not exist is a dangling reference, governed by graph invariant~10; naming an event that exists in a \emph{different} region is a distinct defect, and this requirement is what forbids it. The bounds map may not reach outside the region whose time model declares them.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M13

*Mutation.* delete the `bounds` referent from pin 4's requirement

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1962090) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:406:9:
req:time:aleatoric-reference-locality must state "bounds"; block was:
\begin{requirement} \label{req:time:aleatoric-reference-locality} Every event referenced by an aleatoric region's \texttt{ordering} DAG \MUST{} be an event of that same region. Naming an event that does not exist is a dangling reference, governed by graph invariant~10; naming an event that exists in a \emph{different} region is a distinct defect, and this requirement is what forbids it. The ordering DAG may not reach outside the region whose time model declares them.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M16

*Mutation.* weaken `\MUST{}` to `\SHOULD{}`

*Complete `--no-fail-fast` failure set* (full workspace, 1585 passed / 1 failed):

```
  aleatoric_reference_locality_states_both_referents_and_locality
```

*Failing assertion, verbatim:*

```
---- aleatoric_reference_locality_states_both_referents_and_locality stdout ----

thread 'aleatoric_reference_locality_states_both_referents_and_locality' (1964856) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:406:9:
req:time:aleatoric-reference-locality must state "\\MUST{}"; block was:
\begin{requirement} \label{req:time:aleatoric-reference-locality} Every event referenced by an aleatoric region's \texttt{ordering} DAG, and every event used as a key in its \texttt{bounds} map, \SHOULD{} be an event of that same region. Naming an event that does not exist is a dangling reference, governed by graph invariant~10; naming an event that exists in a \emph{different} region is a distinct defect, and this requirement is what forbids it. Neither the ordering DAG nor the bounds map may reach outside the region whose time model declares them.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```
### M15

*Mutation.* duplicate one entry inside `INVARIANT_TEN_SURFACE`

*Complete `--no-fail-fast` failure set* (full workspace, 1584 passed / 2 failed):

```
  implementation_doc_names_exactly_the_derived_surface
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1949443) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:136:5:
INVARIANT_TEN_SURFACE repeats ["Marker.anchor"]; a repeat collapses silently into the expected set and would let both documents drop a class
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1949444) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:136:5:
INVARIANT_TEN_SURFACE repeats ["Marker.anchor"]; a repeat collapses silently into the expected set and would let both documents drop a class
```
### M17

*Mutation.* put an out-of-vocabulary target on one `INVARIANT_TEN_SURFACE` entry

*Complete `--no-fail-fast` failure set* (full workspace, 1584 passed / 2 failed):

```
  implementation_doc_names_exactly_the_derived_surface
  specification_item_ten_names_exactly_the_derived_surface
```

*Failing assertion, verbatim:*

```
---- implementation_doc_names_exactly_the_derived_surface stdout ----

thread 'implementation_doc_names_exactly_the_derived_surface' (1951281) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:143:13:
INVARIANT_TEN_SURFACE target "nonexistent target" for Marker.anchor uses "nonexistent target", which is outside pin 1a's vocabulary
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- specification_item_ten_names_exactly_the_derived_surface stdout ----

thread 'specification_item_ten_names_exactly_the_derived_surface' (1951282) panicked at crates/epiphany-testkit/tests/invariant_ten_surface.rs:143:13:
INVARIANT_TEN_SURFACE target "nonexistent target" for Marker.anchor uses "nonexistent target", which is outside pin 1a's vocabulary
```

---

## §4. M3 — the passing control

The rung's only mutation whose required outcome is success.

Applied together: test 1's comparison weakened from `assert_eq!(actual,
expected)` to `assert!(actual.is_subset(&expected))` — **that direction
specifically**, since `expected.is_subset(&actual)` still fails after a
deletion — and **M1-B** (delete `StaffInstance.instrument_override` from item
10's nested list).

```
observed failures: none
suites=43 passed=1586 failed=0 ignored=0
```

**M1-B stopped failing.** Equality is load-bearing: without this control,
nothing distinguishes an exact inventory from a spot-check. There is no
assertion diagnostic to quote, and that absence is the evidence.

---

## §5. Gate 6 — pin 9's boundary check, verbatim

`git diff --cached -U0 -- crates/epiphany-core/src/invariants.rs`:

```diff
diff --git a/crates/epiphany-core/src/invariants.rs b/crates/epiphany-core/src/invariants.rs
index 80c8a79..9d739ad 100644
--- a/crates/epiphany-core/src/invariants.rs
+++ b/crates/epiphany-core/src/invariants.rs
@@ -69,12 +69,61 @@ pub enum GraphInvariant {
-    /// 10. Every graph reference resolves to an extant object: cross-cutting
-    ///     structures (incl. anchor targets, annotation layers, tuplet parents,
-    ///     graphic objects) and event-internal references (indeterminate
-    ///     alternatives, trajectory event-pitches, graphic objects, cue sources);
-    ///     structural top-level references (a staff's declared instrument, a
-    ///     staff's group, a staff group's members, a part's staves, a view's
-    ///     active layers — genesis tranche G3a repairs this prose to name what
-    ///     the check body already enforced); and meter/time-signature
-    ///     references at every level a `MeterChange` can appear (a region's
-    ///     time-model meter changes, a region's default metric grid, a
-    ///     measure's declared time signature, a staff instance's local metric
-    ///     grid).
+    /// 10. Every graph reference resolves to an extant object, except where
+    ///     the re-anchoring rules explicitly permit transient dangling states
+    ///     during edits. This surface is **derived from the check bodies**, not
+    ///     copied from prose: `spec/CONTRACT_P13S26_INVARIANT10_SURFACE.md`
+    ///     pin 1 is the sole origin for both this list and `core_spec.tex`'s
+    ///     item 10, because the two were incomplete in different places and
+    ///     neither could be repaired from the other (P13-S26).
+    ///
+    ///   - Slur.start_event — live event.
+    ///   - Slur.end_event — live event.
+    ///   - Tie.start_event — live event.
+    ///   - Tie.end_event — live event.
+    ///   - Beam.events — live event.
+    ///   - SubBeam.events — live event.
+    ///   - Tuplet.members — live event.
+    ///   - Tuplet.parent — extant tuplet.
+    ///   - Spanner.staves — declared staff.
+    ///   - Spanner.start — anchor target.
+    ///   - Spanner.end — anchor target.
+    ///   - Marker.anchor — anchor target.
+    ///   - RepeatStructure.start — anchor target.
+    ///   - RepeatStructure.end — anchor target.
+    ///   - RepeatStructure.kind — anchor target.
+    ///   - RepeatStructure.voltas — anchor target.
+    ///   - ChordSymbol.anchor — anchor target.
+    ///   - AnalyticalAnnotation.anchor — anchor target, extant region, live event.
+    ///   - AnalyticalAnnotation.layer — declared analysis layer.
+    ///   - Comment.anchor — anchor target, extant region, live event.
+    ///   - GraphicGesture.objects — stored graphic object.
+    ///   - GraphicGesture.anchoring — anchor target, declared staff, live event.
+    ///   - LyricLine.events — live event.
+    ///   - Staff.instrument — declared instrument.
+    ///   - StaffInstance.instrument_override — declared instrument.
+    ///   - Staff.group — declared staff group.
+    ///   - StaffGroup.members — declared staff.
+    ///   - PartDefinition.staves — declared staff.
+    ///   - ViewDefinition.active_layers — declared analysis layer.
+    ///   - MetricTimeModel.meters — declared time signature.
+    ///   - StaffBasedContent.default_metric_grid — declared time signature.
+    ///   - Measure.time_signature — declared time signature.
+    ///   - StaffInstance.local_metric_grid — declared time signature.
+    ///   - NotatedComponent.tuplet — extant tuplet.
+    ///   - IndeterminacyHints.alternatives — live event.
+    ///   - TrajectoryEvent.start — live pitch.
+    ///   - TrajectoryEvent.end — live pitch.
+    ///   - GraphicEvent.graphics — stored graphic object.
+    ///   - CueEvent.source — live event.
+    ///   - TempoSegment.start — anchor target.
+    ///   - TempoSegment.end — anchor target.
+    ///
+    ///     Beyond that surface, further checks are reported under this same tag
+    ///     and are NOT part of the normative invariant 10: tempo-map segment
+    ///     shape, ordering and non-overlap (Chapter 3,
+    ///     `req:time:tempo-segment-order`); aleatoric ordering and bounds
+    ///     region locality (Chapter 3,
+    ///     `req:time:aleatoric-reference-locality`); and accidental
+    ///     modification expressibility (Chapter 4,
+    ///     `req:tuning:accidental-modification-compatibility`). That
+    ///     multiplexing is filed as P13-S29 — the public `check_invariant`
+    ///     filter and this violation's `Display` attribute those failures to
+    ///     invariant 10. Repairing it is a behaviour change, out of scope here.
@@ -4648 +4697,6 @@ mod g3a_tests {
-    fn t12_invariant_10_doc_comment_names_the_four_reference_classes() {
+    fn t12_invariant_10_doc_block_slices_and_is_non_empty() {
+        // Narrowed by P13-S26 pin 8. The exact (token, target) comparison lives
+        // in epiphany-testkit's `invariant_ten_surface` guard, which reads both
+        // this block and `core_spec.tex`. This one stays because
+        // `cargo test -p epiphany-core` must still fail when the block is
+        // destroyed, and testkit is a different crate.
@@ -4659,11 +4713,12 @@ mod g3a_tests {
-        for needle in [
-            "staff's group",
-            "group's members",
-            "part's staves",
-            "active layers",
-        ] {
-            assert!(
-                doc_block.contains(needle),
-                "invariant 10's doc comment must name `{needle}`; block was:\n{doc_block}"
-            );
-        }
+        let tokens: Vec<&str> = doc_block
+            .lines()
+            .filter_map(|line| line.trim_start().strip_prefix("/// "))
+            .filter_map(|rest| rest.trim_start().strip_prefix("- "))
+            .filter_map(|rest| rest.split_whitespace().next())
+            .collect();
+
+        assert!(
+            !tokens.is_empty(),
+            "invariant 10's doc block must list its reference surface, one \
+             `- Token — target.` line per class; block was:\n{doc_block}"
+        );
```

Forbidden-pattern scan over that diff's added and removed lines —
`InvariantViolation::new`, `fn check_`, `GraphInvariant::`:

```
$ git diff --cached -U0 -- crates/epiphany-core/src/invariants.rs \
    | grep -E "^[+-]" | grep -v "^[+-][+-]" \
    | grep -E "InvariantViolation::new|fn check_|GraphInvariant::"
$ echo $?
1
```

**No output; exit status 1.** Every hunk falls inside the `/// 10.` doc block or
inside `mod g3a_tests`. No check body was touched, no emission retagged.

---

## §6. Two harness faults

Neither is a contract finding. Both are recorded because each halted the run
under the owner's stop rule, and an unexplained halt reads as a suppressed
mismatch.

**(a) `t12`'s module path.** The harness expected `invariants::tests::t12_…`;
the test lives in `mod g3a_tests` (`invariants.rs:4668`), so the real path is
`invariants::g3a_tests::t12_…`. M4c's *radius* was correct on the first run —
exactly test 2 and `t12` failed — and only the expectation string was wrong.

**(b) M12 applied partially, and it exposed a real weakness.** The first M12
deleted the `ordering` referent from the requirement's **normative clause** and
left the closing recap sentence, which still contains the word. **Test 3 stayed
green.** M12 and M13 were re-applied as full referent deletions and both then
fail as pinned.

**Finding against pin 4a — reported, not patched.** Pin 4a says test 3 buys that
*"neither referent … can silently leave"*. Measured, it buys less: a referent
can leave the sentence carrying the normative force while surviving in prose
that does not. Pin 4a's framing that test 3 is *"weaker than tests 1 and 2, and
stated as such"* is correct; this specific claim overstates by one step. A pin
change is its own amendment with its own review round, so the pin is untouched.

---

## §7. Restoration

After all 38 mutations:

```
suites=43 passed=1586 failed=0 ignored=0
cargo +1.95.0 fmt -p epiphany-core -p epiphany-testkit --check: clean
```

Exactly the structural baseline of §1. Every restoration was a hand write-back
of captured bytes; `git checkout`, `git restore` and `git stash` were never used
against the working tree.

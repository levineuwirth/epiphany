# Pass 13 — candidate ledger

Ambiguities and cross-cut inconsistencies found since Pass 12 closed, filed
per the house rule (a batch pass opens at ≥3 candidates; this file opened
when P13-D1/D2 joined P13-K1). Each entry names the owning DECISIONS record;
this file is the index, not the analysis.

| Id | One-line statement | Filed in | Status |
|---|---|---|---|
| P13-K1 | The K3 verdict for a system pitch introduced by a ModifyEvent replacement value differs across a snapshot cut (in-session `TargetMissing` vs post-snapshot `SystemDerivedContentImmutable`); really "may ModifyEvent introduce never-minted pitch ids" | `crates/epiphany-ops/DECISIONS.md` (Pass-12 G-pass code tranche) | **resolved** (Pass 13: reject the introduction — modify_event refuses a never-minted system-derived pitch, verdict now snapshot-cut-invariant; user "reject the introduction") |
| P13-D1 | Undo-driven event tombstones run graph-side re-anchor/cascade but never ledger-side `reanchor_for_tombstone`: structures leave the graph while staying `Live`, no `RepairRecord` — Ch6's same-step recording MUST is unmet for undo-driven tombstones (pre-existing class: slurs/spanners; repeats now too) | `crates/epiphany-ops/DECISIONS.md` (Schema major 2, Phase D) | open |
| P13-D2 | Cue-cascade recursion re-anchors against the triggering event before its tombstone lands in `objects`: a structure anchored on {X, cue-of-X} can record `Reanchored{to: X}` then `CascadeDeleted` in one effect (contradictory repair trail; plausible by code trace, unexecuted) | `crates/epiphany-ops/DECISIONS.md` (Schema major 2, Phase D) | open |
| P13-D3 | `CreateCrossCutting` validates only event endpoints (`CrossCuttingValue::endpoints()`), so a SPANNER anchored to a missing region/measure mints dangling past `anchor_target_exists`; and non-event referent tombstones (`DeleteRegion` under a region-anchored spanner/repeat) re-anchor nothing — "every referenced endpoint is live" is events-only as implemented | `crates/epiphany-ops/DECISIONS.md` (Phase D follow-up) | **resolved** (Pass 13: mint fixed via `anchor_object_refs`; non-event referent re-anchoring ratified events-only, user "fix the mint only") |

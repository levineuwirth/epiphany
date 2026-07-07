# epiphany-editor-core — Decisions

Decision log for the headless editing seam (`EditorSession`). Older decisions
for this crate were recorded in session notes and the Pass-12 batch rows it
filed (P12-E4, P12-E5); this file starts with their ratification.


## Pass 12 G-pass (2026-07-07): E4/E5 are ratified

Dispositions in `spec/PASS12_RATIFICATION_LOG.md` ("G-pass tranche").
**E4** adopt-as-implemented (`req:format:barrier-matching`): target-free
operations (`SetMetadata`, `DeclareTransaction`) are matched by score-wide
barriers only; opaque `Registered` operations match fully conservatively.
**E5** semantics ratified (`req:format:unsafe-tombstone`): crossing a barrier
immediately deactivates the extension's remaining barriers; the crossing MUST
be durably recorded at the next commit; a tombstoned `required = true`
extension leaves the bundle read-only for dependents. The manifest-side byte
encoding is deferred to the Binary Format companion (new open question there:
the manifest is frozen at major 0, so the record rides the blob layer or a
new chunk kind — next bundle-format tranche);
`extensions_requiring_tombstone()` remains the producer awaiting that
consumer.

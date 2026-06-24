//! Agent K's merge gate: the v0 → v1 operation-payload migration is
//! **deterministic** and **equivalence-preserving** (QUICKSTART Agent K
//! acceptance: "v0 envelopes migrated to v1 reduce to byte-identical canonical
//! state as v0 envelopes did under v0 payloads").
//!
//! The v0 wire shape is reconstructed by [`project_v1_to_v0`] (the total inverse
//! direction), so the gate can drive the migration without a hand-built corpus
//! of historical v0 bytes:
//!
//! ```text
//! v1 corpus ──project──▶ v0 ──migrate(context)──▶ v1'
//! assert reduce(v1') == reduce(v1)        (equivalence-preserving)
//! assert v1' == v1                        (migration inverts projection)
//! assert migrate twice is byte-identical  (deterministic)
//! ```
//!
//! and a **non-vacuity** guard proves the equivalence assertion is not trivially
//! satisfiable: the canonical reduction state is sensitive to a respell's
//! spelling content, so a migration that recovered the *wrong* spelling would be
//! caught.

use epiphany_core::{IdentityContext, ReplicaId, Score};
use epiphany_ops::{
    migrate_v0_envelope, project_v1_to_v0, valuegen, OperationEnvelope, OperationKind,
    OperationPayload, OperationSet,
};

use crate::generators::{content_mutation_pair, operation_envelopes};
use crate::rng::Rng;

/// A context [`Score`] carrying an explicit per-pitch spelling attachment for
/// every `RespellPitch` in the corpus. A respell's spelling is the one payload a
/// v0 projection cannot reconstruct from itself (it kept only a fingerprint); the
/// migration recovers it from exactly these attachments (P12-K1).
fn migration_context(corpus: &[OperationEnvelope]) -> Score {
    let mut score = Score::empty(IdentityContext::new(ReplicaId(1)));
    for env in corpus {
        if let OperationPayload::Primitive(OperationKind::RespellPitch(op)) = &env.payload {
            score
                .spelling_attachments
                .push(valuegen::explicit_spelling_attachment(
                    op.pitch,
                    op.spelling.clone(),
                ));
        }
    }
    score
}

fn reduce_bytes(envs: &[OperationEnvelope]) -> Vec<u8> {
    let mut set = OperationSet::new();
    set.accept_all(envs.iter().cloned());
    set.reduce().canonical_bytes()
}

fn migrate_all(v1: &[OperationEnvelope], ctx: &Score) -> Vec<OperationEnvelope> {
    v1.iter()
        .map(|env| {
            migrate_v0_envelope(project_v1_to_v0(env), ctx)
                .expect("the representative corpus migrates without irreversible payloads")
        })
        .collect()
}

/// The migration equivalence + determinism gate over an `n_ops` random corpus.
pub fn run_migration_equivalence(n_ops: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let v1 = operation_envelopes(&mut rng, n_ops, 3, 6, 6);
    let ctx = migration_context(&v1);

    let migrated = migrate_all(&v1, &ctx);

    // Equivalence-preserving: identical canonical reduction state.
    assert_eq!(
        reduce_bytes(&v1),
        reduce_bytes(&migrated),
        "v0->v1 migration changed the canonical reduction state (seed {seed})"
    );

    // The migration faithfully inverts the projection on the representative
    // payloads (a stronger property than reduction-equivalence alone).
    assert_eq!(
        v1, migrated,
        "migration is not the inverse of projection (seed {seed})"
    );

    // Deterministic: migrating the same projection against the same context
    // twice yields byte-identical envelopes.
    let again = migrate_all(&v1, &ctx);
    assert_eq!(
        migrated, again,
        "migration is not deterministic (seed {seed})"
    );
}

/// Non-vacuity guard: prove the equivalence gate would *fail* if the migration
/// were wrong. A respell's spelling genuinely affects the canonical reduction
/// state, so a migration recovering a different spelling reduces to different
/// bytes — which the equivalence assertion above would catch.
pub fn assert_migration_gate_is_not_vacuous() {
    // `base` inserts a pitch and respells it; `mutated` is identical except the
    // respelling's spelling differs.
    let (base, mutated) = content_mutation_pair();

    // The migration round-trip on `base` is equivalence-preserving.
    let ctx = migration_context(&base);
    let migrated = migrate_all(&base, &ctx);
    assert_eq!(
        reduce_bytes(&base),
        reduce_bytes(&migrated),
        "migration of the controlled corpus is not equivalence-preserving"
    );

    // But the reduction is sensitive to the respelling's content: `base` and
    // `mutated` reduce to *different* bytes. Had the migration recovered the
    // wrong spelling, the equivalence assertion would have diverged just like
    // this — so the gate is discriminating, not vacuous.
    assert_ne!(
        reduce_bytes(&base),
        reduce_bytes(&mutated),
        "non-vacuity control is mis-constructed: differing spellings must reduce differently"
    );
}

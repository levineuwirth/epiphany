//! Reducing an import and reading back what became of every operation.
//!
//! Emitting an operation is not evidence that it applied. The whole set is
//! reduced once onto an empty score, and each envelope's outcome is read from
//! the reduction's own effect log: applied, applied with a repair, a no-op, a
//! refusal (a precondition failing under reduction), a conflict, held pending,
//! or not accepted into the set at all.

use std::collections::BTreeMap;

use epiphany_core::{IdentityContext, OperationId, Score};
use epiphany_ops::{AcceptOutcome, MaterializedState, NoOpReason, OperationEffect, OperationSet};

use crate::emit::Import;

/// What became of one emitted operation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    Applied,
    /// Applied, with the compensating changes the reducer made.
    Repaired(String),
    /// Reduced to no effect for a reason other than a failed precondition.
    NoOp(String),
    /// A precondition failed under reduction.
    Refused(String),
    Conflicted(String),
    /// Held pending, with the reason.
    Pending(String),
    /// Not accepted into the operation set.
    NotAccepted(String),
    /// Accepted but absent from both the effect log and the pending list.
    Missing,
}

impl Verdict {
    /// Whether the operation took effect as authored.
    pub fn applied(&self) -> bool {
        matches!(self, Verdict::Applied)
    }

    /// A short name for the verdict's class.
    pub fn class(&self) -> &'static str {
        match self {
            Verdict::Applied => "applied",
            Verdict::Repaired(_) => "applied with repair",
            Verdict::NoOp(_) => "no-op",
            Verdict::Refused(_) => "refused",
            Verdict::Conflicted(_) => "conflict",
            Verdict::Pending(_) => "pending",
            Verdict::NotAccepted(_) => "not accepted",
            Verdict::Missing => "missing",
        }
    }

    /// The reducer's reason, if the verdict carries one.
    pub fn reason(&self) -> &str {
        match self {
            Verdict::Applied | Verdict::Missing => "",
            Verdict::Repaired(r)
            | Verdict::NoOp(r)
            | Verdict::Refused(r)
            | Verdict::Conflicted(r)
            | Verdict::Pending(r)
            | Verdict::NotAccepted(r) => r,
        }
    }
}

/// An import reduced: the score, the canonical state, and one verdict per
/// emitted envelope, in emission order.
#[derive(Clone, Debug)]
pub struct Reduced {
    pub score: Score,
    pub state: MaterializedState,
    pub verdicts: Vec<Verdict>,
}

impl Reduced {
    /// The indices of the operations that did not apply as authored.
    pub fn rejected(&self) -> impl Iterator<Item = usize> + '_ {
        self.verdicts
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.applied())
            .map(|(i, _)| i)
    }
}

/// Reduces an import's operations onto an empty score and reads back each
/// operation's outcome.
pub fn reduce(import: &Import) -> Reduced {
    let mut set = OperationSet::new();
    let accepted = set.accept_all(import.envelopes.iter().cloned());
    let base = Score::empty(IdentityContext::new(import.replica));
    let out = set.reduce_onto(&base);

    let effects: BTreeMap<OperationId, &OperationEffect> =
        out.state.effects.iter().map(|(id, e)| (*id, e)).collect();
    let pending: BTreeMap<OperationId, String> = out
        .state
        .pending
        .iter()
        .map(|(id, reason)| (*id, format!("{reason:?}")))
        .collect();
    let verdicts = import
        .envelopes
        .iter()
        .zip(&accepted)
        .map(|(envelope, acceptance)| {
            if let AcceptOutcome::Rejected(e) = acceptance {
                return Verdict::NotAccepted(format!("{e:?}"));
            }
            if !matches!(acceptance, AcceptOutcome::Accepted) {
                return Verdict::NotAccepted(format!("{acceptance:?}"));
            }
            if let Some(reason) = pending.get(&envelope.id) {
                return Verdict::Pending(reason.clone());
            }
            match effects.get(&envelope.id) {
                None => Verdict::Missing,
                Some(OperationEffect::Applied) => Verdict::Applied,
                Some(OperationEffect::AppliedWithRepair { repairs }) => {
                    Verdict::Repaired(format!("{repairs:?}"))
                }
                Some(OperationEffect::Conflicted { conflict }) => {
                    Verdict::Conflicted(format!("{conflict:?}"))
                }
                Some(OperationEffect::TombstonedTarget { target }) => {
                    Verdict::NoOp(format!("TombstonedTarget {target:?}"))
                }
                Some(OperationEffect::NoOp { reason }) => match reason {
                    NoOpReason::PreconditionFailedUnderReduction { reason } => {
                        Verdict::Refused(format!("{reason:?}"))
                    }
                    other => Verdict::NoOp(format!("{other:?}")),
                },
            }
        })
        .collect();

    Reduced {
        score: out.score,
        state: out.state,
        verdicts,
    }
}

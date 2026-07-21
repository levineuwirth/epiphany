//! Grammar-directed text projection for operation envelopes and their
//! envelope-level productions.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::textvalue::{read_sexp, Sexp, TextError, TextValue};
use epiphany_core::{OperationId, ReplicaId};

use crate::causal::CausalContext;
use crate::envelope::OperationEnvelope;
use crate::payload::{
    OperationKind, OperationPayload, ResolveConflictPayload, ResolveEquivocationPayload,
};
use crate::stamp::{HybridLogicalClock, OperationStamp};
use crate::undo::UndoTransactionPayload;

fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

fn split_production(s: &Sexp) -> Result<(&str, &[Sexp]), TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: "production",
        found: class_of(s),
    })?;
    let constructor = items
        .first()
        .and_then(Sexp::as_symbol)
        .ok_or(TextError::Syntax(
            "a production is headed by its constructor",
        ))?;
    Ok((constructor, &items[1..]))
}

fn expect_fields<'a>(
    fields: &'a [Sexp],
    type_name: &'static str,
    expected: usize,
) -> Result<&'a [Sexp], TextError> {
    if fields.len() != expected {
        return Err(TextError::Arity {
            type_name,
            expected,
            found: fields.len(),
        });
    }
    Ok(fields)
}

/// The grammar's flattened `(stamp physical-time logical-counter id)`
/// production.
impl TextValue for OperationStamp {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("stamp"),
            self.hlc.physical_time.project(),
            self.hlc.logical_counter.project(),
            self.id.project(),
        ])
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("stamp", 3)?;
        Ok(OperationStamp::new(
            HybridLogicalClock::new(
                epiphany_core::WallClockTime::parse(&fields[0])?,
                u32::parse(&fields[1])?,
            ),
            OperationId::parse(&fields[2])?,
        ))
    }
}

/// The grammar's dotted-version-vector `(causal (replica-seen*) (bytes*))`
/// production.
impl TextValue for CausalContext {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("causal"),
            self.vector.project(),
            self.dots.project(),
        ])
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("causal", 2)?;
        // Parse through the core collection implementations: they reject a
        // duplicate or decrease before BTreeMap/BTreeSet can normalize it.
        Ok(CausalContext {
            vector: BTreeMap::<ReplicaId, u64>::parse(&fields[0])?,
            dots: BTreeSet::<OperationId>::parse(&fields[1])?,
        })
    }
}

/// The grammar's four operation-payload productions, with their records inlined.
impl TextValue for OperationPayload {
    fn project(&self) -> Sexp {
        match self {
            OperationPayload::Primitive(kind) => {
                Sexp::List(vec![Sexp::sym("primitive"), kind.project()])
            }
            OperationPayload::ResolveConflict(payload) => Sexp::List(vec![
                Sexp::sym("resolve-conflict"),
                payload.target.project(),
                payload.action.project(),
            ]),
            OperationPayload::UndoTransaction(payload) => Sexp::List(vec![
                Sexp::sym("undo"),
                payload.target.project(),
                payload.policy.project(),
            ]),
            OperationPayload::ResolveEquivocation(payload) => Sexp::List(vec![
                Sexp::sym("resolve-equivocation"),
                payload.target.project(),
                payload.chosen.project(),
            ]),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (constructor, fields) = split_production(s)?;
        match constructor {
            "primitive" => {
                let fields = expect_fields(fields, "OperationPayload::Primitive", 1)?;
                Ok(OperationPayload::Primitive(OperationKind::parse(
                    &fields[0],
                )?))
            }
            "resolve-conflict" => {
                let fields = expect_fields(fields, "OperationPayload::ResolveConflict", 2)?;
                Ok(OperationPayload::ResolveConflict(ResolveConflictPayload {
                    target: TextValue::parse(&fields[0])?,
                    action: TextValue::parse(&fields[1])?,
                }))
            }
            "undo" => {
                let fields = expect_fields(fields, "OperationPayload::UndoTransaction", 2)?;
                Ok(OperationPayload::UndoTransaction(UndoTransactionPayload {
                    target: TextValue::parse(&fields[0])?,
                    policy: TextValue::parse(&fields[1])?,
                }))
            }
            "resolve-equivocation" => {
                let fields = expect_fields(fields, "OperationPayload::ResolveEquivocation", 2)?;
                Ok(OperationPayload::ResolveEquivocation(
                    ResolveEquivocationPayload {
                        target: TextValue::parse(&fields[0])?,
                        chosen: TextValue::parse(&fields[1])?,
                    },
                ))
            }
            found => Err(TextError::UnknownConstructor {
                type_name: "OperationPayload",
                found: found.to_owned(),
            }),
        }
    }
}

/// The grammar's complete operation-envelope production.
impl TextValue for OperationEnvelope {
    fn project(&self) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("envelope"),
            self.id.project(),
            self.author.project(),
            self.stamp.project(),
            self.causal_context.project(),
            self.transaction.project(),
            self.payload.project(),
        ])
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let fields = s.expect_struct("envelope", 6)?;
        Ok(OperationEnvelope {
            id: TextValue::parse(&fields[0])?,
            author: TextValue::parse(&fields[1])?,
            stamp: TextValue::parse(&fields[2])?,
            causal_context: TextValue::parse(&fields[3])?,
            transaction: TextValue::parse(&fields[4])?,
            payload: TextValue::parse(&fields[5])?,
        })
    }
}

/// Projects one envelope as its canonical single-line s-expression.
///
/// The returned string deliberately has no trailing LF; the enclosing document
/// projection owns line separators.
pub fn project_envelope(envelope: &OperationEnvelope) -> String {
    envelope.project().render()
}

/// Parses one complete canonical envelope line.
///
/// Strictness is enforced where information could otherwise be lost: the
/// reader rejects alternate lexical spellings, and the nested collection
/// parsers reject disorder and duplicates before constructing ordered maps or
/// sets. A whole-envelope re-project guard was mutation-tested and removed
/// because every accepted parse already preserves its input exactly.
pub fn parse_envelope(input: &str) -> Result<OperationEnvelope, TextError> {
    OperationEnvelope::parse(&read_sexp(input)?)
}

#[cfg(test)]
mod tests {
    use core::fmt::Debug;

    use epiphany_core::textvalue::{read_sexp, TextValue};
    use epiphany_core::{
        EventId, OperationId, ReplicaId, TransactionId, TypedObjectId, WallClockTime,
    };

    use super::{parse_envelope, project_envelope};
    use crate::causal::CausalContext;
    use crate::conflict::{ConflictId, ResolutionAction};
    use crate::envdecode::tests::sample_kind;
    use crate::envelope::{EnvelopeHash, OperationEnvelope};
    use crate::payload::{
        OperationKindTag, OperationPayload, ResolveConflictPayload, ResolveEquivocationPayload,
    };
    use crate::stamp::{HybridLogicalClock, OperationStamp};
    use crate::support::{AuthorId, OperationKindRegistryId};
    use crate::undo::{UndoPolicy, UndoTransactionPayload};

    fn round_trip<T>(value: &T)
    where
        T: TextValue + PartialEq + Debug,
    {
        let text = value.project().render();
        let sexp = read_sexp(&text).expect("projected text must be readable");
        let parsed = T::parse(&sexp).expect("projected value must parse");
        assert_eq!(&parsed, value, "value round-trip");
        assert_eq!(parsed.project().render(), text, "text round-trip");
    }

    fn envelope(payload: OperationPayload) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(7), 1);
        OperationEnvelope {
            id,
            author: AuthorId(0x1122_3344),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(42), 7), id),
            causal_context: CausalContext::new()
                .with_seen(ReplicaId(1), 3)
                .with_seen(ReplicaId(2), 5)
                .with_dot(OperationId::new(ReplicaId(3), 9))
                .with_dot(OperationId::new(ReplicaId(4), 11)),
            transaction: Some(TransactionId::new(ReplicaId(7), 5)),
            payload,
        }
    }

    fn round_trip_envelope(value: &OperationEnvelope) {
        round_trip(value);
        let text = project_envelope(value);
        assert!(!text.ends_with('\n'));
        assert_eq!(
            parse_envelope(&text).expect("canonical envelope line must parse"),
            *value
        );
    }

    #[test]
    fn stamp_and_causal_productions_round_trip() {
        let id = OperationId::new(ReplicaId(7), 1);
        round_trip(&OperationStamp::new(
            HybridLogicalClock::new(WallClockTime(42), 7),
            id,
        ));
        round_trip(
            &CausalContext::new()
                .with_seen(ReplicaId(1), 3)
                .with_seen(ReplicaId(2), 5)
                .with_dot(OperationId::new(ReplicaId(3), 9))
                .with_dot(OperationId::new(ReplicaId(4), 11)),
        );
    }

    #[test]
    fn every_primitive_and_all_four_payload_variants_round_trip() {
        for tag in OperationKindTag::PAYLOAD_FREE {
            let payload = OperationPayload::Primitive(sample_kind(*tag));
            round_trip(&payload);
            let value = envelope(payload);
            round_trip_envelope(&value);
        }

        let registered = OperationPayload::Primitive(sample_kind(OperationKindTag::Registered(
            OperationKindRegistryId(1),
        )));
        round_trip(&registered);
        round_trip_envelope(&envelope(registered));

        let conflict = OperationPayload::ResolveConflict(ResolveConflictPayload {
            target: ConflictId(0xDEAD_BEEF),
            action: ResolutionAction::Reanchor {
                new_target: TypedObjectId::Event(EventId::new(ReplicaId(7), 3)),
            },
        });
        round_trip(&conflict);
        round_trip_envelope(&envelope(conflict));

        let undo = OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: TransactionId::new(ReplicaId(7), 5),
            policy: UndoPolicy::BestEffort,
        });
        round_trip(&undo);
        round_trip_envelope(&envelope(undo));

        let equivocation = OperationPayload::ResolveEquivocation(ResolveEquivocationPayload {
            target: OperationId::new(ReplicaId(7), 2),
            chosen: EnvelopeHash([9; 32]),
        });
        round_trip(&equivocation);
        round_trip_envelope(&envelope(equivocation));
    }

    #[test]
    fn causal_collections_reject_duplicates_and_disorder() {
        for noncanonical in [
            "(causal ((#x0000000000000002 5) (#x0000000000000001 3)) ())",
            "(causal ((#x0000000000000001 3) (#x0000000000000001 5)) ())",
            "(causal () (#x00000000000000020000000000000005 #x00000000000000010000000000000003))",
            "(causal () (#x00000000000000010000000000000003 #x00000000000000010000000000000003))",
        ] {
            let sexp = read_sexp(noncanonical).expect("fixture is well-formed text");
            assert!(
                CausalContext::parse(&sexp).is_err(),
                "must reject rather than normalize {noncanonical}"
            );
        }
    }

    #[test]
    fn payload_shapes_are_exact() {
        for noncanonical in [
            "(primitive)",
            "(primitive dismiss)",
            "(resolve-conflict #x000000000000000000000000deadbeef accept-loser extra)",
            "(undo #x00000000000000070000000000000005)",
            "(resolve-equivocation #x00000000000000070000000000000002)",
            "(undo-transaction #x00000000000000070000000000000005 best-effort)",
        ] {
            let sexp = read_sexp(noncanonical).expect("fixture is well-formed text");
            assert!(OperationPayload::parse(&sexp).is_err(), "{noncanonical}");
        }
    }

    #[test]
    fn whole_line_noncanonical_spelling_is_rejected() {
        let value = envelope(OperationPayload::UndoTransaction(UndoTransactionPayload {
            target: TransactionId::new(ReplicaId(7), 5),
            policy: UndoPolicy::BestEffort,
        }));
        let canonical = project_envelope(&value);
        let doubled_space = canonical.replacen(' ', "  ", 1);
        assert!(parse_envelope(&doubled_space).is_err());
        assert!(parse_envelope(&(canonical + "\n")).is_err());
    }
}

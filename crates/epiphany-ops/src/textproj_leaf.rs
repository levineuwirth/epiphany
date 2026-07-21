//! Grammar-directed text projections for operation-layer leaves and sub-vocabularies.

use epiphany_core::textvalue::{Sexp, TextError, TextValue};
use epiphany_core::{Beam, EventId, MusicalPosition, Rest, Slur, Spanner, Tie, TupletId};
use epiphany_determinism::sorted_canonical;

use crate::conflict::{ConflictId, ResolutionAction};
use crate::envelope::EnvelopeHash;
use crate::payload::{
    CrossCuttingValue, PositionRemapping, TransactionCategory, TupletCompensation,
};
use crate::support::AuthorId;
use crate::undo::UndoPolicy;

/// The lexical class of `s`, used by impls in this module and by the registry-id
/// declaration macro. This mirrors the core text layer's private diagnostic helper.
pub(crate) fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

fn constructor<'a>(
    s: &'a Sexp,
    type_name: &'static str,
) -> Result<(&'a str, &'a [Sexp]), TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: type_name,
        found: class_of(s),
    })?;
    let Some(head) = items.first().and_then(Sexp::as_symbol) else {
        return Err(TextError::Syntax(
            "a constructor list is headed by a symbol",
        ));
    };
    Ok((head, &items[1..]))
}

fn expect_arity<'a>(
    fields: &'a [Sexp],
    expected: usize,
    type_name: &'static str,
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

fn unknown(type_name: &'static str, found: &str) -> TextError {
    TextError::UnknownConstructor {
        type_name,
        found: found.to_owned(),
    }
}

fn parse_sorted_sequence<T>(s: &Sexp, what: &'static str) -> Result<Vec<T>, TextError>
where
    T: TextValue + Ord,
{
    let values = Vec::<T>::parse(s)?;
    // These payload fields are Vecs whose binary encoders sort without removing
    // duplicates. Check before constructing the enum so parsing never normalizes
    // a descending input; equal neighbours remain legal in the frozen wire form.
    if values.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(TextError::NotCanonical(what));
    }
    Ok(values)
}

fn parse_reassign_entries(s: &Sexp) -> Result<Vec<(EventId, MusicalPosition)>, TextError> {
    let entries = Vec::<(EventId, MusicalPosition)>::parse(s)?;
    // The encoder sorts by EventId alone and its stable sort preserves the order
    // of duplicate keys. Compare only IDs: equal IDs with any positions are
    // therefore canonical, while a strict decrease would be normalized.
    if entries.windows(2).any(|pair| pair[0].0 > pair[1].0) {
        return Err(TextError::NotCanonical(
            "Reassign entries must be non-decreasing by EventId",
        ));
    }
    Ok(entries)
}

/// Projects an author identifier as its canonical 16 big-endian bytes.
impl TextValue for AuthorId {
    fn project(&self) -> Sexp {
        Sexp::Bytes(self.canonical_bytes().to_vec())
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let Sexp::Bytes(bytes) = s else {
            return Err(TextError::Expected {
                expected: "AuthorId",
                found: class_of(s),
            });
        };
        let bytes: [u8; 16] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| TextError::NotCanonical("an AuthorId is exactly 16 bytes"))?;
        Ok(Self(u128::from_be_bytes(bytes)))
    }
}

/// Projects a conflict identifier as its canonical 16 big-endian bytes.
impl TextValue for ConflictId {
    fn project(&self) -> Sexp {
        Sexp::Bytes(self.canonical_bytes().to_vec())
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let Sexp::Bytes(bytes) = s else {
            return Err(TextError::Expected {
                expected: "ConflictId",
                found: class_of(s),
            });
        };
        let bytes: [u8; 16] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| TextError::NotCanonical("a ConflictId is exactly 16 bytes"))?;
        Ok(Self(u128::from_be_bytes(bytes)))
    }
}

/// Projects an envelope hash as its canonical 32 bytes.
impl TextValue for EnvelopeHash {
    fn project(&self) -> Sexp {
        Sexp::Bytes(self.0.to_vec())
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let Sexp::Bytes(bytes) = s else {
            return Err(TextError::Expected {
                expected: "EnvelopeHash",
                found: class_of(s),
            });
        };
        let bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| TextError::NotCanonical("an EnvelopeHash is exactly 32 bytes"))?;
        Ok(Self(bytes))
    }
}

/// Implements the grammar's `action` production in canonical discriminant order.
impl TextValue for ResolutionAction {
    fn project(&self) -> Sexp {
        match self {
            ResolutionAction::AcceptLoser => Sexp::sym("accept-loser"),
            ResolutionAction::KeepWinner => Sexp::sym("keep-winner"),
            ResolutionAction::Override { override_operation } => {
                Sexp::List(vec![Sexp::sym("override"), override_operation.project()])
            }
            ResolutionAction::Reanchor { new_target } => {
                Sexp::List(vec![Sexp::sym("reanchor"), new_target.project()])
            }
            ResolutionAction::Dismiss => Sexp::sym("dismiss"),
            ResolutionAction::Registered(id) => {
                Sexp::List(vec![Sexp::sym("registered"), id.project()])
            }
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        if let Some(name) = s.as_symbol() {
            return match name {
                "accept-loser" => Ok(Self::AcceptLoser),
                "keep-winner" => Ok(Self::KeepWinner),
                "dismiss" => Ok(Self::Dismiss),
                _ => Err(unknown("ResolutionAction", name)),
            };
        }
        let (name, fields) = constructor(s, "ResolutionAction")?;
        let fields = expect_arity(fields, 1, "ResolutionAction")?;
        match name {
            "override" => Ok(Self::Override {
                override_operation: TextValue::parse(&fields[0])?,
            }),
            "reanchor" => Ok(Self::Reanchor {
                new_target: TextValue::parse(&fields[0])?,
            }),
            "registered" => Ok(Self::Registered(TextValue::parse(&fields[0])?)),
            _ => Err(unknown("ResolutionAction", name)),
        }
    }
}

/// Implements the grammar's `policy` production.
impl TextValue for UndoPolicy {
    fn project(&self) -> Sexp {
        match self {
            UndoPolicy::StrictInverse => Sexp::sym("strict-inverse"),
            UndoPolicy::BestEffort => Sexp::sym("best-effort"),
            UndoPolicy::Cascade => Sexp::sym("cascade"),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s.as_symbol() {
            Some("strict-inverse") => Ok(Self::StrictInverse),
            Some("best-effort") => Ok(Self::BestEffort),
            Some("cascade") => Ok(Self::Cascade),
            Some(name) => Err(unknown("UndoPolicy", name)),
            None => Err(TextError::Expected {
                expected: "UndoPolicy",
                found: class_of(s),
            }),
        }
    }
}

/// Implements the grammar's `tuplet-comp` production in canonical discriminant
/// and payload-field order.
impl TextValue for TupletCompensation {
    fn project(&self) -> Sexp {
        match self {
            TupletCompensation::NotInTuplet => Sexp::sym("not-in-tuplet"),
            TupletCompensation::ReplaceWithRest { rest } => {
                Sexp::List(vec![Sexp::sym("replace-with-rest"), rest.project()])
            }
            TupletCompensation::RewriteTuplets { tuplets } => Sexp::List(vec![
                Sexp::sym("rewrite-tuplets"),
                sorted_canonical(tuplets.clone()).project(),
            ]),
            TupletCompensation::CascadeDeleteTuplets { tuplets } => Sexp::List(vec![
                Sexp::sym("cascade-delete-tuplets"),
                sorted_canonical(tuplets.clone()).project(),
            ]),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        if let Some(name) = s.as_symbol() {
            return match name {
                "not-in-tuplet" => Ok(Self::NotInTuplet),
                _ => Err(unknown("TupletCompensation", name)),
            };
        }
        let (name, fields) = constructor(s, "TupletCompensation")?;
        let fields = expect_arity(fields, 1, "TupletCompensation")?;
        match name {
            "replace-with-rest" => Ok(Self::ReplaceWithRest {
                rest: Rest::parse(&fields[0])?,
            }),
            "rewrite-tuplets" => Ok(Self::RewriteTuplets {
                tuplets: parse_sorted_sequence::<TupletId>(
                    &fields[0],
                    "RewriteTuplets ids must be non-decreasing",
                )?,
            }),
            "cascade-delete-tuplets" => Ok(Self::CascadeDeleteTuplets {
                tuplets: parse_sorted_sequence::<TupletId>(
                    &fields[0],
                    "CascadeDeleteTuplets ids must be non-decreasing",
                )?,
            }),
            _ => Err(unknown("TupletCompensation", name)),
        }
    }
}

/// Implements the grammar's `cross-cutting` production, retaining the canonical
/// discriminant order of `CrossCuttingValue::encode_canonical`.
impl TextValue for CrossCuttingValue {
    fn project(&self) -> Sexp {
        match self {
            CrossCuttingValue::Tie(value) => Sexp::List(vec![Sexp::sym("tie"), value.project()]),
            CrossCuttingValue::Slur(value) => Sexp::List(vec![Sexp::sym("slur"), value.project()]),
            CrossCuttingValue::Beam(value) => Sexp::List(vec![Sexp::sym("beam"), value.project()]),
            CrossCuttingValue::Spanner(value) => {
                Sexp::List(vec![Sexp::sym("spanner"), value.project()])
            }
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let (name, fields) = constructor(s, "CrossCuttingValue")?;
        let fields = expect_arity(fields, 1, "CrossCuttingValue")?;
        match name {
            "tie" => Ok(Self::Tie(Tie::parse(&fields[0])?)),
            "slur" => Ok(Self::Slur(Slur::parse(&fields[0])?)),
            "beam" => Ok(Self::Beam(Beam::parse(&fields[0])?)),
            "spanner" => Ok(Self::Spanner(Spanner::parse(&fields[0])?)),
            _ => Err(unknown("CrossCuttingValue", name)),
        }
    }
}

/// Implements the grammar's `remapping` production. Each reassign entry delegates
/// to the core pair projection for `(EventId, MusicalPosition)`.
impl TextValue for PositionRemapping {
    fn project(&self) -> Sexp {
        match self {
            PositionRemapping::PreserveTime => Sexp::sym("preserve-time"),
            PositionRemapping::Reassign(entries) => {
                let mut entries = entries.clone();
                entries.sort_by_key(|(event, _)| event.canonical_bytes());
                Sexp::List(vec![Sexp::sym("reassign"), entries.project()])
            }
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        if let Some(name) = s.as_symbol() {
            return match name {
                "preserve-time" => Ok(Self::PreserveTime),
                _ => Err(unknown("PositionRemapping", name)),
            };
        }
        let (name, fields) = constructor(s, "PositionRemapping")?;
        if name != "reassign" {
            return Err(unknown("PositionRemapping", name));
        }
        let fields = expect_arity(fields, 1, "PositionRemapping")?;
        Ok(Self::Reassign(parse_reassign_entries(&fields[0])?))
    }
}

/// Implements the five-variant transaction-category sub-vocabulary in canonical
/// discriminant order.
impl TextValue for TransactionCategory {
    fn project(&self) -> Sexp {
        match self {
            TransactionCategory::NoteEntry => Sexp::sym("note-entry"),
            TransactionCategory::Structural => Sexp::sym("structural"),
            TransactionCategory::Layout => Sexp::sym("layout"),
            TransactionCategory::Import => Sexp::sym("import"),
            TransactionCategory::Registered(id) => {
                Sexp::List(vec![Sexp::sym("registered"), id.project()])
            }
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        if let Some(name) = s.as_symbol() {
            return match name {
                "note-entry" => Ok(Self::NoteEntry),
                "structural" => Ok(Self::Structural),
                "layout" => Ok(Self::Layout),
                "import" => Ok(Self::Import),
                _ => Err(unknown("TransactionCategory", name)),
            };
        }
        let (name, fields) = constructor(s, "TransactionCategory")?;
        if name != "registered" {
            return Err(unknown("TransactionCategory", name));
        }
        let fields = expect_arity(fields, 1, "TransactionCategory")?;
        Ok(Self::Registered(TextValue::parse(&fields[0])?))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use epiphany_core::textvalue::read_sexp;
    use epiphany_core::{
        AnchorOffset, BeamId, EventId, MusicalDuration, RationalTime, ReplicaId, SlurId, SpannerId,
        TieId, TimeAnchor, TupletId, VoiceId,
    };

    use super::*;
    use crate::support::{
        ConflictKindRegistryId, ExtensionPreconditionId, IntegrityAnomalyRegistryId,
        OperationKindRegistryId, PreconditionFailureRegistryId, ReanchorReasonRegistryId,
        RepairKindRegistryId, ReplicaAnomalyRegistryId, ResolutionRegistryId,
    };

    fn round_trip<T>(value: T)
    where
        T: TextValue + PartialEq + Debug,
    {
        let rendered = value.project().render();
        let read = read_sexp(&rendered).expect("projected text is valid s-expression");
        let parsed = T::parse(&read).expect("projected value parses");
        assert_eq!(parsed, value);
        assert_eq!(parsed.project().render(), rendered);
    }

    fn event(counter: u64) -> EventId {
        EventId::new(ReplicaId(7), counter)
    }

    fn tuplet(counter: u64) -> TupletId {
        TupletId::new(ReplicaId(7), counter)
    }

    #[test]
    fn byte_leaves_round_trip() {
        round_trip(AuthorId(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff));
        round_trip(ConflictId(0xffee_ddcc_bbaa_9988_7766_5544_3322_1100));
        round_trip(EnvelopeHash([0x5a; 32]));
        round_trip(OperationKindRegistryId(1));
        round_trip(ConflictKindRegistryId(2));
        round_trip(ResolutionRegistryId(3));
        round_trip(RepairKindRegistryId(4));
        round_trip(ReanchorReasonRegistryId(5));
        round_trip(ReplicaAnomalyRegistryId(6));
        round_trip(IntegrityAnomalyRegistryId(7));
        round_trip(ExtensionPreconditionId(8));
        round_trip(PreconditionFailureRegistryId(9));
    }

    #[test]
    fn byte_leaves_reject_noncanonical_lengths() {
        let short = read_sexp("#x00").unwrap();
        assert!(AuthorId::parse(&short).is_err());
        assert!(ConflictId::parse(&short).is_err());
        assert!(EnvelopeHash::parse(&short).is_err());
        assert!(OperationKindRegistryId::parse(&short).is_err());
        assert!(ConflictKindRegistryId::parse(&short).is_err());
        assert!(ResolutionRegistryId::parse(&short).is_err());
        assert!(RepairKindRegistryId::parse(&short).is_err());
        assert!(ReanchorReasonRegistryId::parse(&short).is_err());
        assert!(ReplicaAnomalyRegistryId::parse(&short).is_err());
        assert!(IntegrityAnomalyRegistryId::parse(&short).is_err());
        assert!(ExtensionPreconditionId::parse(&short).is_err());
        assert!(PreconditionFailureRegistryId::parse(&short).is_err());
    }

    #[test]
    fn action_round_trips_every_variant() {
        let values = [
            ResolutionAction::AcceptLoser,
            ResolutionAction::KeepWinner,
            ResolutionAction::Override {
                override_operation: epiphany_core::OperationId::new(ReplicaId(1), 2),
            },
            ResolutionAction::Reanchor {
                new_target: epiphany_core::TypedObjectId::Event(event(3)),
            },
            ResolutionAction::Dismiss,
            ResolutionAction::Registered(ResolutionRegistryId(4)),
        ];
        for value in values {
            round_trip(value);
        }
    }

    #[test]
    fn action_rejects_noncanonical_productions() {
        for bad in ["accept-winner", "(override)", "(reanchor #x00 #x01)"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(ResolutionAction::parse(&sexp).is_err(), "{bad}");
        }
    }

    #[test]
    fn policy_round_trips_every_variant() {
        for value in [
            UndoPolicy::StrictInverse,
            UndoPolicy::BestEffort,
            UndoPolicy::Cascade,
        ] {
            round_trip(value);
        }
    }

    #[test]
    fn policy_rejects_noncanonical_productions() {
        for bad in ["strict", "(cascade)"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(UndoPolicy::parse(&sexp).is_err(), "{bad}");
        }
    }

    #[test]
    fn tuplet_comp_round_trips_every_variant() {
        let rest = crate::valuegen::rest_value(
            event(9),
            VoiceId::new(ReplicaId(7), 1),
            MusicalDuration(
                RationalTime::new(1, 4).expect("one quarter has a nonzero denominator"),
            ),
        );
        for value in [
            TupletCompensation::NotInTuplet,
            TupletCompensation::ReplaceWithRest { rest },
            TupletCompensation::RewriteTuplets {
                tuplets: vec![tuplet(1), tuplet(2)],
            },
            TupletCompensation::CascadeDeleteTuplets {
                tuplets: vec![tuplet(2), tuplet(2)],
            },
        ] {
            round_trip(value);
        }
    }

    #[test]
    fn tuplet_comp_rejects_noncanonical_productions() {
        for bad in ["rewrite-tuplets", "(replace-with-rest)", "(unknown ())"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(TupletCompensation::parse(&sexp).is_err(), "{bad}");
        }
    }

    #[test]
    fn tuplet_rewrite_rejects_descending_ids() {
        let sexp = read_sexp(
            "(rewrite-tuplets (#x00000000000000070000000000000002 #x00000000000000070000000000000001))",
        )
        .unwrap();
        assert!(TupletCompensation::parse(&sexp).is_err());
    }

    #[test]
    fn tuplet_cascade_rejects_descending_ids() {
        let sexp = read_sexp(
            "(cascade-delete-tuplets (#x00000000000000070000000000000002 #x00000000000000070000000000000001))",
        )
        .unwrap();
        assert!(TupletCompensation::parse(&sexp).is_err());
    }

    #[test]
    fn cross_cutting_round_trips_every_variant() {
        let start = event(1);
        let end = event(2);
        let spanner = Spanner {
            id: SpannerId::new(ReplicaId(7), 1),
            start: TimeAnchor::Event {
                id: start,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            end: TimeAnchor::Event {
                id: end,
                offset: AnchorOffset::Musical(MusicalDuration::zero()),
            },
            staves: Vec::new(),
            kind: Default::default(),
            style: Default::default(),
        };
        let values = [
            CrossCuttingValue::Tie(crate::valuegen::tie(
                TieId::new(ReplicaId(7), 1),
                start,
                end,
            )),
            CrossCuttingValue::Slur(crate::valuegen::slur(
                SlurId::new(ReplicaId(7), 1),
                start,
                end,
            )),
            CrossCuttingValue::Beam(crate::valuegen::beam(
                BeamId::new(ReplicaId(7), 1),
                vec![start, end],
            )),
            CrossCuttingValue::Spanner(spanner),
        ];
        for value in values {
            round_trip(value);
        }
    }

    #[test]
    fn cross_cutting_rejects_noncanonical_productions() {
        for bad in ["tie", "(tie)", "(unknown #x00)"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(CrossCuttingValue::parse(&sexp).is_err(), "{bad}");
        }
    }

    #[test]
    fn remapping_round_trips_every_variant() {
        round_trip(PositionRemapping::PreserveTime);
        round_trip(PositionRemapping::Reassign(vec![
            (event(1), MusicalPosition::origin()),
            (
                event(2),
                MusicalPosition(
                    RationalTime::new(3, 4).expect("three quarters has a nonzero denominator"),
                ),
            ),
            (event(2), MusicalPosition::origin()),
        ]));
    }

    #[test]
    fn remapping_rejects_noncanonical_productions() {
        for bad in ["reassign", "(reassign)", "(unknown ())"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(PositionRemapping::parse(&sexp).is_err(), "{bad}");
        }
    }

    #[test]
    fn remapping_rejects_descending_event_ids() {
        let sexp = read_sexp(
            "(reassign ((#x00000000000000070000000000000002 (ratio 0 1)) (#x00000000000000070000000000000001 (ratio 0 1))))",
        )
        .unwrap();
        assert!(PositionRemapping::parse(&sexp).is_err());
    }

    #[test]
    fn transaction_category_round_trips_every_variant() {
        for value in [
            TransactionCategory::NoteEntry,
            TransactionCategory::Structural,
            TransactionCategory::Layout,
            TransactionCategory::Import,
            TransactionCategory::Registered(OperationKindRegistryId(5)),
        ] {
            round_trip(value);
        }
    }

    #[test]
    fn transaction_category_rejects_noncanonical_productions() {
        for bad in ["noteentry", "registered", "(registered)", "(unknown #x00)"] {
            let sexp = read_sexp(bad).unwrap();
            assert!(TransactionCategory::parse(&sexp).is_err(), "{bad}");
        }
    }
}

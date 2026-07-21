//! Grammar-directed Text Projection for primitive operation kinds.
//!
//! Operation payload records are an implementation detail: the grammar inlines
//! their fields after the Operation Catalog name, in canonical encoder order.

use epiphany_core::textvalue::{Sexp, TextError, TextValue};
use epiphany_determinism::{sorted_canonical, CanonicalEncode};
use unicode_normalization::UnicodeNormalization;

use crate::payload::{
    ChangeRegionTimeModelOp, CreateCrossCuttingOp, CreateRegionOp, CreateRepeatStructureOp,
    CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp, DeleteCrossCuttingOp, DeleteEventOp,
    DeleteIdentifiedPitchOp, DeleteRegionOp, DeleteRepeatStructureOp, DeleteStaffInstanceOp,
    DeleteVoiceOp, InsertEventOp, InsertIdentifiedPitchOp, ModifyCrossCuttingOp, ModifyEventOp,
    ModifyIdentifiedPitchOp, OperationKind, OperationKindTag, RespellPitchOp, SetMetadataOp,
    SetMetricGridOp, SetStaffLayoutOp, SetTempoSegmentOp, SetTimeSignatureOp, SetUserPageBreakOp,
    SetUserSystemBreakOp, TransactionDescriptor, TransposeIntervalOp, TransposeOp,
};
use crate::support::OperationKindRegistryId;

/// Builds one operation production from its generated catalog name and its
/// positionally inlined payload fields.
fn production(tag: OperationKindTag, fields: Vec<Sexp>) -> Sexp {
    let mut items = Vec::with_capacity(fields.len() + 1);
    items.push(Sexp::sym(tag.catalog_name()));
    items.extend(fields);
    Sexp::List(items)
}

fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

/// Resolves a production head through the generated tag vocabulary. Registered
/// kinds carry their real id in the first field, so a zero-valued tag is used
/// only to select the exhaustive parsing arm.
fn production_tag(s: &Sexp) -> Result<OperationKindTag, TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: "operation kind",
        found: class_of(s),
    })?;
    let head = items
        .first()
        .and_then(Sexp::as_symbol)
        .ok_or(TextError::Syntax(
            "an operation kind is headed by its catalog name",
        ))?;

    if let Some(tag) = OperationKindTag::PAYLOAD_FREE
        .iter()
        .copied()
        .find(|tag| tag.catalog_name() == head)
    {
        return Ok(tag);
    }

    let registered = OperationKindTag::Registered(OperationKindRegistryId::from_raw(0));
    if registered.catalog_name() == head {
        return Ok(registered);
    }

    Err(TextError::UnknownConstructor {
        type_name: "OperationKind",
        found: head.to_owned(),
    })
}

fn fields(s: &Sexp, tag: OperationKindTag, arity: usize) -> Result<&[Sexp], TextError> {
    s.expect_struct(tag.catalog_name(), arity)
}
/// Reads the grammar's opaque `bytes` terminal. `Vec<u8>`'s generic TextValue
/// implementation denotes a sequence of integers, which is a different
/// production and therefore must not be used for registered payload bytes.
fn parse_bytes(s: &Sexp) -> Result<Vec<u8>, TextError> {
    match s {
        Sexp::Bytes(bytes) => Ok(bytes.clone()),
        _ => Err(TextError::Expected {
            expected: "byte string",
            found: class_of(s),
        }),
    }
}

/// Projects and strictly parses all 31 `kind` productions from
/// `spec/text_projection.tex`.
impl TextValue for OperationKind {
    fn project(&self) -> Sexp {
        match self {
            OperationKind::InsertEvent(op) => production(
                self.tag(),
                vec![op.staff_instance.project(), op.event.project()],
            ),
            OperationKind::DeleteEvent(op) => production(
                self.tag(),
                vec![op.event.project(), op.tuplet_compensation.project()],
            ),
            OperationKind::RespellPitch(op) => {
                production(self.tag(), vec![op.pitch.project(), op.spelling.project()])
            }
            OperationKind::CreateCrossCutting(op) => {
                production(self.tag(), vec![op.structure.project()])
            }
            OperationKind::ChangeRegionTimeModel(op) => production(
                self.tag(),
                vec![
                    op.region.project(),
                    op.new_time_model.project(),
                    sorted_canonical(op.declared_incompatible.clone()).project(),
                    op.remapping.project(),
                ],
            ),
            OperationKind::SetUserSystemBreak(op) => production(
                self.tag(),
                vec![
                    op.region.project(),
                    op.anchor.project(),
                    op.present.project(),
                ],
            ),
            OperationKind::DeclareTransaction(op) => production(
                self.tag(),
                vec![
                    op.id.project(),
                    Sexp::Str(op.label.nfc().collect()),
                    op.category.project(),
                ],
            ),
            OperationKind::Registered(id, bytes) => {
                production(self.tag(), vec![id.project(), Sexp::Bytes(bytes.clone())])
            }
            OperationKind::ModifyEvent(op) => production(self.tag(), vec![op.event.project()]),
            OperationKind::Transpose(op) => production(
                self.tag(),
                vec![
                    sorted_canonical(op.targets.clone()).project(),
                    op.chromatic_steps.project(),
                ],
            ),
            OperationKind::InsertIdentifiedPitch(op) => {
                production(self.tag(), vec![op.event.project(), op.pitch.project()])
            }
            OperationKind::DeleteIdentifiedPitch(op) => {
                production(self.tag(), vec![op.pitch.project()])
            }
            OperationKind::ModifyIdentifiedPitch(op) => {
                production(self.tag(), vec![op.pitch.project(), op.value.project()])
            }
            OperationKind::DeleteCrossCutting(op) => {
                production(self.tag(), vec![op.structure.project()])
            }
            OperationKind::ModifyCrossCutting(op) => {
                production(self.tag(), vec![op.structure.project()])
            }
            OperationKind::CreateRegion(op) => production(self.tag(), vec![op.region.project()]),
            OperationKind::DeleteRegion(op) => production(self.tag(), vec![op.region.project()]),
            OperationKind::CreateStaffInstance(op) => {
                production(self.tag(), vec![op.region.project(), op.instance.project()])
            }
            OperationKind::DeleteStaffInstance(op) => {
                production(self.tag(), vec![op.staff_instance.project()])
            }
            OperationKind::CreateVoice(op) => production(
                self.tag(),
                vec![op.staff_instance.project(), op.voice.project()],
            ),
            OperationKind::DeleteVoice(op) => production(self.tag(), vec![op.voice.project()]),
            OperationKind::SetMetadata(op) => production(self.tag(), vec![op.metadata.project()]),
            OperationKind::SetMetricGrid(op) => {
                production(self.tag(), vec![op.region.project(), op.grid.project()])
            }
            OperationKind::SetUserPageBreak(op) => production(
                self.tag(),
                vec![
                    op.region.project(),
                    op.anchor.project(),
                    op.present.project(),
                ],
            ),
            OperationKind::CreateStaff(op) => production(self.tag(), vec![op.staff.project()]),
            OperationKind::SetTimeSignature(op) => production(
                self.tag(),
                vec![
                    op.region.project(),
                    op.anchor.project(),
                    op.time_signature.project(),
                ],
            ),
            OperationKind::SetTempoSegment(op) => production(
                self.tag(),
                vec![
                    op.region.project(),
                    op.start.project(),
                    op.segment.project(),
                ],
            ),
            OperationKind::SetStaffLayout(op) => production(
                self.tag(),
                vec![
                    op.staff_instance.project(),
                    op.instrument_override.project(),
                    op.staff_lines_override.project(),
                    op.visible.project(),
                ],
            ),
            OperationKind::CreateRepeatStructure(op) => {
                production(self.tag(), vec![op.repeat.project()])
            }
            OperationKind::DeleteRepeatStructure(op) => {
                production(self.tag(), vec![op.repeat.project()])
            }
            OperationKind::TransposeInterval(op) => production(
                self.tag(),
                vec![op.targets.project(), op.interval.project()],
            ),
        }
    }

    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let tag = production_tag(s)?;
        Ok(match tag {
            OperationKindTag::InsertEvent => {
                let [staff_instance, event] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::InsertEvent(InsertEventOp {
                    staff_instance: TextValue::parse(staff_instance)?,
                    event: TextValue::parse(event)?,
                })
            }
            OperationKindTag::DeleteEvent => {
                let [event, tuplet_compensation] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::DeleteEvent(DeleteEventOp {
                    event: TextValue::parse(event)?,
                    tuplet_compensation: TextValue::parse(tuplet_compensation)?,
                })
            }
            OperationKindTag::RespellPitch => {
                let [pitch, spelling] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::RespellPitch(RespellPitchOp {
                    pitch: TextValue::parse(pitch)?,
                    spelling: TextValue::parse(spelling)?,
                })
            }
            OperationKindTag::CreateCrossCutting => {
                let [structure] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
                    structure: TextValue::parse(structure)?,
                })
            }
            OperationKindTag::ChangeRegionTimeModel => {
                let [region, new_time_model, declared_incompatible, remapping] = fields(s, tag, 4)?
                else {
                    unreachable!("the arity-4 check returned four fields")
                };
                let declared_incompatible: Vec<epiphany_core::EventId> =
                    Vec::parse(declared_incompatible)?;
                // The encoder sorts this sequence. Reject disorder before storing
                // the order-preserving Vec, rather than accepting text that the
                // next projection would silently normalize.
                if declared_incompatible
                    .windows(2)
                    .any(|w| w[0].to_canonical_bytes() > w[1].to_canonical_bytes())
                {
                    return Err(TextError::NotStrictlyIncreasing(
                        "declared incompatible event ids",
                    ));
                }
                OperationKind::ChangeRegionTimeModel(ChangeRegionTimeModelOp {
                    region: TextValue::parse(region)?,
                    new_time_model: TextValue::parse(new_time_model)?,
                    declared_incompatible,
                    remapping: TextValue::parse(remapping)?,
                })
            }
            OperationKindTag::SetUserSystemBreak => {
                let [region, anchor, present] = fields(s, tag, 3)? else {
                    unreachable!("the arity-3 check returned three fields")
                };
                OperationKind::SetUserSystemBreak(SetUserSystemBreakOp {
                    region: TextValue::parse(region)?,
                    anchor: TextValue::parse(anchor)?,
                    present: TextValue::parse(present)?,
                })
            }
            OperationKindTag::DeclareTransaction => {
                let [id, label, category] = fields(s, tag, 3)? else {
                    unreachable!("the arity-3 check returned three fields")
                };
                let label = String::parse(label)?;
                // TransactionDescriptor's encoder normalizes this sole text
                // field. Mirror envdecode's inequality guard so parsing rejects
                // a spelling that the next projection would normalize.
                if label.nfc().collect::<String>() != label {
                    return Err(TextError::NotCanonical("transaction label is not NFC"));
                }
                OperationKind::DeclareTransaction(TransactionDescriptor {
                    id: TextValue::parse(id)?,
                    label,
                    category: TextValue::parse(category)?,
                })
            }
            OperationKindTag::Registered(_) => {
                let [id, bytes] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::Registered(TextValue::parse(id)?, parse_bytes(bytes)?)
            }
            OperationKindTag::ModifyEvent => {
                let [event] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::ModifyEvent(ModifyEventOp {
                    event: TextValue::parse(event)?,
                })
            }
            OperationKindTag::Transpose => {
                let [targets, chromatic_steps] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                let targets: Vec<epiphany_core::PitchId> = Vec::parse(targets)?;
                // This frozen payload is a sorted multiset, not a set. Mirror
                // envdecode's strict-decrease comparison so duplicate targets
                // remain legal and are replayed repeatedly.
                if targets
                    .windows(2)
                    .any(|w| w[0].to_canonical_bytes() > w[1].to_canonical_bytes())
                {
                    return Err(TextError::NotStrictlyIncreasing("transpose targets"));
                }
                OperationKind::Transpose(TransposeOp {
                    targets,
                    chromatic_steps: TextValue::parse(chromatic_steps)?,
                })
            }
            OperationKindTag::InsertIdentifiedPitch => {
                let [event, pitch] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::InsertIdentifiedPitch(InsertIdentifiedPitchOp {
                    event: TextValue::parse(event)?,
                    pitch: TextValue::parse(pitch)?,
                })
            }
            OperationKindTag::DeleteIdentifiedPitch => {
                let [pitch] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteIdentifiedPitch(DeleteIdentifiedPitchOp {
                    pitch: TextValue::parse(pitch)?,
                })
            }
            OperationKindTag::ModifyIdentifiedPitch => {
                let [pitch, value] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::ModifyIdentifiedPitch(ModifyIdentifiedPitchOp {
                    pitch: TextValue::parse(pitch)?,
                    value: TextValue::parse(value)?,
                })
            }
            OperationKindTag::DeleteCrossCutting => {
                let [structure] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteCrossCutting(DeleteCrossCuttingOp {
                    structure: TextValue::parse(structure)?,
                })
            }
            OperationKindTag::ModifyCrossCutting => {
                let [structure] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::ModifyCrossCutting(ModifyCrossCuttingOp {
                    structure: TextValue::parse(structure)?,
                })
            }
            OperationKindTag::InsertRegion => {
                let [region] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::CreateRegion(CreateRegionOp {
                    region: TextValue::parse(region)?,
                })
            }
            OperationKindTag::DeleteRegion => {
                let [region] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteRegion(DeleteRegionOp {
                    region: TextValue::parse(region)?,
                })
            }
            OperationKindTag::InsertStaffInstance => {
                let [region, instance] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                    region: TextValue::parse(region)?,
                    instance: TextValue::parse(instance)?,
                })
            }
            OperationKindTag::DeleteStaffInstance => {
                let [staff_instance] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteStaffInstance(DeleteStaffInstanceOp {
                    staff_instance: TextValue::parse(staff_instance)?,
                })
            }
            OperationKindTag::CreateVoice => {
                let [staff_instance, voice] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::CreateVoice(CreateVoiceOp {
                    staff_instance: TextValue::parse(staff_instance)?,
                    voice: TextValue::parse(voice)?,
                })
            }
            OperationKindTag::DeleteVoice => {
                let [voice] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteVoice(DeleteVoiceOp {
                    voice: TextValue::parse(voice)?,
                })
            }
            OperationKindTag::SetMetadata => {
                let [metadata] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::SetMetadata(SetMetadataOp {
                    metadata: TextValue::parse(metadata)?,
                })
            }
            OperationKindTag::SetMetricGrid => {
                let [region, grid] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::SetMetricGrid(SetMetricGridOp {
                    region: TextValue::parse(region)?,
                    grid: TextValue::parse(grid)?,
                })
            }
            OperationKindTag::SetUserPageBreak => {
                let [region, anchor, present] = fields(s, tag, 3)? else {
                    unreachable!("the arity-3 check returned three fields")
                };
                OperationKind::SetUserPageBreak(SetUserPageBreakOp {
                    region: TextValue::parse(region)?,
                    anchor: TextValue::parse(anchor)?,
                    present: TextValue::parse(present)?,
                })
            }
            OperationKindTag::InsertStaff => {
                let [staff] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::CreateStaff(CreateStaffOp {
                    staff: TextValue::parse(staff)?,
                })
            }
            OperationKindTag::SetTimeSignature => {
                let [region, anchor, time_signature] = fields(s, tag, 3)? else {
                    unreachable!("the arity-3 check returned three fields")
                };
                OperationKind::SetTimeSignature(SetTimeSignatureOp {
                    region: TextValue::parse(region)?,
                    anchor: TextValue::parse(anchor)?,
                    time_signature: TextValue::parse(time_signature)?,
                })
            }
            OperationKindTag::SetTempoSegment => {
                let [region, start, segment] = fields(s, tag, 3)? else {
                    unreachable!("the arity-3 check returned three fields")
                };
                OperationKind::SetTempoSegment(SetTempoSegmentOp {
                    region: TextValue::parse(region)?,
                    start: TextValue::parse(start)?,
                    segment: TextValue::parse(segment)?,
                })
            }
            OperationKindTag::SetStaffLayout => {
                let [staff_instance, instrument_override, staff_lines_override, visible] =
                    fields(s, tag, 4)?
                else {
                    unreachable!("the arity-4 check returned four fields")
                };
                OperationKind::SetStaffLayout(SetStaffLayoutOp {
                    staff_instance: TextValue::parse(staff_instance)?,
                    instrument_override: TextValue::parse(instrument_override)?,
                    staff_lines_override: TextValue::parse(staff_lines_override)?,
                    visible: TextValue::parse(visible)?,
                })
            }
            OperationKindTag::CreateRepeatStructure => {
                let [repeat] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::CreateRepeatStructure(CreateRepeatStructureOp {
                    repeat: TextValue::parse(repeat)?,
                })
            }
            OperationKindTag::DeleteRepeatStructure => {
                let [repeat] = fields(s, tag, 1)? else {
                    unreachable!("the arity-1 check returned one field")
                };
                OperationKind::DeleteRepeatStructure(DeleteRepeatStructureOp {
                    repeat: TextValue::parse(repeat)?,
                })
            }
            OperationKindTag::TransposeInterval => {
                let [targets, interval] = fields(s, tag, 2)? else {
                    unreachable!("the arity-2 check returned two fields")
                };
                OperationKind::TransposeInterval(TransposeIntervalOp {
                    // CanonicalSet's TextValue parser performs the grammar's
                    // strictly-increasing set check before BTreeSet construction.
                    targets: TextValue::parse(targets)?,
                    interval: TextValue::parse(interval)?,
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envdecode::tests::sample_kind;
    use crate::payload::{OperationKind, OperationKindTag};
    use epiphany_core::textvalue::read_sexp;

    fn all_tags() -> impl Iterator<Item = OperationKindTag> {
        OperationKindTag::PAYLOAD_FREE
            .iter()
            .copied()
            .chain(std::iter::once(OperationKindTag::Registered(
                OperationKindRegistryId::from_raw(0),
            )))
    }

    fn round_trip(value: &OperationKind) {
        let text = value.project().render();
        let sexp = read_sexp(&text).expect("the projection is valid canonical text");
        let parsed = OperationKind::parse(&sexp).expect("the projection parses");
        assert_eq!(&parsed, value);
        assert_eq!(parsed.project().render(), text);
    }

    fn swap_first_two_in_sequence(s: &mut Sexp, field: usize) {
        let Sexp::List(production) = s else {
            panic!("operation projection is a list")
        };
        let Sexp::List(sequence) = &mut production[field] else {
            panic!("selected field is a sequence")
        };
        assert!(sequence.len() >= 2, "fixture has two ordered elements");
        sequence.swap(0, 1);
    }

    fn duplicate_first_in_sequence(s: &mut Sexp, field: usize) {
        let Sexp::List(production) = s else {
            panic!("operation projection is a list")
        };
        let Sexp::List(sequence) = &mut production[field] else {
            panic!("selected field is a sequence")
        };
        assert!(sequence.len() >= 2, "fixture has two ordered elements");
        sequence[1] = sequence[0].clone();
    }

    #[test]
    fn every_operation_kind_round_trips_with_canonical_text() {
        let tags: Vec<_> = all_tags().collect();
        assert_eq!(tags.len(), 31, "the grammar has 31 kind productions");
        for tag in tags {
            round_trip(&sample_kind(tag));
        }
    }

    #[test]
    fn operation_kind_rejects_unknown_constructor_and_wrong_arity() {
        let unknown = read_sexp("(unknown-operation #x00)").expect("well-formed text");
        assert!(OperationKind::parse(&unknown).is_err());

        let sample = sample_kind(OperationKindTag::DeleteRegion);
        let Sexp::List(mut items) = sample.project() else {
            panic!("operation projection is a list")
        };
        items.push(Sexp::int(0));
        assert!(OperationKind::parse(&Sexp::List(items)).is_err());
    }

    #[test]
    fn transaction_label_projects_nfc_and_rejects_non_nfc_text() {
        let mut sample = sample_kind(OperationKindTag::DeclareTransaction);
        let OperationKind::DeclareTransaction(descriptor) = &mut sample else {
            panic!("tag fixture constructs its corresponding operation")
        };
        descriptor.label = "e\u{301}".to_owned();

        let Sexp::List(mut items) = sample.project() else {
            panic!("operation projection is a list")
        };
        assert_eq!(items[2], Sexp::Str("\u{e9}".to_owned()));

        items[2] = Sexp::Str("e\u{301}".to_owned());
        assert!(OperationKind::parse(&Sexp::List(items)).is_err());
    }

    #[test]
    fn registered_payload_uses_and_requires_the_bytes_terminal() {
        let sample = sample_kind(OperationKindTag::Registered(
            OperationKindRegistryId::from_raw(0),
        ));
        let Sexp::List(mut items) = sample.project() else {
            panic!("operation projection is a list")
        };
        assert!(matches!(
            items.as_slice(),
            [Sexp::Symbol(_), Sexp::Bytes(_), Sexp::Bytes(_)]
        ));

        items[2] = Sexp::List(vec![Sexp::int(1), Sexp::int(2)]);
        assert!(OperationKind::parse(&Sexp::List(items)).is_err());
    }

    #[test]
    fn transpose_multiset_rejects_strict_decrease_but_accepts_duplicate() {
        let sample = sample_kind(OperationKindTag::Transpose);
        let mut decreasing = sample.project();
        swap_first_two_in_sequence(&mut decreasing, 1);
        assert!(OperationKind::parse(&decreasing).is_err());

        let mut duplicated = sample.project();
        duplicate_first_in_sequence(&mut duplicated, 1);
        let parsed = OperationKind::parse(&duplicated).expect("duplicates are legal in a multiset");
        assert_eq!(parsed.project(), duplicated);
    }

    #[test]
    fn change_region_declared_incompatible_rejects_decrease_but_accepts_duplicate() {
        let sample = sample_kind(OperationKindTag::ChangeRegionTimeModel);
        let mut decreasing = sample.project();
        swap_first_two_in_sequence(&mut decreasing, 3);
        assert!(OperationKind::parse(&decreasing).is_err());

        let mut duplicated = sample.project();
        duplicate_first_in_sequence(&mut duplicated, 3);
        let parsed = OperationKind::parse(&duplicated).expect("non-decreasing permits duplicates");
        assert_eq!(parsed.project(), duplicated);
    }

    #[test]
    fn transpose_interval_targets_reject_duplicate_and_decrease() {
        let sample = sample_kind(OperationKindTag::TransposeInterval);

        let mut duplicated = sample.project();
        duplicate_first_in_sequence(&mut duplicated, 1);
        assert!(OperationKind::parse(&duplicated).is_err());

        let mut decreasing = sample.project();
        swap_first_two_in_sequence(&mut decreasing, 1);
        assert!(OperationKind::parse(&decreasing).is_err());
    }
}

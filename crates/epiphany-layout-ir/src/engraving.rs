//! Engraving-decision records (Chapter 7 §"Engraving Decisions").
//!
//! "When the engraver makes a decision (stem direction, accidental ordering,
//! beam consolidation), the decision is recorded in the IR. The decision can be
//! inspected, overridden, and traced" (Chapter 7 §"Design Principles"). The
//! pipeline records decisions explicitly so they survive every stage and remain
//! attributable to their source. v0 implements the decision *records* and their
//! provenance and override interfaces (the QUICKSTART scope item); production
//! engraving algorithms remain layered specifications beyond the v0 stub.

use epiphany_core::{StemDirection, TypedObjectId};
use epiphany_determinism::{DomainTag, Preimage};

use crate::provenance::LayoutObjectId;
use crate::spatial::Point;

/// A content-derived identifier for an engraving decision. Derived from the
/// decision's target and kind, so equal decisions on the same target share an
/// id and the records stay stable across re-engraving.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EngravingDecisionId(pub u128);

/// A registry id for an extension-defined [`EngravingDecisionKind::Registered`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EngravingDecisionRegistryId(pub u128);

/// A user-override identifier referenced by [`DecisionSource::UserOverride`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EngravingOverrideId(pub u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AuthorId(pub u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ForeignFormatId(pub u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PluginId(pub u128);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Timestamp(pub i64);

/// The authoritative or transient target of an engraving override.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OverrideTarget {
    ScoreGraph(TypedObjectId),
    IrSynthesized(LayoutObjectId),
}

/// An override's binding strength.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OverridePriority {
    Hard,
    Soft,
}

/// Provenance of a user/import/plugin override.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OverrideOrigin {
    User {
        author: AuthorId,
        timestamp: Timestamp,
    },
    Import {
        format: ForeignFormatId,
    },
    Plugin {
        plugin: PluginId,
    },
    Internal,
}

/// Core override vocabulary. More detailed engraving payloads are represented
/// by stable registered ids until their companion algorithm specifications land.
#[derive(Clone, PartialEq, Debug)]
pub enum OverrideKind {
    StemDirection(StemDirection),
    AccidentalParenthesized(bool),
    AccidentalVisible(bool),
    SystemBreak,
    PageBreak,
    HiddenObject,
    CustomPosition(Point),
    LedgerLineSuppression,
    Registered(u128),
}

/// A projected engraving override (Chapter 7 §"Engraving Overrides").
#[derive(Clone, PartialEq, Debug)]
pub struct EngravingOverride {
    pub id: EngravingOverrideId,
    pub target: OverrideTarget,
    pub kind: OverrideKind,
    pub priority: OverridePriority,
    pub origin: OverrideOrigin,
}

/// Where an engraving decision came from (Chapter 7 §"Note Layout":
/// `DecisionSource`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DecisionSource {
    /// Derived from automatic engraving rules.
    Automatic,
    /// Derived from a user override in the score graph.
    UserOverride(EngravingOverrideId),
    /// Derived from an IR-stage override.
    IrOverride,
}

/// A decision the engraver recorded (Chapter 7 §"Engraving Decisions":
/// `EngravingDecisionKind`). v0 carries a representative subset; the catalog is
/// extensible (the trailing `Registered` variant).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EngravingDecisionKind {
    /// The stem direction chosen for a note or chord.
    StemDirection(StemDirection),
    /// The number of ledger lines a note requires.
    LedgerLineCount(u8),
    /// A system break placed here.
    SystemBreak,
    /// A page break placed here.
    PageBreak,
    /// An extension-defined decision kind.
    Registered(EngravingDecisionRegistryId),
}

impl EngravingDecisionKind {
    /// A stable discriminant byte, part of the decision-id preimage.
    fn discriminant(&self) -> u8 {
        match self {
            EngravingDecisionKind::StemDirection(_) => 0,
            EngravingDecisionKind::LedgerLineCount(_) => 1,
            EngravingDecisionKind::SystemBreak => 2,
            EngravingDecisionKind::PageBreak => 3,
            EngravingDecisionKind::Registered(_) => 4,
        }
    }
}

/// An engraving-decision record (Chapter 7 §"Engraving Decisions":
/// `EngravingDecision`). Carried forward through every IR stage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EngravingDecision {
    pub id: EngravingDecisionId,
    pub target: LayoutObjectId,
    pub kind: EngravingDecisionKind,
    pub source: DecisionSource,
}

impl EngravingDecision {
    /// An automatic decision on `target`, with a content-derived id.
    pub fn automatic(target: LayoutObjectId, kind: EngravingDecisionKind) -> Self {
        let source = DecisionSource::Automatic;
        EngravingDecision {
            id: derive_decision_id(target, &kind, source),
            target,
            kind,
            source,
        }
    }

    /// A decision on `target` attributed to `source`, with a content-derived id
    /// that includes the source (so the same target+kind from an automatic rule
    /// and from a user override are distinct decisions).
    pub fn with_source(
        target: LayoutObjectId,
        kind: EngravingDecisionKind,
        source: DecisionSource,
    ) -> Self {
        EngravingDecision {
            id: derive_decision_id(target, &kind, source),
            target,
            kind,
            source,
        }
    }
}

/// Derives an [`EngravingDecisionId`] from its target, kind, and source, so
/// equal decisions share an id and differing ones do not.
///
/// An engraving decision is a non-canonical layout-namespace object, so the
/// preimage is domain-separated under the layout tag
/// [`DomainTag::LAYOUT_OBJECT_ID`] (`MUSCLOID`) — the same tag as
/// [`crate::provenance::LayoutObjectId`] — with a literal `engraving-decision`
/// discriminator prefix so a decision id can never alias a layout-object id
/// within that namespace.
fn derive_decision_id(
    target: LayoutObjectId,
    kind: &EngravingDecisionKind,
    source: DecisionSource,
) -> EngravingDecisionId {
    let mut p = Preimage::new(DomainTag::LAYOUT_OBJECT_ID);
    p.push_bytes(b"engraving-decision");
    p.push_u64_le((target.0 >> 64) as u64);
    p.push_u64_le(target.0 as u64);
    p.push_u64_le(kind.discriminant() as u64);
    match kind {
        EngravingDecisionKind::StemDirection(d) => {
            p.push_u64_le(matches!(d, StemDirection::Up) as u64);
        }
        EngravingDecisionKind::LedgerLineCount(n) => {
            p.push_u64_le(*n as u64);
        }
        EngravingDecisionKind::Registered(r) => {
            p.push_u64_le((r.0 >> 64) as u64);
            p.push_u64_le(r.0 as u64);
        }
        EngravingDecisionKind::SystemBreak | EngravingDecisionKind::PageBreak => {}
    }
    match source {
        DecisionSource::Automatic => {
            p.push_u64_le(0);
        }
        DecisionSource::UserOverride(id) => {
            p.push_u64_le(1);
            p.push_u64_le((id.0 >> 64) as u64);
            p.push_u64_le(id.0 as u64);
        }
        DecisionSource::IrOverride => {
            p.push_u64_le(2);
        }
    }
    EngravingDecisionId(p.finish_trunc128())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_id_is_content_derived_and_stable() {
        let target = LayoutObjectId(0xABCD);
        let a = EngravingDecision::automatic(target, EngravingDecisionKind::SystemBreak);
        let b = EngravingDecision::automatic(target, EngravingDecisionKind::SystemBreak);
        assert_eq!(a, b);
        // Different kind on the same target → different id.
        let c = EngravingDecision::automatic(target, EngravingDecisionKind::PageBreak);
        assert_ne!(a.id, c.id);
        // Different target → different id.
        let d = EngravingDecision::automatic(LayoutObjectId(1), EngravingDecisionKind::SystemBreak);
        assert_ne!(a.id, d.id);
    }

    #[test]
    fn decision_source_changes_the_id() {
        let target = LayoutObjectId(5);
        let auto = EngravingDecision::automatic(target, EngravingDecisionKind::SystemBreak);
        let over = EngravingDecision::with_source(
            target,
            EngravingDecisionKind::SystemBreak,
            DecisionSource::IrOverride,
        );
        assert_eq!(auto.source, DecisionSource::Automatic);
        assert_ne!(
            auto.id, over.id,
            "the decision source participates in the id"
        );
    }

    #[test]
    fn stem_direction_payload_changes_the_id() {
        let target = LayoutObjectId(7);
        let up = EngravingDecision::automatic(
            target,
            EngravingDecisionKind::StemDirection(StemDirection::Up),
        );
        let down = EngravingDecision::automatic(
            target,
            EngravingDecisionKind::StemDirection(StemDirection::Down),
        );
        assert_ne!(up.id, down.id);
    }
}

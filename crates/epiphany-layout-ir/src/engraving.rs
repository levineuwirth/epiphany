//! Engraving-decision records (Chapter 7 §"Engraving Decisions").
//!
//! "When the engraver makes a decision (stem direction, accidental ordering,
//! beam consolidation), the decision is recorded in the IR. The decision can be
//! inspected, overridden, and traced" (Chapter 7 §"Design Principles"). The
//! pipeline records decisions explicitly so they survive every stage and remain
//! attributable to their source. v0 implements the decision *records* and their
//! provenance and override interfaces (the QUICKSTART scope item); production
//! engraving algorithms remain layered specifications beyond the v0 stub.

use epiphany_core::{CanonicalValue, RegionId, StemDirection, TimeAnchor, TypedObjectId};
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
///
/// A break override addresses a *position*, not an object (Chapter 7
/// §"Engraving Overrides"): the kind carries the break's [`TimeAnchor`], while
/// the override's `ScoreGraph` target names the owning region.
#[derive(Clone, PartialEq, Debug)]
pub enum OverrideKind {
    StemDirection(StemDirection),
    AccidentalParenthesized(bool),
    AccidentalVisible(bool),
    SystemBreak { anchor: TimeAnchor },
    PageBreak { anchor: TimeAnchor },
    HiddenObject,
    CustomPosition(Point),
    LedgerLineSuppression,
    Registered(u128),
}

impl OverrideKind {
    /// A stable discriminant byte, part of the override-id preimage and the
    /// projection's deterministic ordering key.
    pub(crate) fn discriminant(&self) -> u8 {
        match self {
            OverrideKind::StemDirection(_) => 0,
            OverrideKind::AccidentalParenthesized(_) => 1,
            OverrideKind::AccidentalVisible(_) => 2,
            OverrideKind::SystemBreak { .. } => 3,
            OverrideKind::PageBreak { .. } => 4,
            OverrideKind::HiddenObject => 5,
            OverrideKind::CustomPosition(_) => 6,
            OverrideKind::LedgerLineSuppression => 7,
            OverrideKind::Registered(_) => 8,
        }
    }
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

impl EngravingOverride {
    /// A system-break override projected from a region's authoritative
    /// `user_system_breaks` list (Chapter 5 §"Staff-Based Content").
    pub fn projected_system_break(region: RegionId, anchor: TimeAnchor) -> Self {
        Self::projected_break(region, OverrideKind::SystemBreak { anchor })
    }

    /// A page-break override projected from a region's authoritative
    /// `user_page_breaks` list (Chapter 5 §"Staff-Based Content").
    pub fn projected_page_break(region: RegionId, anchor: TimeAnchor) -> Self {
        Self::projected_break(region, OverrideKind::PageBreak { anchor })
    }

    /// The shared shape of a projected break override (Chapter 7 §"Engraving
    /// Overrides"): the kind carries the break's anchor, the `ScoreGraph`
    /// target names the owning region, the binding is `Soft` (the layout
    /// SHOULD honor it), and the origin is `Internal` — break authorship
    /// (author, timestamp) lives in the operation log, not the materialized
    /// break lists, until the snapshot-undo refinement (P11-C8) surfaces it.
    fn projected_break(region: RegionId, kind: OverrideKind) -> Self {
        EngravingOverride {
            id: derive_break_override_id(region, &kind),
            target: OverrideTarget::ScoreGraph(TypedObjectId::Region(region)),
            kind,
            priority: OverridePriority::Soft,
            origin: OverrideOrigin::Internal,
        }
    }
}

/// Derives an [`EngravingOverrideId`] for a projected break override from its
/// owning region, its kind discriminant, and the break anchor's canonical
/// bytes — so equal breaks share an id across re-projection and distinct ones
/// never collide.
///
/// Like the engraving-decision id, the override is a non-canonical
/// layout-namespace object, so the preimage is domain-separated under
/// [`DomainTag::LAYOUT_OBJECT_ID`] (`MUSCLOID`) with a literal
/// `engraving-override` discriminator prefix, so an override id can alias
/// neither a layout-object id nor a decision id within that namespace.
fn derive_break_override_id(region: RegionId, kind: &OverrideKind) -> EngravingOverrideId {
    let anchor = match kind {
        OverrideKind::SystemBreak { anchor } | OverrideKind::PageBreak { anchor } => anchor,
        _ => unreachable!("projected break overrides carry a break kind"),
    };
    let mut p = Preimage::new(DomainTag::LAYOUT_OBJECT_ID);
    p.push_bytes(b"engraving-override");
    p.push_bytes(&region.canonical_bytes());
    p.push_u64_le(kind.discriminant() as u64);
    p.push_bytes(&anchor.canonical_bytes());
    EngravingOverrideId(p.finish_trunc128())
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
    fn projected_break_override_ids_are_content_derived() {
        use epiphany_core::{RegionId, WallClockTime};
        let region = RegionId::from_raw(9);
        let anchor = TimeAnchor::WallClock {
            time: WallClockTime(7),
        };
        let a = EngravingOverride::projected_system_break(region, anchor.clone());
        let b = EngravingOverride::projected_system_break(region, anchor.clone());
        assert_eq!(a, b, "equal breaks share an id across re-projection");
        // A page break at the same anchor is a distinct override…
        let page = EngravingOverride::projected_page_break(region, anchor.clone());
        assert_ne!(a.id, page.id);
        // …as is the same break at a different anchor…
        let other = EngravingOverride::projected_system_break(
            region,
            TimeAnchor::WallClock {
                time: WallClockTime(8),
            },
        );
        assert_ne!(a.id, other.id);
        // …or in a different owning region.
        let elsewhere =
            EngravingOverride::projected_system_break(RegionId::from_raw(10), anchor.clone());
        assert_ne!(a.id, elsewhere.id);
        // The projected shape the spec pins: the kind carries the break anchor,
        // the ScoreGraph target names the owning region, the binding is Soft,
        // and the origin is Internal (authorship lives in the op log, P11-C8).
        assert_eq!(a.kind, OverrideKind::SystemBreak { anchor });
        assert_eq!(
            a.target,
            OverrideTarget::ScoreGraph(TypedObjectId::Region(region))
        );
        assert_eq!(a.priority, OverridePriority::Soft);
        assert_eq!(a.origin, OverrideOrigin::Internal);
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

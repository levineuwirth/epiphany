//! The per-region time axis (Chapter 7 §"Layout Regions" / §"The Time Axis").
//!
//! [`TimeAxisModel`] is a **tagged enum** over the three built-in region time
//! models plus a registered variant for extension-defined axes — *not*
//! `Box<dyn TimeAxis>` (QUICKSTART, Agent E; Chapter 7: "The enum form is
//! canonical for serialization, hashing, and conformance comparison"). The
//! dynamic [`TimeAxis`] interface is also provided for spacing implementations.

use epiphany_core::{MusicalPosition, Region, RegionTimeModel, WallClockTime};

use crate::{SpringSlotId, StaffSpace};

/// A registry id for an extension-defined [`TimeAxisModel::Registered`]
/// (Chapter 7: `TimeAxisRegistryId`). Opaque in v0 (no external registries).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TimeAxisRegistryId(pub u128);

/// Canonical opaque payload for an extension-defined time axis.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SerializedRegisteredAxis(pub Vec<u8>);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TimePoint {
    Musical(MusicalPosition),
    WallClock(WallClockTime),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TimeRange {
    Musical {
        start: MusicalPosition,
        end: MusicalPosition,
    },
    WallClock {
        start: WallClockTime,
        end: WallClockTime,
    },
}

/// Metric-axis projection data. The prototype populates slots during spacing.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MetricTimeAxis {
    pub slots: Vec<SpringSlotId>,
}

/// Proportional-axis projection data.
#[derive(Clone, PartialEq, Debug)]
pub struct ProportionalTimeAxis {
    pub duration_ns: i64,
    pub space_per_second: StaffSpace,
    pub slots: Vec<SpringSlotId>,
}

/// Aleatoric-axis projection data. Slot order is topological layer order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AleatoricTimeAxis {
    pub slots: Vec<SpringSlotId>,
}

/// The canonical representation of a region's time axis (Chapter 7). The
/// variants correspond to the three built-in [`RegionTimeModel`]s; the
/// `Registered` variant carries an extension-defined axis by registry id.
#[derive(Clone, PartialEq, Debug)]
pub enum TimeAxisModel {
    /// Metric time: positions map through measures and beats.
    Metric(MetricTimeAxis),
    /// Proportional time: horizontal position is linear in wall-clock time over
    /// the region's `duration_ns` nanoseconds.
    Proportional(ProportionalTimeAxis),
    /// Aleatoric time: ordering is a DAG, not a metric line.
    Aleatoric(AleatoricTimeAxis),
    /// An extension-defined axis kind.
    Registered(TimeAxisRegistryId, SerializedRegisteredAxis),
}

/// The built-in axis kinds, the discriminator of [`TimeAxisModel`]
/// (Chapter 7: `TimeAxisKind`).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TimeAxisKind {
    Metric,
    Proportional,
    Aleatoric,
    Registered(TimeAxisRegistryId),
}

/// Dynamic time-axis interface used by spacing implementations. The tagged
/// [`TimeAxisModel`] remains the canonical representation.
pub trait TimeAxis: Send + Sync {
    fn kind(&self) -> TimeAxisKind;
    fn project(&self, time: TimePoint) -> SpringSlotId;
    fn slots(&self) -> &[SpringSlotId];
    fn affected_slots(&self, range: TimeRange) -> Vec<SpringSlotId>;
}

impl TimeAxisModel {
    /// This axis's kind discriminator.
    pub fn kind(&self) -> TimeAxisKind {
        match self {
            TimeAxisModel::Metric(_) => TimeAxisKind::Metric,
            TimeAxisModel::Proportional(_) => TimeAxisKind::Proportional,
            TimeAxisModel::Aleatoric(_) => TimeAxisKind::Aleatoric,
            TimeAxisModel::Registered(id, _) => TimeAxisKind::Registered(*id),
        }
    }
}

impl TimeAxis for TimeAxisModel {
    fn kind(&self) -> TimeAxisKind {
        TimeAxisModel::kind(self)
    }

    fn project(&self, _time: TimePoint) -> SpringSlotId {
        self.slots().first().copied().unwrap_or(SpringSlotId(0))
    }

    fn slots(&self) -> &[SpringSlotId] {
        match self {
            TimeAxisModel::Metric(axis) => &axis.slots,
            TimeAxisModel::Proportional(axis) => &axis.slots,
            TimeAxisModel::Aleatoric(axis) => &axis.slots,
            TimeAxisModel::Registered(_, _) => &[],
        }
    }

    fn affected_slots(&self, _range: TimeRange) -> Vec<SpringSlotId> {
        self.slots().to_vec()
    }
}

/// Maps a score region's [`RegionTimeModel`] to its layout [`TimeAxisModel`]
/// (Chapter 7 §"Region Uniformity": the region kind shows up here and in the
/// object mix, never in the container type).
pub fn time_axis_of(region: &Region) -> TimeAxisModel {
    match &region.time_model {
        RegionTimeModel::Metric(_) => TimeAxisModel::Metric(MetricTimeAxis::default()),
        RegionTimeModel::Proportional(p) => TimeAxisModel::Proportional(ProportionalTimeAxis {
            duration_ns: p.duration.0,
            space_per_second: StaffSpace(1.0),
            slots: Vec::new(),
        }),
        RegionTimeModel::Aleatoric(_) => TimeAxisModel::Aleatoric(AleatoricTimeAxis::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_matches_variant() {
        assert_eq!(
            TimeAxisModel::Metric(MetricTimeAxis::default()).kind(),
            TimeAxisKind::Metric
        );
        assert_eq!(
            TimeAxisModel::Proportional(ProportionalTimeAxis {
                duration_ns: 42,
                space_per_second: StaffSpace(1.0),
                slots: vec![],
            })
            .kind(),
            TimeAxisKind::Proportional
        );
        assert_eq!(
            TimeAxisModel::Aleatoric(AleatoricTimeAxis::default()).kind(),
            TimeAxisKind::Aleatoric
        );
        let r = TimeAxisRegistryId(7);
        assert_eq!(
            TimeAxisModel::Registered(r, SerializedRegisteredAxis::default()).kind(),
            TimeAxisKind::Registered(r)
        );
    }
}

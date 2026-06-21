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

/// One placement on the time axis: the spring slot occupying a given time.
/// Placements are held in ascending time order, so the axis maps a queried time
/// to the slot covering it (the greatest placement at or before the query).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotPlacement {
    pub time: TimePoint,
    pub slot: SpringSlotId,
}

/// Metric-axis projection data: the time→slot placements, populated during
/// spacing (a region's measure/beat grid resolves to ordered spring slots).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MetricTimeAxis {
    pub placements: Vec<SlotPlacement>,
}

/// Proportional-axis projection data: horizontal position is linear in
/// wall-clock time, and the placements map each slot's time onto the axis.
#[derive(Clone, PartialEq, Debug)]
pub struct ProportionalTimeAxis {
    pub duration_ns: i64,
    pub space_per_second: StaffSpace,
    pub placements: Vec<SlotPlacement>,
}

/// Aleatoric-axis projection data. Placement order is topological layer order.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AleatoricTimeAxis {
    pub placements: Vec<SlotPlacement>,
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

/// Compares two [`TimePoint`]s of the *same* kind; mixed kinds are
/// incomparable (`None`), which a uniform region never produces.
fn time_cmp(a: &TimePoint, b: &TimePoint) -> Option<core::cmp::Ordering> {
    match (a, b) {
        (TimePoint::Musical(x), TimePoint::Musical(y)) => Some(x.cmp(y)),
        (TimePoint::WallClock(x), TimePoint::WallClock(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Dynamic time-axis interface used by spacing implementations. The tagged
/// [`TimeAxisModel`] remains the canonical representation.
pub trait TimeAxis: Send + Sync {
    fn kind(&self) -> TimeAxisKind;
    /// The spring slot covering `time`: the placement with the greatest time at
    /// or before `time` (or the first placement if `time` precedes them all).
    fn project(&self, time: TimePoint) -> SpringSlotId;
    /// The spring slots in time order.
    fn slots(&self) -> Vec<SpringSlotId>;
    /// The spring slots whose time falls in the half-open `range` `[start, end)`.
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

    /// The axis's time→slot placements, in ascending time order.
    pub fn placements(&self) -> &[SlotPlacement] {
        match self {
            TimeAxisModel::Metric(axis) => &axis.placements,
            TimeAxisModel::Proportional(axis) => &axis.placements,
            TimeAxisModel::Aleatoric(axis) => &axis.placements,
            TimeAxisModel::Registered(_, _) => &[],
        }
    }

    /// Returns this axis populated with `placements` (sorted into ascending time
    /// order). A `Registered` axis is opaque and returned unchanged. This is how
    /// the spacing stage drives the axis from its resolved spring slots.
    pub fn with_placements(self, mut placements: Vec<SlotPlacement>) -> Self {
        placements.sort_by(|a, b| time_cmp(&a.time, &b.time).unwrap_or(core::cmp::Ordering::Equal));
        match self {
            TimeAxisModel::Metric(mut axis) => {
                axis.placements = placements;
                TimeAxisModel::Metric(axis)
            }
            TimeAxisModel::Proportional(mut axis) => {
                axis.placements = placements;
                TimeAxisModel::Proportional(axis)
            }
            TimeAxisModel::Aleatoric(mut axis) => {
                axis.placements = placements;
                TimeAxisModel::Aleatoric(axis)
            }
            other @ TimeAxisModel::Registered(_, _) => other,
        }
    }
}

impl TimeAxis for TimeAxisModel {
    fn kind(&self) -> TimeAxisKind {
        TimeAxisModel::kind(self)
    }

    fn project(&self, time: TimePoint) -> SpringSlotId {
        let placements = self.placements();
        // The greatest placement at or before `time` (the slot covering it)...
        let covering = placements.iter().rfind(|p| {
            matches!(
                time_cmp(&p.time, &time),
                Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
            )
        });
        // ...or the first placement when `time` precedes them all.
        covering
            .or_else(|| placements.first())
            .map(|p| p.slot)
            .unwrap_or(SpringSlotId(0))
    }

    fn slots(&self) -> Vec<SpringSlotId> {
        self.placements().iter().map(|p| p.slot).collect()
    }

    fn affected_slots(&self, range: TimeRange) -> Vec<SpringSlotId> {
        let (start, end) = match range {
            TimeRange::Musical { start, end } => {
                (TimePoint::Musical(start), TimePoint::Musical(end))
            }
            TimeRange::WallClock { start, end } => {
                (TimePoint::WallClock(start), TimePoint::WallClock(end))
            }
        };
        self.placements()
            .iter()
            .filter(|p| {
                let at_or_after_start = matches!(
                    time_cmp(&p.time, &start),
                    Some(core::cmp::Ordering::Greater | core::cmp::Ordering::Equal)
                );
                let before_end = matches!(time_cmp(&p.time, &end), Some(core::cmp::Ordering::Less));
                at_or_after_start && before_end
            })
            .map(|p| p.slot)
            .collect()
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
            placements: Vec::new(),
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
                placements: vec![],
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

    #[test]
    fn project_and_affected_slots_consume_placements() {
        // Built out of time order; `with_placements` sorts by time.
        let axis = TimeAxisModel::Metric(MetricTimeAxis::default()).with_placements(vec![
            SlotPlacement {
                time: TimePoint::WallClock(WallClockTime(10)),
                slot: SpringSlotId(1),
            },
            SlotPlacement {
                time: TimePoint::WallClock(WallClockTime(30)),
                slot: SpringSlotId(3),
            },
            SlotPlacement {
                time: TimePoint::WallClock(WallClockTime(20)),
                slot: SpringSlotId(2),
            },
        ]);

        // Slots come back in ascending time order.
        assert_eq!(
            axis.slots(),
            vec![SpringSlotId(1), SpringSlotId(2), SpringSlotId(3)]
        );
        // project() returns the covering slot (greatest time <= query)...
        assert_eq!(
            axis.project(TimePoint::WallClock(WallClockTime(25))),
            SpringSlotId(2)
        );
        assert_eq!(
            axis.project(TimePoint::WallClock(WallClockTime(30))),
            SpringSlotId(3)
        );
        // ...and the first slot when the query precedes every placement.
        assert_eq!(
            axis.project(TimePoint::WallClock(WallClockTime(5))),
            SpringSlotId(1)
        );
        // affected_slots() respects the half-open range [10, 30): excludes 30.
        assert_eq!(
            axis.affected_slots(TimeRange::WallClock {
                start: WallClockTime(10),
                end: WallClockTime(30),
            }),
            vec![SpringSlotId(1), SpringSlotId(2)]
        );

        // An empty axis projects to the default slot and has no slots.
        let empty = TimeAxisModel::Metric(MetricTimeAxis::default());
        assert_eq!(
            empty.project(TimePoint::WallClock(WallClockTime(0))),
            SpringSlotId(0)
        );
        assert!(empty.slots().is_empty());
    }
}

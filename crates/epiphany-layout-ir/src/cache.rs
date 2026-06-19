//! Incremental-layout dependency index and stage cache (Chapter 7).

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{RegionId, TypedObjectId};

use crate::{LayoutObjectId, Provenance};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SystemId(pub u128);

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LogicalRegionCache {
    pub objects: BTreeSet<LayoutObjectId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ConstrainedRegionCache {
    pub objects: BTreeSet<LayoutObjectId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ResolvedSystemCache {
    pub objects: BTreeSet<LayoutObjectId>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FineLayoutCache {
    pub objects: BTreeSet<LayoutObjectId>,
}

/// Bidirectional score-object/layout-object dependency index.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DependencyIndex {
    pub forward: BTreeMap<TypedObjectId, BTreeSet<LayoutObjectId>>,
    pub reverse: BTreeMap<LayoutObjectId, BTreeSet<TypedObjectId>>,
}

impl DependencyIndex {
    pub fn insert(&mut self, provenance: &Provenance) {
        self.remove(provenance.stable_id);
        let dependencies: BTreeSet<_> = std::iter::once(provenance.source)
            .chain(provenance.dependencies.iter().copied())
            .collect();
        for dependency in &dependencies {
            self.forward
                .entry(*dependency)
                .or_default()
                .insert(provenance.stable_id);
        }
        self.reverse.insert(provenance.stable_id, dependencies);
    }

    pub fn remove(&mut self, object: LayoutObjectId) {
        let Some(dependencies) = self.reverse.remove(&object) else {
            return;
        };
        for dependency in dependencies {
            if let Some(objects) = self.forward.get_mut(&dependency) {
                objects.remove(&object);
                if objects.is_empty() {
                    self.forward.remove(&dependency);
                }
            }
        }
    }

    pub fn affected_by(
        &self,
        changed: impl IntoIterator<Item = TypedObjectId>,
    ) -> BTreeSet<LayoutObjectId> {
        changed
            .into_iter()
            .filter_map(|object| self.forward.get(&object))
            .flat_map(|objects| objects.iter().copied())
            .collect()
    }
}

/// Cached stage partitions with fine-grained invalidation.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LayoutCache {
    pub dependencies: DependencyIndex,
    pub logical: BTreeMap<RegionId, LogicalRegionCache>,
    pub constrained: BTreeMap<RegionId, ConstrainedRegionCache>,
    pub resolved: BTreeMap<SystemId, ResolvedSystemCache>,
    pub fine_cache: FineLayoutCache,
}

impl LayoutCache {
    /// Invalidates every indexed layout object depending on `changed` and
    /// removes it from all stage partitions.
    pub fn invalidate(
        &mut self,
        changed: impl IntoIterator<Item = TypedObjectId>,
    ) -> BTreeSet<LayoutObjectId> {
        let invalidated = self.dependencies.affected_by(changed);
        for object in &invalidated {
            self.dependencies.remove(*object);
            self.fine_cache.objects.remove(object);
            for cache in self.logical.values_mut() {
                cache.objects.remove(object);
            }
            for cache in self.constrained.values_mut() {
                cache.objects.remove(object);
            }
            for cache in self.resolved.values_mut() {
                cache.objects.remove(object);
            }
        }
        invalidated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{EventId, StaffId};

    #[test]
    fn dependency_index_invalidates_source_and_additional_dependencies() {
        let source = TypedObjectId::Event(EventId::from_raw(1));
        let dependency = TypedObjectId::Staff(StaffId::from_raw(2));
        let provenance = Provenance::projected(source, vec![dependency]);
        let mut cache = LayoutCache::default();
        cache.dependencies.insert(&provenance);
        cache.fine_cache.objects.insert(provenance.stable_id);

        let invalidated = cache.invalidate([dependency]);
        assert_eq!(invalidated, BTreeSet::from([provenance.stable_id]));
        assert!(cache.fine_cache.objects.is_empty());
        assert!(cache.dependencies.reverse.is_empty());
    }
}

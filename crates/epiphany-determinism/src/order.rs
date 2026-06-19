//! Canonical iteration order.
//!
//! Appendix D §"Ordered Iteration over Sets and Maps": whenever canonical
//! output, serialization, or hashing depends on iterating a collection, the
//! iteration **must** occur in a specified total order. Hash-map iteration
//! order and B-tree implementation differences must never leak into canonical
//! output.
//!
//! This module provides the tools that make that easy to obey:
//!
//! * [`CanonicalMap`] / [`CanonicalSet`] — `BTreeMap` / `BTreeSet` aliases.
//!   A `BTree*` iterates in key order by construction, so reaching for these
//!   instead of `HashMap` / `HashSet` removes the most common leak at the
//!   type level. (Use them when the key's `Ord` already *is* the canonical
//!   order — e.g. typed identifiers compared on their canonical byte form.)
//! * [`sort_canonical`] / [`sorted_canonical`] — sort a slice/vector of
//!   [`CanonicalByteOrder`] elements by their canonical byte form. Use these to
//!   project a `HashMap`'s entries, or any unordered batch, into the normative
//!   order right before serialization or hashing.
//!
//! Crucially, byte-lexicographic order is **not** universal. Appendix D defines
//! several non-byte orders: operation envelopes (causal → HLC physical → HLC
//! logical → id), chunk references (kind → content hash → offset), and rational
//! sequences (numeric value). Byte-sorting those would be *wrong*. The
//! [`CanonicalByteOrder`] marker gates [`sort_canonical`] to exactly the types
//! whose canonical order is their byte order (identifiers, content hashes, NFC
//! strings); types with a different order implement their own comparison and
//! must not carry the marker.

use crate::serialize::CanonicalEncode;
use std::collections::{BTreeMap, BTreeSet};

/// A map whose iteration order is its key order. Use where the key's `Ord` is
/// the canonical order. Drop-in for `BTreeMap`.
pub type CanonicalMap<K, V> = BTreeMap<K, V>;

/// A set whose iteration order is its element order. Drop-in for `BTreeSet`.
pub type CanonicalSet<T> = BTreeSet<T>;

/// Marker for types whose canonical total order **is** the lexicographic order
/// of their canonical bytes (Appendix D §"Ordered Iteration": typed
/// identifiers, content hashes, conflict ids, NFC strings).
///
/// Implement this only when byte order equals the spec's canonical order for
/// the type. Types ordered some other way — operation envelopes (causal/HLC),
/// chunk references (kind → hash → offset), rationals (numeric value) — **must
/// not** implement it; they provide their own ordering and are not eligible for
/// [`sort_canonical`].
pub trait CanonicalByteOrder: CanonicalEncode {}

/// Sorts `items` in place, ascending by each element's canonical byte form.
///
/// Restricted to [`CanonicalByteOrder`] types, so it cannot be misapplied to a
/// type whose canonical order is not byte-lexicographic. This is the
/// deterministic replacement for "iterate the `HashMap`": collect the entries,
/// then put them in canonical order before they affect canonical output.
pub fn sort_canonical<T: CanonicalByteOrder>(items: &mut [T]) {
    items.sort_by_key(|a| a.to_canonical_bytes());
}

/// Returns `items` sorted ascending by canonical byte form.
pub fn sorted_canonical<T: CanonicalByteOrder>(mut items: Vec<T>) -> Vec<T> {
    sort_canonical(&mut items);
    items
}

// Byte-lexicographic order is the canonical order for these: content hashes
// (chunk-ref ordering, Appendix D) and the byte-only domain tags. Typed graph
// identifiers in epiphany-core implement this marker there. Numeric types
// (`QuantizedCoord`, `CanonicalF64`) intentionally do NOT: their byte order is
// not their canonical order, so `sort_canonical` rejects them at compile time.
impl CanonicalByteOrder for crate::hash::ContentHash {}
impl CanonicalByteOrder for crate::hash::ChunkId {}
impl CanonicalByteOrder for crate::domain::DomainTag {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{ChunkId, ContentHash};
    use std::collections::HashMap;

    fn id(n: u8) -> ChunkId {
        ChunkId(ContentHash([n; 32]))
    }

    #[test]
    fn sort_canonical_orders_by_canonical_bytes() {
        let mut v = vec![id(3), id(1), id(2)];
        sort_canonical(&mut v);
        let by_bytes: Vec<_> = v.iter().map(|q| q.to_canonical_bytes()).collect();
        let mut expected = by_bytes.clone();
        expected.sort();
        assert_eq!(by_bytes, expected);
    }

    #[test]
    fn hashmap_projection_is_order_independent() {
        // Insert in two different orders; canonical projection must match.
        let mut a: HashMap<u8, ChunkId> = HashMap::new();
        let mut b: HashMap<u8, ChunkId> = HashMap::new();
        for k in [5u8, 1, 9, 3, 7] {
            a.insert(k, id(k));
        }
        for k in [7u8, 3, 9, 1, 5] {
            b.insert(k, id(k));
        }
        let proj = |m: &HashMap<u8, ChunkId>| {
            sorted_canonical(m.values().copied().collect::<Vec<_>>())
                .iter()
                .flat_map(|q| q.to_canonical_bytes())
                .collect::<Vec<u8>>()
        };
        assert_eq!(proj(&a), proj(&b));
    }

    #[test]
    fn canonical_map_iterates_in_key_order() {
        let mut m: CanonicalMap<u32, &str> = CanonicalMap::new();
        m.insert(3, "c");
        m.insert(1, "a");
        m.insert(2, "b");
        let keys: Vec<u32> = m.keys().copied().collect();
        assert_eq!(keys, vec![1, 2, 3]);
    }
}

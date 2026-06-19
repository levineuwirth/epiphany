//! Cross-module checks of the Appendix D guarantees, driven only through the
//! crate's public surface. These complement the per-module unit tests by
//! asserting the contract the way downstream crates will consume it.

use epiphany_determinism::{
    blake3_256, canonical_f64_bytes, sorted_canonical, CanonicalDecode, CanonicalEncode,
    CanonicalF64, ChunkId, ContentHash, DomainTag, Preimage, QuantizedCoord, SystemDomainTag,
    Tolerance, ToleranceClass, ToleranceGovernance,
};

#[test]
fn negative_zero_is_invisible_to_canonical_state() {
    // Appendix D §"Permitted Forms": -0.0 canonicalizes to +0.0.
    assert_eq!(canonical_f64_bytes(-0.0), canonical_f64_bytes(0.0));
    assert_eq!(CanonicalF64::new(-0.0), CanonicalF64::new(0.0));
}

#[test]
fn nan_and_inf_never_enter_canonical_state() {
    // Rejection is at runtime in all profiles, not just debug (Appendix D
    // requires rejecting NaN/inf *at serialization time*).
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(CanonicalF64::new(bad).is_none());
        assert!(canonical_f64_bytes(bad).is_none());
        assert!(CanonicalF64::decode_canonical(&bad.to_le_bytes()).is_err());
    }
}

#[test]
fn content_address_is_domain_separated() {
    // Identical payload, different domain -> different address.
    let blob = Preimage::new(DomainTag::BLOB)
        .push_bytes(b"same bytes")
        .finish();
    let chunk = Preimage::new(DomainTag::CHUNK)
        .push_bytes(b"same bytes")
        .finish();
    assert_ne!(blob, chunk);
    // The blob form matches the dedicated BlobId constructor.
    assert_eq!(blob, ContentHash::of_blob(b"same bytes"));
}

#[test]
fn content_hash_is_blake3_256() {
    // 32-byte output, matching the published empty-input vector.
    let h = ContentHash(blake3_256(b""));
    assert_eq!(h.as_bytes().len(), 32);
    assert_eq!(
        h.to_hex(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
}

#[test]
fn domain_tags_are_validated_at_construction() {
    // The footgun the type now closes: a foreign tag cannot be minted.
    assert!(DomainTag::from_bytes(*b"BAD_TAG!").is_none());
    // An unregistered MUSC.... domain (not built-in, not a MUSCS system tag).
    assert!(DomainTag::from_bytes(*b"MUSCWXYZ").is_none());
    // Non-ASCII bytes rejected even with the format prefix.
    assert!(DomainTag::from_bytes([b'M', b'U', b'S', b'C', 0xFF, b'A', b'B', b'C']).is_none());
    // Extension system tags must begin MUSCS and not shadow a built-in.
    assert!(SystemDomainTag::new_extension(*b"MUSCSVCE").is_none());
    assert!(SystemDomainTag::new_extension(*b"MUSCSEXT").is_some());
    // A non-system tag cannot seed a system identifier (Finding: type-enforced).
    assert!(SystemDomainTag::new(DomainTag::CHUNK).is_none());
}

#[test]
fn quantized_coord_absorbs_subgrid_float_noise() {
    // Two "implementations" disagreeing by < 1/2048 staff space converge.
    let base = 7.0;
    let a = QuantizedCoord::from_staff_spaces(base + 0.0004);
    let b = QuantizedCoord::from_staff_spaces(base - 0.0004);
    assert_eq!(a, b);
    assert_eq!(a.unwrap().units, 7 * 1024);
}

#[test]
fn invalid_geometry_is_rejected_not_normalized() {
    // NaN/inf/out-of-range must not become valid canonical coordinates.
    assert_eq!(QuantizedCoord::from_staff_spaces(f64::NAN), None);
    assert_eq!(QuantizedCoord::from_staff_spaces(f64::INFINITY), None);
    assert_eq!(QuantizedCoord::from_staff_spaces(1e300), None);
}

#[test]
fn canonical_iteration_is_order_independent() {
    // Same multiset of chunk ids, two insertion orders, one canonical order.
    let ids = |order: [u8; 4]| -> Vec<u8> {
        let v: Vec<ChunkId> = order
            .iter()
            .map(|&n| ChunkId(ContentHash([n; 32])))
            .collect();
        sorted_canonical(v)
            .iter()
            .flat_map(|c| c.to_canonical_bytes())
            .collect()
    };
    assert_eq!(ids([3, 1, 4, 2]), ids([2, 4, 1, 3]));
}

#[test]
fn tolerances_are_typed_not_ad_hoc_epsilons() {
    let t = Tolerance::absolute(
        ToleranceClass::LayoutCoordinate,
        0.01,
        ToleranceGovernance::Validation,
    )
    .unwrap();
    assert_eq!(t.class.unit(), "staff spaces");
    assert!(t.within(1.005, 1.0));
}

#[test]
fn public_round_trip_is_byte_stable() {
    let q = QuantizedCoord::from_units(123_456);
    assert_eq!(
        QuantizedCoord::decode_canonical(&q.to_canonical_bytes()).unwrap(),
        q
    );

    let id = ChunkId(ContentHash([9u8; 32]));
    let bytes = id.to_canonical_bytes();
    assert_eq!(
        ChunkId::decode_canonical(&bytes)
            .unwrap()
            .to_canonical_bytes(),
        bytes
    );
}

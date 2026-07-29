//! The five W3 §3E invariants (`spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`
//! lines 472-487), **asserted**, not merely honoured (task requirement,
//! recipe §5). Every checker here returns `Result<(), String>` naming the
//! fixture and the invariant, so a caller can either `.expect()` it (the
//! generator does, and a violation is a hard failure) or assert it rejects a
//! deliberately broken case (the mutation tests in this module's `tests`
//! submodule do that, per the task's mutation-first requirement).

use crate::types::{SpikeCluster, SpikeResolvedText, SpikeShapedSegment};

/// Invariant 1: every `ClusterMap` offset and every `ShapedSegment::source`
/// bound is a valid UTF-8 boundary in `text`.
pub fn assert_utf8_boundaries(fixture_id: &str, rt: &SpikeResolvedText) -> Result<(), String> {
    let check = |label: &str, offset: u32| -> Result<(), String> {
        let o = offset as usize;
        if o > rt.text.len() || !rt.text.is_char_boundary(o) {
            return Err(format!(
                "{fixture_id}: invariant 1 (UTF-8 boundaries) violated: {label} offset {o} is not \
                 a char boundary in a {}-byte string",
                rt.text.len()
            ));
        }
        Ok(())
    };
    for (i, seg) in rt.segments.iter().enumerate() {
        check(&format!("segment[{i}].source.start"), seg.source.start)?;
        check(&format!("segment[{i}].source.end"), seg.source.end)?;
    }
    for (i, c) in rt.clusters.clusters.iter().enumerate() {
        check(&format!("cluster[{i}].source.start"), c.source.start)?;
        check(&format!("cluster[{i}].source.end"), c.source.end)?;
        for (j, stop) in c.caret_stops.iter().enumerate() {
            check(
                &format!("cluster[{i}].caret_stops[{j}].source_offset"),
                stop.source_offset,
            )?;
        }
    }
    Ok(())
}

/// Invariant 2: segment source ranges cover the whole string, totally and
/// without invalid overlap, in **logical** order (visual order may differ
/// under bidi — this crate stores `segments` in logical order throughout,
/// see `crate::shape`).
pub fn assert_segments_partition_totally(
    fixture_id: &str,
    rt: &SpikeResolvedText,
) -> Result<(), String> {
    let mut expected_next: u32 = 0;
    for (i, seg) in rt.segments.iter().enumerate() {
        if seg.source.start != expected_next {
            return Err(format!(
                "{fixture_id}: invariant 2 (total partition) violated: segment[{i}] starts at byte \
                 {}, but the partition left off at {expected_next} — a gap or overlap",
                seg.source.start
            ));
        }
        if seg.source.end < seg.source.start {
            return Err(format!(
                "{fixture_id}: invariant 2 violated: segment[{i}] has end {} < start {}",
                seg.source.end, seg.source.start
            ));
        }
        expected_next = seg.source.end;
    }
    let total = rt.text.len() as u32;
    if expected_next != total {
        return Err(format!(
            "{fixture_id}: invariant 2 (total partition) violated: segments cover up to byte \
             {expected_next}, but the string is {total} bytes"
        ));
    }
    Ok(())
}

/// Invariant 3: every cluster carries its source range (structural — always
/// present), its glyph indices, and its caret stops, **each stop with a
/// geometric position and a bidi affinity** (structural — the type has no
/// way to omit either). What is *not* structurally guaranteed, and is
/// checked here: every cluster has **at least one** caret stop (a cluster
/// with none would silently have no caret contract at all), and every glyph
/// index a resolved cluster names actually exists in its segment's glyph
/// list.
pub fn assert_clusters_carry_required_fields(
    fixture_id: &str,
    rt: &SpikeResolvedText,
) -> Result<(), String> {
    for (i, c) in rt.clusters.clusters.iter().enumerate() {
        if c.caret_stops.is_empty() {
            return Err(format!(
                "{fixture_id}: invariant 3 violated: cluster[{i}] ({:?}) carries no caret stops",
                c.source
            ));
        }
        if c.segment >= rt.segments.len() {
            return Err(format!(
                "{fixture_id}: invariant 3 violated: cluster[{i}] names segment {} but there are \
                 only {} segments",
                c.segment,
                rt.segments.len()
            ));
        }
        let seg = &rt.segments[c.segment];
        for &gi in &c.glyph_indices {
            if gi as usize >= seg.glyphs.len() {
                return Err(format!(
                    "{fixture_id}: invariant 3 violated: cluster[{i}] names glyph index {gi} in \
                     segment {}, which has only {} glyphs",
                    c.segment,
                    seg.glyphs.len()
                ));
            }
        }
    }
    Ok(())
}

/// Invariant 4: a cluster that shaping could not resolve is represented
/// **diagnostically** (an explicit unresolved marker), never dropped.
/// `expect_unresolved` names whether *this* fixture is expected to contain
/// one (only F-C does); when it is, this also checks the marker's segment
/// carries no glyphs and no face (`crate::findings::W3_F3`) — a "resolved:
/// false" flag with glyphs attached would be a marker that lies.
pub fn assert_unresolved_clusters_are_diagnostic(
    fixture_id: &str,
    rt: &SpikeResolvedText,
    expect_unresolved: bool,
) -> Result<(), String> {
    let unresolved: Vec<&SpikeCluster> = rt
        .clusters
        .clusters
        .iter()
        .filter(|c| !c.resolved)
        .collect();
    if expect_unresolved && unresolved.is_empty() {
        return Err(format!(
            "{fixture_id}: invariant 4 violated: this fixture is expected to carry an unresolved \
             cluster, but none is present — an uncovered codepoint must never be silently dropped"
        ));
    }
    if !expect_unresolved && !unresolved.is_empty() {
        return Err(format!(
            "{fixture_id}: invariant 4 check mismatch: this fixture is not expected to carry an \
             unresolved cluster, but {} are present",
            unresolved.len()
        ));
    }
    for c in &unresolved {
        if !c.glyph_indices.is_empty() {
            return Err(format!(
                "{fixture_id}: invariant 4 violated: unresolved cluster {:?} carries glyph \
                 indices — an unresolved marker that also claims glyphs is not diagnostic, it is \
                 a silent substitution",
                c.source
            ));
        }
        let seg = &rt.segments[c.segment];
        if seg.face.is_some() {
            return Err(format!(
                "{fixture_id}: invariant 4 violated: unresolved cluster {:?} belongs to a segment \
                 that names a face ({:?}) — an unresolved cluster's segment must be the \
                 `face: None` marker (crate::findings::W3_F3)",
                c.source, seg.face
            ));
        }
    }
    Ok(())
}

/// Invariant 5: positions are staff-space, y-up, quantized on the
/// `crate::QUANTIZE_GRID` (`1/1024`) grid — the same convention as glyph
/// positions, so text quantization is not a second convention.
pub fn assert_positions_quantized(fixture_id: &str, rt: &SpikeResolvedText) -> Result<(), String> {
    let check = |label: String, x: f64, y: f64| -> Result<(), String> {
        if !crate::quantize::is_on_grid(x) || !crate::quantize::is_on_grid(y) {
            return Err(format!(
                "{fixture_id}: invariant 5 (1/1024 grid) violated: {label} at ({x}, {y}) is not \
                 on the grid"
            ));
        }
        Ok(())
    };
    for (i, seg) in rt.segments.iter().enumerate() {
        for (j, g) in seg.glyphs.iter().enumerate() {
            check(
                format!("segment[{i}].glyphs[{j}].offset"),
                g.offset.x,
                g.offset.y,
            )?;
        }
    }
    for (i, c) in rt.clusters.clusters.iter().enumerate() {
        for (j, s) in c.caret_stops.iter().enumerate() {
            check(
                format!("cluster[{i}].caret_stops[{j}].position"),
                s.position.x,
                s.position.y,
            )?;
        }
    }
    check("origin".to_string(), rt.origin.x, rt.origin.y)?;
    check(
        "bounds.left/bottom".to_string(),
        rt.bounds.left,
        rt.bounds.bottom,
    )?;
    check(
        "bounds.right/top".to_string(),
        rt.bounds.right,
        rt.bounds.top,
    )?;
    check(
        "reserved_box.left/bottom".to_string(),
        rt.reserved_box.left,
        rt.reserved_box.bottom,
    )?;
    check(
        "reserved_box.right/top".to_string(),
        rt.reserved_box.right,
        rt.reserved_box.top,
    )?;
    Ok(())
}

/// Extra, recipe-specific check (§7, F-D): a direction-boundary offset
/// carries two caret stops with **different affinities at different
/// geometric positions** — not merely two stops that happen to coincide,
/// which would satisfy invariant 3's cardinality but defeat the reason
/// affinity exists.
///
/// **Applied to F-D and deliberately NOT to F-C** (recipe §12, W3-F4). F-C's
/// byte 5 is also a direction boundary — Latin LTR into Arabic RTL — but its
/// downstream side is an *unresolved, zero-advance* cluster, so both
/// affinities necessarily land on the identical position (measured: staff-space
/// x = 3.130859375 for both). Enforcing distinctness there would demand a
/// difference that cannot exist, so the exemption is correct; it is written
/// down because an unstated exemption is indistinguishable from having
/// forgotten the case. The caller decides which boundaries to check, and the
/// recipe records why.
pub fn assert_direction_boundary_stops_differ(
    fixture_id: &str,
    rt: &SpikeResolvedText,
    boundary_offset: u32,
) -> Result<(), String> {
    let stops: Vec<_> = rt
        .clusters
        .clusters
        .iter()
        .flat_map(|c| c.caret_stops.iter())
        .filter(|s| s.source_offset == boundary_offset)
        .collect();
    if stops.len() != 2 {
        return Err(format!(
            "{fixture_id}: byte {boundary_offset} must carry exactly 2 caret stops (one per \
             affinity), found {}",
            stops.len()
        ));
    }
    let (a, b) = (stops[0], stops[1]);
    if a.affinity == b.affinity {
        return Err(format!(
            "{fixture_id}: byte {boundary_offset}'s two stops share affinity {:?} instead of one \
             each",
            a.affinity
        ));
    }
    let dx = (a.position.x - b.position.x).abs();
    let dy = (a.position.y - b.position.y).abs();
    if dx < 1e-9 && dy < 1e-9 {
        return Err(format!(
            "{fixture_id}: byte {boundary_offset}'s two stops sit at the identical position \
             {:?} — affinity exists precisely so these differ",
            a.position
        ));
    }
    Ok(())
}

/// Runs invariants 1, 2, 3, 5 (universal) plus invariant 4 (parameterized by
/// whether this fixture is expected to carry an unresolved cluster). Every
/// fixture in this recipe calls this; F-D additionally calls
/// [`assert_direction_boundary_stops_differ`] at its two direction
/// boundaries (see `crate::fixtures`).
pub fn assert_all(fixture_id: &str, rt: &SpikeResolvedText, expect_unresolved: bool) {
    assert_utf8_boundaries(fixture_id, rt).expect("invariant 1");
    assert_segments_partition_totally(fixture_id, rt).expect("invariant 2");
    assert_clusters_carry_required_fields(fixture_id, rt).expect("invariant 3");
    assert_unresolved_clusters_are_diagnostic(fixture_id, rt, expect_unresolved)
        .expect("invariant 4");
    assert_positions_quantized(fixture_id, rt).expect("invariant 5");
}

/// Exposed for `crate::output`'s validator and for cross-crate tests that
/// want to confirm a `SpikeShapedSegment`'s face/glyph coherence without
/// depending on `crate::shape` directly.
pub fn segment_is_unresolved_marker(seg: &SpikeShapedSegment) -> bool {
    seg.face.is_none() && seg.glyphs.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{
        SemVerRecord, SpikeShaperId, SpikeTextShapingIdentity, SpikeUnicodeComponent,
    };
    use crate::types::{
        SpikeBoundingBox, SpikeCaretAffinity, SpikeCaretStop, SpikeCluster, SpikeClusterMap,
        SpikeGlyphStyle, SpikeLanguageTag, SpikePoint, SpikePositionedGlyph, SpikeProvenance,
        SpikeResolvedText, SpikeScriptTag, SpikeShapedSegment, SpikeStaffSpace, SpikeTextAlign,
        SpikeTextDirection,
    };

    fn dummy_identity() -> SpikeTextShapingIdentity {
        SpikeTextShapingIdentity {
            faces: Vec::new(),
            shaper: SpikeShaperId("rustybuzz".to_string()),
            shaper_version: SemVerRecord {
                major: 0,
                minor: 20,
                patch: 1,
            },
            features: Vec::new(),
            unicode_bidi: SpikeUnicodeComponent {
                impl_name: "unicode-bidi".to_string(),
                crate_version: "0.3.18".to_string(),
                unicode_version: Some("16.0.0".to_string()),
            },
            unicode_segmentation: SpikeUnicodeComponent {
                impl_name: "unicode-segmentation".to_string(),
                crate_version: "1.13.3".to_string(),
                unicode_version: Some("17.0.0".to_string()),
            },
        }
    }

    fn dummy_provenance() -> SpikeProvenance {
        SpikeProvenance {
            source: crate::types::SpikeTypedObjectId {
                discriminant: 0,
                canonical_bytes_hex: "00".repeat(18),
            },
            synthesis: None,
            dependencies: Vec::new(),
            stable_id: 0,
        }
    }

    /// A minimal, valid two-byte "ab" run: one segment, one cluster per
    /// grapheme, both invariants-satisfying. Every mutation test below
    /// starts from this and breaks exactly one thing.
    fn minimal_valid() -> SpikeResolvedText {
        SpikeResolvedText {
            provenance: dummy_provenance(),
            text: "ab".to_string(),
            shaping: dummy_identity(),
            segments: vec![SpikeShapedSegment {
                face: Some(0),
                glyphs: vec![
                    SpikePositionedGlyph {
                        glyph_id: 1,
                        offset: SpikePoint::new(0.0, 0.0),
                        transform: None,
                    },
                    SpikePositionedGlyph {
                        glyph_id: 2,
                        offset: SpikePoint::new(0.5, 0.0),
                        transform: None,
                    },
                ],
                source: 0..2,
                direction: SpikeTextDirection::Ltr,
                script: SpikeScriptTag("Latn".to_string()),
                language: SpikeLanguageTag(None),
                size: SpikeStaffSpace(1.28),
            }],
            clusters: SpikeClusterMap {
                clusters: vec![
                    SpikeCluster {
                        source: 0..1,
                        segment: 0,
                        glyph_indices: vec![0],
                        resolved: true,
                        grapheme_count: 1,
                        caret_stops: vec![SpikeCaretStop {
                            source_offset: 0,
                            position: SpikePoint::new(0.0, 0.0),
                            affinity: SpikeCaretAffinity::Downstream,
                        }],
                    },
                    SpikeCluster {
                        source: 1..2,
                        segment: 0,
                        glyph_indices: vec![1],
                        resolved: true,
                        grapheme_count: 1,
                        caret_stops: vec![SpikeCaretStop {
                            source_offset: 1,
                            position: SpikePoint::new(0.5, 0.0),
                            affinity: SpikeCaretAffinity::Downstream,
                        }],
                    },
                ],
            },
            bounds: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            reserved_box: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            // Recipe §3's nominal `1.6` is not itself on the `1/1024` grid
            // (see `crate::quantize`'s doc comment) — this test fixture uses
            // the honestly-quantized value, `1638/1024`, exactly as
            // `crate::fixtures::build_fixture` does for the real run.
            origin: SpikePoint::new(1638.0 / 1024.0, 0.0),
            align: SpikeTextAlign::Start,
            style: SpikeGlyphStyle { rgba: 0x0000_00ff },
            layer: 0,
        }
    }

    #[test]
    fn minimal_valid_passes_every_invariant() {
        let rt = minimal_valid();
        assert_utf8_boundaries("T", &rt).unwrap();
        assert_segments_partition_totally("T", &rt).unwrap();
        assert_clusters_carry_required_fields("T", &rt).unwrap();
        assert_unresolved_clusters_are_diagnostic("T", &rt, false).unwrap();
        assert_positions_quantized("T", &rt).unwrap();
    }

    // ---- Invariant 1: UTF-8 boundary mutations ----

    #[test]
    fn invariant_1_kills_a_mid_char_segment_bound() {
        let mut rt = minimal_valid();
        rt.text = "é".to_string(); // 2-byte UTF-8, so offset 1 is mid-char
        rt.segments[0].source = 0..1; // deliberately not a char boundary
        let err = assert_utf8_boundaries("T", &rt).unwrap_err();
        assert!(err.contains("invariant 1"), "{err}");
    }

    #[test]
    fn invariant_1_kills_an_out_of_range_cluster_offset() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops[0].source_offset = 99;
        let err = assert_utf8_boundaries("T", &rt).unwrap_err();
        assert!(err.contains("invariant 1"), "{err}");
    }

    // ---- Invariant 2: partition mutations ----

    #[test]
    fn invariant_2_kills_a_gap_between_segments() {
        let mut rt = minimal_valid();
        rt.segments.push(SpikeShapedSegment {
            face: Some(0),
            glyphs: vec![],
            source: 2..2, // no-op segment, but let's actually make a real gap instead
            direction: SpikeTextDirection::Ltr,
            script: SpikeScriptTag("Latn".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        });
        // Force a genuine gap: first segment now claims to stop at byte 1,
        // leaving byte 1..2 uncovered.
        rt.segments[0].source = 0..1;
        rt.segments[1].source = 2..2;
        let err = assert_segments_partition_totally("T", &rt).unwrap_err();
        assert!(err.contains("invariant 2"), "{err}");
    }

    #[test]
    fn invariant_2_kills_coverage_stopping_short_of_the_string_end() {
        let mut rt = minimal_valid();
        rt.segments[0].source = 0..1; // string is 2 bytes; this covers only 1
        let err = assert_segments_partition_totally("T", &rt).unwrap_err();
        assert!(err.contains("invariant 2"), "{err}");
    }

    // ---- Invariant 3: cluster field mutations ----

    #[test]
    fn invariant_3_kills_a_cluster_with_no_caret_stops() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops.clear();
        let err = assert_clusters_carry_required_fields("T", &rt).unwrap_err();
        assert!(err.contains("invariant 3"), "{err}");
    }

    #[test]
    fn invariant_3_kills_a_glyph_index_out_of_range() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].glyph_indices = vec![99];
        let err = assert_clusters_carry_required_fields("T", &rt).unwrap_err();
        assert!(err.contains("invariant 3"), "{err}");
    }

    // ---- Invariant 4: unresolved-marker mutations ----

    #[test]
    fn invariant_4_kills_a_dropped_unresolved_cluster() {
        let rt = minimal_valid();
        // This fixture carries zero unresolved clusters; asking the checker
        // to require one (as F-C's caller does) must fail.
        let err = assert_unresolved_clusters_are_diagnostic("T", &rt, true).unwrap_err();
        assert!(err.contains("invariant 4"), "{err}");
    }

    #[test]
    fn invariant_4_kills_an_unresolved_cluster_that_still_carries_glyphs() {
        let mut rt = minimal_valid();
        rt.segments.push(SpikeShapedSegment {
            face: None,
            glyphs: vec![],
            source: 2..2,
            direction: SpikeTextDirection::Ltr,
            script: SpikeScriptTag("Zzzz".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        });
        rt.clusters.clusters.push(SpikeCluster {
            source: 2..2,
            segment: 1,
            glyph_indices: vec![0], // a marker that also claims a glyph — invalid
            resolved: false,
            grapheme_count: 1,
            caret_stops: vec![SpikeCaretStop {
                source_offset: 2,
                position: SpikePoint::new(1.0, 0.0),
                affinity: SpikeCaretAffinity::Downstream,
            }],
        });
        let err = assert_unresolved_clusters_are_diagnostic("T", &rt, true).unwrap_err();
        assert!(err.contains("invariant 4"), "{err}");
    }

    #[test]
    fn invariant_4_kills_an_unresolved_marker_whose_segment_names_a_face() {
        let mut rt = minimal_valid();
        rt.segments.push(SpikeShapedSegment {
            face: Some(0), // should be None for an unresolved marker
            glyphs: vec![],
            source: 2..2,
            direction: SpikeTextDirection::Ltr,
            script: SpikeScriptTag("Zzzz".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        });
        rt.clusters.clusters.push(SpikeCluster {
            source: 2..2,
            segment: 1,
            glyph_indices: vec![],
            resolved: false,
            grapheme_count: 1,
            caret_stops: vec![SpikeCaretStop {
                source_offset: 2,
                position: SpikePoint::new(1.0, 0.0),
                affinity: SpikeCaretAffinity::Downstream,
            }],
        });
        let err = assert_unresolved_clusters_are_diagnostic("T", &rt, true).unwrap_err();
        assert!(err.contains("invariant 4"), "{err}");
    }

    // ---- Invariant 5: quantization mutations ----

    #[test]
    fn invariant_5_kills_an_off_grid_glyph_offset() {
        let mut rt = minimal_valid();
        rt.segments[0].glyphs[0].offset.x = 0.1234567; // not a multiple of 1/1024
        let err = assert_positions_quantized("T", &rt).unwrap_err();
        assert!(err.contains("invariant 5"), "{err}");
    }

    #[test]
    fn invariant_5_kills_an_off_grid_caret_stop() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops[0].position.y = 0.0009;
        let err = assert_positions_quantized("T", &rt).unwrap_err();
        assert!(err.contains("invariant 5"), "{err}");
    }

    // ---- Direction-boundary distinctness ----

    #[test]
    fn boundary_check_kills_two_stops_at_the_same_position() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops.push(SpikeCaretStop {
            source_offset: 0,
            position: SpikePoint::new(0.0, 0.0), // identical to the existing stop at offset 0
            affinity: SpikeCaretAffinity::Upstream,
        });
        let err = assert_direction_boundary_stops_differ("T", &rt, 0).unwrap_err();
        assert!(err.contains("identical position"), "{err}");
    }

    #[test]
    fn boundary_check_kills_two_stops_sharing_one_affinity() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops.push(SpikeCaretStop {
            source_offset: 0,
            position: SpikePoint::new(9.0, 0.0), // different position...
            affinity: SpikeCaretAffinity::Downstream, // ...but same affinity as the existing one
        });
        let err = assert_direction_boundary_stops_differ("T", &rt, 0).unwrap_err();
        assert!(err.contains("share affinity"), "{err}");
    }

    #[test]
    fn boundary_check_kills_a_missing_second_stop() {
        let rt = minimal_valid();
        // Only one stop exists at offset 0.
        let err = assert_direction_boundary_stops_differ("T", &rt, 0).unwrap_err();
        assert!(err.contains("exactly 2"), "{err}");
    }

    #[test]
    fn boundary_check_accepts_two_distinct_stops() {
        let mut rt = minimal_valid();
        rt.clusters.clusters[0].caret_stops.push(SpikeCaretStop {
            source_offset: 0,
            position: SpikePoint::new(-1.0, 0.0),
            affinity: SpikeCaretAffinity::Upstream,
        });
        assert_direction_boundary_stops_differ("T", &rt, 0).unwrap();
    }
}

//! Strict parsing of canonical Text Projection documents.
//!
//! Parsing is grammar-directed over [`epiphany_core::textvalue::Sexp`] values
//! read by [`epiphany_core::textvalue::read_sexp`], reports
//! [`epiphany_core::textvalue::TextError`], and delegates envelope productions
//! to [`epiphany_ops::parse_envelope`].
//!
//! # Free functions, not `TextValue` impls
//!
//! `TextValue` is a core-crate trait and every document-line type
//! (`Manifest`, `ProfileDeclaration`, ...) belongs to `epiphany-bundle`, which
//! does not depend on `epiphany-core`. Implementing a foreign trait for a
//! foreign type from this third crate is exactly what the orphan rule
//! forbids, and no document-line production needs it anyway: every one
//! bottoms out in `bytes`, `integer`, `bool`, `string`, `option`, or a closed
//! vocabulary, never a Chapter-5 `value`. So this module is free functions
//! throughout, one pair (or one parser, where there is no corresponding
//! `project_*` in this file) per grammar production.
//!
//! # Two rejections, and why each is a rejection rather than a normalization
//!
//! Every check below rejects rather than repairs, per `req:textproj:strict-parse`.
//! Two are load-bearing enough to call out specifically:
//!
//! * **Any `(blob ...)` line is rejected**, unconditionally, in
//!   [`parse_document`]. At `crate::COMPANION_VERSION` no canonical operation
//!   and no canonical reduced state can carry a `BlobId` (a source scan --
//!   the companion `epiphany-core`/`epiphany-ops` crates never mention the
//!   type), so canonical state cannot reference a blob and every blob line is
//!   necessarily unreferenced (`req:textproj:reject-unreferenced-blobs`).
//!   Accepting one would stage a blob into the bundle that the very next
//!   projection silently drops -- losing data and falsifying
//!   `project(serialize(parse(T))) == T` for that text. The `(blob ...)`
//!   *production* itself still parses -- `parse_blob` is exercised directly
//!   against synthetic data below -- so the accept side is ready the moment
//!   the reachability predicate becomes non-empty; only the document-level
//!   decision to use it is a permanent "no".
//! * **Any header version other than `crate::COMPANION_VERSION` is rejected
//!   at line one** (`req:textproj:header-version`). Multi-version acceptance
//!   and migrate-on-read are deferred spec decisions; this parser does not
//!   speculate about them.
//!
//! # Section order is a sequence, not a set
//!
//! `projection ::= header document lineage? profile* extension*
//! canonical-base? blob* envelope*` is consumed by [`parse_document`] in
//! exactly that order, greedily and without backtracking: each stage takes
//! every line that belongs to it and stops at the first line that does not.
//! Anything left over once every stage has run -- a repeated singular
//! section, a section that shows up before the one it follows, or simple
//! garbage -- is rejected as a whole. A parser that instead recognised every
//! line by its own head symbol and sorted them into place would accept
//! exactly the disordered and repeated texts this design rejects; that is
//! the normalization `req:textproj:strict-parse` forbids, so this parser
//! does not do it.

use epiphany_bundle::{
    ChunkKind, DocumentId, ExtensionId, FrontierBytes, LineageId, ProfileConstraints,
    ProfileDeclaration, ProfileId, ProfileRegistryId, ReductionAlgorithmVersion, RetentionPolicy,
    SchemaVersion, SemVer, SnapshotId, WallClockDuration,
};
use epiphany_core::textvalue::{read_sexp, Sexp, TextError, TextValue};
use epiphany_ops::{canonical_reduction_order, parse_envelope, OperationEnvelope};

use crate::{
    TextBlob, TextCanonicalBase, TextChunk, TextDocument, TextExtension, COMPANION_VERSION,
};

// ===========================================================================
// The document-level driver.
// ===========================================================================

/// Parses a complete canonical Text Projection into a [`TextDocument`],
/// rejecting anything that is not the canonical projection of the document it
/// denotes (`req:textproj:strict-parse`).
///
/// `text` must be exactly what `req:textproj:envelope-per-line` describes:
/// lines separated by a single U+000A, a final U+000A, and nothing else
/// trailing. Every line is one complete s-expression, and the sequence of
/// lines must follow `projection`'s section order precisely -- see the
/// module documentation for why an out-of-order or repeated section is a
/// rejection rather than something to sort back into place.
pub fn parse_document(text: &str) -> Result<TextDocument, TextError> {
    let raw_lines = split_lines(text)?;
    let mut lines = Lines {
        lines: raw_lines,
        pos: 0,
    };

    // header: mandatory, first (`req:textproj:header-version`).
    require_line(
        &mut lines,
        "text-projection",
        "a projection must begin with a header line naming the companion version",
        parse_header,
    )?;

    // document: mandatory, immediately after the header.
    let document_id = require_line(
        &mut lines,
        "document",
        "a projection must carry a document line immediately after its header",
        parse_document_id_line,
    )?;

    // lineage?
    let lineage_id = take_line(&mut lines, "lineage", parse_lineage)?;

    // profile*, in canonical (profile_id, version) order -- no repeats.
    let mut profiles = Vec::new();
    while let Some(profile) = take_line(&mut lines, "profile", parse_profile)? {
        profiles.push(profile);
    }
    if profiles
        .windows(2)
        .any(|w| (w[0].profile_id, w[0].version) >= (w[1].profile_id, w[1].version))
    {
        return Err(TextError::NotStrictlyIncreasing(
            "profile declarations must be ordered, without repetition, by (profile_id, version)",
        ));
    }

    // extension*, in canonical (extension_id, version) order -- no repeats.
    let mut extensions = Vec::new();
    while let Some(extension) = take_line(&mut lines, "extension", parse_extension)? {
        extensions.push(extension);
    }
    if extensions
        .windows(2)
        .any(|w| (w[0].extension_id, w[0].version) >= (w[1].extension_id, w[1].version))
    {
        return Err(TextError::NotStrictlyIncreasing(
            "extension declarations must be ordered, without repetition, by (extension_id, version)",
        ));
    }

    // canonical-base?
    let canonical_base = take_line(&mut lines, "canonical-base", parse_canonical_base)?;

    // blob*: collected only so the rejection below can fire; see the module
    // documentation and `req:textproj:reject-unreferenced-blobs`.
    let mut blobs = Vec::new();
    while let Some(blob) = take_line(&mut lines, "blob", parse_blob)? {
        blobs.push(blob);
    }
    if !blobs.is_empty() {
        return Err(TextError::NotCanonical(
            "a (blob ...) line is unreferenced by canonical state: no canonical operation or \
             reduced state can carry a BlobId at this companion version, so no blob line can \
             ever be canonical here",
        ));
    }

    // envelope*, in canonical reduction order.
    let mut envelopes = Vec::new();
    while let Some(line) = lines.peek() {
        let sexp = read_sexp(line)?;
        if line_head(&sexp) != Some("envelope") {
            break;
        }
        lines.advance();
        envelopes.push(parse_envelope(line)?);
    }
    check_envelope_order(&envelopes)?;

    // Anything left over is a line that belongs to no remaining stage: a
    // repeated singular section, a section reappearing after a later one, or
    // plain garbage after the envelopes. Every one of those is a rejection,
    // never a re-sort.
    if lines.peek().is_some() {
        return Err(TextError::Syntax(
            "a line appears out of the projection's normative section order, or a section that \
             may occur at most once is repeated",
        ));
    }

    Ok(TextDocument {
        document_id,
        lineage_id,
        profiles,
        extensions,
        canonical_base,
        blobs: Vec::new(),
        envelopes,
    })
}

/// A cursor over a projection's lines, advanced only by [`take_line`] and
/// [`require_line`] so every stage of [`parse_document`] consumes lines in
/// exactly one pass.
struct Lines<'a> {
    lines: Vec<&'a str>,
    pos: usize,
}

impl<'a> Lines<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.lines.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }
}

/// Splits a whole projection into its lines, enforcing the layout half of
/// `req:textproj:envelope-per-line`: a single trailing U+000A and no other
/// trailing whitespace. Without this check a projection missing its final
/// U+000A would still split into the same lines by `str::split('\n')` and
/// parse identically -- the trailing-newline requirement would be silently
/// unenforced. An embedded blank line (a stray `U+000A U+000A`, anywhere)
/// needs no special case here: it becomes an empty line, and every stage
/// below rejects an empty line when it tries to read an s-expression from it.
fn split_lines(text: &str) -> Result<Vec<&str>, TextError> {
    let body = text.strip_suffix('\n').ok_or(TextError::Syntax(
        "a projection must end with exactly one U+000A and no other trailing whitespace",
    ))?;
    Ok(body.split('\n').collect())
}

/// The head symbol of a line's s-expression, or `None` if it is not headed by
/// one (a bare leaf line is never a valid section head, so this only ever
/// hides a genuine mismatch).
fn line_head(sexp: &Sexp) -> Option<&str> {
    sexp.as_list()?.first()?.as_symbol()
}

/// Consumes the next line and applies `parse` to it, but only if it is
/// present and its head symbol is `head`. Otherwise the cursor is left
/// untouched and `Ok(None)` is returned -- the mechanism by which every
/// optional or repeatable section in `projection` is optional, and by which a
/// line belonging to a later or earlier stage is left for another stage (or
/// the final leftover check) to deal with, rather than being consumed out of
/// place.
fn take_line<'a, T>(
    lines: &mut Lines<'a>,
    head: &str,
    parse: impl FnOnce(&Sexp) -> Result<T, TextError>,
) -> Result<Option<T>, TextError> {
    let Some(line) = lines.peek() else {
        return Ok(None);
    };
    let sexp = read_sexp(line)?;
    if line_head(&sexp) != Some(head) {
        return Ok(None);
    }
    lines.advance();
    Ok(Some(parse(&sexp)?))
}

/// As [`take_line`], but the line is mandatory: a missing or mismatched line
/// is `missing`, not `Ok(None)`. Used for the header and the document line,
/// the two productions `projection` requires unconditionally.
fn require_line<'a, T>(
    lines: &mut Lines<'a>,
    head: &str,
    missing: &'static str,
    parse: impl FnOnce(&Sexp) -> Result<T, TextError>,
) -> Result<T, TextError> {
    let Some(line) = lines.peek() else {
        return Err(TextError::Syntax(missing));
    };
    let sexp = read_sexp(line)?;
    if line_head(&sexp) != Some(head) {
        return Err(TextError::Syntax(missing));
    }
    lines.advance();
    parse(&sexp)
}

/// `req:textproj:derived-ordering`'s "every other sequence keeps the binary
/// order" names the envelopes' binary order as
/// [`epiphany_ops::canonical_reduction_order`] -- a deterministic function of
/// the envelopes' own causal contexts and stamps, not a free choice a writer
/// makes. So a text whose envelope lines are already in that order is the
/// only text `req:textproj:canonical-text` permits for that envelope set, and
/// accepting some other order here -- and silently keeping it -- would be
/// exactly the normalization `req:textproj:strict-parse` forbids: the next
/// projection re-sorts the same envelopes into canonical order, so
/// `project(serialize(parse(T))) == T` fails for precisely the malformed `T`
/// this rejects up front.
fn check_envelope_order(envelopes: &[OperationEnvelope]) -> Result<(), TextError> {
    let given: Vec<&OperationEnvelope> = envelopes.iter().collect();
    let canonical = canonical_reduction_order(&given);
    let already_canonical = given
        .iter()
        .zip(canonical.iter())
        .all(|(a, b)| std::ptr::eq(*a, *b));
    if !already_canonical {
        return Err(TextError::NotStrictlyIncreasing(
            "envelope lines must appear in canonical reduction order \
             (epiphany_ops::canonical_reduction_order)",
        ));
    }
    Ok(())
}

// ===========================================================================
// Leaves shared by several productions.
// ===========================================================================

/// The lexical class of `s`, for error messages. Mirrors the private helper
/// of the same name in `epiphany_core::textvalue` and in every `textproj_*`
/// module of `epiphany-ops`: `Sexp::class` is not public, so each grammar-
/// directed module restates this one match rather than depend on it.
fn class_of(s: &Sexp) -> &'static str {
    match s {
        Sexp::List(_) => "list",
        Sexp::Symbol(_) => "symbol",
        Sexp::Int(_) => "integer",
        Sexp::Bytes(_) => "byte string",
        Sexp::Str(_) => "string",
    }
}

/// Reads the grammar's opaque `bytes` terminal as a plain byte string.
/// `Vec<u8>`'s generic `TextValue` impl denotes a *sequence of integers*, a
/// different production, so it must never stand in for an opaque payload,
/// identifier, or hash -- the same trap the operation layer names in
/// `textproj_kind.rs`.
fn parse_bytes(s: &Sexp) -> Result<Vec<u8>, TextError> {
    match s {
        Sexp::Bytes(bytes) => Ok(bytes.clone()),
        _ => Err(TextError::Expected {
            expected: "byte string",
            found: class_of(s),
        }),
    }
}

/// Reads a fixed 16-byte opaque identifier: a `DocumentId`, `LineageId`,
/// `SnapshotId`, `ExtensionId`, or a custom `ProfileId`'s registry id. `what`
/// names the identifier for the rejection message.
fn parse_id16(s: &Sexp, what: &'static str) -> Result<[u8; 16], TextError> {
    parse_bytes(s)?
        .try_into()
        .map_err(|_| TextError::NotCanonical(what))
}

/// The grammar's anonymous `version ::= "(" integer " " integer " " integer
/// ")"` production, shared verbatim by the header (against
/// `crate::COMPANION_VERSION`) and by every `SemVer` field.
fn parse_version_triple(s: &Sexp) -> Result<(u32, u32, u32), TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: "version",
        found: class_of(s),
    })?;
    let [major, minor, patch] = items else {
        return Err(TextError::Arity {
            type_name: "version",
            expected: 3,
            found: items.len(),
        });
    };
    Ok((u32::parse(major)?, u32::parse(minor)?, u32::parse(patch)?))
}

fn parse_semver(s: &Sexp) -> Result<SemVer, TextError> {
    let (major, minor, patch) = parse_version_triple(s)?;
    Ok(SemVer::new(major, minor, patch))
}

/// The grammar's `profile-id` production: a bare symbol for each
/// closed-vocabulary profile, and `(custom <bytes>)` -- the one profile
/// identity that is not a bare symbol -- for `ProfileId::Custom`, whose
/// registry id is 16 opaque bytes.
fn parse_profile_id(s: &Sexp) -> Result<ProfileId, TextError> {
    if let Some(name) = s.as_symbol() {
        return match name {
            "full" => Ok(ProfileId::Full),
            "read-only" => Ok(ProfileId::ReadOnly),
            "lite" => Ok(ProfileId::Lite),
            _ => Err(TextError::UnknownConstructor {
                type_name: "ProfileId",
                found: name.to_owned(),
            }),
        };
    }
    let fields = s.expect_struct("custom", 1)?;
    let registry_id = parse_id16(
        &fields[0],
        "a custom ProfileId registry id is exactly 16 bytes",
    )?;
    Ok(ProfileId::Custom(ProfileRegistryId(registry_id)))
}

// ===========================================================================
// header, document, lineage.
// ===========================================================================

/// `header ::= "(text-projection " version ")"`. Accepts exactly
/// `crate::COMPANION_VERSION`; any other version is a rejection at line one
/// (`req:textproj:header-version`). Multi-version acceptance and
/// migrate-on-read are deferred spec decisions this parser does not
/// speculate about.
fn parse_header(s: &Sexp) -> Result<(), TextError> {
    let fields = s.expect_struct("text-projection", 1)?;
    let version = parse_version_triple(&fields[0])?;
    if version != COMPANION_VERSION {
        return Err(TextError::NotCanonical(
            "the header names a companion version other than the one this crate implements",
        ));
    }
    Ok(())
}

/// `document ::= "(document " bytes ")"`.
fn parse_document_id_line(s: &Sexp) -> Result<DocumentId, TextError> {
    let fields = s.expect_struct("document", 1)?;
    Ok(DocumentId(parse_id16(
        &fields[0],
        "a DocumentId is exactly 16 bytes",
    )?))
}

/// `lineage ::= "(lineage " bytes ")"`.
fn parse_lineage(s: &Sexp) -> Result<LineageId, TextError> {
    let fields = s.expect_struct("lineage", 1)?;
    Ok(LineageId(parse_id16(
        &fields[0],
        "a LineageId is exactly 16 bytes",
    )?))
}

// ===========================================================================
// profile, constraints, retention.
// ===========================================================================

/// `profile ::= "(profile " profile-id " " version " " constraints ")"`.
fn parse_profile(s: &Sexp) -> Result<ProfileDeclaration, TextError> {
    let fields = s.expect_struct("profile", 3)?;
    Ok(ProfileDeclaration {
        profile_id: parse_profile_id(&fields[0])?,
        version: parse_semver(&fields[1])?,
        constraints: parse_constraints(&fields[2])?,
    })
}

/// `constraints ::= "(constraints " integer " " retention ")"`.
fn parse_constraints(s: &Sexp) -> Result<ProfileConstraints, TextError> {
    let fields = s.expect_struct("constraints", 2)?;
    Ok(ProfileConstraints {
        max_uncompressed_block_size: u64::parse(&fields[0])?,
        retention_policy: parse_retention(&fields[1])?,
    })
}

/// `retention ::= "(retention " integer " " option " " bool ")"`.
///
/// The middle field is `Option<WallClockDuration>`, and `WallClockDuration` is
/// a newtype over `i64`: per `req:textproj:value-projection` clause 2 a
/// newtype projects transparently as its field alone, so the option wraps a
/// bare integer, not a `(wall-clock-duration <integer>)` struct. This is
/// **`epiphany_bundle::WallClockDuration`**, not `epiphany_core`'s
/// same-named type -- the retention policy is a bundle type through and
/// through.
fn parse_retention(s: &Sexp) -> Result<RetentionPolicy, TextError> {
    let fields = s.expect_struct("retention", 3)?;
    Ok(RetentionPolicy {
        retain_previous_manifests: u32::parse(&fields[0])?,
        retain_duration: Option::<i64>::parse(&fields[1])?.map(WallClockDuration),
        retain_named_checkpoints: bool::parse(&fields[2])?,
    })
}

// ===========================================================================
// extension, chunk, chunk-kind, schema.
// ===========================================================================

/// `extension ::= "(extension " bytes " " version " " bool " (" chunk* ") "
/// bytes " " bytes ")"` -- id, version, required, chunks, affected-kinds,
/// barriers, the ratified declaration order. `affected_object_kinds` and
/// `edit_barriers` are opaque byte strings, never structured: the bundle
/// preserves them without interpreting them, and this projection interprets
/// nothing the bundle does not.
fn parse_extension(s: &Sexp) -> Result<TextExtension, TextError> {
    let fields = s.expect_struct("extension", 6)?;
    Ok(TextExtension {
        extension_id: ExtensionId(parse_id16(
            &fields[0],
            "an ExtensionId is exactly 16 bytes",
        )?),
        version: parse_semver(&fields[1])?,
        required: bool::parse(&fields[2])?,
        chunks: parse_chunks(&fields[3])?,
        affected_object_kinds: parse_bytes(&fields[4])?,
        edit_barriers: parse_bytes(&fields[5])?,
    })
}

/// `chunk ::= "(chunk " chunk-kind " " schema " " bytes ")"`: a preserved
/// extension chunk root projected as kind, schema version, and uncompressed
/// payload -- never a `ChunkRef`, which the projection has no file to point
/// into (`req:textproj:derive-or-carry`).
fn parse_chunk(s: &Sexp) -> Result<TextChunk, TextError> {
    let fields = s.expect_struct("chunk", 3)?;
    Ok(TextChunk {
        kind: parse_chunk_kind(&fields[0])?,
        schema_version: parse_schema_version(&fields[1])?,
        payload: parse_bytes(&fields[2])?,
    })
}

/// The chunk* sequence inside an `extension` line. `req:textproj:derived-ordering`
/// orders and de-duplicates an extension's preserved chunk roots by their
/// *projected form* -- the binary `ChunkRef` order breaks ties on the file
/// offset, which this projection erases, so the binary order cannot be
/// inherited here. Accepting a disordered or duplicated chunk list and
/// re-sorting it at serialize time would be normalization; this rejects it
/// instead.
fn parse_chunks(s: &Sexp) -> Result<Vec<TextChunk>, TextError> {
    let items = s.as_list().ok_or(TextError::Expected {
        expected: "chunk sequence",
        found: class_of(s),
    })?;
    let mut out = Vec::with_capacity(items.len());
    let mut previous_rendered: Option<String> = None;
    for item in items {
        let rendered = item.render();
        if previous_rendered
            .as_deref()
            .is_some_and(|previous| rendered.as_str() <= previous)
        {
            return Err(TextError::NotStrictlyIncreasing(
                "an extension's preserved chunk roots must be ordered, without repetition, by \
                 projected form",
            ));
        }
        out.push(parse_chunk(item)?);
        previous_rendered = Some(rendered);
    }
    Ok(out)
}

/// `chunk-kind`: a bare symbol per `ChunkKind` variant, in the same
/// declaration order as the discriminants `req:format:chunkkind-discriminants`
/// pins.
fn parse_chunk_kind(s: &Sexp) -> Result<ChunkKind, TextError> {
    match s.as_symbol() {
        Some("operation-envelope-block") => Ok(ChunkKind::OperationEnvelopeBlock),
        Some("operation-index") => Ok(ChunkKind::OperationIndex),
        Some("snapshot") => Ok(ChunkKind::Snapshot),
        Some("blob") => Ok(ChunkKind::Blob),
        Some("extension-data") => Ok(ChunkKind::ExtensionData),
        Some("text-projection") => Ok(ChunkKind::TextProjection),
        Some("layout-cache") => Ok(ChunkKind::LayoutCache),
        Some("integrity-index") => Ok(ChunkKind::IntegrityIndex),
        Some("manifest") => Ok(ChunkKind::Manifest),
        Some(name) => Err(TextError::UnknownConstructor {
            type_name: "ChunkKind",
            found: name.to_owned(),
        }),
        None => Err(TextError::Expected {
            expected: "chunk-kind",
            found: class_of(s),
        }),
    }
}

/// `schema ::= "(schema " integer " " integer ")"`.
fn parse_schema_version(s: &Sexp) -> Result<SchemaVersion, TextError> {
    let fields = s.expect_struct("schema", 2)?;
    Ok(SchemaVersion::new(
        u16::parse(&fields[0])?,
        u16::parse(&fields[1])?,
    ))
}

// ===========================================================================
// canonical-base.
// ===========================================================================

/// `canonical-base ::= "(canonical-base " bytes " " bytes " " integer " "
/// profile-id " " schema " " bytes ")"` -- snapshot id, frontier, reduction
/// version, profile, root schema, root payload. The `SnapshotId` is the one
/// identity `req:textproj:derive-or-carry` carries verbatim rather than
/// re-deriving (schema major 0 has no snapshot producer, so it has nothing to
/// derive from); the root chunk's own id and content hash are re-derived by
/// whoever serializes this back into a bundle, never read from the text.
fn parse_canonical_base(s: &Sexp) -> Result<TextCanonicalBase, TextError> {
    let fields = s.expect_struct("canonical-base", 6)?;
    Ok(TextCanonicalBase {
        snapshot_id: SnapshotId(parse_id16(&fields[0], "a SnapshotId is exactly 16 bytes")?),
        covers_causal_frontier: FrontierBytes::from_bytes(parse_bytes(&fields[1])?),
        reduction_algorithm_version: ReductionAlgorithmVersion(u32::parse(&fields[2])?),
        profile_id: parse_profile_id(&fields[3])?,
        root_schema_version: parse_schema_version(&fields[4])?,
        root_payload: parse_bytes(&fields[5])?,
    })
}

// ===========================================================================
// blob.
// ===========================================================================

/// `blob ::= "(blob " string " " option " " bytes ")"` -- media type,
/// declared maximum uncompressed length, payload.
///
/// This is the production-level parser only. It accepts a well-formed
/// `(blob ...)` line unconditionally, exactly as every other production
/// parser here does; the decision that **no** accepted blob line may reach a
/// [`TextDocument`] is made once, at the document level, in
/// [`parse_document`] (`req:textproj:reject-unreferenced-blobs`). Keeping the
/// two apart -- this function parses, `parse_document` rejects -- means the
/// accept side is already correct and already tested the day the
/// reachability predicate stops returning the empty set, rather than needing
/// to be written from scratch alongside a spec bump.
fn parse_blob(s: &Sexp) -> Result<TextBlob, TextError> {
    let fields = s.expect_struct("blob", 3)?;
    Ok(TextBlob {
        media_type: String::parse(&fields[0])?,
        declared_max_uncompressed_length: Option::<u64>::parse(&fields[1])?,
        payload: parse_bytes(&fields[2])?,
    })
}

// ===========================================================================
// Tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{OperationId, ReplicaId, WallClockTime};
    use epiphany_ops::{
        project_envelope, AuthorId, CausalContext, EnvelopeHash, HybridLogicalClock,
        OperationStamp, ResolveEquivocationPayload,
    };

    // -----------------------------------------------------------------
    // Test fixtures.
    // -----------------------------------------------------------------

    /// Joins `lines` with a single U+000A each, including a final one --
    /// exactly `req:textproj:envelope-per-line`'s layout.
    fn projection(lines: &[&str]) -> String {
        let mut out = String::new();
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    const HEADER: &str = "(text-projection (0 7 0))";
    const DOCUMENT: &str = "(document #x00000000000000000000000000000001)";

    /// A minimal but complete valid projection: just the two mandatory lines.
    fn minimal_valid_document() -> String {
        projection(&[HEADER, DOCUMENT])
    }

    /// A simple, independent (empty causal context) envelope, so several of
    /// these can be combined without any causal-order machinery beyond their
    /// HLC physical time.
    fn sample_envelope(replica: u64, counter: u64, physical_time: i64) -> OperationEnvelope {
        let id = OperationId::new(ReplicaId(replica), counter);
        OperationEnvelope {
            id,
            author: AuthorId(0x1122_3344),
            stamp: OperationStamp::new(
                HybridLogicalClock::new(WallClockTime(physical_time), 0),
                id,
            ),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: epiphany_ops::OperationPayload::ResolveEquivocation(
                ResolveEquivocationPayload {
                    target: id,
                    chosen: EnvelopeHash([7; 32]),
                },
            ),
        }
    }

    // -----------------------------------------------------------------
    // The worked example: a real, spec-authored, multi-section document.
    // -----------------------------------------------------------------

    /// The Grammar chapter's worked example, read live from the companion so
    /// this test cannot drift from the document it claims to parse.
    fn worked_example() -> String {
        const SPEC: &str = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../spec/text_projection.tex"
        ));
        SPEC.split_once("\\chapter{A Worked Example}")
            .expect("the specification contains the worked example")
            .1
            .split_once("\\begin{lstlisting}\n")
            .expect("the worked example contains a projection listing")
            .1
            .split_once("\\end{lstlisting}")
            .expect("the worked projection listing is closed")
            .0
            .to_owned()
    }

    #[test]
    fn the_worked_example_parses_to_its_documented_fields() {
        let text = worked_example();
        let document = parse_document(&text).expect("the worked example is a valid projection");

        assert_eq!(document.document_id, DocumentId([0x05; 16]));
        assert_eq!(document.lineage_id, None);
        assert_eq!(document.profiles.len(), 1);
        assert_eq!(document.profiles[0].profile_id, ProfileId::Full);
        assert_eq!(document.profiles[0].version, SemVer::new(0, 1, 0));
        assert_eq!(
            document.profiles[0].constraints.max_uncompressed_block_size,
            67_108_864
        );
        assert_eq!(
            document.profiles[0]
                .constraints
                .retention_policy
                .retain_previous_manifests,
            1
        );
        assert_eq!(
            document.profiles[0]
                .constraints
                .retention_policy
                .retain_duration,
            None
        );
        assert!(
            document.profiles[0]
                .constraints
                .retention_policy
                .retain_named_checkpoints
        );
        assert!(document.extensions.is_empty());

        let base = document
            .canonical_base
            .as_ref()
            .expect("the worked example carries a canonical base");
        assert_eq!(
            base.snapshot_id,
            SnapshotId([0x1f, 0x8b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
        );
        assert_eq!(base.covers_causal_frontier, FrontierBytes::empty());
        assert_eq!(
            base.reduction_algorithm_version,
            ReductionAlgorithmVersion(1)
        );
        assert_eq!(base.profile_id, ProfileId::Full);
        assert_eq!(base.root_schema_version, SchemaVersion::V0);
        assert_eq!(base.root_payload, vec![0u8, 0u8]);

        assert!(document.blobs.is_empty());
        assert_eq!(document.envelopes.len(), 1);
    }

    // -----------------------------------------------------------------
    // A synthetic document exercising lineage, multiple profiles, an
    // extension with preserved chunks, a canonical base, and multiple
    // envelopes -- everything the worked example alone does not reach.
    // -----------------------------------------------------------------

    #[test]
    fn a_rich_synthetic_document_parses_every_section() {
        let lineage = "(lineage #x00000000000000000000000000000002)";
        let profile_full = "(profile full (0 1 0) (constraints 67108864 (retention 1 () true)))";
        let profile_read_only =
            "(profile read-only (0 1 0) (constraints 1024 (retention 0 (some 5) false)))";
        let extension = "(extension #x00000000000000000000000000000003 (1 0 0) true \
             ((chunk operation-envelope-block (schema 0 1) #xaa) (chunk snapshot (schema 0 1) #xbb)) \
             #xaabb #xccdd)";
        let base =
            "(canonical-base #x00000000000000000000000000000004 #x 1 full (schema 0 1) #x0102)";

        let e1 = sample_envelope(1, 1, 10);
        let e2 = sample_envelope(2, 1, 20);
        let e1_line = project_envelope(&e1);
        let e2_line = project_envelope(&e2);

        let text = projection(&[
            HEADER,
            DOCUMENT,
            lineage,
            profile_full,
            profile_read_only,
            extension,
            base,
            &e1_line,
            &e2_line,
        ]);

        let document = parse_document(&text).expect("a well-formed rich document parses");
        assert_eq!(
            document.lineage_id,
            Some(LineageId([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]))
        );
        assert_eq!(document.profiles.len(), 2);
        assert_eq!(document.extensions.len(), 1);
        assert_eq!(document.extensions[0].chunks.len(), 2);
        assert_eq!(
            document.extensions[0].chunks[0].kind,
            ChunkKind::OperationEnvelopeBlock
        );
        assert_eq!(document.extensions[0].chunks[1].kind, ChunkKind::Snapshot);
        assert_eq!(
            document.extensions[0].affected_object_kinds,
            vec![0xaa, 0xbb]
        );
        assert_eq!(document.extensions[0].edit_barriers, vec![0xcc, 0xdd]);
        assert!(document.canonical_base.is_some());
        assert_eq!(document.envelopes, vec![e1, e2]);
    }

    // -----------------------------------------------------------------
    // Rejection classes. Each is mutation-verified (see the task report):
    // the anchor assertion below is deleted or inverted, a NAMED test in
    // this list is confirmed to fail, and the code is restored.
    // -----------------------------------------------------------------

    #[test]
    fn non_canonical_header_version_is_rejected() {
        let text = projection(&["(text-projection (0 6 0))", DOCUMENT]);
        assert_eq!(
            parse_document(&text),
            Err(TextError::NotCanonical(
                "the header names a companion version other than the one this crate implements"
            ))
        );
    }

    #[test]
    fn the_companion_version_itself_is_accepted() {
        // The mutation-testing counterpart of the above: the exact version
        // this crate implements must still parse.
        assert!(parse_document(&minimal_valid_document()).is_ok());
    }

    #[test]
    fn a_blob_line_is_always_rejected() {
        let blob_line = "(blob \"audio/wav\" () #x00)";
        let text = projection(&[HEADER, DOCUMENT, blob_line]);
        assert!(matches!(
            parse_document(&text),
            Err(TextError::NotCanonical(_))
        ));
    }

    #[test]
    fn the_blob_production_itself_parses_synthetic_data() {
        // Line-level parse of `(blob ...)`, exercised directly against
        // synthetic data per the task contract: the accept side is ready
        // even though `parse_document` never lets it through.
        let sexp = read_sexp("(blob \"audio/wav\" (some 1024) #x0a0b)").unwrap();
        let blob = parse_blob(&sexp).expect("a well-formed blob production parses");
        assert_eq!(blob.media_type, "audio/wav");
        assert_eq!(blob.declared_max_uncompressed_length, Some(1024));
        assert_eq!(blob.payload, vec![0x0a, 0x0b]);
    }

    #[test]
    fn missing_trailing_lf_is_rejected() {
        let text = minimal_valid_document();
        let without_final_lf = text.strip_suffix('\n').unwrap();
        assert_eq!(
            parse_document(without_final_lf),
            Err(TextError::Syntax(
                "a projection must end with exactly one U+000A and no other trailing whitespace"
            ))
        );
    }

    #[test]
    fn an_extra_trailing_blank_line_is_rejected() {
        let mut text = minimal_valid_document();
        text.push('\n'); // a second, stray trailing U+000A
        assert!(parse_document(&text).is_err());
    }

    #[test]
    fn missing_header_is_rejected() {
        assert!(parse_document("").is_err());
    }

    #[test]
    fn missing_document_line_is_rejected() {
        let text = projection(&[HEADER]);
        assert_eq!(
            parse_document(&text),
            Err(TextError::Syntax(
                "a projection must carry a document line immediately after its header"
            ))
        );
    }

    #[test]
    fn a_repeated_header_line_is_rejected() {
        let text = projection(&[HEADER, HEADER, DOCUMENT]);
        assert!(parse_document(&text).is_err());
    }

    #[test]
    fn a_repeated_document_line_is_rejected() {
        let text = projection(&[HEADER, DOCUMENT, DOCUMENT]);
        assert_eq!(
            parse_document(&text),
            Err(TextError::Syntax(
                "a line appears out of the projection's normative section order, or a section \
                 that may occur at most once is repeated"
            ))
        );
    }

    #[test]
    fn out_of_order_sections_are_rejected_not_sorted() {
        let profile = "(profile full (0 1 0) (constraints 67108864 (retention 1 () true)))";
        let extension = "(extension #x00000000000000000000000000000003 (1 0 0) false () #x #x)";
        let base = "(canonical-base #x00000000000000000000000000000004 #x 1 full (schema 0 1) #x)";
        let lineage = "(lineage #x00000000000000000000000000000002)";
        let e1_line = project_envelope(&sample_envelope(1, 1, 10));

        let scenarios: &[&[&str]] = &[
            // A profile line before the (mandatory) document line.
            &[HEADER, profile, DOCUMENT],
            // Lineage after a profile, instead of before it.
            &[HEADER, DOCUMENT, profile, lineage],
            // An extension before the profile section.
            &[HEADER, DOCUMENT, extension, profile],
            // A canonical base before the extension section.
            &[HEADER, DOCUMENT, base, extension],
            // An envelope before the canonical base.
            &[HEADER, DOCUMENT, &e1_line, base],
        ];
        for lines in scenarios {
            let text = projection(lines);
            assert!(
                parse_document(&text).is_err(),
                "expected rejection for out-of-order lines: {lines:?}"
            );
        }
    }

    #[test]
    fn profile_declarations_must_be_ordered_and_unique() {
        let full = "(profile full (0 1 0) (constraints 1 (retention 0 () false)))";
        let read_only = "(profile read-only (0 1 0) (constraints 1 (retention 0 () false)))";

        // Correct ascending order accepts.
        let ok = projection(&[HEADER, DOCUMENT, full, read_only]);
        assert!(parse_document(&ok).is_ok());

        // Reversed order is rejected, not silently re-sorted.
        let reversed = projection(&[HEADER, DOCUMENT, read_only, full]);
        assert_eq!(
            parse_document(&reversed),
            Err(TextError::NotStrictlyIncreasing(
                "profile declarations must be ordered, without repetition, by (profile_id, version)"
            ))
        );

        // An exact repeat (same key) is rejected too.
        let repeated = projection(&[HEADER, DOCUMENT, full, full]);
        assert!(parse_document(&repeated).is_err());
    }

    #[test]
    fn extension_declarations_must_be_ordered_and_unique() {
        let ext_a = "(extension #x00000000000000000000000000000001 (1 0 0) false () #x #x)";
        let ext_b = "(extension #x00000000000000000000000000000002 (1 0 0) false () #x #x)";

        let ok = projection(&[HEADER, DOCUMENT, ext_a, ext_b]);
        assert!(parse_document(&ok).is_ok());

        let reversed = projection(&[HEADER, DOCUMENT, ext_b, ext_a]);
        assert_eq!(
            parse_document(&reversed),
            Err(TextError::NotStrictlyIncreasing(
                "extension declarations must be ordered, without repetition, by (extension_id, version)"
            ))
        );

        let repeated = projection(&[HEADER, DOCUMENT, ext_a, ext_a]);
        assert!(parse_document(&repeated).is_err());
    }

    #[test]
    fn extension_chunk_roots_must_be_ordered_and_deduplicated() {
        let chunk_a = "(chunk operation-envelope-block (schema 0 1) #xaa)";
        let chunk_b = "(chunk snapshot (schema 0 1) #xbb)";

        let ordered = format!(
            "(extension #x00000000000000000000000000000001 (1 0 0) false ({chunk_a} {chunk_b}) #x #x)"
        );
        let text = projection(&[HEADER, DOCUMENT, &ordered]);
        assert!(parse_document(&text).is_ok());

        let reversed = format!(
            "(extension #x00000000000000000000000000000001 (1 0 0) false ({chunk_b} {chunk_a}) #x #x)"
        );
        let text = projection(&[HEADER, DOCUMENT, &reversed]);
        assert_eq!(
            parse_document(&text),
            Err(TextError::NotStrictlyIncreasing(
                "an extension's preserved chunk roots must be ordered, without repetition, by \
                 projected form"
            ))
        );

        let duplicated = format!(
            "(extension #x00000000000000000000000000000001 (1 0 0) false ({chunk_a} {chunk_a}) #x #x)"
        );
        let text = projection(&[HEADER, DOCUMENT, &duplicated]);
        assert!(parse_document(&text).is_err());
    }

    #[test]
    fn envelope_lines_must_be_in_canonical_reduction_order() {
        let e1 = sample_envelope(1, 1, 10);
        let e2 = sample_envelope(2, 1, 20);
        let e1_line = project_envelope(&e1);
        let e2_line = project_envelope(&e2);

        // Ascending physical time is the canonical order: accepted.
        let ok = projection(&[HEADER, DOCUMENT, &e1_line, &e2_line]);
        let parsed = parse_document(&ok).expect("already-canonical envelope order is accepted");
        assert_eq!(parsed.envelopes, vec![e1, e2]);

        // Swapped: no longer canonical reduction order, rejected outright.
        let swapped = projection(&[HEADER, DOCUMENT, &e2_line, &e1_line]);
        assert_eq!(
            parse_document(&swapped),
            Err(TextError::NotStrictlyIncreasing(
                "envelope lines must appear in canonical reduction order \
                 (epiphany_ops::canonical_reduction_order)"
            ))
        );
    }

    #[test]
    fn profile_id_accepts_every_closed_symbol_and_the_custom_form() {
        for (text, expected) in [
            ("full", ProfileId::Full),
            ("read-only", ProfileId::ReadOnly),
            ("lite", ProfileId::Lite),
            (
                "(custom #x00000000000000000000000000000009)",
                ProfileId::Custom(ProfileRegistryId([
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9,
                ])),
            ),
        ] {
            let sexp = read_sexp(text).unwrap();
            assert_eq!(parse_profile_id(&sexp).unwrap(), expected);
        }
    }

    #[test]
    fn profile_id_rejects_an_unknown_symbol_and_a_malformed_custom_id() {
        assert!(parse_profile_id(&read_sexp("bespoke").unwrap()).is_err());
        assert!(parse_profile_id(&read_sexp("(custom #x00)").unwrap()).is_err());
    }

    #[test]
    fn chunk_kind_covers_exactly_the_nine_variants() {
        for (name, kind) in [
            (
                "operation-envelope-block",
                ChunkKind::OperationEnvelopeBlock,
            ),
            ("operation-index", ChunkKind::OperationIndex),
            ("snapshot", ChunkKind::Snapshot),
            ("blob", ChunkKind::Blob),
            ("extension-data", ChunkKind::ExtensionData),
            ("text-projection", ChunkKind::TextProjection),
            ("layout-cache", ChunkKind::LayoutCache),
            ("integrity-index", ChunkKind::IntegrityIndex),
            ("manifest", ChunkKind::Manifest),
        ] {
            let sexp = read_sexp(name).unwrap();
            assert_eq!(parse_chunk_kind(&sexp).unwrap(), kind);
        }
        assert!(parse_chunk_kind(&read_sexp("unknown-kind").unwrap()).is_err());
    }

    // -----------------------------------------------------------------
    // The suite's own reach.
    // -----------------------------------------------------------------

    /// Distinct rejection classes this file's tests exercise. Each entry
    /// corresponds to a check in `parse_document` or a leaf parser above that
    /// rejects rather than normalizes; the list documents, and the assertion
    /// pins, how many of them this file actually drives to a rejection.
    const EXERCISED_REJECTION_CLASSES: &[&str] = &[
        "non-canonical header version",
        "a (blob ...) line, wherever it appears",
        "missing final U+000A",
        "an extra trailing U+000A",
        "missing header line",
        "missing document line",
        "repeated header line",
        "repeated document line",
        "a section line before the mandatory document line",
        "a section reappearing after a later section (lineage after profile)",
        "an extension before the profile section",
        "a canonical-base before the extension section",
        "an envelope before the canonical base",
        "profile declarations out of (profile_id, version) order",
        "duplicate profile declaration key",
        "extension declarations out of (extension_id, version) order",
        "duplicate extension declaration key",
        "extension chunk roots out of projected-form order",
        "duplicate extension chunk roots",
        "envelope lines out of canonical reduction order",
        "an unknown ProfileId symbol",
        "a malformed custom ProfileId registry id",
        "an unknown ChunkKind symbol",
    ];

    #[test]
    fn the_suite_s_reach_is_counted() {
        assert_eq!(
            EXERCISED_REJECTION_CLASSES.len(),
            23,
            "update this count, and add a test, whenever a new rejection class is exercised"
        );
    }
}

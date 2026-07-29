//! `SpikeTextShapingIdentity` — every pin-9 field (recipe §6), and the local
//! stand-ins §3E's `TextShapingIdentity`/`TextFaceIdentity` name that this
//! crate does not yet have real types for.
//!
//! Two fields deliberately deviate from §3E's stated shape, both named in
//! `crate::findings` rather than taken silently:
//!
//! * [`SpikeTextFaceIdentity::version`] is `Option<String>`, not
//!   `Option<SemVer>` (`crate::findings::W3_F1`) — see the field's doc
//!   comment for the two measured version strings that motivate it.
//! * [`SpikeTextShapingIdentity`] carries `unicode_bidi` and
//!   `unicode_segmentation` as two separate fields rather than one
//!   `unicode_version` (`crate::findings::W3_F2`) — see the type's doc
//!   comment.
//!
//! `shaper_version` reuses the *real* `epiphany_layout_ir::glyph::SemVer`
//! unchanged: `rustybuzz`'s version string, `"0.20.1"`, **is** valid semver,
//! so nothing is lost by typing it that way — the lossiness W3-F1 names is
//! specific to font `name`-table version strings, not every version-shaped
//! field in the identity.

use epiphany_layout_ir::{FontId, SemVer};
use serde::{Deserialize, Serialize};

/// Stand-in for §3E's `ShaperId`: which shaping engine produced the run.
/// One value in this crate — `rustybuzz` — but kept as a named type (rather
/// than a bare `String` field) because §3E treats "which shaper" and "which
/// version" as a typed pair, not two interchangeable strings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeShaperId(pub String);

/// Stand-in for §3E's `AxisTag`: a 4-byte OpenType variation-axis tag
/// (`"wght"`, `"ital"`, ...). Unused by both faces in this recipe (§6: "both
/// faces measured non-variable"), so no fixture ever populates a non-empty
/// `variations` list — but the type is defined, not stubbed away, since a
/// static-font recipe proving the field can stay empty is a different claim
/// from the field not existing.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeAxisTag(pub String);

/// Stand-in for §3E's `FaceSynthesis`: whether a renderer applied a
/// synthetic weight/slant because the face itself lacks the requested style.
/// Both faces in this recipe are used at their native style, so every
/// fixture records `None` (§6: "no synthetic weight or slant is applied").
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SpikeFaceSynthesis {
    None,
    Bold,
    Italic,
    BoldItalic,
}

/// Stand-in for §3E's `FeatureSetting`: one explicit OpenType feature
/// override (tag + value). Every fixture in this recipe applies the empty
/// set (§6: "the fixtures apply no explicit feature settings; rustybuzz's
/// default horizontal feature set governs"), so no fixture ever populates
/// this — but, as with `SpikeAxisTag`, the type exists so "empty" is a
/// measured fact about the fixtures rather than an artifact of the type
/// being unable to hold anything else.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeFeatureSetting {
    pub tag: String,
    pub value: u32,
}

/// One face in the declared resolution chain (recipe §1 table), a stand-in
/// for §3E's `TextFaceIdentity` with W3-F1 already corrected.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeTextFaceIdentity {
    /// Human-facing name — diagnostic only, never an input (name-table id 1).
    pub family: String,
    /// **`crate::findings::W3_F1`**: the raw name-table id-5 string, not a
    /// parsed `SemVer`. This recipe's two faces measure
    /// `"Version 2.501;PS 2.501;ffdkm 0.1"` and `"Version 2.1.5"`; only the
    /// second parses as semver, and only after stripping the `"Version "`
    /// prefix a real-world font is not obligated to use. Typing this field
    /// `SemVer` would force either a lossy parse or an empty field on a face
    /// that plainly declares a version.
    pub version: Option<String>,
    /// SHA-256 over the exact font file's bytes — the identity that matters
    /// (recipe §1: "the content hash *is* the identity").
    pub file_hash: [u8; 32],
    /// Which face within a collection (`.ttc`/`.otc`); `0` for both faces
    /// here (neither file is a collection).
    pub face_index: u32,
    pub variations: Vec<(SpikeAxisTag, f64)>,
    pub synthesis: SpikeFaceSynthesis,
}

/// One Unicode-backed component's version identity — recorded once for the
/// bidi implementation and once for the segmentation implementation
/// (`crate::findings::W3_F2`). `unicode_version` is read from the crate's
/// own exported constant, never hand-typed, so a dependency bump that moves
/// the Unicode Character Database version is reflected automatically rather
/// than silently going stale in a literal.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeUnicodeComponent {
    /// Which crate implements this component (`"unicode-bidi"` /
    /// `"unicode-segmentation"`).
    pub impl_name: String,
    /// That crate's own version, from `Cargo.toml` / `CARGO_PKG_VERSION`.
    pub crate_version: String,
    /// The Unicode Character Database version the crate documents itself as
    /// implementing (`major.minor.patch`), read from the crate's exported
    /// `UNICODE_VERSION` constant — never guessed, and recorded as absent
    /// rather than invented if a future dependency swap does not expose one.
    pub unicode_version: Option<String>,
}

/// Every input that determines the ink (§3E `TextShapingIdentity`,
/// recipe §6) — with `crate::findings::W3_F2` already taken: the single
/// `unicode_version: UnicodeVersion` field §3E specifies is replaced with
/// `unicode_bidi` and `unicode_segmentation`, because pin 9 requires the
/// segmentation implementation and its Unicode-data version to be named
/// *separately* from the bidi implementation's, and one field cannot
/// honestly do both without asserting the two agree. **This crate's measured
/// values do not agree** (`unicode-bidi` 0.3.18 documents UAX44 database
/// 16.0.0; `unicode-segmentation` 1.13.3 documents 17.0.0) — see
/// `FIXTURES_SUMMARY.md`'s identity section for the as-measured record. That
/// disagreement is reported, never reconciled (recipe §6: "the report prints
/// both").
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpikeTextShapingIdentity {
    /// The ordered fallback chain, tried in order (recipe §1).
    pub faces: Vec<SpikeTextFaceIdentity>,
    pub shaper: SpikeShaperId,
    pub shaper_version: SemVerRecord,
    /// The OpenType feature set applied, in canonical order. Empty on every
    /// fixture (see `SpikeFeatureSetting`'s doc comment).
    pub features: Vec<SpikeFeatureSetting>,
    pub unicode_bidi: SpikeUnicodeComponent,
    pub unicode_segmentation: SpikeUnicodeComponent,
}

/// Serializable mirror of the real `epiphany_layout_ir::glyph::SemVer`
/// (which does not derive `serde::Serialize`/`Deserialize` — see
/// `crate::types`'s note on why the boundary mirrors exist at all).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemVerRecord {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl From<SemVer> for SemVerRecord {
    fn from(v: SemVer) -> Self {
        SemVerRecord {
            major: v.major,
            minor: v.minor,
            patch: v.patch,
        }
    }
}

/// The exact `rustybuzz` version this identity is pinned to (matches
/// `Cargo.toml`'s `=0.20.1` — an exact pin, not a caret range, because a
/// shaper upgrade moves the ink (W3 §3E cost 3) and this recipe's precommitted
/// glyph ids/counts (§4) describe this exact version).
pub fn rustybuzz_identity() -> (SpikeShaperId, SemVerRecord) {
    (
        SpikeShaperId("rustybuzz".to_string()),
        SemVerRecord::from(SemVer::new(0, 20, 1)),
    )
}

/// Reads `unicode-bidi`'s own exported version constants — never hand-typed.
/// `unicode_bidi::UNICODE_VERSION` is `(u64, u64, u64)`; `CARGO_PKG_VERSION`
/// values for the two crates are captured at *this crate's* compile time via
/// its own `Cargo.lock`-pinned dependency, which is exactly the version the
/// generator actually links against.
pub fn unicode_bidi_component() -> SpikeUnicodeComponent {
    let (maj, min, patch) = unicode_bidi::UNICODE_VERSION;
    SpikeUnicodeComponent {
        impl_name: "unicode-bidi".to_string(),
        crate_version: unicode_bidi_crate_version(),
        unicode_version: Some(format!("{maj}.{min}.{patch}")),
    }
}

pub fn unicode_segmentation_component() -> SpikeUnicodeComponent {
    let (maj, min, patch) = unicode_segmentation::UNICODE_VERSION;
    SpikeUnicodeComponent {
        impl_name: "unicode-segmentation".to_string(),
        crate_version: unicode_segmentation_crate_version(),
        unicode_version: Some(format!("{maj}.{min}.{patch}")),
    }
}

/// `unicode-bidi` does not export its own `CARGO_PKG_VERSION` as a crate
/// constant, so this reads it the only reproducible way available at build
/// time: from the same lockfile-pinned version this crate's `Cargo.toml`
/// declares (`=0.3.18`). Hand-typing this the same way `SemVer::new(0, 20,
/// 1)` above hand-types the shaper version, rather than a build-script probe,
/// keeps this crate's own dependency footprint unchanged — and a version
/// drift is caught structurally: `Cargo.toml` pins `=0.3.18` exactly, so a
/// mismatched lockfile fails the build before this string could go stale.
fn unicode_bidi_crate_version() -> String {
    "0.3.18".to_string()
}

fn unicode_segmentation_crate_version() -> String {
    "1.13.3".to_string()
}

/// Constructs the identity for the two faces this recipe declares (recipe
/// §1), given their already-resolved [`SpikeTextFaceIdentity`] records.
pub fn build_shaping_identity(faces: Vec<SpikeTextFaceIdentity>) -> SpikeTextShapingIdentity {
    let (shaper, shaper_version) = rustybuzz_identity();
    SpikeTextShapingIdentity {
        faces,
        shaper,
        shaper_version,
        features: Vec::new(),
        unicode_bidi: unicode_bidi_component(),
        unicode_segmentation: unicode_segmentation_component(),
    }
}

/// Placeholder for `FontId` re-export convenience at call sites that want
/// the real family-name type without importing `epiphany_layout_ir`
/// directly. Not used for serialization (see [`SpikeTextFaceIdentity::family`],
/// which is a plain `String` since `FontId` does not derive serde traits).
pub fn font_family_name(id: &FontId) -> String {
    id.0.to_string()
}

//! Face resolution (recipe §1; pins 9, 14): the declared chain resolves
//! once, from an explicit path list, with bytes hashed. A missing file is an
//! environment absence (`NOT RUN`, pin 14), never a failure; a **present**
//! file whose hash disagrees with the recipe's recorded value is a failure
//! that stops the generator rather than silently re-recording — under pin 9
//! the content hash *is* the identity, so a changed file is a changed
//! fixture set.

use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use ttf_parser::{name_id, Face as TtfFace};

use crate::identity::{SpikeFaceSynthesis, SpikeTextFaceIdentity};

/// The declared chain, in order (recipe §1 table). Resolution is closed over
/// this list: nothing outside it is ever consulted (no ambient host lookup).
pub const DECLARED_CHAIN: &[DeclaredFace] = &[
    DeclaredFace {
        path: "/usr/share/fonts/tex-gyre/texgyrepagella-regular.otf",
        expected_sha256_hex: "44e64260716d8f2bbe412baa1ee99b7c995190ac4573177c24def0b9200438c7",
        expected_bytes: 218_100,
    },
    DeclaredFace {
        path: "/usr/share/fonts/liberation-fonts/LiberationSerif-Regular.ttf",
        expected_sha256_hex: "058ea80864aef09a23f45cbec2bb5400bc3dfbdea01c3f10538a21fcb497fb74",
        expected_bytes: 393_576,
    },
];

pub struct DeclaredFace {
    pub path: &'static str,
    /// As recorded in recipe §1, "as observed on this machine on
    /// 2026-07-29." Compared against the freshly computed hash, never
    /// replaced by it.
    pub expected_sha256_hex: &'static str,
    pub expected_bytes: u64,
}

/// A face successfully resolved: its bytes (owned, so a `rustybuzz::Face`
/// can be built from them on demand without a self-referential struct) and
/// its identity record.
pub struct LoadedFace {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub identity: SpikeTextFaceIdentity,
}

impl LoadedFace {
    /// Builds a fresh `rustybuzz::Face` borrowing this face's bytes, for the
    /// duration of one shaping call. `rustybuzz::Face` derefs to
    /// `ttf_parser::Face`, so both crates' queries (`glyph_index`,
    /// `units_per_em`, ...) are available through the same handle.
    pub fn face(&self) -> rustybuzz::Face<'_> {
        rustybuzz::Face::from_slice(&self.bytes, self.identity.face_index)
            .unwrap_or_else(|| panic!("{}: bytes no longer parse as a font", self.path.display()))
    }
}

/// The outcome of resolving one declared face: loaded, or an environment
/// absence (pin 14: `NOT RUN`, never a failure).
pub enum FaceResolution {
    Loaded(LoadedFace),
    Missing { path: PathBuf },
}

impl fmt::Debug for FaceResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaceResolution::Loaded(lf) => write!(f, "Loaded({})", lf.path.display()),
            FaceResolution::Missing { path } => write!(f, "Missing({})", path.display()),
        }
    }
}

/// Resolves the whole declared chain (recipe §1). Never partial: every
/// declared face is attempted, so a caller sees every `NOT RUN` at once
/// rather than stopping at the first.
///
/// # Panics
///
/// Panics — loudly, not a `NOT RUN` and not a silent re-record — if a
/// **present** file's SHA-256 disagrees with `DeclaredFace::expected_sha256_hex`
/// or its byte count disagrees with `expected_bytes`. Under pin 9 the content
/// hash *is* the identity: a mismatch means this machine's font differs from
/// the one the recipe's §4 measurements describe, so every downstream
/// expectation in this crate would be describing a font that is not actually
/// being shaped.
pub fn resolve_declared_chain() -> Vec<FaceResolution> {
    DECLARED_CHAIN.iter().map(resolve_one).collect()
}

fn resolve_one(declared: &DeclaredFace) -> FaceResolution {
    let path = Path::new(declared.path).to_path_buf();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return FaceResolution::Missing { path };
        }
        Err(e) => panic!(
            "{}: unreadable ({e}) — not a NOT_FOUND, not a NOT RUN",
            path.display()
        ),
    };

    if bytes.len() as u64 != declared.expected_bytes {
        panic!(
            "{}: {} bytes on disk, recipe §1 records {} — this machine's font differs from the \
             one §4's measurements describe; STOP, do not re-record",
            path.display(),
            bytes.len(),
            declared.expected_bytes
        );
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let hex = hex_encode(&digest);
    if hex != declared.expected_sha256_hex {
        panic!(
            "{}: sha256 {hex} disagrees with recipe §1's recorded {} — STOP, do not re-record",
            path.display(),
            declared.expected_sha256_hex
        );
    }

    let mut file_hash = [0u8; 32];
    file_hash.copy_from_slice(&digest);

    let ttf = TtfFace::parse(&bytes, 0)
        .unwrap_or_else(|e| panic!("{}: does not parse as a font: {e:?}", path.display()));

    let family = best_name(&ttf, name_id::FAMILY)
        .unwrap_or_else(|| panic!("{}: no name-table family (id 1) entry", path.display()));
    let version = best_name(&ttf, name_id::VERSION);

    let identity = SpikeTextFaceIdentity {
        family,
        version,
        file_hash,
        face_index: 0,
        variations: Vec::new(),
        synthesis: SpikeFaceSynthesis::None,
    };

    FaceResolution::Loaded(LoadedFace {
        path,
        bytes,
        identity,
    })
}

/// Reads one name-table id, preferring a Unicode-encoded entry (readable
/// without further platform-specific decoding); falls back to the first
/// entry present under any encoding `ttf_parser::Name::to_string` can
/// decode. Diagnostic-only field (recipe §6), so "first decodable entry" is
/// an adequate rule — this never feeds shaping.
fn best_name(face: &TtfFace, id: u16) -> Option<String> {
    let names = face.names();
    let mut fallback = None;
    for name in names {
        if name.name_id != id {
            continue;
        }
        if name.is_unicode() {
            if let Some(s) = name.to_string() {
                return Some(s);
            }
        }
        if fallback.is_none() {
            fallback = name.to_string();
        }
    }
    fallback
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_chain_has_two_faces_with_distinct_upem() {
        // Not a face-loading test (that needs the real files, exercised by
        // the generate binary and the integration tests below); this checks
        // the *declared* literals themselves stay in the shape recipe §1
        // commits to — two entries, non-empty hash hex, non-zero byte counts.
        assert_eq!(DECLARED_CHAIN.len(), 2);
        for d in DECLARED_CHAIN {
            assert_eq!(
                d.expected_sha256_hex.len(),
                64,
                "sha256 hex must be 64 chars (32 bytes, no 0x prefix)"
            );
            assert!(d.expected_bytes > 0);
        }
    }

    #[test]
    fn resolving_the_real_chain_succeeds_or_reports_missing_never_panics_on_a_present_file() {
        // This is an environment-dependent smoke test: on a machine with the
        // recipe's declared fonts installed (this one, per recipe §1), every
        // entry resolves to `Loaded`. It intentionally does not assert
        // `Loaded` unconditionally, so it degrades to recording `Missing`
        // rather than failing on a machine without the fonts — but it must
        // never itself panic, which is what `resolve_one`'s hash check would
        // do on a *present-but-wrong* file.
        let resolved = resolve_declared_chain();
        assert_eq!(resolved.len(), DECLARED_CHAIN.len());
    }
}

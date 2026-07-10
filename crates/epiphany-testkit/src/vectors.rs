//! The cross-implementation decode conformance corpus (P4 of the
//! decode-hardening track).
//!
//! `spec/vectors/decode_vectors.txt` is a committed, human-diffable list of byte
//! strings and their normative accept/reject verdict. It is what a *second*
//! implementation is checked against — the reference implementation's fuzzers
//! prove its own decoders self-consistent, which says nothing about whether a
//! foreign decoder agrees with the format.
//!
//! Two properties are enforced here:
//!
//! 1. **The committed file is what the generator produces.** A wire-format
//!    change that moves a vector's bytes must move the file too, deliberately,
//!    in the diff.
//! 2. **Every vector gets its declared verdict** from the owning crate's
//!    decoder. An `accept` vector additionally must re-encode to its own bytes.
//!
//! The `class` column is informative: implementations need not agree on error
//! taxonomy, only on accept versus reject.

/// The committed corpus.
pub const COMMITTED: &str = include_str!("../../../spec/vectors/decode_vectors.txt");

/// The path a regeneration writes to, relative to the workspace root.
pub const PATH: &str = "spec/vectors/decode_vectors.txt";

const HEADER: &str = "\
# Epiphany decode conformance vectors — format version 1
#
# Generated. Regenerate with:
#     cargo run -q -p epiphany-testkit --example generate_vectors
# `epiphany_testkit::vectors::the_committed_corpus_matches_the_generator` fails
# on drift, so a wire-format change must land here deliberately.
#
# One vector per line, space-separated:
#
#     <surface>  <verdict>  <class>  <name>  <hex>
#
# `verdict` is `accept` or `reject`, and is the ONLY normative column besides
# the bytes. A conforming decoder must accept every `accept` vector and reject
# every `reject` vector. An accepted value must additionally re-encode to
# exactly the vector's bytes: canonical decode is injective, which is what
# content-addressing rests on.
#
# `class` names why a `reject` vector is rejected. It is informative only —
# implementations need not agree on error taxonomy. `-` where not applicable.
#
# `<hex>` is lowercase, no separators; `-` denotes the empty byte string.
";

fn to_hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_string();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s == "-" {
        return Some(Vec::new());
    }
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Every vector, in a stable order: the operation layer, then the bundle wire.
fn all() -> Vec<(String, String, String, String, Vec<u8>)> {
    let ops = epiphany_ops::vectors::decode_vectors()
        .into_iter()
        .map(|(s, v, c, n, b)| {
            (
                s.to_string(),
                v.to_string(),
                c.to_string(),
                n.to_string(),
                b,
            )
        });
    let bundle = epiphany_bundle::vectors::decode_vectors()
        .into_iter()
        .map(|(s, v, c, n, b)| {
            (
                s.to_string(),
                v.to_string(),
                c.to_string(),
                n.to_string(),
                b,
            )
        });
    ops.chain(bundle).collect()
}

/// Renders the corpus file.
pub fn render() -> String {
    let mut out = String::from(HEADER);
    let mut surface = String::new();
    for (s, v, c, n, b) in all() {
        if s != surface {
            out.push_str("\n# ");
            out.push_str(&s);
            out.push('\n');
            surface = s.clone();
        }
        out.push_str(&format!("{s} {v} {c} {n} {}\n", to_hex(&b)));
    }
    out
}

/// One parsed row.
pub struct Row {
    pub surface: String,
    pub verdict: String,
    pub class: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Parses the corpus file, skipping comments and blank lines.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 5 {
            return Err(format!(
                "line {}: expected 5 columns, got {}",
                i + 1,
                f.len()
            ));
        }
        rows.push(Row {
            surface: f[0].to_string(),
            verdict: f[1].to_string(),
            class: f[2].to_string(),
            name: f[3].to_string(),
            bytes: from_hex(f[4]).ok_or_else(|| format!("line {}: bad hex", i + 1))?,
        });
    }
    Ok(rows)
}

/// Runs every vector against the owning crate's decoder, returning the number
/// checked or the disagreements.
pub fn verify(text: &str) -> Result<usize, Vec<String>> {
    let rows = match parse(text) {
        Ok(r) => r,
        Err(e) => return Err(vec![e]),
    };
    let mut failures = Vec::new();
    for row in &rows {
        let result = epiphany_ops::vectors::check(&row.surface, &row.bytes)
            .or_else(|| epiphany_bundle::vectors::check(&row.surface, &row.bytes));
        let Some(result) = result else {
            failures.push(format!("{}: no decoder owns this surface", row.surface));
            continue;
        };
        // `Ok(injective)` = accepted; `Err` = rejected. A decoder that accepts
        // a `reject` vector fails even if it then re-encodes it faithfully, and
        // one that accepts an `accept` vector non-injectively fails too. The two
        // must not be collapsed: silently normalizing non-canonical bytes IS
        // accepting them, and is the defect the corpus exists to catch.
        match (row.verdict.as_str(), &result) {
            ("accept", Ok(true)) | ("reject", Err(_)) => {}
            ("accept", Ok(false)) => failures.push(format!(
                "{}/{}: accepted, but the value does not re-encode to its bytes",
                row.surface, row.name
            )),
            ("reject", Ok(injective)) => failures.push(format!(
                "{}/{} ({}): declared reject, but was ACCEPTED (injective={injective})",
                row.surface, row.name, row.class
            )),
            _ => failures.push(format!(
                "{}/{} ({}): declared {}, got {:?}",
                row.surface, row.name, row.class, row.verdict, result
            )),
        }
    }
    if failures.is_empty() {
        Ok(rows.len())
    } else {
        Err(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file in the tree is exactly what the generator emits. A wire-format
    /// change must land in this diff.
    #[test]
    fn the_committed_corpus_matches_the_generator() {
        assert_eq!(
            COMMITTED,
            render(),
            "\n{PATH} is stale. Regenerate:\n    \
             cargo run -q -p epiphany-testkit --example generate_vectors\n"
        );
    }

    /// The reference implementation satisfies the contract it is publishing. If
    /// it cannot, the corpus is wrong, not the decoder.
    #[test]
    fn the_reference_implementation_agrees_with_every_vector() {
        match verify(COMMITTED) {
            Ok(n) => assert!(n >= 25, "only {n} vectors — the corpus has thinned"),
            Err(failures) => panic!(
                "{} disagreement(s):\n{}",
                failures.len(),
                failures.join("\n")
            ),
        }
    }

    /// The corpus must pin the rejection classes this repository learned the
    /// hard way. Losing one would quietly stop testing it.
    #[test]
    fn the_corpus_pins_every_class_we_have_shipped_a_bug_in() {
        let rows = parse(COMMITTED).expect("parses");
        let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        for required in [
            // A guard catches this; no per-site check exists (P2).
            "non-canonical-map-order",
            // Only a per-site check catches this; a guard is blind (P2).
            "non-canonical-vec-order",
            // A guard *masked* this in the manifest; the index had none (P3).
            "lenient-sub-codec",
            "trailing-bytes",
            "truncated",
            "unknown-discriminant",
            "count-exceeds-remaining",
        ] {
            assert!(
                classes.contains(&required),
                "the corpus no longer pins `{required}`"
            );
        }
    }

    /// Both verdicts on every surface, or the corpus pins half a contract.
    #[test]
    fn every_surface_carries_both_verdicts() {
        use std::collections::BTreeMap;
        let rows = parse(COMMITTED).expect("parses");
        let mut seen: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
        for r in &rows {
            let e = seen.entry(r.surface.as_str()).or_default();
            match r.verdict.as_str() {
                "accept" => e.0 = true,
                "reject" => e.1 = true,
                other => panic!("unknown verdict {other}"),
            }
        }
        assert!(seen.len() >= 5, "surfaces: {:?}", seen.keys());
        for (surface, (accept, reject)) in seen {
            assert!(accept, "{surface} has no accept vector");
            assert!(reject, "{surface} has no reject vector");
        }
    }
}

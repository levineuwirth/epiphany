//! A committed completeness gate on the Text Projection companion's grammar.
//!
//! Version 0.3.0 claimed a "machine-checked" grammar. It was checked, once, by a
//! script that was never committed — true of that run and of nothing durable.
//! This is the durable form, and it is exactly the class of evidence P2–P4
//! established: a claim about coverage is a lock, and an uncommitted check is not
//! one.
//!
//! What it enforces:
//!
//! 1. Every nonterminal the grammar references is defined, and every nonterminal
//!    it defines is reachable from `projection`.
//! 2. The `kind` alternatives are **exactly** the operation vocabulary, *derived*
//!    from `OperationKindTag` rather than transcribed. Adding a tag without
//!    adding its production fails here, and the `match` below fails to compile
//!    without a name for it — the same two-layer guarantee
//!    `operation_kind_tag_vocabulary!` gives the decoder.
//! 3. The `chunk-kind` alternatives are exactly the `ChunkKind` vocabulary.
//! 4. The escape rule excludes the four codepoints
//!    `req:textproj:string-escapes` requires a writer to escape — the
//!    contradiction that 0.4.0 fixed.
//! 5. `value` admits a bare parenthesised list. Without it the grammar cannot
//!    derive a sequence, so it cannot derive an ordinary pitched note — the
//!    omission 0.5.0 fixed. And it must *not* re-introduce a symbol-headed
//!    struct alternative: shape does not distinguish a struct from a sequence,
//!    and `req:textproj:schema-directed` says so rather than pretending
//!    otherwise.

use std::collections::{BTreeSet, VecDeque};

use epiphany_bundle::ChunkKind;
use epiphany_ops::OperationKindTag;

const SPEC: &str = include_str!("../../../spec/text_projection.tex");

/// The grammar block: from the `projection` production to the end of its listing.
///
/// Located by name, never by column. The 0.4.0 reflow of the production headers
/// broke a column-anchored needle, and a checker that cannot find the grammar
/// silently checks nothing.
fn grammar() -> &'static str {
    let start = SPEC
        .lines()
        .scan(0usize, |acc, l| {
            let here = *acc;
            *acc += l.len() + 1;
            Some((here, l))
        })
        .find(|(_, l)| {
            l.split_once("::=")
                .is_some_and(|(lhs, _)| lhs.trim() == "projection")
        })
        .map(|(at, _)| at)
        .expect("the grammar block begins with the `projection` production");
    let end = SPEC[start..]
        .find("\\end{lstlisting}")
        .expect("the grammar block is a listing");
    &SPEC[start..start + end]
}

/// Strips `;` line comments, then removes `<...>` prose spans, which may run
/// across lines. A per-line stripper leaks the second line's characters, and the
/// leaked fragments look like nonterminals.
fn uncommented(g: &str) -> String {
    let no_comments: String = g
        .lines()
        .map(|l| l.split(';').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::new();
    let mut depth = 0usize;
    for c in no_comments.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            '\n' if depth > 0 => out.push('\n'),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

/// The left-hand side of every production.
fn defined(g: &str) -> BTreeSet<String> {
    uncommented(g)
        .lines()
        .filter_map(|l| {
            let (lhs, _) = l.split_once("::=")?;
            let name = lhs.trim();
            (!name.is_empty() && name.chars().all(is_name_char)).then(|| name.to_string())
        })
        .collect()
}

/// Removes quoted terminals, angle-bracket prose, and character classes, leaving
/// only nonterminal references.
fn strip_terminals(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                }
            }
            '\'' => {
                for d in chars.by_ref() {
                    if d == '\'' {
                        break;
                    }
                }
            }
            '[' => {
                for d in chars.by_ref() {
                    if d == ']' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Every nonterminal referenced on a right-hand side, per production.
fn references(g: &str) -> Vec<(String, BTreeSet<String>)> {
    let text = uncommented(g);
    // A production may continue onto `|` continuation lines, which have no `::=`.
    let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
    for line in text.lines() {
        let (lhs, rhs) = match line.split_once("::=") {
            Some((l, r)) if l.trim().chars().all(is_name_char) && !l.trim().is_empty() => {
                out.push((l.trim().to_string(), BTreeSet::new()));
                (l.trim().to_string(), r)
            }
            _ => match out.last() {
                Some((name, _)) => (name.clone(), line),
                None => continue,
            },
        };
        let _ = lhs;
        let bare = strip_terminals(rhs);
        let mut token = String::new();
        let entry = &mut out.last_mut().expect("a production is open").1;
        for c in bare.chars().chain(std::iter::once(' ')) {
            if is_name_char(c) {
                token.push(c);
            } else {
                // A nonterminal is `[a-z] [a-z0-9-]*`, so a token that does not
                // begin with a letter is not one. Without this, the `U+0022` in
                // the escape production reads as a nonterminal named `0022`.
                if token.starts_with(|c: char| c.is_ascii_lowercase()) {
                    entry.insert(std::mem::take(&mut token));
                }
                token.clear();
            }
        }
    }
    out
}

#[test]
fn every_nonterminal_is_defined_and_reachable() {
    let g = grammar();
    let defined = defined(g);
    assert!(defined.contains("projection"), "the start symbol exists");

    let refs = references(g);
    let used: BTreeSet<String> = refs.iter().flat_map(|(_, r)| r.iter().cloned()).collect();

    let undefined: Vec<&String> = used.difference(&defined).collect();
    assert!(
        undefined.is_empty(),
        "grammar references undefined nonterminals: {undefined:?}"
    );

    // Reachability from `projection`.
    let edges: std::collections::BTreeMap<String, BTreeSet<String>> = refs.into_iter().collect();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from(vec!["projection".to_string()]);
    while let Some(n) = queue.pop_front() {
        if !seen.insert(n.clone()) {
            continue;
        }
        for next in edges.get(&n).into_iter().flatten() {
            queue.push_back(next.clone());
        }
    }
    let unreachable: Vec<&String> = defined.difference(&seen).collect();
    assert!(
        unreachable.is_empty(),
        "grammar defines unreachable nonterminals: {unreachable:?}"
    );
}

/// The Operation Catalog's section name, hyphenated, for each tag.
///
/// Exhaustive over `OperationKindTag`, so a new operation kind cannot be added
/// without naming it here — and `the_kind_productions_are_the_operation_vocabulary`
/// then fails until the grammar carries it. The tag names are *not* the catalog
/// names: the tag space renamed three pairs (`InsertRegion`, `InsertStaff`,
/// `InsertStaffInstance`), and the projection follows the semantics.
fn catalog_name(tag: OperationKindTag) -> &'static str {
    match tag {
        OperationKindTag::InsertEvent => "insert-event",
        OperationKindTag::DeleteEvent => "delete-event",
        OperationKindTag::ModifyEvent => "modify-event",
        OperationKindTag::RespellPitch => "respell-pitch",
        OperationKindTag::Transpose => "transpose",
        OperationKindTag::TransposeInterval => "transpose-interval",
        OperationKindTag::CreateCrossCutting => "create-cross-cutting",
        OperationKindTag::DeleteCrossCutting => "delete-cross-cutting",
        OperationKindTag::ModifyCrossCutting => "modify-cross-cutting",
        OperationKindTag::ChangeRegionTimeModel => "change-region-time-model",
        OperationKindTag::InsertRegion => "create-region",
        OperationKindTag::DeleteRegion => "delete-region",
        OperationKindTag::InsertStaffInstance => "create-staff-instance",
        OperationKindTag::DeleteStaffInstance => "delete-staff-instance",
        OperationKindTag::SetUserSystemBreak => "set-user-system-break",
        OperationKindTag::SetUserPageBreak => "set-user-page-break",
        OperationKindTag::DeclareTransaction => "declare-transaction",
        OperationKindTag::Registered(_) => "registered",
        OperationKindTag::InsertIdentifiedPitch => "insert-identified-pitch",
        OperationKindTag::DeleteIdentifiedPitch => "delete-identified-pitch",
        OperationKindTag::ModifyIdentifiedPitch => "modify-identified-pitch",
        OperationKindTag::CreateVoice => "create-voice",
        OperationKindTag::DeleteVoice => "delete-voice",
        OperationKindTag::SetMetadata => "set-metadata",
        OperationKindTag::SetMetricGrid => "set-metric-grid",
        OperationKindTag::InsertStaff => "create-staff",
        OperationKindTag::SetTimeSignature => "set-time-signature",
        OperationKindTag::SetTempoSegment => "set-tempo-segment",
        OperationKindTag::SetStaffLayout => "set-staff-layout",
        OperationKindTag::CreateRepeatStructure => "create-repeat-structure",
        OperationKindTag::DeleteRepeatStructure => "delete-repeat-structure",
    }
}

/// The right-hand side of `production`, spanning its continuation lines.
///
/// Located by name, not by column: a grammar reflow must not silently turn a
/// check off. Version 0.3.0's checks were column-anchored and did exactly that.
fn production_block(production: &str) -> &'static str {
    let g = grammar();
    let start = g
        .lines()
        .scan(0usize, |acc, l| {
            let here = *acc;
            *acc += l.len() + 1;
            Some((here, l))
        })
        .find(|(_, l)| {
            l.split_once("::=")
                .is_some_and(|(lhs, _)| lhs.trim() == production)
        })
        .map(|(at, l)| at + l.find("::=").expect("has ::=") + 3)
        .unwrap_or_else(|| panic!("the grammar defines `{production}`"));
    let rest = &g[start..];
    let end = rest
        .lines()
        .scan(0usize, |acc, l| {
            let here = *acc;
            *acc += l.len() + 1;
            Some((here, l))
        })
        .find(|(_, l)| l.contains("::=") && !l.trim_start().starts_with('|'))
        .map(|(at, _)| at)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The alternatives of `production`, as the constructor symbol each opens with.
fn alternatives(production: &str) -> BTreeSet<String> {
    let block = production_block(production);

    let mut out = BTreeSet::new();
    for (i, _) in block.match_indices("\"(") {
        let name: String = block[i + 2..]
            .chars()
            .take_while(|c| is_name_char(*c))
            .collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    // Bare-symbol alternatives, e.g. `"not-in-tuplet"`.
    for (i, _) in block.match_indices('"') {
        let tail = &block[i + 1..];
        if tail.starts_with('(') {
            continue;
        }
        let name: String = tail.chars().take_while(|c| is_name_char(*c)).collect();
        if !name.is_empty() && tail[name.len()..].starts_with('"') {
            out.insert(name);
        }
    }
    out
}

#[test]
fn the_kind_productions_are_the_operation_vocabulary() {
    let expected: BTreeSet<String> = OperationKindTag::PAYLOAD_FREE
        .iter()
        .copied()
        .chain(std::iter::once(OperationKindTag::Registered(
            epiphany_ops::OperationKindRegistryId(0),
        )))
        .map(|t| catalog_name(t).to_string())
        .collect();
    assert_eq!(
        expected.len(),
        31,
        "30 payload-free kinds plus `Registered`"
    );

    let actual = alternatives("kind");
    assert_eq!(
        actual,
        expected,
        "the grammar's `kind` alternatives must be exactly the operation vocabulary.\n\
         missing from the grammar: {:?}\n\
         present but not a kind: {:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(&expected).collect::<Vec<_>>()
    );
}

#[test]
fn the_chunk_kind_productions_are_the_chunk_vocabulary() {
    let expected: BTreeSet<String> = (0u8..=8)
        .map(|d| {
            let kind =
                ChunkKind::from_discriminant(d).unwrap_or_else(|| panic!("chunk kind {d} exists"));
            kebab(&format!("{kind:?}"))
        })
        .collect();
    assert_eq!(expected.len(), 9);
    assert_eq!(alternatives("chunk-kind"), expected);
}

/// `OperationEnvelopeBlock` -> `operation-envelope-block`.
fn kebab(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `req:textproj:string-escapes` requires a writer to escape exactly the
/// quotation mark, the backslash, U+000A and U+0009, and a parser to reject a
/// literal one. Version 0.3.0's `unescaped` production admitted the backslash,
/// contradicting the requirement it sits beneath.
#[test]
fn the_escape_grammar_agrees_with_the_escape_requirement() {
    assert!(
        defined(grammar()).contains("escape"),
        "the escape sequences must be a production of their own"
    );

    // `unescaped` must exclude every character the requirement obliges a writer to
    // escape. Version 0.3.0 admitted the backslash here while requiring it escaped.
    let unescaped = production_block("unescaped");
    for codepoint in ["U+0022", "U+005C", "U+000A", "U+0009"] {
        assert!(
            unescaped.contains(codepoint),
            "`unescaped` must exclude {codepoint} by codepoint; it reads: {unescaped}"
        );
    }

    // Each escape is *two* characters: the U+005C introducer and one more. Spelling
    // the introducer as a quoted terminal `"\\"` reads as two backslashes and makes
    // every escape three characters long -- the requirement says two.
    let escapes: Vec<Vec<String>> = uncommented(production_block("escape"))
        .split('|')
        .map(|alt| alt.split_whitespace().map(str::to_string).collect())
        .collect();
    let tails: BTreeSet<String> = escapes
        .iter()
        .map(|alt| {
            assert_eq!(
                alt.len(),
                2,
                "an escape is exactly two characters, the introducer and one more; \
                 found {alt:?}"
            );
            assert_eq!(
                alt[0], "U+005C",
                "every escape is introduced by U+005C, written as a codepoint so no \
                 quoting convention can make it ambiguous; found {:?}",
                alt[0]
            );
            alt[1].clone()
        })
        .collect();
    let expected: BTreeSet<String> = ["U+0022", "U+005C", "\"n\"", "\"t\""]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        tails, expected,
        "the escapes are exactly \\\", \\\\, \\n and \\t -- no more, no fewer"
    );
}

/// `value` must admit a bare parenthesised list, or the grammar cannot derive any
/// sequence — a `PitchedEvent`'s `articulations` among them, which makes an
/// ordinary `insert-event` line underivable. It must equally *not* carry a
/// symbol-headed struct alternative: a sequence whose first element is a fieldless
/// variant has exactly that shape, so a grammar claiming to tell them apart would
/// be lying. `req:textproj:schema-directed` carries the distinction instead.
#[test]
fn value_admits_a_bare_list_and_claims_no_shape_it_cannot_distinguish() {
    let block = uncommented(production_block("value"));
    let alts: Vec<String> = block
        .split('|')
        .map(|a| a.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();

    assert!(
        alts.iter().any(|a| a == "\"(\" value* \")\""),
        "`value` must admit a bare parenthesised list, else no sequence is \
         derivable; alternatives were {alts:?}"
    );
    assert!(
        !alts.iter().any(|a| a.contains("symbol \" \" value*")),
        "`value` must not claim to distinguish a struct from a sequence by shape; \
         alternatives were {alts:?}"
    );

    // The requirement that carries the distinction must exist and be cited where
    // it is relied on: the value rule, the strict-parse rule, and the grammar.
    assert!(SPEC.contains("\\label{req:textproj:schema-directed}"));
    let citations = SPEC.matches("req:textproj:schema-directed").count();
    assert!(
        citations >= 4,
        "expected the label plus citations from the value rule, strict parsing \
         and the grammar; found {citations}"
    );
}

/// The mono font must not apply TeX ligatures. `tlig` rewrites `\"` as a right
/// curly quote and `--` as an en dash, so the grammar -- which delimits terminals
/// with U+0022 and builds escapes from U+005C -- would render characters other
/// than the ones it specifies. A syntax document cannot misprint its own syntax.
#[test]
fn the_mono_font_does_not_substitute_glyphs_in_the_grammar() {
    let mono = SPEC
        .lines()
        .find(|l| l.starts_with("\\setmonofont"))
        .expect("the document sets a mono font");
    assert!(
        !mono.contains("Ligatures"),
        "the mono font must not enable ligatures; it reads: {mono}"
    );
}

/// The two sequences whose binary order reads erased physical attributes must be
/// named by the derived-ordering requirement, and it must be cited where they are
/// defined. A rule nobody points at is a rule nobody applies.
#[test]
fn the_derived_ordering_requirement_is_cited_where_it_applies() {
    assert!(SPEC.contains("\\label{req:textproj:derived-ordering}"));
    let citations = SPEC.matches("req:textproj:derived-ordering").count();
    assert!(
        citations >= 4,
        "expected the label plus citations from the value rule, the blob \
         requirement and the extension requirement; found {citations}"
    );
}

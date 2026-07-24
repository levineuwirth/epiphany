//! Parses the SVG path `d` grammar the bundled outlines are generated in into
//! the shared typed [`PathCommand`] form (Editor T4-pre W2 pin 4), and
//! re-emits it in the generator's exact byte formatting so a round-trip
//! proves the typed form describes the same geometry (pin 5).
//!
//! **Correction to the contract's stated grammar.**
//! `spec/CONTRACT_EDITOR_T4PRE_W2_GLYPHS.md`'s "verified starting point"
//! describes the generated grammar as "absolute `M`/`L`/`C`/`Z`, decimal
//! coordinates, 4 decimal places". Verified against all 37 bundled glyphs
//! before writing this parser, the real grammar is wider on both counts:
//!
//! * The generator (`tools/extract_bravura_outlines.py`, via
//!   `fontTools.pens.svgPathPen.SVGPathPen`'s default `optimizeCommands`)
//!   also emits absolute `V` (vertical-only lineto) and `H` (horizontal-only
//!   lineto) wherever a lineto's target shares an axis with the current
//!   point — 23 of the 37 bundled glyphs use at least one. [`PathCommand`]
//!   has no shorthand variant, so [`parse_d`] lowers `V`/`H` to
//!   [`PathCommand::LineTo`]; [`emit_d`] reconstructs the shorthand
//!   byte-for-byte from geometry alone (comparing the target to the current
//!   point — see its doc comment), which is exactly what the round-trip
//!   test (pin 5, contract test g1) proves for every bundled glyph.
//! * Coordinates are rounded to *at most* 4 decimals with trailing zeros (and
//!   a bare `-0`) stripped by the generator's own `round_d`
//!   (`tools/extract_bravura_outlines.py:180-185`), so the printed precision
//!   varies per number — 0 to 3 fractional digits are observed in the
//!   bundled data (never 4, though the grammar allows it); it is not a fixed
//!   width.
//!
//! Every command in the observed grammar carries exactly one point (`M`,
//! `L`), one coordinate (`V`, `H`), three points (`C`), or none (`Z`) — the
//! generator never merges consecutive same-type commands into a
//! multi-coordinate group, so the parser does not need to handle that SVG
//! generality either.

use epiphany_layout_ir::{PathCommand, Point};

/// Parses an absolute SVG path `d` string in the generator's grammar
/// (`M`/`L`/`C`/`V`/`H`/`Z`, one point/coordinate per command, absolute
/// coordinates) into typed path commands.
///
/// Panics on malformed input. The input is always this crate's own bundled,
/// generator-produced constant data — never external or untrusted text — so
/// a parse failure is a bug in this parser or the bundled table, not a
/// runtime condition a caller should recover from.
pub(crate) fn parse_d(d: &str) -> Vec<PathCommand> {
    let bytes = d.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    let mut cur = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);

    let take_number = |bytes: &[u8], i: &mut usize| -> f32 {
        while *i < bytes.len() && bytes[*i] == b' ' {
            *i += 1;
        }
        let start = *i;
        if *i < bytes.len() && bytes[*i] == b'-' {
            *i += 1;
        }
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b'.' {
            *i += 1;
            while *i < bytes.len() && bytes[*i].is_ascii_digit() {
                *i += 1;
            }
        }
        let tok = std::str::from_utf8(&bytes[start..*i])
            .unwrap_or_else(|e| panic!("non-UTF-8 number token in {d:?}: {e}"));
        tok.parse::<f32>()
            .unwrap_or_else(|e| panic!("bad number token {tok:?} in {d:?}: {e}"))
    };

    while i < bytes.len() {
        let cmd = bytes[i];
        i += 1;
        match cmd {
            b'M' => {
                let x = take_number(bytes, &mut i);
                let y = take_number(bytes, &mut i);
                out.push(PathCommand::MoveTo(Point::new(x, y)));
                cur = (x, y);
                subpath_start = cur;
            }
            b'L' => {
                let x = take_number(bytes, &mut i);
                let y = take_number(bytes, &mut i);
                out.push(PathCommand::LineTo(Point::new(x, y)));
                cur = (x, y);
            }
            b'V' => {
                let y = take_number(bytes, &mut i);
                cur = (cur.0, y);
                out.push(PathCommand::LineTo(Point::new(cur.0, cur.1)));
            }
            b'H' => {
                let x = take_number(bytes, &mut i);
                cur = (x, cur.1);
                out.push(PathCommand::LineTo(Point::new(cur.0, cur.1)));
            }
            b'C' => {
                let c1x = take_number(bytes, &mut i);
                let c1y = take_number(bytes, &mut i);
                let c2x = take_number(bytes, &mut i);
                let c2y = take_number(bytes, &mut i);
                let tx = take_number(bytes, &mut i);
                let ty = take_number(bytes, &mut i);
                out.push(PathCommand::CurveTo {
                    control1: Point::new(c1x, c1y),
                    control2: Point::new(c2x, c2y),
                    to: Point::new(tx, ty),
                });
                cur = (tx, ty);
            }
            b'Z' => {
                out.push(PathCommand::Close);
                cur = subpath_start;
            }
            other => panic!(
                "unsupported path command byte {:#04x} ({}) in {d:?}: the bundled grammar is \
                 absolute M/L/C/V/H/Z only",
                other, other as char
            ),
        }
    }
    out
}

/// Re-emits typed path commands in the generator's exact `d`-string
/// formatting (pin 5's round-trip proof).
///
/// [`PathCommand::LineTo`] carries no record of whether it was originally an
/// `L`, `V`, or `H` command — the shared type has no shorthand variant (see
/// the module doc). This reconstructs the shorthand from geometry alone,
/// matching `fontTools.pens.svgPathPen.SVGPathPen`'s own rule: emit `V` when
/// only the *x* coordinate is unchanged from the current point, `H` when
/// only *y* is unchanged, and `L` otherwise (including the degenerate
/// zero-length case, x and y both unchanged, which does not occur in any
/// bundled glyph — verified below). Numbers are formatted exactly as the
/// generator's `round_d`: at most 4 decimals, trailing zeros and a trailing
/// `.` stripped, `-0`/empty normalised to `0`.
///
/// Exists solely for the round-trip proof (test g1) — nothing in production
/// code re-serializes the typed form (pin 5: `render-svg` always emits the
/// *stored* `d` string), so this is compiled only for `cargo test`.
#[cfg(test)]
pub(crate) fn emit_d(commands: &[PathCommand]) -> String {
    let mut out = String::new();
    let mut cur = (0.0f32, 0.0f32);
    let mut subpath_start = (0.0f32, 0.0f32);
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(p) => {
                let (x, y) = (p.x.0, p.y.0);
                out.push('M');
                push_num(&mut out, x);
                out.push(' ');
                push_num(&mut out, y);
                cur = (x, y);
                subpath_start = cur;
            }
            PathCommand::LineTo(p) => {
                let (x, y) = (p.x.0, p.y.0);
                let x_same = x == cur.0;
                let y_same = y == cur.1;
                if x_same && !y_same {
                    out.push('V');
                    push_num(&mut out, y);
                } else if y_same && !x_same {
                    out.push('H');
                    push_num(&mut out, x);
                } else {
                    out.push('L');
                    push_num(&mut out, x);
                    out.push(' ');
                    push_num(&mut out, y);
                }
                cur = (x, y);
            }
            PathCommand::CurveTo {
                control1,
                control2,
                to,
            } => {
                out.push('C');
                push_num(&mut out, control1.x.0);
                out.push(' ');
                push_num(&mut out, control1.y.0);
                out.push(' ');
                push_num(&mut out, control2.x.0);
                out.push(' ');
                push_num(&mut out, control2.y.0);
                out.push(' ');
                push_num(&mut out, to.x.0);
                out.push(' ');
                push_num(&mut out, to.y.0);
                cur = (to.x.0, to.y.0);
            }
            PathCommand::Close => {
                out.push('Z');
                cur = subpath_start;
            }
        }
    }
    out
}

/// Formats one coordinate exactly as the generator's `round_d`: at most 4
/// decimals, trailing zeros and a trailing `.` stripped, `-0`/empty
/// normalised to `0`. `emit_d`'s only caller; test-only for the same reason.
#[cfg(test)]
fn push_num(out: &mut String, v: f32) {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = write!(s, "{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.is_empty() || s == "-0" {
        s = "0".to_owned();
    }
    out.push_str(&s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outlines_generated::BRAVURA_OUTLINES;

    /// (g1) The packet's load-bearing test: every bundled glyph's `d` string
    /// parses and re-emits byte-for-byte identical. If this holds for all 37
    /// bundled glyphs, the typed form provably describes the same geometry —
    /// no geometric spot-checking is needed (pin 5, primary path, not the
    /// sanctioned coordinate-sequence fallback).
    #[test]
    fn every_bundled_glyph_round_trips_byte_for_byte() {
        assert_eq!(
            BRAVURA_OUTLINES.len(),
            37,
            "sanity: the bundled glyph count moved"
        );
        for o in BRAVURA_OUTLINES {
            let parsed = parse_d(o.path);
            let reemitted = emit_d(&parsed);
            assert_eq!(
                reemitted, o.path,
                "{}: parse -> emit did not round-trip byte-for-byte",
                o.name
            );
        }
    }

    /// (g2) `Close` survives parsing: every bundled glyph ends with `Z`, and
    /// the parsed command list must carry a trailing `PathCommand::Close`,
    /// not merely reproduce the byte (which g1 already proves) — this
    /// exercises the *typed* value directly, independent of re-emission.
    #[test]
    fn close_survives_parsing_as_a_typed_command() {
        for o in BRAVURA_OUTLINES {
            let parsed = parse_d(o.path);
            assert!(
                matches!(parsed.last(), Some(PathCommand::Close)),
                "{}: parsed commands must end with Close",
                o.name
            );
            // Every bundled path has at least one closed subpath, so Close
            // must appear at least once, not just coincidentally last.
            assert!(
                parsed.iter().any(|c| matches!(c, PathCommand::Close)),
                "{}: parsed commands must contain a Close",
                o.name
            );
        }
    }

    /// (g6) absolute, not relative — a hand-verified case, not a
    /// re-derivation through the parser under test. `augmentationDot`'s `d`
    /// (transcribed below, independently of [`BRAVURA_OUTLINES`]) is four
    /// cubic curves tracing a circle back to its own start point `(0.4, 0)`.
    /// Read by eye, every command's numbers *are* the absolute point they
    /// land on. Were the parser instead accumulating each command's numbers
    /// onto the current point (SVG's lowercase/relative convention), the
    /// first curve's endpoint would still land right (an all-positive
    /// glyph's first hop can't distinguish the two rules), but the second
    /// curve's endpoint would land at `(0.2, 0.2) + (0, 0) = (0.2, 0.2)`
    /// instead of the correct `(0, 0)`, and every command after that would
    /// drift further from a straightforward accumulation of offsets. This
    /// pins the correct (absolute) reading explicitly, command by command.
    #[test]
    fn parsed_coordinates_are_absolute_not_relative() {
        let d = "M0.4 0C0.4 0.112 0.312 0.2 0.2 0.2C0.088 0.2 0 0.112 0 0\
                  C0 -0.112 0.088 -0.2 0.2 -0.2C0.312 -0.2 0.4 -0.112 0.4 0Z";
        // Sanity: this is really `augmentationDot`'s bundled `d`, not a
        // stand-in string that happens to look similar.
        let augmentation_dot = BRAVURA_OUTLINES
            .iter()
            .find(|o| o.name == "augmentationDot")
            .expect("augmentationDot is bundled");
        assert_eq!(augmentation_dot.path, d);

        let expected = vec![
            PathCommand::MoveTo(Point::new(0.4, 0.0)),
            PathCommand::CurveTo {
                control1: Point::new(0.4, 0.112),
                control2: Point::new(0.312, 0.2),
                to: Point::new(0.2, 0.2),
            },
            PathCommand::CurveTo {
                control1: Point::new(0.088, 0.2),
                control2: Point::new(0.0, 0.112),
                to: Point::new(0.0, 0.0),
            },
            PathCommand::CurveTo {
                control1: Point::new(0.0, -0.112),
                control2: Point::new(0.088, -0.2),
                to: Point::new(0.2, -0.2),
            },
            PathCommand::CurveTo {
                control1: Point::new(0.312, -0.2),
                control2: Point::new(0.4, -0.112),
                to: Point::new(0.4, 0.0),
            },
            PathCommand::Close,
        ];
        assert_eq!(parse_d(d), expected);
    }
}

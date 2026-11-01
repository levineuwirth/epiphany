//! The family's lines drawn between their anchors (X4d.1): pedal lines,
//! ottavas, text lines, brackets and glissandi.
//!
//! Each rule is MuseScore 4.7.5's default where Gould's *Behind Bars* agrees
//! or gives no figure, as resolve §2's reference rule asks (D99); the source
//! and the book's printed page are cited at each constant. Sizes are staff
//! spaces: a text at MuseScore's points over its 1.75 mm staff space (D79's
//! Q7), a line's width and dash as its style gives them, a dash's length and
//! gap MuseScore's multiples of the line's width.

use crate::constrained::LineDashes;
use crate::spatial::{BoundingBox, Point, Transform2D};
use crate::text::{symbol_run, ResolvedText, Symbol, SymbolPlace, Turn};
use crate::Provenance;

/// A pedal line (`Pedal`, `TLayout::layoutPedalSegment`): its sign's
/// baseline 2.5 under its staff (`pedalPosBelow`), the line's width
/// (`pedalLineWidth`), its hook's height (`pedalHookHeight`), its distance
/// from the ink over it (`pedalMinDistance`); the sign starts half a space
/// before its note (`Pedal::linePos`), and the line ends 0.75 before the
/// column its release falls in, or 1.25 before a note where the next pedal of
/// its staff starts (`Pedal::linePos`). Gould sets pedal marks under the
/// lowest staff, a solid line ending in an upward vertical at the release,
/// the release on the barline where the pedal is let go at a bar's end, and a
/// reminder at a new system (pp. 332 to 335).
pub(crate) const PEDAL_BELOW: f32 = 2.5;
pub(crate) const PEDAL_LINE_WIDTH: f32 = 0.11;
pub(crate) const PEDAL_HOOK: f32 = 1.2;
pub(crate) const PEDAL_MIN_DISTANCE: f32 = 0.7;
pub(crate) const PEDAL_SIGN_LEAD: f32 = 0.5;
pub(crate) const PEDAL_END_INSET: f32 = 0.75;
pub(crate) const PEDAL_END_INSET_RETAKEN: f32 = 1.25;
/// SMuFL's pedal sign and its brackets (`keyboardPedalPed`,
/// `keyboardPedalParensLeft`, `keyboardPedalParensRight`): MuseScore's begin
/// text and continuation, "(Ped.)" (`pedalText`, `pedalContinueText`).
pub(crate) const PEDAL_SIGN: char = '\u{E650}';
pub(crate) const PEDAL_PARENS: (char, char) = ('\u{E676}', '\u{E677}');

/// An ottava (`Ottava`, `TLayout::layoutOttavaSegment`): above, its numeral's
/// top 2.0 over its staff (`ottavaPosAbove`, `ottavaTextAlignAbove`, the line
/// hanging from it); below, the numeral's baseline 2.0 under its staff
/// (`ottavaPosBelow`, `ottavaTextAlignBelow`, the line on it); the line's
/// width and dashes (`ottavaLineWidth`, `ottavaDashLineLen` and
/// `ottavaDashGapLen`, 6 widths each); its hook toward the staff
/// (`ottavaHookAbove`, `ottavaHookBelow`); its distance from the ink under it
/// (`ottavaMinDistance`); its end a space past its last note's ink, never
/// past the next note (`Ottava::linePos`). Gould: the numeral alone or with
/// its "va", the dotted line from the 8's top above the staff and its base
/// below, a corner just after the last note's head and dots, the numeral
/// again at a new system (pp. 28 to 30); MuseScore draws the numeral alone
/// (`ottavaNumbersOnly`), as LilyPond 2.26 does (`ottavation-numbers`).
pub(crate) const OTTAVA_POS: f32 = 2.0;
pub(crate) const OTTAVA_LINE_WIDTH: f32 = 0.11;
pub(crate) const OTTAVA_DASHES: LineDashes = LineDashes {
    dash: 6.0 * OTTAVA_LINE_WIDTH,
    gap: 6.0 * OTTAVA_LINE_WIDTH,
};
pub(crate) const OTTAVA_HOOK: f32 = 1.0;
pub(crate) const OTTAVA_MIN_DISTANCE: f32 = 0.7;
pub(crate) const OTTAVA_END_PAST: f32 = 1.0;

/// A text line (`TextLine`, `TLayout::layoutTextLineSegment`): above, the
/// line 1.0 over its staff (`textLinePosAbove`) through its words' middle
/// (`textLineTextAlign`), Edwin at 10 points; its width, dashes (4 widths
/// each, `textLineDashLineLen`, `textLineDashGapLen`) and a bracket's hook
/// (`textLineHookHeight`); its distance from the ink under it
/// (`textLineMinDistance`); its end a space before the column its end falls
/// in (`SLine::linePos`).
pub(crate) const TEXT_LINE_ABOVE: f32 = 1.0;
pub(crate) const TEXT_LINE_WIDTH: f32 = 0.15;
pub(crate) const TEXT_LINE_DASHES: LineDashes = LineDashes {
    dash: 4.0 * TEXT_LINE_WIDTH,
    gap: 4.0 * TEXT_LINE_WIDTH,
};
pub(crate) const TEXT_LINE_HOOK: f32 = 1.5;
pub(crate) const TEXT_LINE_MIN_DISTANCE: f32 = 0.7;
pub(crate) const TEXT_LINE_END_INSET: f32 = 1.0;
pub(crate) const TEXT_LINE_SIZE: f32 = 2.016;

/// A verbal crescendo or diminuendo, MuseScore's hairpin of the line type
/// (`Hairpin`, `HairpinType::CRESC_LINE`): its words italic at 10 points
/// (`hairpinFontStyle`, `hairpinFontSize`) on the line's baseline
/// (`hairpinTextAlign`), 2.0 under its staff or 1.0 over it
/// (`hairpinLinePosBelow`, `hairpinLinePosAbove`), the line the hairpin's
/// width (`hairpinLineWidth`) in dashes of 6 widths and gaps of 9
/// (`hairpinLineDashLineLen`, `hairpinLineDashGapLen`), "(cresc.)" or
/// "(dim.)" at a new system (`hairpinCrescContText`, `hairpinDecrescContText`).
/// Gould: lower-case italic (p. 101), widely spaced dashes, a reminder in
/// brackets at a new system (p. 106).
pub(crate) const VERBAL_HAIRPIN_BELOW: f32 = 2.0;
pub(crate) const VERBAL_HAIRPIN_ABOVE: f32 = 1.0;
pub(crate) const VERBAL_HAIRPIN_WIDTH: f32 = 0.12;
pub(crate) const VERBAL_HAIRPIN_DASHES: LineDashes = LineDashes {
    dash: 6.0 * VERBAL_HAIRPIN_WIDTH,
    gap: 9.0 * VERBAL_HAIRPIN_WIDTH,
};

/// The gap between a line's words or sign and the line
/// (`TextLineBase`'s `gapBetweenTextAndLine`).
pub(crate) const LINE_TEXT_GAP: f32 = 0.5;

/// A glissando (`Glissando`, `TLayout::layoutNoteAnchoredLine`,
/// `TDraw::draw(GlissandoSegment)`): from its first head's centre to its
/// last's, each end clipped a quarter space clear of its chord's ink at its
/// head (the end's at the half of the head the line comes from), a quarter
/// space up or down at each end between heads on one line; its width
/// (`glissandoLineWidth`); its words, italic at 8 points
/// (`glissandoFontSize`), centred along it where they fit, their baseline
/// 0.1 over it, 0.4 over a wavy one. Gould: a thin straight line exactly
/// between the heads, short of an accidental, angled toward the slide between
/// heads of one letter, its words close to it (pp. 140 to 142).
pub(crate) const GLISSANDO_WIDTH: f32 = 0.15;
pub(crate) const GLISSANDO_CLEAR: f32 = 0.25;
/// How far past its head's middle a glissando's last end reads its chord's
/// ink, toward the half it comes from (`ledgerLineWidth`).
pub(crate) const GLISSANDO_LEDGER: f32 = 0.16;
pub(crate) const GLISSANDO_SAME_LINE: f32 = 0.25;
pub(crate) const GLISSANDO_TEXT_SIZE: f32 = 1.613;
pub(crate) const GLISSANDO_TEXT_RAISE: f32 = 0.1;
pub(crate) const GLISSANDO_WAVY_TEXT_RAISE: f32 = 0.4;
pub(crate) const GLISSANDO_WORDS: &str = "gliss.";

/// The ottava's numeral for a shift of `octaves` (`ottava`, `quindicesima`,
/// `ventiduesima`), or `None` for one MuseScore names no numeral for.
pub(crate) fn ottava_numeral(octaves: i8) -> Option<char> {
    match octaves.unsigned_abs() {
        1 => Some('\u{E510}'),
        2 => Some('\u{E514}'),
        3 => Some('\u{E517}'),
        _ => None,
    }
}

/// Whether a text line's words begin with a dynamic term, and which: `true`
/// for a crescendo, `false` for a diminuendo. Such a line is a verbal
/// hairpin, below by the hairpin rule (D79's Q11).
pub(crate) fn dynamic_term(words: &str) -> Option<bool> {
    let word = words
        .trim_start()
        .trim_start_matches('(')
        .trim_start()
        .to_lowercase();
    if word.starts_with("cresc") {
        Some(true)
    } else if word.starts_with("dim") || word.starts_with("decresc") {
        Some(false)
    } else {
        None
    }
}

/// SMuFL symbols set side by side at their music size from `origin`, each
/// at the pen its predecessors' advances leave, as MuseScore sets a line's
/// `<sym>` text at its music font's size (`pedalMusicalSymbolsScale`,
/// `ottavaMusicalSymbolsScale`, 1.0): Bravura Text draws the pedal sign and
/// the ottava numerals larger than Bravura, the same outlines, so each is
/// scaled to Bravura's box ([`sign_scale`]).
pub(crate) fn symbols(provenance: Provenance, codepoints: &[char], origin: Point) -> ResolvedText {
    // Set at Bravura Text's size, then each glyph scaled about its origin by
    // its transform: a run sets one size per segment, and "(Ped.)" mixes the
    // brackets, at Bravura's size there, with the scaled sign.
    let mut pen = 0.0f32;
    let placed: Vec<Symbol> = codepoints
        .iter()
        .map(|&codepoint| {
            let symbol = Symbol {
                codepoint,
                at: SymbolPlace::Origin(Point::new(pen, 0.0)),
                scale: 1.0,
                turn: Turn::None,
            };
            pen += sign_advance(codepoint);
            symbol
        })
        .collect();
    let mut run = symbol_run(provenance, &placed, origin);
    let face = epiphany_text::Face::BravuraText;
    let mut ink: Option<[f32; 4]> = None;
    let mut index = 0;
    for segment in &mut run.segments {
        let unit = segment.size.0 / f32::from(face.units_per_em());
        for glyph in &mut segment.glyphs {
            let Some(&codepoint) = codepoints.get(index) else {
                break;
            };
            index += 1;
            let k = sign_scale(codepoint);
            if k != 1.0 {
                glyph.transform = Some(Transform2D {
                    matrix: [[k, 0.0, 0.0], [0.0, k, 0.0], [0.0, 0.0, 1.0]],
                });
            }
            let Some([l, b, r, t]) = face.glyph_bounds(glyph.glyph_id) else {
                continue;
            };
            let (x, y) = (glyph.offset.x.0, glyph.offset.y.0);
            let placed = [
                x + l * unit * k,
                y + b * unit * k,
                x + r * unit * k,
                y + t * unit * k,
            ];
            ink = Some(match ink {
                None => placed,
                Some(i) => [
                    i[0].min(placed[0]),
                    i[1].min(placed[1]),
                    i[2].max(placed[2]),
                    i[3].max(placed[3]),
                ],
            });
        }
    }
    let [l, b, r, t] = ink.unwrap_or([0.0; 4]);
    run.bounds = BoundingBox::new(l, b, r, t);
    run.reserved_box = run.bounds;
    run
}

/// The advances of the line signs, in staff spaces (Bravura 1.392's
/// `glyphAdvanceWidths`): the pedal sign, its brackets, the ottava numerals,
/// the glissando's wiggle.
pub(crate) fn sign_advance(codepoint: char) -> f32 {
    match codepoint {
        '\u{E650}' => 4.076,
        '\u{E676}' => 0.66,
        '\u{E677}' => 0.668,
        '\u{E510}' => 1.544,
        '\u{E514}' => 2.668,
        '\u{E517}' => 3.0,
        '\u{EAAF}' => 0.96,
        _ => 0.0,
    }
}

/// How far Bravura Text's glyph for a line sign is scaled to stand at
/// Bravura's size: its pedal sign is drawn 1.6254 times Bravura's and its
/// ottava numerals 2.0628 times, measured on the pinned faces against
/// Bravura's boxes (`the_line_signs_are_the_symbols_they_name`).
pub(crate) fn sign_scale(codepoint: char) -> f32 {
    match codepoint {
        '\u{E650}' => 1.0 / 1.6254,
        '\u{E510}' | '\u{E514}' | '\u{E517}' => 1.0 / 2.0628,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{MarkerId, TypedObjectId};

    fn prov() -> Provenance {
        Provenance::projected(TypedObjectId::Marker(MarkerId::from_raw(1)), vec![])
    }

    /// Each sign the lines draw is the SMuFL symbol it names, at Bravura's
    /// size: its ink, set from Bravura Text, is Bravura's box for that name
    /// (`bravura_metadata.json` at the pinned commit).
    #[test]
    fn the_line_signs_are_the_symbols_they_name() {
        for (codepoint, (l, b, r, t)) in [
            (PEDAL_SIGN, (0.0, -0.032, 4.076, 2.22)),
            (PEDAL_PARENS.0, (-0.008, -0.136, 0.66, 2.172)),
            (PEDAL_PARENS.1, (0.0, -0.136, 0.668, 2.172)),
            ('\u{E510}', (0.0, -0.04, 1.544, 1.852)),
            ('\u{E514}', (0.0, -0.04, 2.668, 1.844)),
            ('\u{E517}', (0.016, -0.028, 3.0, 1.852)),
            (
                crate::constrained::GLISSANDO_WIGGLE,
                (-0.1, 0.0, 1.124, 0.444),
            ),
        ] {
            let run = symbols(prov(), &[codepoint], Point::new(0.0, 0.0));
            let got = (
                run.bounds.left.0,
                run.bounds.bottom.0,
                run.bounds.right.0,
                run.bounds.top.0,
            );
            for (g, w) in [(got.0, l), (got.1, b), (got.2, r), (got.3, t)] {
                assert!((g - w).abs() < 0.02, "U+{:04X}: {got:?}", codepoint as u32);
            }
        }
    }

    /// "(Ped.)" sets each of its symbols at Bravura's size, the sign scaled
    /// beside its brackets at theirs: its ink runs from the left bracket's
    /// left to the right bracket's right, no taller than a bracket.
    #[test]
    fn a_continued_pedal_sets_each_symbol_at_its_size() {
        let run = symbols(
            prov(),
            &[PEDAL_PARENS.0, PEDAL_SIGN, PEDAL_PARENS.1],
            Point::new(0.0, 0.0),
        );
        let right = 0.66 + 4.076 + 0.668;
        assert!(
            (run.bounds.left.0 - -0.008).abs() < 0.02,
            "{:?}",
            run.bounds
        );
        assert!(
            (run.bounds.right.0 - right).abs() < 0.02,
            "{:?}",
            run.bounds
        );
        assert!((run.bounds.top.0 - 2.22).abs() < 0.02, "{:?}", run.bounds);
    }

    #[test]
    fn a_dynamic_term_opens_a_verbal_hairpin() {
        assert_eq!(dynamic_term("cresc."), Some(true));
        assert_eq!(dynamic_term("(cresc. poco a poco)"), Some(true));
        assert_eq!(dynamic_term("dim."), Some(false));
        assert_eq!(dynamic_term("Decresc."), Some(false));
        assert_eq!(dynamic_term("rit."), None);
        assert_eq!(dynamic_term("poco cresc."), None);
    }
}

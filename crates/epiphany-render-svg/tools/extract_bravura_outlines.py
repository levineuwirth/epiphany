#!/usr/bin/env python3
"""Extract genuine Bravura SMuFL glyph outlines into a Rust table.

Reproducible generator for `epiphany-render-svg`'s bundled outline data. It
fetches the official OFL `Bravura.otf` and the SMuFL `glyphnames.json`, then
emits `src/outlines_generated.rs` with each glyph's outline as an SVG path in
**staff-space**, **y-up** coordinates (the renderer's coordinate system).

Usage:
    python3 -m venv .venv && . .venv/bin/activate && pip install fonttools
    python3 extract_bravura_outlines.py > ../crates/epiphany-render-svg/src/outlines_generated.rs

The font is NOT vendored; only the generated Rust is committed. Bravura is
© Steinberg Media Technologies GmbH under the SIL Open Font License 1.1; the
extracted outlines are redistributed under the same license (see OFL.txt).
"""
import hashlib, json, math, re, sys, urllib.request

# Pinned, immutable sources. A moving branch (`master` / `gh-pages`) would make
# regeneration non-reproducible: a future font update would silently change the
# outlines. Both sources are pinned to a commit SHA and their bytes verified
# against a recorded SHA-256, so a substituted or updated source is rejected
# rather than quietly accepted. To deliberately move to a newer font, bump the
# ref AND the checksum together (a reviewable change), then regenerate.
FONT_TAG = "1.392"  # steinbergmedia/bravura tag bravura-1.392
FONT_REF = "301087ca0b0d30b65d81bc3e718ff64b613e2a9a"
NAMES_REF = "31a327a29640c313b12076739987bd7f25bdddde"  # w3c/smufl gh-pages
FONT_URL = f"https://raw.githubusercontent.com/steinbergmedia/bravura/{FONT_REF}/redist/otf/Bravura.otf"
NAMES_URL = f"https://raw.githubusercontent.com/w3c/smufl/{NAMES_REF}/metadata/glyphnames.json"
FONT_SHA256 = "dca2d90c88437a701b1c2e71fa54e76f9fa41d7deee935d74dc871ea66ecfdd2"
NAMES_SHA256 = "1d05352599a20983d1c901635dc75d76f063c0987a7bee65f145325fc3e0d29f"

# Exactly the glyph set the v0 layout pipeline can name (layout-ir BRAVURA_METRICS).
NAMES = ["noteheadBlack","noteheadHalf","noteheadWhole","noteheadDoubleWhole",
"gClef","fClef","cClef","accidentalSharp","accidentalFlat","accidentalNatural",
"accidentalDoubleSharp","restWhole","restHalf","restQuarter","rest8th",
"flag8thUp","flag8thDown","augmentationDot",
"timeSig0","timeSig1","timeSig2","timeSig3","timeSig4","timeSig5","timeSig6",
"timeSig7","timeSig8","timeSig9","timeSigCommon",
"barlineSingle","barlineFinal","dynamicForte","dynamicPiano"]

def verify(data, expected, what):
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        sys.exit(f"{what} SHA-256 mismatch:\n  expected {expected}\n  actual   {actual}\n"
                 "the pinned source changed; refusing to regenerate against an unverified font "
                 "(bump FONT_REF/NAMES_REF and the checksum deliberately if this is intended)")
    return data

def load():
    from fontTools.ttLib import TTFont
    import io
    if "--local" in sys.argv:
        font_bytes = open("Bravura.otf", "rb").read()
        names_bytes = open("glyphnames.json", "rb").read()
    else:
        font_bytes = urllib.request.urlopen(FONT_URL).read()
        names_bytes = urllib.request.urlopen(NAMES_URL).read()
    verify(font_bytes, FONT_SHA256, "Bravura.otf")
    verify(names_bytes, NAMES_SHA256, "glyphnames.json")
    font = TTFont(io.BytesIO(font_bytes))
    names = json.loads(names_bytes)
    return font, names

def round_d(d, nd=4):
    def r(m):
        v = round(float(m.group(0)), nd)
        s = f"{v:.{nd}f}".rstrip('0').rstrip('.')
        return "0" if s in ("", "-0") else s
    return re.sub(r'-?\d+\.?\d*(?:e-?\d+)?', r, d)

def main():
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.pens.boundsPen import BoundsPen
    font, glyphnames = load()
    upm = font["head"].unitsPerEm
    sp = upm / 4.0           # font units per staff space (SMuFL em = 4 staff spaces)
    scale = 1.0 / sp
    gs = font.getGlyphSet()
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    rows = []
    metrics = []  # (name, advance, [l,b,r,t]) in 1/1024-staff-space integer units
    for name in NAMES:
        cp = int(glyphnames[name]["codepoint"].replace("U+", ""), 16)
        g = cmap.get(cp)
        if g is None:
            print(f"// MISSING {name} U+{cp:04X}", file=sys.stderr); continue
        pen = SVGPathPen(gs)
        gs[g].draw(TransformPen(pen, (scale, 0, 0, scale, 0, 0)))
        d = round_d(pen.getCommands())
        bp = BoundsPen(gs); gs[g].draw(bp)
        l, b, r, t = ([round(v * scale, 4) for v in bp.bounds] if bp.bounds else [0, 0, 0, 0])
        rows.append((name, cp, d, (l, b, r, t)))
        # Companion metrics for layout-ir `BRAVURA_METRICS` (1/1024 staff space):
        # the advance (from hmtx), and a bbox rounded *outward* from the outline's
        # bbox — floor the mins, ceil the maxes — so the integer metric box always
        # *contains* the drawn outline. The engraver evaluates collisions from the
        # metric box; an inward (nearest) round could leave it a hair smaller than
        # the ink, making a no-collision result microscopically false on paper.
        adv1024 = round(hmtx[g][0] * scale * 1024)
        bbox1024 = [math.floor(l * 1024), math.floor(b * 1024),
                    math.ceil(r * 1024), math.ceil(t * 1024)]
        metrics.append((name, adv1024, bbox1024))
    rows.sort()
    print("// --- BRAVURA_METRICS rows (advance, [l,b,r,t] in 1/1024 staff space) ---",
          file=sys.stderr)
    for name, adv1024, bbox1024 in sorted(metrics):
        print(f'    GlyphMetric::new("{name}", {adv1024}, {bbox1024}),', file=sys.stderr)
    o = []
    o.append("//! GENERATED by `tools/extract_bravura_outlines.py` — do not edit by hand.")
    o.append("//!")
    o.append("//! Real Bravura SMuFL glyph outlines, extracted from the official OFL")
    o.append(f"//! `Bravura.otf` (unitsPerEm = {upm}; 1 staff space = {sp:g} font units, since the")
    o.append("//! SMuFL em is 4 staff spaces). Path `d` data is in **staff-space** units, **y-up**")
    o.append("//! (musical convention, positive y = higher pitch), relative to each glyph's")
    o.append("//! origin, rounded to 4 decimals. The renderer applies a single y-flip wrapper.")
    o.append("//!")
    o.append(f"//! Source (pinned + SHA-256 verified on extraction): Bravura {FONT_TAG},")
    o.append(f"//! steinbergmedia/bravura @ {FONT_REF}")
    o.append(f"//! (sha256 {FONT_SHA256});")
    o.append(f"//! glyph names from w3c/smufl gh-pages @ {NAMES_REF}")
    o.append(f"//! (sha256 {NAMES_SHA256}).")
    o.append("//!")
    o.append("//! Bravura is (c) Steinberg Media Technologies GmbH under the SIL Open Font")
    o.append("//! License 1.1; these extracted outlines are redistributed under the same license")
    o.append("//! (see `tools/OFL.txt`).")
    o.append("")
    o.append("/// One bundled glyph outline: SMuFL name, codepoint, the SVG path `d` in")
    o.append("/// staff-space / y-up coordinates, and the outline's tight bounding box")
    o.append("/// `[left, bottom, right, top]` in staff spaces.")
    o.append("pub(crate) struct BravuraOutline {")
    o.append("    pub name: &'static str,")
    o.append("    pub codepoint: u32,")
    o.append("    pub path: &'static str,")
    o.append("    pub bbox: [f32; 4],")
    o.append("}")
    o.append("")
    o.append("/// Every glyph the v0 layout pipeline can name (the `BRAVURA_METRICS` set), with")
    o.append("/// its genuine Bravura outline. Sorted by name for deterministic binary search.")
    o.append("pub(crate) const BRAVURA_OUTLINES: &[BravuraOutline] = &[")
    for name, cp, d, (l, b, r, t) in rows:
        o.append("    BravuraOutline {")
        o.append(f"        name: {json.dumps(name)},")
        o.append(f"        codepoint: 0x{cp:04X},")
        o.append(f"        path: {json.dumps(d)},")
        o.append(f"        bbox: [{l}, {b}, {r}, {t}],")
        o.append("    },")
    o.append("];")
    sys.stdout.write("\n".join(o) + "\n")
    print(f"// extracted {len(rows)}/{len(NAMES)} glyphs", file=sys.stderr)

if __name__ == "__main__":
    main()

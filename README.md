# Epiphany

Epiphany is a free and open-source music-notation platform built on a
specified score model. The model is a typed score graph edited only through
semantic operations, which a canonical, deterministic reduction merges, so
replicas that have seen the same operations converge on the same score
byte for byte. Documents persist in a crash-safe, content-addressed bundle
format. An engraver turns the score into laid-out pages and SVG, and a
headless editor core with a small native demo drives the whole path from a
keystroke to a redrawn page.

The specification in `spec/` is the normative source: `core_spec.tex` and
five companions (binary format, operation catalog, quality metrics, reference
suite, text projection), built to the PDFs beside them. The rules that no
change may break are in [`docs/invariants.md`](docs/invariants.md).

## Layout

| crate | role |
|---|---|
| `epiphany-determinism` | canonical hashing, floats, iteration order and tolerances |
| `epiphany-core` | the score graph, pitch and time, and the graph invariants |
| `epiphany-ops` | operations, causal context and the canonical reduction |
| `epiphany-bundle` | the `.musc` bundle file format and its crash recovery |
| `epiphany-textproj` | the canonical text projection of a bundle |
| `epiphany-layout-ir` | the layout intermediate representation and solver interface |
| `epiphany-engrave` | the engraving solver |
| `epiphany-glyphs` | Bravura SMuFL glyph outlines |
| `epiphany-render-svg` | SVG rendering of a laid-out score |
| `epiphany-editor-core` | the headless editor: selection, hit testing, edits, undo |
| `epiphany-editor-gui` | a demo editor window over the editor core |
| `epiphany-testkit` | conformance, round-trip, convergence and budget harnesses |

`spikes/` is a separate Cargo workspace holding the editor-toolkit spike; it is
not part of the product build.

## Building and checking

Epiphany is a Cargo workspace. CI pins its toolchains; install them with
`rustup` (the gate names the exact versions and says which one is missing).
The demo editor on Linux also needs the X11 and keyboard development
libraries CI installs: `libxcb-render0-dev libxcb-shape0-dev
libxcb-xfixes0-dev libxkbcommon-dev libssl-dev` or their equivalents.

    scripts/gate                  # fmt, clippy, rustdoc, build, tests, diff check,
                                  # requirement labels removed from origin/main
    scripts/gate --full           # plus the rest of what CI runs
    scripts/gate --spec           # plus the xelatex builds of the specification,
                                  # and the labels TeX no longer defines
    scripts/gate --print-plan     # list the stages

The gate's closing line reports the test counts. To see the engraver's output
and the demo editor:

    cargo run -p epiphany-render-svg --example render_fixture -- ten_measure_single_staff --solver=real > out.svg
    cargo run -p epiphany-editor-gui

The specification builds with `xelatex`, not `pdflatex`:
`cd spec && latexmk -xelatex -interaction=nonstopmode core_spec.tex`.

## Status

The substrate is built and tested: the score graph, the operation catalog,
canonical reduction with two-replica convergence, the bundle format with its
crash-recovery sweep, the text projection and the determinism layer. The
engraver is early. It draws clefs, noteheads, stems, ledger lines and
barlines and breaks systems, but draws each barline at the start of its
measure instead of the end, and does not yet draw flags, dots, beams, ties, tuplets, articulations or
dynamics. The demo editor edits a built-in fixture and cannot yet open or
save a file.
The work now under way is to import MusicXML exported from real scores and
engrave it, so that every capability is judged against music someone wrote.

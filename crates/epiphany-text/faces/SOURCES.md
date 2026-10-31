# Where these faces come from

Every face here is the upstream file, byte for byte. None is subset,
renamed or otherwise modified, so each keeps its own name under the SIL Open
Font License 1.1 (its Reserved Font Name included), and its licence and
FONTLOG travel beside it. `src/lib.rs` embeds the files with
`include_bytes!`, and `tests/faces.rs` locks each by its BLAKE3 hash, so a
face that changes is a deliberate, reviewed change.

| file | face | upstream | SHA-256 |
|---|---|---|---|
| `Edwin-Roman.otf` | Edwin Roman 0.54 | MuseScore `v4.7.5` (`3654226c2e99289916916953a98e585a3d3b315a`), `fonts/edwin/Edwin-Roman.otf` | `edf9bddcdf7a62d0d799151d863556cf5e0958ad0a01e9f1d46afbaaa5a36e1c` |
| `Edwin-Italic.otf` | Edwin Italic 0.54 | the same, `fonts/edwin/Edwin-Italic.otf` | `07fb34383b545f162603513798d11fd27382a562d512535ac652a32ffbc8692f` |
| `Edwin-Bold.otf` | Edwin Bold 0.54 | the same, `fonts/edwin/Edwin-Bold.otf` | `3cb6aebf02120af63c4b49cce8e653fb77e584961425a181485b77c6ad38d81c` |
| `Edwin-BdIta.otf` | Edwin Bold Italic 0.54 | the same, `fonts/edwin/Edwin-BdIta.otf` | `611c3974acb6d23f60e1214b543475436da9849c44cd839a04b75b19c042d0f1` |
| `BravuraText.otf` | Bravura Text 1.392 | steinbergmedia/bravura `301087ca0b0d30b65d81bc3e718ff64b613e2a9a`, `redist/otf/BravuraText.otf` | `decf5d2955e190cbdb6de6e906d266652aa340ed010ecb0d3e003a29228ac582` |

Bravura Text comes from the same Bravura commit the repository's music
glyphs are extracted from (`epiphany-glyphs/tools/extract_bravura_outlines.py`).

Licences: Edwin is © 2021 MuseScore Limited, with Reserved Font Name
"Edwin" (`Edwin-OFL.txt`, `Edwin-FONTLOG.txt`, `Edwin-README.md`, from the
same MuseScore directory). Bravura Text is © 2019 Steinberg Media
Technologies GmbH, with Reserved Font Name "Bravura" (`Bravura-OFL.txt`, the
repository's `LICENSE.txt`, and `Bravura-FONTLOG.txt`, its
`redist/FONTLOG.txt`). Both are under the SIL Open Font License, Version 1.1.

To refresh a face, fetch it from a named upstream commit, record the commit
and the file's SHA-256 here, and update the hash in `tests/faces.rs` in the
same change.

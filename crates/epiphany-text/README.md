# epiphany-text

The text faces the engraver sets words in, and the shaper that sets them.
This is disposition E of `spec/ANALYSIS_TEXT_RUN_PRIMITIVES.md`, as the core
specification states it in `req:layoutir:text-run` and
`req:layoutir:text-faces`.

- **Faces.** Edwin in four faces (MuseScore's text face) and Bravura Text,
  unmodified files under `faces/` with their licences and FONTLOGs, embedded
  with `include_bytes!`. Each is identified by BLAKE3 over its bytes under
  the `MUSCFNTF` tag, and `tests/faces.rs` locks every hash. `faces/SOURCES.md`
  names the upstream commit and SHA-256 of each file.
- **Shaping.** `shape` sets a string in a chain of faces: each character in
  the first face that covers it, each run in one face shaped by harfrust
  (pinned `=0.14.0`) at the face's own units, left to right, with the face's
  default features. A character no face covers is returned unresolved and
  drawn from no face. The result is integer font units; the engraver scales
  it to staff spaces.
- **Outlines.** `Face::outline` and `Face::glyph_bounds` read a glyph from the
  same bytes through skrifa (pinned `=0.48.0`), so an export draws exactly the
  glyphs the layout shaped.

No face is ever looked up on the host. Selectable text in an exported PDF,
and faces carried inside a document's bundle, are later questions.

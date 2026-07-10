//! Regenerates `spec/vectors/decode_vectors.txt`, the cross-implementation
//! decode conformance corpus. Run from the workspace root.
fn main() {
    let text = epiphany_testkit::vectors::render();
    let path = epiphany_testkit::vectors::PATH;
    std::fs::write(path, &text).unwrap_or_else(|e| panic!("writing {path}: {e}"));
    let rows = epiphany_testkit::vectors::parse(&text).expect("parses");
    eprintln!("wrote {} vectors to {path}", rows.len());
}

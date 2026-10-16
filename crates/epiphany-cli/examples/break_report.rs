//! Compares where a MusicXML file breaks its systems and pages with where the
//! engraver opens them on the import.
//!
//!     cargo run --release -p epiphany-cli --example break_report -- <file.musicxml>...
//!
//! For each file: the measures (by index from 1) the file breaks the line or
//! the page before, the measures each engraved system opens with, and every
//! difference: a break of the file's the layout does not make, and a system
//! the layout opens that the file does not.

use std::collections::BTreeSet;
use std::path::Path;

use epiphany_cli::{engrave_loaded, load};
use epiphany_musicxml::source::SourceBreak;

fn main() {
    for path in std::env::args().skip(1) {
        let loaded = match load(Path::new(&path)) {
            Ok(loaded) => loaded,
            Err(e) => {
                println!("{path}: {e}");
                continue;
            }
        };
        let source = &loaded.import.source;
        let file_systems: BTreeSet<usize> = source
            .measures
            .iter()
            .enumerate()
            .filter(|(_, m)| m.break_before.is_some())
            .map(|(i, _)| i + 1)
            .collect();
        let file_pages: BTreeSet<usize> = source
            .measures
            .iter()
            .enumerate()
            .filter(|(_, m)| m.break_before == Some(SourceBreak::Page))
            .map(|(i, _)| i + 1)
            .collect();
        let engraved = engrave_loaded(&loaded);
        // Measure ids of the first staff, by index.
        let ids = &loaded.import.ids.measures[0][0];
        let mut systems = BTreeSet::new();
        let mut pages = BTreeSet::new();
        for page in &engraved.layout.pages {
            let mut first_on_page = true;
            for system in &page.systems {
                let Some(index) = system
                    .measures
                    .iter()
                    .filter_map(|m| ids.iter().position(|id| *id == m.measure))
                    .min()
                else {
                    continue;
                };
                if index > 0 {
                    systems.insert(index + 1);
                    if first_on_page {
                        pages.insert(index + 1);
                    }
                }
                first_on_page = false;
            }
        }
        let missed: Vec<_> = file_systems.difference(&systems).collect();
        let added: Vec<_> = systems.difference(&file_systems).collect();
        let pages_missed: Vec<_> = file_pages.difference(&pages).collect();
        let pages_added: Vec<_> = pages.difference(&file_pages).collect();
        println!(
            "{path}: {} measures; the file breaks before {} measures ({} pages), the layout \
             before {} ({} pages); file breaks not made {:?}; systems added {:?}; page breaks \
             not made {:?}; pages added {:?}",
            source.measures.len(),
            file_systems.len(),
            file_pages.len(),
            systems.len(),
            pages.len(),
            missed,
            added,
            pages_missed,
            pages_added
        );
    }
}

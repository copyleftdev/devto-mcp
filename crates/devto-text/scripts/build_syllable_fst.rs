//! Build the syllable FST from the CMUdict reduction. Run once; the output is committed.
//!
//!   cargo run -p devto-text --features build-data --bin build-syllable-fst
//!
//! An FST rather than a HashMap because the map is consulted per word over whole
//! articles: this one is memory-mapped, shares prefixes across 126k entries, and needs no
//! parse at startup.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/data");
    let input = BufReader::new(File::open(format!("{dir}/cmudict_syllables.tsv"))?);

    // fst::MapBuilder requires keys in lexicographic byte order.
    let mut pairs: Vec<(String, u64)> = Vec::new();
    for line in input.lines() {
        let line = line?;
        let Some((word, count)) = line.split_once('\t') else {
            continue;
        };
        pairs.push((word.to_string(), count.trim().parse()?));
    }
    pairs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    pairs.dedup_by(|a, b| a.0 == b.0);

    let out = BufWriter::new(File::create(format!("{dir}/cmudict_syllables.fst"))?);
    let mut builder = fst::MapBuilder::new(out)?;
    for (word, count) in &pairs {
        builder.insert(word.as_bytes(), *count)?;
    }
    builder.finish()?;

    println!("{} entries written", pairs.len());
    Ok(())
}

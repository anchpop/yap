//! Verify every course's `language_data.hash` against the actual split
//! archives: both recorded XXH3 hashes and byte sizes must match the files on
//! disk. This is the integrity gate `scripts/commit-regenerated-packs.sh`
//! runs before committing regenerated packs — sizes alone can't catch an
//! interrupted regeneration that left mixed halves or a stale hash file, and
//! committing that would strand clients in a redownload loop (the frontend
//! verifies downloads against the committed hash file).
//!
//!   cargo run --release --example verify_pack_hashes

use language_utils::language_pack::{CORE_FILENAME, SENTENCES_FILENAME};
use xxhash_rust::xxh3::xxh3_64;

fn main() {
    let mut failed = false;
    let mut checked = 0usize;

    let mut dirs: Vec<_> = std::fs::read_dir("out")
        .expect("run from the repo root (no out/ directory found)")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().contains("_for_"))
                && p.join("language_data.hash").exists()
        })
        .collect();
    dirs.sort();
    assert!(
        !dirs.is_empty(),
        "no course directories with language_data.hash"
    );

    for dir in dirs {
        let pair = dir.file_name().unwrap().to_string_lossy().into_owned();
        let metadata = std::fs::read_to_string(dir.join("language_data.hash")).unwrap();
        let mut lines = metadata.trim().lines();

        for filename in [CORE_FILENAME, SENTENCES_FILENAME] {
            let Some(line) = lines.next() else {
                println!("{pair}: language_data.hash is missing the line for {filename}");
                failed = true;
                continue;
            };
            let (recorded_hash, recorded_size) = line.trim().split_once(';').unwrap();
            let bytes = match std::fs::read(dir.join(filename)) {
                Ok(bytes) => bytes,
                Err(e) => {
                    println!("{pair}: cannot read {filename}: {e}");
                    failed = true;
                    continue;
                }
            };
            if recorded_size != bytes.len().to_string() {
                println!(
                    "{pair}: {filename} is {} bytes, hash file records {recorded_size}",
                    bytes.len()
                );
                failed = true;
            } else if recorded_hash != xxh3_64(&bytes).to_string() {
                println!("{pair}: {filename} content does not match its recorded hash");
                failed = true;
            } else {
                checked += 1;
            }
        }
        if lines.next().is_some() {
            println!("{pair}: language_data.hash has more than two lines");
            failed = true;
        }
    }

    assert!(!failed, "pack hash verification failed (see above)");
    println!("verified {checked} archives against their recorded hashes");
}

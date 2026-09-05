//! Shared osmo cache at `.cache`. Optional R2 mirroring uses `YAP_CACHE_BUCKET`
//! and `R2_ACCOUNT_ID` / `R2_ACCESS_KEY_ID` / `R2_SECRET_ACCESS_KEY`.
//! `warm` pulls remote segments before importing and deleting pre-osmo cache files.

use std::path::{Path, PathBuf};

use osmo::{Bucket, Store};

const CACHE_DIR: &str = ".cache";
// v1 used the bucket root for per-file mirroring.
const BUCKET_PREFIX: &str = "v2";

pub fn store() -> Store {
    Store::open(CACHE_DIR)
}

fn bucket() -> Option<Bucket> {
    let name = std::env::var("YAP_CACHE_BUCKET")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Bucket::with_prefix(name, BUCKET_PREFIX))
}

pub fn enabled() -> bool {
    bucket().is_some()
}

/// Pull before migration so remote keys are not imported into duplicate segments.
/// Sync failures are logged; the local cache remains usable.
pub async fn warm() {
    if let Some(bucket) = bucket() {
        println!(
            "cache: warming {CACHE_DIR} from bucket '{}'…",
            bucket.bucket
        );
        match store().pull(&bucket).await {
            Ok(stats) if stats.downloaded > 0 => {
                println!("cache: downloaded {} segment(s)", stats.downloaded)
            }
            Ok(_) => println!("cache: already in sync with bucket"),
            Err(e) => eprintln!("cache: pull failed: {e}"),
        }
    }
    if let Err(e) = migrate_legacy_layouts(&store(), Path::new(CACHE_DIR)).await {
        eprintln!("cache: legacy migration failed: {e}");
    }
}

/// Upload any new cache entries to the bucket. Call this at the end of a run.
pub async fn flush() {
    let Some(bucket) = bucket() else { return };
    match store().push(&bucket).await {
        Ok(stats) if stats.uploaded > 0 => {
            println!("cache: uploaded {} segment(s) to bucket", stats.uploaded)
        }
        Ok(_) => println!("cache: already in sync with bucket"),
        Err(e) => eprintln!("cache: flush failed: {e}"),
    }
}

/// Fold every legacy cache layout under `root` into the store. Each step is a cheap
/// no-op once its source is gone, so this is safe (and fast) to run every warm.
async fn migrate_legacy_layouts(store: &Store, root: &Path) -> anyhow::Result<()> {
    // tysm `NNN.kv` shards at the cache root, and per-model shard dirs that
    // subtitle-corpus used to keep under subtitle-ocr/ (its requests hash the model, so
    // the entries coexist fine under one namespace).
    let mut imported = tysm::cache::migrate_legacy(root, store).await?;
    let ocr_root = root.join("subtitle-ocr");
    if ocr_root.is_dir() {
        for entry in std::fs::read_dir(&ocr_root)? {
            let dir = entry?.path();
            if dir.is_dir() {
                imported += tysm::cache::migrate_legacy(&dir, store).await?;
                let _ = std::fs::remove_dir(&dir);
            }
        }
        let _ = std::fs::remove_dir(&ocr_root);
    }

    imported += migrate_google_translate(store, &root.join("google_translate")).await?;

    // Wiktionary HTML: `wiktionary/<lang>/<word>.html` files → `wiktionary/{lang}/{word}`.
    let wiktionary = root.join("wiktionary");
    if wiktionary.is_dir() {
        for entry in std::fs::read_dir(&wiktionary)? {
            let lang_dir = entry?.path();
            let Some(lang) = file_name(&lang_dir) else {
                continue;
            };
            if lang_dir.is_dir() {
                let prefix = format!("wiktionary/{lang}");
                imported += import_dir_files(store, &lang_dir, &prefix, "html").await?;
                let _ = std::fs::remove_dir(&lang_dir);
            }
        }
        let _ = std::fs::remove_dir(&wiktionary);
    }

    // Google TTS: the live cache (mis-nested under wav2vec2/ by the old path derivation)
    // first so it wins key collisions, then the long-orphaned top-level dir.
    imported += import_dir_files(
        store,
        &root.join("wav2vec2").join("google-tts"),
        "google-tts",
        "json",
    )
    .await?;
    imported += import_dir_files(store, &root.join("google-tts"), "google-tts", "json").await?;
    let _ = std::fs::remove_dir_all(root.join("google-tts"));

    // wav2vec2: `wav2vec2/<version>/<hash>.json` → `wav2vec2/{version}/{hash}`.
    let wav2vec2 = root.join("wav2vec2");
    if wav2vec2.is_dir() {
        for entry in std::fs::read_dir(&wav2vec2)? {
            let version_dir = entry?.path();
            let Some(version) = file_name(&version_dir) else {
                continue;
            };
            if version_dir.is_dir() {
                let prefix = format!("wav2vec2/{version}");
                imported += import_dir_files(store, &version_dir, &prefix, "json").await?;
                let _ = std::fs::remove_dir(&version_dir);
            }
        }
        let _ = std::fs::remove_dir(&wav2vec2);
    }

    // Leftover control file from the old directory-mirroring osmo.
    let _ = std::fs::remove_file(root.join(".osmo.json"));

    if imported > 0 {
        println!("cache: migrated {imported} legacy cache entr(y/ies) into the store");
    }
    Ok(())
}

/// Import the old Google Translate cache: `master_cache.json` (a `{hash: translation}`
/// map) plus any per-entry `{hash}.json` crash files, all under `translate/{hash}`.
async fn migrate_google_translate(store: &Store, dir: &Path) -> anyhow::Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    // Per-entry crash files first: they were written after the master, and `import`
    // keeps the first occurrence of a key.
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|e| e == "json")
            && stem.parse::<u64>().is_ok()
            && let Ok(translation) = std::fs::read(&path)
        {
            entries.push((format!("translate/{stem}"), translation));
        }
    }
    let master = dir.join("master_cache.json");
    if let Ok(contents) = std::fs::read_to_string(&master) {
        let map: std::collections::HashMap<u64, String> =
            serde_json::from_str(&contents).unwrap_or_default();
        entries.extend(
            map.into_iter()
                .map(|(hash, t)| (format!("translate/{hash}"), t.into_bytes())),
        );
    }
    if entries.is_empty() {
        // Nothing parsed; still drop an empty leftover dir.
        let _ = std::fs::remove_dir(dir);
        return Ok(0);
    }
    let imported = store.import(entries).await?;
    std::fs::remove_dir_all(dir)?;
    Ok(imported)
}

/// Import every file in `dir` (non-recursively) as `{key_prefix}/{stem-or-name}`,
/// deleting the files afterwards. The given extension is removed from
/// the key (`abandon.html` → `abandon`); files with other extensions keep their full name.
async fn import_dir_files(
    store: &Store,
    dir: &Path,
    key_prefix: &str,
    extension: &str,
) -> anyhow::Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = file_name(&path) else {
            continue;
        };
        let key_part = if path.extension().is_some_and(|e| e == extension) {
            name.trim_end_matches(&format!(".{extension}")).to_string()
        } else {
            name
        };
        files.push((format!("{key_prefix}/{key_part}"), path));
    }
    if files.is_empty() {
        return Ok(0);
    }
    let imported = store
        .import(
            files
                .iter()
                .filter_map(|(key, path)| Some((key.clone(), std::fs::read(path).ok()?))),
        )
        .await?;
    for (_, path) in files {
        let _ = std::fs::remove_file(path);
    }
    Ok(imported)
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1_record(key: &str, val: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(key.len() as u32).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(val.len() as u32).to_le_bytes());
        out.extend_from_slice(val);
        out
    }

    /// One synthetic `.cache` with every legacy layout, migrated end-to-end.
    #[tokio::test]
    async fn migrates_every_legacy_layout() {
        let cache_dir = tempfile::tempdir().unwrap();
        let root = cache_dir.path();

        // tysm shard at the root + a subtitle-ocr per-model shard dir.
        std::fs::write(root.join("042.kv"), v1_record("llmkey", b"llm response")).unwrap();
        let ocr_dir = root.join("subtitle-ocr").join("gpt-5.6-luna");
        std::fs::create_dir_all(&ocr_dir).unwrap();
        std::fs::write(ocr_dir.join("117.kv"), v1_record("ocrkey", b"ocr response")).unwrap();

        // Google translate: master map + a newer per-entry crash file for the same hash.
        let gt = root.join("google_translate");
        std::fs::create_dir_all(&gt).unwrap();
        std::fs::write(
            gt.join("master_cache.json"),
            r#"{"12345":"stale translation","67890":"only in master"}"#,
        )
        .unwrap();
        std::fs::write(gt.join("12345.json"), "fresh translation").unwrap();

        // Wiktionary HTML per language (apostrophe in the word, like hors-d'œuvre).
        let wik = root.join("wiktionary").join("french");
        std::fs::create_dir_all(&wik).unwrap();
        std::fs::write(wik.join("hors-d'œuvre.html"), "<html>starter</html>").unwrap();

        // google-tts: live entries mis-nested under wav2vec2/, plus the orphaned
        // top-level dir holding a stale value for the same hash.
        let tts_live = root.join("wav2vec2").join("google-tts");
        std::fs::create_dir_all(&tts_live).unwrap();
        std::fs::write(tts_live.join("00ff.json"), r#"{"live":true}"#).unwrap();
        let tts_orphan = root.join("google-tts");
        std::fs::create_dir_all(&tts_orphan).unwrap();
        std::fs::write(tts_orphan.join("00ff.json"), r#"{"live":false}"#).unwrap();
        std::fs::write(tts_orphan.join("aa11.json"), r#"{"orphan_only":true}"#).unwrap();

        // wav2vec2 prediction partitioned by version.
        let w2v = root.join("wav2vec2").join("model@abc__greedy_v1");
        std::fs::create_dir_all(&w2v).unwrap();
        std::fs::write(w2v.join("deadbeef.json"), r#"{"raw_phonemes":[]}"#).unwrap();

        std::fs::write(root.join(".osmo.json"), "{}").unwrap();

        let store = Store::open(root);
        migrate_legacy_layouts(&store, root).await.unwrap();

        let read = |key: &str| {
            let store = store.clone();
            let key = key.to_string();
            async move {
                store
                    .read(&key)
                    .await
                    .map(|b| String::from_utf8(b).unwrap())
            }
        };
        assert_eq!(read("tysm/llmkey").await.as_deref(), Some("llm response"));
        assert_eq!(read("tysm/ocrkey").await.as_deref(), Some("ocr response"));
        assert_eq!(
            read("translate/12345").await.as_deref(),
            Some("fresh translation"),
            "crash file must beat the master map"
        );
        assert_eq!(
            read("translate/67890").await.as_deref(),
            Some("only in master")
        );
        assert_eq!(
            read("wiktionary/french/hors-d'œuvre").await.as_deref(),
            Some("<html>starter</html>")
        );
        assert_eq!(
            read("google-tts/00ff").await.as_deref(),
            Some(r#"{"live":true}"#),
            "live tts cache must beat the orphaned dir"
        );
        assert_eq!(
            read("google-tts/aa11").await.as_deref(),
            Some(r#"{"orphan_only":true}"#)
        );
        assert_eq!(
            read("wav2vec2/model@abc__greedy_v1/deadbeef")
                .await
                .as_deref(),
            Some(r#"{"raw_phonemes":[]}"#)
        );

        // Legacy sources are gone; only the store remains.
        for gone in [
            "042.kv",
            "subtitle-ocr",
            "google_translate",
            "wiktionary",
            "google-tts",
            ".osmo.json",
        ] {
            assert!(!root.join(gone).exists(), "{gone} should be deleted");
        }
        assert!(
            !root.join("wav2vec2").exists(),
            "wav2vec2 dir empty after migration and removed"
        );

        // Running it again is a no-op.
        migrate_legacy_layouts(&store, root).await.unwrap();
        assert_eq!(read("tysm/llmkey").await.as_deref(), Some("llm response"));
    }
}

//! Optional mirroring of the local `.cache` directory to a Cloudflare R2 bucket via the
//! [`osmo`] sync library.
//!
//! tysm now stores its response cache as `NNN.kv` append-only record-log shards; osmo's
//! `records` strategy understands that exact format and merges shards losslessly across
//! machines. The Google Translate cache (a single JSON map) rides along via `json_merge`.
//! Together this lets a fresh machine / CI warm the (multi-GB) cache from the bucket
//! instead of recomputing LLM responses, translations, etc.
//!
//! Enabled only when `YAP_CACHE_BUCKET` is set (plus R2 credentials in the environment:
//! `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`). Otherwise every function
//! here is a no-op and behaviour is unchanged.

use std::path::Path;

use osmo::Bucket;

const CACHE_DIR: &str = ".cache";

/// osmo strategy overrides written to `.cache/.osmo.json`:
/// - tysm's `NNN.kv` shard logs are append-only key→value records (union by key);
/// - the Google Translate cache is one mutable JSON map (union by top-level key).
///
/// Everything else (TTS, wav2vec2, wiktionary) is immutable/content-addressed — the
/// default `path` strategy, so no rule needed.
const OSMO_SETTINGS: &str = r#"{"files":[{"path":"*.kv","strategy":"records"},{"path":"google_translate/master_cache.json","strategy":"json_merge"}]}"#;

fn bucket() -> Option<Bucket> {
    let name = std::env::var("YAP_CACHE_BUCKET")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Bucket::new(name))
}

/// Whether cache mirroring is configured for this run.
pub fn enabled() -> bool {
    bucket().is_some()
}

/// Ensure `.cache/.osmo.json` declares the per-file sync strategies.
async fn ensure_settings() {
    let path = Path::new(CACHE_DIR).join(".osmo.json");
    // Always (re)write: cheap, and keeps the rules current if we change them.
    if let Err(e) = tokio::fs::create_dir_all(CACHE_DIR).await {
        eprintln!("osmo: could not create {CACHE_DIR}: {e}");
        return;
    }
    if let Err(e) = tokio::fs::write(&path, OSMO_SETTINGS).await {
        eprintln!("osmo: could not write {}: {e}", path.display());
    }
}

/// Warm the local cache from the bucket (best-effort, once per process). Call this *before*
/// anything reads the cache (LLM clients, the Google Translate cache, etc.).
pub async fn warm() {
    let Some(bucket) = bucket() else { return };
    // Fold any legacy per-entry cache files (`dir/NNN/<key>`) into the `NNN.kv` shard logs
    // first, so we mirror ~1000 objects rather than millions.
    if let Err(e) = tysm::cache::migrate(Path::new(CACHE_DIR)).await {
        eprintln!("osmo: cache migration failed: {e}");
    }
    ensure_settings().await;
    println!("osmo: warming {CACHE_DIR} from bucket '{}'…", bucket.bucket);
    osmo::ensure_pulled(Path::new(CACHE_DIR), &bucket).await;
}

/// Upload any new cache entries to the bucket. Call this at the end of a run.
pub async fn flush() {
    let Some(bucket) = bucket() else { return };
    match osmo::flush(Path::new(CACHE_DIR), &bucket).await {
        Ok(stats) if stats.skipped => println!("osmo: already in sync with bucket"),
        Ok(stats) => println!("osmo: uploaded {} object(s) to bucket", stats.uploaded),
        Err(e) => eprintln!("osmo: flush failed: {e}"),
    }
}

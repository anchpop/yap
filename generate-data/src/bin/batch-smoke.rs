//! Smoke test for OpenAI's Batch API through tysm: three throwaway prompts
//! forced onto the batch path (no live fallback), printing every status poll
//! and the answers. Run from the repo root so `.env` and `.cache` resolve.
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct Answer {
    /// The number, spelled out in English.
    spelled: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let model = std::env::args().nth(1).unwrap_or_else(|| "gpt-5.6-luna".to_string());
    let client = tysm::chat_completions::ChatClient::from_env(&model)?
        .with_cache_directory("./.cache")
        .with_small_batch_threshold(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let prompts: Vec<(u64, String)> = (1..=3)
        .map(|i| (i, format!("Spell out the number {} in English words. (run {nonce})", i * 7)))
        .collect();
    let started = std::time::Instant::now();
    let results = client
        .batch_chat_with_system_prompt_fn::<_, _, Answer>(
            "Answer with the requested field only.",
            &prompts,
            |(_, p)| p.clone(),
            |batch| {
                println!(
                    "  [{:>6.0}s] batch {} status={:?} total={} completed={} failed={}",
                    started.elapsed().as_secs_f64(),
                    batch.id,
                    batch.status,
                    batch.request_counts.total,
                    batch.request_counts.completed,
                    batch.request_counts.failed
                );
            },
        )
        .await?;
    for ((n, _), r) in results {
        match r {
            Ok(a) => println!("{n}: {}", a.spelled),
            Err(e) => println!("{n}: ERROR {e:#}"),
        }
    }
    println!("done in {:.0}s", started.elapsed().as_secs_f64());
    Ok(())
}

use language_utils::language_pack::LanguagePack;
use language_utils::{Atom, Literal, SentenceGrams, SpurGram};
use lasso::Spur;
use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _, persistent};

/// Match learnable grams to literal statuses, using each literal at most once.
#[allow(clippy::type_complexity)]
pub fn match_grams_to_literals<'a, T>(
    encoded_sentence: &SentenceGrams<SpurGram>,
    literals: &'a [(Literal<Spur>, T)],
    language_pack: &LanguagePack,
) -> Vec<(SpurGram, Vec<(Literal<Spur>, &'a T)>)> {
    let mut used_literals = vec![false; literals.len()];
    let mut results = Vec::new();

    for sentence_gram in &encoded_sentence.grams {
        let Some(gram_spur) = sentence_gram.learnable().copied() else {
            continue;
        };

        let gram = language_pack.gram_rodeo.resolve(&gram_spur);

        let mut matched_statuses = Vec::new();

        for atom in gram.iter() {
            let Atom::Tok(word) = atom else {
                continue;
            };
            let word_spur = word.text;

            for (i, (literal, status)) in literals.iter().enumerate() {
                if !used_literals[i] && literal.word.text == word_spur {
                    used_literals[i] = true;
                    matched_statuses.push((*literal, status));
                    break;
                }
            }
        }

        results.push((gram_spur, matched_statuses));
    }

    results
}

/// Logs elapsed time on drop.
pub struct PerfTimer {
    label: String,
    start_time: f64,
}

impl PerfTimer {
    pub fn new(label: impl Into<String>) -> Self {
        let start_time = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);

        Self {
            label: label.into(),
            start_time,
        }
    }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window()
            && let Some(performance) = window.performance()
        {
            let duration = performance.now() - self.start_time;
            log::info!("[PERF] {}: {:.2}ms", self.label, duration);
        }
    }
}

/// The user's current local timezone offset from UTC, as a `chrono::FixedOffset`.
///
/// On wasm, this reads the browser's timezone via `js_sys::Date::getTimezoneOffset`, which
/// returns minutes that are positive when local time is *behind* UTC. Off-wasm (tests, native
/// tooling) it falls back to UTC.
pub fn current_local_offset() -> chrono::FixedOffset {
    #[cfg(target_arch = "wasm32")]
    let offset_seconds = {
        // getTimezoneOffset() is minutes and positive when local is behind UTC.
        let offset_minutes = js_sys::Date::new_0().get_timezone_offset();
        (-offset_minutes * 60.0) as i32
    };
    #[cfg(not(target_arch = "wasm32"))]
    let offset_seconds = 0;

    chrono::FixedOffset::east_opt(offset_seconds)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).expect("UTC offset is always valid"))
}

pub(crate) async fn get_or_create_device_id(
    weapon_dir: &persistent::DirectoryHandle,
    user_id: &Option<String>,
) -> Result<String, persistent::Error> {
    let file_name = if user_id.is_some() {
        "device-id"
    } else {
        "device-id-logged-out"
    };

    let device_id_file = weapon_dir
        .get_file_handle_with_options(file_name, &opfs::GetFileHandleOptions { create: false })
        .await;

    match device_id_file {
        Ok(file_handle) => {
            let bytes = file_handle.read().await?;
            let device_id = String::from_utf8(bytes).unwrap_or_else(|_| {
                log::error!("Device ID file contained invalid UTF-8 data");
                eyedee::generate_uuid()
            });
            Ok(device_id)
        }
        Err(_) => {
            let device_id = eyedee::generate_uuid();

            let mut file_handle = weapon_dir
                .get_file_handle_with_options(
                    file_name,
                    &opfs::GetFileHandleOptions { create: true },
                )
                .await?;

            let mut writable = file_handle
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await?;

            writable.write_at_cursor_pos(device_id.as_bytes()).await?;

            writable.close().await?;

            Ok(device_id)
        }
    }
}

/// Base URL of the yap AI backend. The `local-backend` feature wins (local
/// dev), then the `YAP_AI_BACKEND_URL` compile-time env var (beta/staging
/// builds pointing at a non-production backend), then production.
pub fn ai_server_url() -> &'static str {
    if cfg!(feature = "local-backend") {
        "http://localhost:21516"
    } else {
        option_env!("YAP_AI_BACKEND_URL").unwrap_or("https://yap-ai-backend.fly.dev")
    }
}

pub async fn hit_ai_server(
    method: fetch_happen::Method,
    path: &str,
    request: Option<impl serde::Serialize>,
    access_token: Option<&String>,
) -> Result<fetch_happen::Response, fetch_happen::Error> {
    let client = fetch_happen::Client;
    let url = ai_server_url();
    // Always include an Authorization header - use "anonymous" as dummy token when not logged in
    let token = access_token.map(|t| t.as_str()).unwrap_or("anonymous");

    let full_url = format!("{url}{path}");

    let mut req = match method {
        fetch_happen::Method::GET => client.get(&full_url),
        fetch_happen::Method::POST => client.post(&full_url),
        fetch_happen::Method::PATCH => client.patch(&full_url),
        fetch_happen::Method::PUT => client.put(&full_url),
        fetch_happen::Method::DELETE => client.delete(&full_url),
        _ => panic!("Unsupported HTTP method"),
    };

    req = req.header("Authorization", format!("Bearer {token}"));

    if let Some(body) = request {
        req = req.json(&body)?;
    }

    req.send().await
}

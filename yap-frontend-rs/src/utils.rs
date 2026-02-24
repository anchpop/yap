use language_utils::language_pack::LanguagePack;
use language_utils::{Atom, Literal, SentenceGrams, SpurGram};
use lasso::Spur;
use opfs::{DirectoryHandle as _, FileHandle as _, WritableFileStream as _, persistent};

/// Matches sentence grams to literals, returning each learnable gram
/// with references to the matched literal statuses.
///
/// The function handles "used literal" tracking internally - each literal
/// is matched to at most one atom across all grams.
///
/// # Arguments
/// * `sentence_spur` - The interned sentence to look up
/// * `literals` - Slice of (text_spur, status) pairs for each literal
/// * `language_pack` - The language pack containing encoded sentences and gram data
///
/// # Returns
/// A vector of (gram, matched_statuses) pairs for each learnable gram in the sentence.
/// `matched_statuses` contains references to the status values for literals that matched
/// atoms in the gram.
pub fn match_grams_to_literals<'a, T>(
    encoded_sentence: &SentenceGrams<SpurGram>,
    literals: &'a [(Literal<Spur>, T)],
    language_pack: &LanguagePack,
) -> Vec<(SpurGram, Vec<(Literal<Spur>, &'a T)>)> {
    // Track which literals have been used
    let mut used_literals = vec![false; literals.len()];
    let mut results = Vec::new();

    for sentence_gram in &encoded_sentence.grams {
        // Only process learnable grams
        let Some(gram_spur) = sentence_gram.learnable().copied() else {
            continue;
        };

        let gram = language_pack.gram_rodeo.resolve(&gram_spur);

        let mut matched_statuses = Vec::new();

        for atom in gram.iter() {
            let Atom::Tok(word) = atom else {
                continue; // Skip control tokens
            };
            let word_spur = word.text;

            // Find next unused literal matching this word's Spur
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

/// Performance instrumentation helper that measures time from creation to drop.
/// Logs the duration to the console when dropped.
pub struct PerfTimer {
    label: String,
    start_time: f64,
}

impl PerfTimer {
    /// Create a new performance timer with the given label.
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

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
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
            // Read existing device ID
            let bytes = file_handle.read().await?;
            let device_id = String::from_utf8(bytes).unwrap_or_else(|_| {
                log::error!("Device ID file contained invalid UTF-8 data");
                eyedee::get_uuid()
            });
            Ok(device_id)
        }
        Err(_) => {
            // Generate new device ID
            let device_id = eyedee::get_uuid();

            // Save it to OPFS
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

            writable
                .write_at_cursor_pos(device_id.as_bytes().to_vec())
                .await?;

            writable.close().await?;

            Ok(device_id)
        }
    }
}

pub async fn hit_ai_server(
    method: fetch_happen::Method,
    path: &str,
    request: Option<impl serde::Serialize>,
    access_token: Option<&String>,
) -> Result<fetch_happen::Response, fetch_happen::Error> {
    let client = fetch_happen::Client;
    let url = if cfg!(feature = "local-backend") {
        "http://localhost:8080"
    } else {
        "https://yap-ai-backend.fly.dev"
    };
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

    let response = req.send().await?;
    Ok(response)
}

use futures::StreamExt as _;
use language_utils::{
    Course, Language,
    language_pack::{ArchivedLanguagePack, LanguagePack},
};
use opfs::{
    DirectoryHandle as _, FileHandle as _, WritableFileStream as _,
    persistent::{self, DirectoryHandle},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;

use crate::utils;

static LANGUAGE_DATA_HASHES: LazyLock<BTreeMap<Course, &'static str>> = LazyLock::new(|| {
    let mut hashes = BTreeMap::new();
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::French,
        },
        include_str!("../../out/fra_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::French,
            target_language: Language::English,
        },
        include_str!("../../out/eng_for_fra/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Spanish,
        },
        include_str!("../../out/spa_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Korean,
        },
        include_str!("../../out/kor_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::German,
        },
        include_str!("../../out/deu_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Italian,
        },
        include_str!("../../out/ita_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Portuguese,
        },
        include_str!("../../out/por_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Russian,
        },
        include_str!("../../out/rus_for_eng/language_data.hash"),
    );
    hashes
});

/// Parses hash metadata from format "hash;size_in_bytes" and returns (hash, size)
fn parse_hash_metadata(metadata: &str) -> (u64, usize) {
    let metadata = metadata.trim();
    let (hash_str, size_str) = metadata.split_once(';').unwrap();
    let hash = hash_str.parse().unwrap();
    let size = size_str.parse().unwrap();
    (hash, size)
}

fn language_data_hash_for_course(course: Course) -> Option<&'static str> {
    LANGUAGE_DATA_HASHES.get(&course).copied()
}

fn course_directory_slug(course: Course) -> String {
    format!(
        "{}_for_{}",
        course.target_language.iso_639_3(),
        course.native_language.iso_639_3()
    )
}

const LANGUAGE_DATA_WRITE_CHUNK_SIZE: usize = 1024 * 1024;
const LANGUAGE_DATA_WRITE_ATTEMPTS: usize = 3;
const LANGUAGE_DATA_TARGET_CHUNK_COUNT: usize = 5;

#[derive(serde::Serialize)]
struct LanguageDataRequest {
    course: Course,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_size: Option<usize>,
}

fn language_data_chunk_size(total_size: usize) -> usize {
    total_size.max(1).div_ceil(LANGUAGE_DATA_TARGET_CHUNK_COUNT)
}

fn language_data_chunk_count(total_size: usize) -> usize {
    total_size
        .max(1)
        .div_ceil(language_data_chunk_size(total_size))
}

fn language_data_chunk_filename(hash: u64, total_size: usize, chunk_index: usize) -> String {
    let chunk_len = language_data_chunk_len(total_size, chunk_index);
    format!(
        "language_data_{hash}.chunk_{chunk_index:02}_of_{chunk_count:02}.size_{chunk_len}",
        chunk_count = language_data_chunk_count(total_size)
    )
}

fn language_data_chunk_filenames(hash: u64, total_size: usize) -> Vec<String> {
    (0..language_data_chunk_count(total_size))
        .map(|chunk_index| language_data_chunk_filename(hash, total_size, chunk_index))
        .collect()
}

fn language_data_chunk_len(total_size: usize, chunk_index: usize) -> usize {
    let chunk_size = language_data_chunk_size(total_size);
    let chunk_start = chunk_index * chunk_size;
    total_size.saturating_sub(chunk_start).min(chunk_size)
}

fn current_language_data_filenames(hash: u64, total_size: usize) -> BTreeSet<String> {
    language_data_chunk_filenames(hash, total_size)
        .into_iter()
        .collect()
}

fn is_retryable_persistent_error(error: &persistent::Error) -> bool {
    let error_text = format!("{error:?}");
    error_text.contains("UnknownError")
        || error_text.contains("QuotaExceededError")
        || error_text.contains("out of memory")
}

pub(crate) async fn get_language_pack(
    data_directory_handle: &DirectoryHandle,
    course: Course,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<LanguagePack, LanguageDataError> {
    let _perf_timer = utils::PerfTimer::new("get_language_pack");
    let course_directory = course_directory_slug(course);
    let mut language_directory = data_directory_handle
        .get_directory_handle_with_options(
            &course_directory,
            &opfs::GetDirectoryHandleOptions { create: true },
        )
        .await
        .map_err(LanguageDataError::Persistent)?;

    let (language_data_hash, language_data_size) = parse_hash_metadata(
        language_data_hash_for_course(course)
            .ok_or(LanguageDataError::UnsupportedCourse(course))?,
    );

    log::info!(
        "expected language_data_hash for {:?}->{:?}: {language_data_hash}",
        course.native_language,
        course.target_language
    );
    {
        let mut files = language_directory
            .entries()
            .await
            .map_err(LanguageDataError::Persistent)?;
        while let Some(Ok((filename, _))) = files.next().await {
            log::info!("Found language data file: {filename}");
        }
    }

    let bytes = match read_cached_language_data(
        &mut language_directory,
        language_data_hash,
        language_data_size,
        set_loading_state,
    )
    .await?
    {
        Some(bytes) => {
            log::info!("Language data from chunked local storage hash matches expectation");
            bytes
        }
        None => {
            let _perf_timer = utils::PerfTimer::new("downloading and caching language data");
            log::info!(
                "Downloading and caching language data because the chunked language data cache was missing or invalid"
            );
            download_and_cache_language_data(
                &mut language_directory,
                course,
                language_data_hash,
                language_data_size,
                set_loading_state,
            )
            .await?
        }
    };

    set_loading_state("Deserializing language data", 100.0);
    let loading_perf_timer = utils::PerfTimer::new("Deserializing language data");

    let chunk_filenames = language_data_chunk_filenames(language_data_hash, language_data_size);

    let deserialize_result = rkyv::access::<ArchivedLanguagePack, rkyv::rancor::Error>(&bytes[..])
        .map_err(|e| format!("access: {e:?}"))
        .and_then(|archived| {
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
                .map_err(|e| format!("deserialize: {e:?}"))
        });

    let deserialized = match deserialize_result {
        Ok(d) => d,
        Err(e) => {
            log::error!("Error loading language data ({e}), removing cache and re-downloading");
            remove_language_data_files(&mut language_directory, &chunk_filenames).await;
            let bytes = download_and_cache_language_data(
                &mut language_directory,
                course,
                language_data_hash,
                language_data_size,
                set_loading_state,
            )
            .await?;
            let archived = rkyv::access::<ArchivedLanguagePack, rkyv::rancor::Error>(&bytes[..])
                .inspect_err(|e| log::error!("2nd error accessing language data: {e:?}"))
                .map_err(LanguageDataError::Rkyv)?;
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
                .inspect_err(|e| log::error!("2nd error deserializing language data: {e:?}"))
                .map_err(LanguageDataError::Rkyv)?
        }
    };

    drop(loading_perf_timer);

    Ok(deserialized)
}

#[derive(Debug, thiserror::Error)]
pub enum LanguageDataError {
    #[error("OPFS error: {0:?}")]
    Persistent(persistent::Error),

    #[error("Rkyv error")]
    Rkyv(#[source] rkyv::rancor::Error),

    #[error("AI server error:")]
    AiServer(#[source] fetch_happen::Error),

    #[error("Server returned HTTP {0}")]
    ServerError(u16),

    #[error("{0}")]
    InvalidData(String),

    #[error("Unsupported course: {0:?}")]
    UnsupportedCourse(Course),
}

impl From<LanguageDataError> for wasm_bindgen::JsValue {
    fn from(error: LanguageDataError) -> Self {
        match error {
            LanguageDataError::Persistent(error) => {
                wasm_bindgen::JsValue::from_str(&format!("OPFS error: {error:?}"))
            }
            LanguageDataError::Rkyv(error) => {
                wasm_bindgen::JsValue::from_str(&format!("Rkyv error: {error:?}"))
            }
            LanguageDataError::AiServer(error) => {
                let prefix = match &error {
                    fetch_happen::Error::JsError(_) => "Network error",
                    fetch_happen::Error::HttpError(_, _) => "AI server HTTP error",
                    fetch_happen::Error::JsonError(_) => "AI server JSON error",
                    fetch_happen::Error::Aborted => "AI server request aborted",
                };
                wasm_bindgen::JsValue::from_str(&format!("{prefix}: {error}"))
            }
            LanguageDataError::ServerError(status) => {
                wasm_bindgen::JsValue::from_str(&format!("Server returned HTTP {status}"))
            }
            LanguageDataError::InvalidData(message) => wasm_bindgen::JsValue::from_str(&message),
            LanguageDataError::UnsupportedCourse(course) => {
                wasm_bindgen::JsValue::from_str(&format!("Unsupported course: {course:?}"))
            }
        }
    }
}

async fn read_cached_language_data(
    language_directory_handle: &mut DirectoryHandle,
    expected_hash: u64,
    expected_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Option<Vec<u8>>, LanguageDataError> {
    let chunk_filenames = language_data_chunk_filenames(expected_hash, expected_size);

    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let file_handle = match language_directory_handle
            .get_file_handle_with_options(filename, &opfs::GetFileHandleOptions { create: false })
            .await
        {
            Ok(file_handle) => file_handle,
            Err(_) => return Ok(None),
        };

        let expected_chunk_len = language_data_chunk_len(expected_size, chunk_index);
        let actual_chunk_len = file_handle
            .size()
            .await
            .map_err(LanguageDataError::Persistent)? as usize;

        if actual_chunk_len != expected_chunk_len {
            log::warn!(
                "Chunk size mismatch for {filename}. Expected {expected_chunk_len} bytes, got {actual_chunk_len}. Re-downloading."
            );
            if let Err(error) = language_directory_handle.remove_entry(filename).await {
                log::warn!("Failed to remove invalid language data chunk {filename}: {error:?}");
            }
            return Ok(None);
        }
    }

    let mut bytes = Vec::with_capacity(expected_size);
    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let chunk_bytes = match get_cached_chunk_bytes(
            language_directory_handle,
            filename,
            language_data_chunk_len(expected_size, chunk_index),
        )
        .await
        {
            Ok(Some(chunk_bytes)) => chunk_bytes,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        };

        bytes.extend_from_slice(&chunk_bytes);
        let progress = (bytes.len() as f64 / expected_size.max(1) as f64) * 100.0;
        set_loading_state("Loading...", progress as f32);
    }

    let computed_hash = const_xxh3(&bytes);
    if computed_hash != expected_hash {
        log::warn!(
            "Chunked language data hash mismatch! Expected: {expected_hash}, Got: {computed_hash}. Removing cached chunks."
        );
        remove_language_data_files(language_directory_handle, &chunk_filenames).await;
        return Ok(None);
    }

    Ok(Some(bytes))
}

async fn download_and_cache_language_data(
    language_directory_handle: &mut DirectoryHandle,
    course: Course,
    expected_hash: u64,
    expected_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    let chunk_size = language_data_chunk_size(expected_size);
    let chunk_filenames = language_data_chunk_filenames(expected_hash, expected_size);
    let mut downloaded_bytes = 0usize;
    let mut bytes = Vec::with_capacity(expected_size);

    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let expected_chunk_len = language_data_chunk_len(expected_size, chunk_index);

        if let Some(chunk_bytes) =
            get_cached_chunk_bytes(language_directory_handle, filename, expected_chunk_len).await?
        {
            bytes.extend_from_slice(&chunk_bytes);
            downloaded_bytes += chunk_bytes.len();
            let progress = (downloaded_bytes as f64 / expected_size.max(1) as f64) * 100.0;
            set_loading_state(
                &format!("Downloading {:?} language data", course.target_language),
                progress as f32,
            );
            continue;
        }

        let chunk_bytes = download_language_data_chunk(
            course,
            chunk_index,
            chunk_size,
            expected_chunk_len,
            downloaded_bytes,
            expected_size,
            set_loading_state,
        )
        .await?;

        cache_language_data_bytes(language_directory_handle, filename, &chunk_bytes).await?;
        bytes.extend_from_slice(&chunk_bytes);
        downloaded_bytes += chunk_bytes.len();
    }

    set_loading_state("Verifying language data", 100.0);
    let computed_hash = const_xxh3(&bytes);
    if computed_hash != expected_hash {
        remove_language_data_files(language_directory_handle, &chunk_filenames).await;
        return Err(LanguageDataError::InvalidData(format!(
            "Downloaded language data hash mismatch. Expected {expected_hash}, got {computed_hash}"
        )));
    }

    remove_stale_language_data_files(
        language_directory_handle,
        &current_language_data_filenames(expected_hash, expected_size),
    )
    .await?;

    log::info!("Language data successfully loaded and cached in chunks!");
    Ok(bytes)
}

async fn download_language_data_chunk(
    course: Course,
    chunk_index: usize,
    chunk_size: usize,
    expected_chunk_len: usize,
    downloaded_before_chunk: usize,
    expected_total_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    let response = {
        let path: &str = "/language-data";
        let request = LanguageDataRequest {
            course,
            chunk_index: Some(chunk_index),
            chunk_size: Some(chunk_size),
        };
        async move {
            let client = fetch_happen::Client;
            let url = if cfg!(feature = "local-backend") {
                "http://localhost:21516"
            } else {
                "https://yap-ai-backend.fly.dev"
            };
            let full_url = format!("{url}{path}");
            client.post(&full_url).json(&request)?.send().await
        }
    }
    .await
    .map_err(LanguageDataError::AiServer)?;

    if !response.ok() {
        log::error!(
            "Server returned error while fetching chunk {}: {}",
            chunk_index,
            response.status()
        );
        return Err(LanguageDataError::ServerError(response.status()));
    }

    let reader = response
        .stream_reader()
        .map_err(LanguageDataError::AiServer)?;

    let mut chunk_bytes = Vec::with_capacity(expected_chunk_len);
    let mut last_logged_percent = downloaded_before_chunk * 100 / expected_total_size.max(1);

    loop {
        match reader.read_chunk().await {
            Ok(Some(chunk)) => {
                chunk_bytes.extend_from_slice(&chunk);
                let progress = ((downloaded_before_chunk + chunk_bytes.len()) as f64
                    / expected_total_size.max(1) as f64)
                    * 100.0;
                let progress_int = progress as usize;
                if progress_int > last_logged_percent {
                    set_loading_state(
                        &format!("Downloading {:?} language data", course.target_language),
                        progress as f32,
                    );
                    last_logged_percent = progress_int;
                }
            }
            Ok(None) => break,
            Err(error) => return Err(LanguageDataError::AiServer(error)),
        }
    }

    if chunk_bytes.len() != expected_chunk_len {
        return Err(LanguageDataError::InvalidData(format!(
            "Downloaded language data chunk {chunk_index} had {actual} bytes, expected {expected_chunk_len}",
            actual = chunk_bytes.len()
        )));
    }

    Ok(chunk_bytes)
}

async fn get_cached_chunk_bytes(
    language_directory_handle: &mut DirectoryHandle,
    filename: &str,
    expected_chunk_len: usize,
) -> Result<Option<Vec<u8>>, LanguageDataError> {
    let file_handle = match language_directory_handle
        .get_file_handle_with_options(filename, &opfs::GetFileHandleOptions { create: false })
        .await
    {
        Ok(file_handle) => file_handle,
        Err(_) => return Ok(None),
    };
    let chunk_bytes = file_handle
        .read()
        .await
        .map_err(LanguageDataError::Persistent)?;

    if chunk_bytes.len() == expected_chunk_len {
        return Ok(Some(chunk_bytes));
    }

    if let Err(error) = language_directory_handle.remove_entry(filename).await {
        log::warn!("Failed to remove invalid language data chunk {filename}: {error:?}");
    }

    log::warn!(
        "Removing invalid language data chunk {filename}: expected {expected_chunk_len} bytes, got {actual}",
        actual = chunk_bytes.len()
    );
    Ok(None)
}

async fn cache_language_data_bytes(
    language_directory_handle: &mut DirectoryHandle,
    filename: &str,
    bytes: &[u8],
) -> Result<(), LanguageDataError> {
    for attempt in 1..=LANGUAGE_DATA_WRITE_ATTEMPTS {
        let result: Result<(), persistent::Error> = async {
            let mut language_data_file = language_directory_handle
                .get_file_handle_with_options(
                    filename,
                    &opfs::GetFileHandleOptions { create: true },
                )
                .await?;
            let mut writable = language_data_file
                .create_writable_with_options(&opfs::CreateWritableOptions {
                    keep_existing_data: false,
                })
                .await?;

            for chunk in bytes.chunks(LANGUAGE_DATA_WRITE_CHUNK_SIZE) {
                writable.write_at_cursor_pos(chunk).await?;
            }

            writable.close().await?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt < LANGUAGE_DATA_WRITE_ATTEMPTS
                    && is_retryable_persistent_error(&error) =>
            {
                log::warn!(
                    "Retrying language data OPFS write after transient error on attempt {attempt}: {error:?}"
                );
                if let Err(cleanup_error) = language_directory_handle.remove_entry(filename).await {
                    log::warn!(
                        "Failed to remove partially written chunk {filename}: {cleanup_error:?}"
                    );
                }
            }
            Err(error) => return Err(LanguageDataError::Persistent(error)),
        }
    }

    Ok(())
}

async fn remove_stale_language_data_files(
    language_directory_handle: &mut DirectoryHandle,
    current_filenames: &BTreeSet<String>,
) -> Result<(), LanguageDataError> {
    let files_to_delete = stale_language_data_files(language_directory_handle, current_filenames)
        .await
        .map_err(LanguageDataError::Persistent)?;

    for filename in files_to_delete {
        log::info!("Removing old language data file: {filename}");
        if let Err(e) = language_directory_handle.remove_entry(&filename).await {
            log::warn!("Failed to remove old language data file {filename}: {e:?}");
        }
    }

    Ok(())
}

async fn remove_language_data_files(
    language_directory_handle: &mut DirectoryHandle,
    filenames: &[String],
) {
    for filename in filenames {
        if let Err(error) = language_directory_handle.remove_entry(filename).await {
            log::warn!("Failed to remove language data file {filename}: {error:?}");
        }
    }
}

async fn stale_language_data_files(
    language_directory_handle: &mut DirectoryHandle,
    current_filenames: &BTreeSet<String>,
) -> Result<Vec<String>, persistent::Error> {
    let mut entries = language_directory_handle.entries().await?;
    let mut files_to_delete = Vec::new();

    while let Some(Ok((filename, _))) = entries.next().await {
        if filename.starts_with("language_data_") && !current_filenames.contains(&filename) {
            files_to_delete.push(filename);
        }
    }

    Ok(files_to_delete)
}

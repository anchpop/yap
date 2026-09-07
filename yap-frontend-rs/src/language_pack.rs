use futures::StreamExt as _;
use language_utils::{
    Course, Language,
    language_pack::{
        ArchivedLanguagePackCore, ArchivedLanguagePackSentences, LanguagePack, LanguagePackCore,
        LanguagePackSentences,
    },
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
            native_language: Language::French,
            target_language: Language::Portuguese,
        },
        include_str!("../../out/por_for_fra/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Russian,
        },
        include_str!("../../out/rus_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Hindi,
        },
        include_str!("../../out/hin_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Thai,
        },
        include_str!("../../out/tha_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::ChineseSimplified,
        },
        include_str!("../../out/zho-hans_for_eng/language_data.hash"),
    );
    hashes.insert(
        Course {
            native_language: Language::English,
            target_language: Language::Japanese,
        },
        include_str!("../../out/jpn_for_eng/language_data.hash"),
    );
    hashes
});

/// Which half of the split language pack a request refers to. The core
/// (dictionary, frequencies, pronunciation) is a fraction of the size of the
/// sentence data and is downloaded first, so the placement test can start
/// before the sentences have arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackPart {
    Core,
    Sentences,
}

impl PackPart {
    fn slug(self) -> &'static str {
        match self {
            PackPart::Core => "core",
            PackPart::Sentences => "sentences",
        }
    }

    fn describe(self, course: Course) -> String {
        match self {
            PackPart::Core => format!("Downloading {:?} dictionary", course.target_language),
            PackPart::Sentences => format!("Downloading {:?} sentences", course.target_language),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PartMeta {
    hash: u64,
    size: usize,
}

/// Parses the two-line hash metadata file: line 1 is the core's
/// `hash;size_in_bytes`, line 2 the sentences'.
fn parse_hash_metadata(metadata: &str) -> (PartMeta, PartMeta) {
    let mut parts = metadata.trim().lines().map(|line| {
        let (hash_str, size_str) = line.trim().split_once(';').unwrap();
        PartMeta {
            hash: hash_str.parse().unwrap(),
            size: size_str.parse().unwrap(),
        }
    });
    let core = parts.next().unwrap();
    let sentences = parts.next().unwrap();
    (core, sentences)
}

fn language_data_hashes_for_course(
    course: Course,
) -> Result<(PartMeta, PartMeta), LanguageDataError> {
    LANGUAGE_DATA_HASHES
        .get(&course)
        .copied()
        .map(parse_hash_metadata)
        .ok_or(LanguageDataError::UnsupportedCourse(course))
}

fn course_directory_slug(course: Course) -> String {
    format!(
        "{}_for_{}",
        course.target_language.code(),
        course.native_language.code()
    )
}

const LANGUAGE_DATA_WRITE_CHUNK_SIZE: usize = 1024 * 1024;
const LANGUAGE_DATA_WRITE_ATTEMPTS: usize = 3;
/// Download/cache granularity. A fixed chunk size (instead of a fixed chunk
/// count) keeps resume granularity sane now that parts range from ~2 MB to
/// ~200 MB.
const LANGUAGE_DATA_CHUNK_SIZE: usize = 16 * 1024 * 1024;

#[derive(serde::Serialize)]
struct LanguageDataRequest {
    course: Course,
    part: PackPart,
    chunk_index: usize,
    chunk_size: usize,
}

fn language_data_chunk_count(total_size: usize) -> usize {
    total_size.max(1).div_ceil(LANGUAGE_DATA_CHUNK_SIZE)
}

fn language_data_chunk_filename(part: PackPart, meta: PartMeta, chunk_index: usize) -> String {
    let chunk_len = language_data_chunk_len(meta.size, chunk_index);
    format!(
        "language_data_{slug}_{hash}.chunk_{chunk_index:02}_of_{chunk_count:02}.size_{chunk_len}",
        slug = part.slug(),
        hash = meta.hash,
        chunk_count = language_data_chunk_count(meta.size)
    )
}

fn language_data_chunk_filenames(part: PackPart, meta: PartMeta) -> Vec<String> {
    (0..language_data_chunk_count(meta.size))
        .map(|chunk_index| language_data_chunk_filename(part, meta, chunk_index))
        .collect()
}

fn language_data_chunk_len(total_size: usize, chunk_index: usize) -> usize {
    let chunk_start = chunk_index * LANGUAGE_DATA_CHUNK_SIZE;
    total_size
        .saturating_sub(chunk_start)
        .min(LANGUAGE_DATA_CHUNK_SIZE)
}

/// Every filename both halves of the current pack version are allowed to
/// occupy; anything else with the `language_data_` prefix is stale.
fn current_language_data_filenames(core: PartMeta, sentences: PartMeta) -> BTreeSet<String> {
    language_data_chunk_filenames(PackPart::Core, core)
        .into_iter()
        .chain(language_data_chunk_filenames(
            PackPart::Sentences,
            sentences,
        ))
        .collect()
}

fn is_retryable_persistent_error(error: &persistent::Error) -> bool {
    let error_text = format!("{error:?}");
    error_text.contains("UnknownError")
        || error_text.contains("QuotaExceededError")
        || error_text.contains("out of memory")
}

/// Load just the core half and assemble a sentence-less pack: enough for the
/// placement test and word lookups while the sentence half is still on the
/// wire.
pub(crate) async fn load_language_pack_core(
    data_directory_handle: &DirectoryHandle,
    course: Course,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<LanguagePack, LanguageDataError> {
    let _perf_timer = bridgerton::platform::PerfTimer::new("load_language_pack_core");
    let (core_meta, _sentences_meta) = language_data_hashes_for_course(course)?;
    let mut language_directory = course_data_directory(data_directory_handle, course).await?;

    let core = load_part(
        &mut language_directory,
        course,
        PackPart::Core,
        core_meta,
        set_loading_state,
        deserialize_core,
    )
    .await?;

    Ok(LanguagePack::from_parts(core, None))
}

pub(crate) async fn load_language_pack(
    data_directory_handle: &DirectoryHandle,
    course: Course,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<LanguagePack, LanguageDataError> {
    let _perf_timer = bridgerton::platform::PerfTimer::new("load_language_pack");
    let (core_meta, sentences_meta) = language_data_hashes_for_course(course)?;
    let mut language_directory = course_data_directory(data_directory_handle, course).await?;

    let core = load_part(
        &mut language_directory,
        course,
        PackPart::Core,
        core_meta,
        set_loading_state,
        deserialize_core,
    )
    .await?;
    let sentences = load_part(
        &mut language_directory,
        course,
        PackPart::Sentences,
        sentences_meta,
        set_loading_state,
        deserialize_sentences,
    )
    .await?;

    // Both halves are cached and valid: anything else is a previous version.
    remove_stale_language_data_files(
        &mut language_directory,
        &current_language_data_filenames(core_meta, sentences_meta),
    )
    .await?;

    set_loading_state("Preparing language data", 100.0);
    let assemble_timer = bridgerton::platform::PerfTimer::new("Assembling language pack");
    let pack = LanguagePack::from_parts(core, Some(sentences));
    drop(assemble_timer);
    Ok(pack)
}

async fn course_data_directory(
    data_directory_handle: &DirectoryHandle,
    course: Course,
) -> Result<DirectoryHandle, LanguageDataError> {
    data_directory_handle
        .get_directory_handle_with_options(
            &course_directory_slug(course),
            &opfs::GetDirectoryHandleOptions { create: true },
        )
        .await
        .map_err(LanguageDataError::Persistent)
}

fn deserialize_core(bytes: &[u8]) -> Result<LanguagePackCore, rkyv::rancor::Error> {
    let archived = rkyv::access::<ArchivedLanguagePackCore, rkyv::rancor::Error>(bytes)?;
    rkyv::deserialize::<LanguagePackCore, rkyv::rancor::Error>(archived)
}

fn deserialize_sentences(bytes: &[u8]) -> Result<LanguagePackSentences, rkyv::rancor::Error> {
    let archived = rkyv::access::<ArchivedLanguagePackSentences, rkyv::rancor::Error>(bytes)?;
    rkyv::deserialize::<LanguagePackSentences, rkyv::rancor::Error>(archived)
}

/// Get one part's bytes (cache or network) and deserialize them. A cached
/// copy that fails to deserialize is discarded and re-downloaded once.
async fn load_part<T: Send + 'static>(
    language_directory: &mut DirectoryHandle,
    course: Course,
    part: PackPart,
    meta: PartMeta,
    set_loading_state: &impl Fn(&str, f32),
    deserialize: fn(&[u8]) -> Result<T, rkyv::rancor::Error>,
) -> Result<T, LanguageDataError> {
    let bytes =
        ensure_part_bytes(language_directory, course, part, meta, set_loading_state).await?;

    set_loading_state("Deserializing language data", 100.0);
    let _perf_timer = bridgerton::platform::PerfTimer::new("Deserializing language data part");
    match deserialize_part(bytes, deserialize).await {
        Ok(value) => Ok(value),
        Err(LanguageDataError::Rkyv(e)) => {
            log::error!(
                "Error deserializing language data part {}: {e}\nre-downloading",
                part.slug()
            );
            remove_language_data_files(
                language_directory,
                &language_data_chunk_filenames(part, meta),
            )
            .await;
            let bytes =
                ensure_part_bytes(language_directory, course, part, meta, set_loading_state)
                    .await?;
            deserialize_part(bytes, deserialize).await
        }
        Err(error) => Err(error),
    }
}

async fn deserialize_part<T: Send + 'static>(
    bytes: Vec<u8>,
    deserialize: fn(&[u8]) -> Result<T, rkyv::rancor::Error>,
) -> Result<T, LanguageDataError> {
    let result = bridgerton::platform::run_blocking(move || deserialize(&bytes)).await?;
    result.map_err(LanguageDataError::Rkyv)
}

async fn ensure_part_bytes(
    language_directory: &mut DirectoryHandle,
    course: Course,
    part: PackPart,
    meta: PartMeta,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    if let Some(bytes) =
        read_cached_language_data(language_directory, part, meta, set_loading_state).await?
    {
        log::info!(
            "Language data part {} from chunked local storage hash matches expectation",
            part.slug()
        );
        return Ok(bytes);
    }
    let _perf_timer =
        bridgerton::platform::PerfTimer::new("downloading and caching language data part");
    log::info!(
        "Downloading language data part {} because the chunked cache was missing or invalid",
        part.slug()
    );
    download_and_cache_language_data(language_directory, course, part, meta, set_loading_state)
        .await
}

#[derive(Debug, thiserror::Error)]
#[bridgerton::bridge(error)]
pub enum LanguageDataError {
    #[error(transparent)]
    Io(
        #[from]
        #[bridge(message)]
        std::io::Error,
    ),

    #[error("OPFS error: {0:?}")]
    Persistent(#[bridge(message)] persistent::Error),

    #[error("Rkyv error: {0}")]
    Rkyv(
        #[source]
        #[bridge(message)]
        rkyv::rancor::Error,
    ),

    #[error("AI server error: {0}")]
    AiServer(
        #[source]
        #[bridge(message)]
        fetch_happen::Error,
    ),

    #[error("Server returned HTTP {0}")]
    ServerError(u16),

    #[error("{0}")]
    InvalidData(String),

    #[error("Unsupported course: {0:?}")]
    UnsupportedCourse(Course),
}

async fn read_cached_language_data(
    language_directory_handle: &mut DirectoryHandle,
    part: PackPart,
    meta: PartMeta,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Option<Vec<u8>>, LanguageDataError> {
    let chunk_filenames = language_data_chunk_filenames(part, meta);

    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let file_handle = match language_directory_handle
            .get_file_handle_with_options(filename, &opfs::GetFileHandleOptions { create: false })
            .await
        {
            Ok(file_handle) => file_handle,
            Err(_) => return Ok(None),
        };

        let expected_chunk_len = language_data_chunk_len(meta.size, chunk_index);
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

    let mut bytes = Vec::with_capacity(meta.size);
    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let chunk_bytes = match get_cached_chunk_bytes(
            language_directory_handle,
            filename,
            language_data_chunk_len(meta.size, chunk_index),
        )
        .await
        {
            Ok(Some(chunk_bytes)) => chunk_bytes,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        };

        bytes.extend_from_slice(&chunk_bytes);
        let progress = (bytes.len() as f64 / meta.size.max(1) as f64) * 100.0;
        set_loading_state("Loading...", progress as f32);
    }

    let computed_hash = const_xxh3(&bytes);
    if computed_hash != meta.hash {
        log::warn!(
            "Chunked language data hash mismatch! Expected: {expected}, Got: {computed_hash}. Removing cached chunks.",
            expected = meta.hash
        );
        remove_language_data_files(language_directory_handle, &chunk_filenames).await;
        return Ok(None);
    }

    Ok(Some(bytes))
}

async fn download_and_cache_language_data(
    language_directory_handle: &mut DirectoryHandle,
    course: Course,
    part: PackPart,
    meta: PartMeta,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    let chunk_filenames = language_data_chunk_filenames(part, meta);
    let mut downloaded_bytes = 0usize;
    let mut bytes = Vec::with_capacity(meta.size);

    for (chunk_index, filename) in chunk_filenames.iter().enumerate() {
        let expected_chunk_len = language_data_chunk_len(meta.size, chunk_index);

        if let Some(chunk_bytes) =
            get_cached_chunk_bytes(language_directory_handle, filename, expected_chunk_len).await?
        {
            bytes.extend_from_slice(&chunk_bytes);
            downloaded_bytes += chunk_bytes.len();
            let progress = (downloaded_bytes as f64 / meta.size.max(1) as f64) * 100.0;
            set_loading_state(&part.describe(course), progress as f32);
            continue;
        }

        let chunk_bytes = download_language_data_chunk(
            course,
            part,
            chunk_index,
            expected_chunk_len,
            downloaded_bytes,
            meta.size,
            set_loading_state,
        )
        .await?;

        cache_language_data_bytes(language_directory_handle, filename, &chunk_bytes).await?;
        bytes.extend_from_slice(&chunk_bytes);
        downloaded_bytes += chunk_bytes.len();
    }

    set_loading_state("Verifying language data", 100.0);
    let computed_hash = const_xxh3(&bytes);
    if computed_hash != meta.hash {
        remove_language_data_files(language_directory_handle, &chunk_filenames).await;
        return Err(LanguageDataError::InvalidData(format!(
            "Downloaded language data hash mismatch. Expected {expected}, got {computed_hash}",
            expected = meta.hash
        )));
    }

    log::info!(
        "Language data part {} successfully loaded and cached in chunks!",
        part.slug()
    );
    Ok(bytes)
}

async fn download_language_data_chunk(
    course: Course,
    part: PackPart,
    chunk_index: usize,
    expected_chunk_len: usize,
    downloaded_before_chunk: usize,
    expected_total_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    let request = LanguageDataRequest {
        course,
        part,
        chunk_index,
        chunk_size: LANGUAGE_DATA_CHUNK_SIZE,
    };
    let url = format!("{}/language-data", crate::utils::ai_server_url());
    fetch_language_data_chunk(
        &url,
        request,
        expected_chunk_len,
        downloaded_before_chunk,
        expected_total_size,
        set_loading_state,
    )
    .await
}

async fn fetch_language_data_chunk(
    url: &str,
    request: LanguageDataRequest,
    expected_chunk_len: usize,
    downloaded_before_chunk: usize,
    expected_total_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    let LanguageDataRequest {
        course,
        part,
        chunk_index,
        ..
    } = request;
    let response = fetch_happen::Client
        .post(url)
        .json(&request)
        .map_err(LanguageDataError::AiServer)?
        .send()
        .await
        .map_err(LanguageDataError::AiServer)?;

    if !response.ok() {
        log::error!(
            "Server returned error while fetching {} chunk {}: {}",
            part.slug(),
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
                    set_loading_state(&part.describe(course), progress as f32);
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn streamed_native_download_reports_progress_and_checks_responses() {
        let course = Course {
            target_language: Language::French,
            native_language: Language::English,
        };
        for (status, body, succeeds) in [
            (200, b"pack".as_slice(), true),
            (200, b"bad".as_slice(), false),
            (503, b"busy".as_slice(), false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break request.len();
                    }
                };
                let header = String::from_utf8(request.clone()).unwrap();
                assert!(header.starts_with("POST /language-data HTTP/1.1"));
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                request.resize(header_end + length, 0);
                socket.read_exact(&mut request[header_end..]).await.unwrap();
                let request: serde_json::Value =
                    serde_json::from_slice(&request[header_end..]).unwrap();
                assert_eq!(request["part"], "core");
                assert_eq!(request["chunk_index"], 0);
                assert_eq!(request["chunk_size"], LANGUAGE_DATA_CHUNK_SIZE);
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            });
            let progress = std::cell::RefCell::new(Vec::new());
            let result = fetch_language_data_chunk(
                &format!("http://{address}/language-data"),
                LanguageDataRequest {
                    course,
                    part: PackPart::Core,
                    chunk_index: 0,
                    chunk_size: LANGUAGE_DATA_CHUNK_SIZE,
                },
                4,
                0,
                4,
                &|_, percent| progress.borrow_mut().push(percent),
            )
            .await;
            server.await.unwrap();
            if succeeds {
                assert_eq!(result.unwrap(), b"pack");
                assert_eq!(progress.borrow().last(), Some(&100.0));
            } else if status == 503 {
                assert!(matches!(result, Err(LanguageDataError::ServerError(503))));
            } else {
                assert!(matches!(result, Err(LanguageDataError::InvalidData(_))));
            }
        }
    }
}

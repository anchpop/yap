use futures::StreamExt as _;
use language_utils::{
    Course, Language,
    language_pack::{ArchivedLanguagePack, LanguagePack},
};
use opfs::{
    DirectoryHandle as _, FileHandle as _, WritableFileStream as _,
    persistent::{self, DirectoryHandle},
};
use std::{collections::BTreeMap, sync::LazyLock};
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

    let filename = format!("language_data_{language_data_hash}.rkyv");
    let language_data_file = language_directory
        .get_file_handle_with_options(&filename, &opfs::GetFileHandleOptions { create: false })
        .await;

    let bytes = match language_data_file {
        Ok(language_data_file) => {
            // Cache hit - read from local storage
            let _perf_timer = utils::PerfTimer::new("reading language data from local storage");
            let bytes = language_data_file
                .read()
                .await
                .map_err(LanguageDataError::Persistent)?;
            let computed_hash = const_xxh3(&bytes);
            if computed_hash != language_data_hash {
                log::warn!(
                    "Language data hash mismatch! Expected: {language_data_hash}, Got: {computed_hash}"
                );
                log::info!(
                    "Language data cache miss for {:?}->{:?}, fetching from server...",
                    course.native_language,
                    course.target_language
                );
                download_and_cache_language_data(
                    &mut language_directory,
                    course,
                    language_data_hash,
                    language_data_size,
                    set_loading_state,
                )
                .await?
            } else {
                log::info!("Language data from local storage hash matches expectation");
                bytes
            }
        }
        Err(e) => {
            let _perf_timer = utils::PerfTimer::new("downloading and caching language data");
            log::info!(
                "Downloading and caching language data because the language data file ({filename}) was not found: {e:?}"
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
    // Common deserialization logic for both cache hit and miss
    let archived = rkyv::access::<ArchivedLanguagePack, rkyv::rancor::Error>(&bytes[..]);

    let deserialized = match archived {
        Ok(archived) => rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
            .inspect_err(|e| {
                log::error!("Error deserializing language data: {e:?}");
            })
            .unwrap(),
        Err(e) => {
            log::error!("Error when accessing language data: {e}\nre-downloading language data");
            let bytes = download_and_cache_language_data(
                &mut language_directory,
                course,
                language_data_hash,
                language_data_size,
                set_loading_state,
            )
            .await?;
            let archived = rkyv::access::<ArchivedLanguagePack, rkyv::rancor::Error>(&bytes[..])
                .inspect_err(|e| {
                    log::error!("2nd error accessing language data: {e:?}");
                })
                .map_err(LanguageDataError::Rkyv)?;
            rkyv::deserialize::<LanguagePack, rkyv::rancor::Error>(archived)
                .inspect_err(|e| {
                    log::error!("Error deserializing language data: {e:?}");
                })
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
                wasm_bindgen::JsValue::from_str(&format!("AI server error: {error:?}"))
            }
            LanguageDataError::UnsupportedCourse(course) => {
                wasm_bindgen::JsValue::from_str(&format!("Unsupported course: {course:?}"))
            }
        }
    }
}

async fn download_and_cache_language_data(
    language_directory_handle: &mut DirectoryHandle,
    course: Course,
    expected_hash: u64,
    expected_size: usize,
    set_loading_state: &impl Fn(&str, f32),
) -> Result<Vec<u8>, LanguageDataError> {
    set_loading_state(
        &format!("Downloading {:?} language data", course.target_language),
        0.0,
    );
    let response = {
        let path: &str = "/language-data";
        let request = Some(course);
        async move {
            let client = fetch_happen::Client;
            let url = if cfg!(feature = "local-backend") {
                "http://localhost:21516"
            } else {
                "https://yap-ai-backend.fly.dev"
            };
            let full_url = format!("{url}{path}");
            let mut req = client.post(&full_url);
            if let Some(body) = request {
                req = req.json(&body)?;
            }
            let response = req.send().await?;
            Ok(response)
        }
    }
    .await
    .map_err(LanguageDataError::AiServer)?;

    if !response.ok() {
        log::info!("Server returned error: {}", response.status());
        panic!("Server returned error: {}", response.status());
    }

    // Try to get content-length header, fallback to expected_size
    let content_length = response
        .header("content-length")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<usize>().ok())
        .or(if expected_size > 0 {
            Some(expected_size)
        } else {
            None
        });

    // Stream the response with progress tracking
    let reader = response
        .stream_reader()
        .map_err(LanguageDataError::AiServer)?;

    let mut bytes = Vec::new();
    let mut last_logged_percent = 0;

    loop {
        match reader.read_chunk().await {
            Ok(Some(chunk)) => {
                bytes.extend_from_slice(&chunk);

                // Report progress if we know the total size
                if let Some(total) = content_length {
                    let progress = (bytes.len() as f64 / total as f64) * 100.0;
                    let progress_int = progress as u32;

                    // Update every 1%
                    if progress_int > last_logged_percent {
                        set_loading_state(
                            &format!("Downloading {:?} language data", course.target_language),
                            progress as f32,
                        );
                        last_logged_percent = progress_int;
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                return Err(LanguageDataError::AiServer(e));
            }
        }
    }

    set_loading_state("Verifying language data", 100.0);
    let language_data_hash = {
        let computed_hash = const_xxh3(&bytes);

        if computed_hash != expected_hash {
            log::warn!(
                "Language data hash mismatch! Expected: {expected_hash}, Got: {computed_hash}. Proceeding anyway..."
            );
        } else {
            log::info!("Language data hash verified.");
        }
        computed_hash
    };
    let mut language_data_file = language_directory_handle
        .get_file_handle_with_options(
            &format!("language_data_{language_data_hash}.rkyv"),
            &opfs::GetFileHandleOptions { create: true },
        )
        .await
        .map_err(LanguageDataError::Persistent)?;
    let mut writable = language_data_file
        .create_writable_with_options(&opfs::CreateWritableOptions {
            keep_existing_data: false,
        })
        .await
        .map_err(LanguageDataError::Persistent)?;
    writable
        .write_at_cursor_pos(bytes.clone())
        .await
        .map_err(LanguageDataError::Persistent)?;
    writable
        .close()
        .await
        .map_err(LanguageDataError::Persistent)?;

    set_loading_state("Cleaning up old language data files", 100.0);
    // Clean up old language data files
    let files_to_delete = {
        let current_filename = format!("language_data_{language_data_hash}.rkyv");
        let mut entries = language_directory_handle
            .entries()
            .await
            .map_err(LanguageDataError::Persistent)?;
        let mut files_to_delete = Vec::new();

        // Collect filenames to delete first
        while let Some(Ok((filename, _))) = entries.next().await {
            if filename.starts_with("language_data_")
                && filename.ends_with(".hash")
                && filename != current_filename
            {
                files_to_delete.push(filename);
            }
        }

        files_to_delete
    };

    // Now delete the collected files
    for filename in files_to_delete {
        log::info!("Removing old language data file: {filename}");
        if let Err(e) = language_directory_handle.remove_entry(&filename).await {
            log::warn!("Failed to remove old language data file {filename}: {e:?}");
        }
    }

    log::info!("Language data successfully loaded and cached!");
    Ok(bytes)
}

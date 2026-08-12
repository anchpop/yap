//! Shared OpenSubtitles API access for the downloader and the one-off
//! `recover-subtitles` binary.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Why OpenSubtitles refused to serve a download.
///
/// Distinguished from ordinary failures because a long recovery run must stop
/// cleanly and resumably when the daily allowance is gone, rather than burning
/// through every remaining movie recording spurious "no match" verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Throttled {
    /// 406 — the account's download allowance for the day is spent.
    QuotaExhausted,
    /// 429 — too many requests too quickly; retryable after a pause.
    TooManyRequests,
}

impl std::fmt::Display for Throttled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuotaExhausted => write!(f, "OpenSubtitles download quota exhausted"),
            Self::TooManyRequests => write!(f, "OpenSubtitles rate limit hit"),
        }
    }
}

impl std::error::Error for Throttled {}

/// The `sentence-sources/movies` directory for one language pack.
pub fn movies_dir(language_iso639_3: &str) -> PathBuf {
    PathBuf::from(format!(
        "./generate-data/data/{language_iso639_3}/sentence-sources/movies"
    ))
}

/// Response from /discover/popular endpoint
#[derive(Debug, Deserialize)]
pub struct PopularMoviesResponse {
    pub data: Vec<PopularMovie>,
}

#[derive(Debug, Deserialize)]
pub struct PopularMovie {
    pub attributes: PopularMovieAttributes,
}

#[derive(Debug, Deserialize)]
pub struct PopularMovieAttributes {
    pub title: String,
    #[serde(rename = "imdb_id")]
    pub imdb_id: Option<u64>,
    pub year: Option<String>,
}

/// Response from /subtitles search endpoint
#[derive(Debug, Deserialize)]
pub struct SubtitleSearchResponse {
    pub data: Vec<SubtitleResult>,
}

#[derive(Debug, Deserialize)]
pub struct SubtitleResult {
    pub attributes: SubtitleAttributes,
}

#[derive(Debug, Deserialize)]
pub struct SubtitleAttributes {
    #[serde(rename = "feature_details")]
    pub feature_details: FeatureDetails,
    pub files: Vec<SubtitleFile>,
    pub download_count: Option<u64>,
    #[serde(default)]
    pub from_trusted: Option<bool>,
    #[serde(default)]
    pub ai_translated: bool,
    #[serde(default)]
    pub machine_translated: bool,
    pub ratings: Option<f64>,
    #[serde(default)]
    pub release: Option<String>,
    #[serde(default)]
    pub upload_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FeatureDetails {
    #[serde(rename = "imdb_id")]
    pub imdb_id: Option<u64>,
    pub title: Option<String>,
    pub year: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct SubtitleFile {
    #[serde(rename = "file_id")]
    pub file_id: u64,
}

/// Download link response
#[derive(Debug, Deserialize)]
pub struct DownloadResponse {
    pub link: String,
    #[serde(rename = "file_name")]
    pub file_name: String,
}

/// Rank subtitle candidates best-first: trusted, then most-downloaded, then
/// best-rated.
///
/// Tuple ordering is ascending, so each key is negated to put the desirable
/// value first. Written as a key function rather than a chain of early returns
/// because the latter is easy to get subtly wrong — an earlier version
/// reported `Less` when *both* sides were trusted, which is not a valid
/// ordering and left the sort arbitrary among trusted subtitles.
pub fn rank_by_quality(results: &mut [SubtitleResult]) {
    results.sort_by(|a, b| {
        let rank = |s: &SubtitleResult| {
            (
                !s.attributes.from_trusted.unwrap_or(false),
                std::cmp::Reverse(s.attributes.download_count.unwrap_or(0)),
            )
        };
        rank(a).cmp(&rank(b)).then_with(|| {
            b.attributes
                .ratings
                .unwrap_or(0.0)
                .partial_cmp(&a.attributes.ratings.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
}

pub struct OpenSubtitlesClient {
    api_key: String,
    pub client: reqwest::Client,
    access_token: Option<String>,
}

impl OpenSubtitlesClient {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("yap-language-learning v0.1")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            api_key,
            client,
            access_token: None,
        }
    }

    /// Login to get JWT access token
    pub async fn login(&mut self, username: &str, password: &str) -> Result<()> {
        let url = "https://api.opensubtitles.com/api/v1/login";

        let mut body = HashMap::new();
        body.insert("username", username);
        body.insert("password", password);

        let response = self
            .client
            .post(url)
            .header("Api-Key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        #[derive(Deserialize)]
        struct LoginResponse {
            token: String,
        }

        let login_response: LoginResponse = response.json().await?;
        self.access_token = Some(login_response.token);

        println!("✓ Successfully authenticated");
        Ok(())
    }

    /// Get popular movies from the discover/popular endpoint
    pub async fn get_popular_movies(
        &self,
        language: &str,
        limit: usize,
    ) -> Result<Vec<PopularMovie>> {
        let url = format!(
            "https://api.opensubtitles.com/api/v1/discover/popular?languages={language}&type=movie"
        );

        println!("Fetching popular movies: {url}");

        let response = self
            .client
            .get(&url)
            .header("Api-Key", &self.api_key)
            .send()
            .await?;

        let status = response.status();
        println!("Response status: {status}");

        if !status.is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("API error ({status}): {error_text}"));
        }

        let popular_response: PopularMoviesResponse = response.json().await?;

        println!("Found {} popular movies", popular_response.data.len());

        Ok(popular_response.data.into_iter().take(limit).collect())
    }

    /// Search for subtitles for a specific movie by IMDB ID
    pub async fn search_subtitles_for_movie(
        &self,
        imdb_id: u64,
        language: &str,
    ) -> Result<Vec<SubtitleResult>> {
        let url = format!(
            "https://api.opensubtitles.com/api/v1/subtitles?imdb_id={imdb_id}&languages={language}"
        );

        let response = self
            .client
            .get(&url)
            .header("Api-Key", &self.api_key)
            .send()
            .await?;

        if let Some(throttled) = throttle_reason(response.status()) {
            return Err(throttled.into());
        }
        let response = response.error_for_status()?;

        let body = response
            .text()
            .await
            .context("Failed to get subtitle search response")?;
        let search_response: SubtitleSearchResponse = serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse subtitle search response: {body}"))?;

        Ok(search_response.data)
    }

    /// Download a subtitle file, returning its raw contents.
    pub async fn download_subtitle(&self, file_id: u64) -> Result<String> {
        let url = "https://api.opensubtitles.com/api/v1/download";

        let mut body = HashMap::new();
        body.insert("file_id", file_id);

        let mut request = self.client.post(url).header("Api-Key", &self.api_key);

        if let Some(token) = &self.access_token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }

        let response = request.json(&body).send().await?;
        if let Some(throttled) = throttle_reason(response.status()) {
            return Err(throttled.into());
        }
        let response = response.error_for_status()?;

        let download_response: DownloadResponse = response.json().await?;

        let srt_response = self.client.get(&download_response.link).send().await?;
        let srt_content = srt_response.text().await?;

        // Rate limiting: wait 500ms between requests
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Ok(srt_content)
    }
}

/// Map the HTTP statuses OpenSubtitles uses for throttling onto [`Throttled`].
fn throttle_reason(status: reqwest::StatusCode) -> Option<Throttled> {
    match status.as_u16() {
        406 => Some(Throttled::QuotaExhausted),
        429 => Some(Throttled::TooManyRequests),
        _ => None,
    }
}

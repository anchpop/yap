use anyhow::Context as _;
use indicatif::{ProgressBar, ProgressStyle};
use language_utils::{Course, Language};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead as _, BufReader, Write};
use std::path::Path;

/// The language's multiword-term list: Wiktionary categories + extra +
/// discovered terms − banned terms. Only the Wiktionary download is cached
/// (`wiktionary_multiword_terms.txt`; delete it to force a refetch) — the
/// merged list is REBUILT on every call, so terms discovered since the last
/// run are adopted without any manual cache-busting. Discovery runs with no
/// human review step; a load-if-exists cache here would silently freeze
/// adoption. The merged file is still written each time: it's committed
/// provenance, and sense_discovery reads it for its novelty check.
pub async fn ensure_multiword_terms_file(
    course: &Course,
    base_path: &Path,
) -> anyhow::Result<Vec<String>> {
    let Course {
        target_language, ..
    } = course;
    let multiword_terms_file = base_path.join("target_language_multiword_terms.txt");
    let wiktionary_cache = base_path.join("wiktionary_multiword_terms.txt");

    let extra_terms = extra_multiword_terms(*target_language)
        .await
        .context("Failed to get extra multiword terms")?;

    let terms: Vec<String> = if wiktionary_cache.exists() {
        let content = std::fs::read_to_string(&wiktionary_cache)
            .context("Failed to read wiktionary terms cache")?;
        content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    } else if multiword_terms_file.exists() {
        // Bootstrap the download cache from the previously merged file by
        // subtracting the locally sourced terms, instead of refetching:
        // Wiktionary categories drift, and a refetch would churn every
        // committed term list at once. (A term that was in both Wiktionary
        // and the local lists loses its download provenance here, but the
        // merged result is identical.) Delete the cache to really refetch.
        let extra_set: BTreeSet<&str> = extra_terms.iter().map(String::as_str).collect();
        let content = std::fs::read_to_string(&multiword_terms_file)
            .context("Failed to read multiword terms file")?;
        let derived: Vec<String> = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !extra_set.contains(line))
            .map(String::from)
            .collect();
        let mut file =
            File::create(&wiktionary_cache).context("Failed to create wiktionary terms cache")?;
        for term in &derived {
            writeln!(file, "{term}")?;
        }
        derived
    } else {
        let downloaded = download_multiword_terms(*target_language)
            .await
            .context("Failed to download multiword terms")?;
        let mut file =
            File::create(&wiktionary_cache).context("Failed to create wiktionary terms cache")?;
        for term in &downloaded {
            writeln!(file, "{term}")?;
        }
        downloaded
    };
    let banned_terms = match target_language {
        Language::French => vec!["de le", "de les", "à le", "à les", "fait que", "aller y"],
        Language::Spanish => vec!["de el", "a el"], // Spanish contractions that become "del" and "al"
        Language::English => vec!["me thinketh"],
        Language::Korean => vec![],
        Language::German => vec!["daß"],

        Language::ChineseSimplified
        | Language::ChineseTraditional
        | Language::Japanese
        | Language::Russian
        | Language::Portuguese => vec![],
        Language::Italian => vec![],
        Language::Hindi => vec![],
        Language::Thai => vec![],
    };
    let banned_terms = banned_terms
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let banned_terms = banned_terms.iter().collect::<BTreeSet<_>>();
    let terms = terms
        .into_iter()
        .chain(extra_terms)
        .filter(|term| !banned_terms.contains(term))
        .collect::<BTreeSet<_>>();

    let mut file =
        File::create(&multiword_terms_file).context("Failed to create multiword terms file")?;

    for term in &terms {
        writeln!(file, "{term}")?;
    }

    Ok(terms.into_iter().collect())
}

/// Returns the set of multiword terms that are discontinuous (contained `...` in the
/// original extra_multiword_terms files). These patterns have gaps where other tokens
/// appear between the anchors (e.g., French "ne...que", German "weder...noch").
pub fn get_discontinuous_terms(course: &Course) -> BTreeSet<String> {
    let language_code = course.target_language.code();
    let mut discontinuous = BTreeSet::new();

    for suffix in ["extra_multiword_terms.txt"] {
        let path = format!("./generate-data/data/{language_code}/{suffix}");
        if let Ok(file) = File::open(Path::new(&path)) {
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok) {
                let raw = line.trim().to_string();
                if raw.contains("...") {
                    let cleaned = raw.replace("...", "").replace("  ", " ").trim().to_string();
                    if !cleaned.is_empty() {
                        discontinuous.insert(cleaned);
                    }
                }
            }
        }
    }

    discontinuous
}

async fn extra_multiword_terms(language: Language) -> anyhow::Result<Vec<String>> {
    let language_code = language.code();
    let mut terms = Vec::new();

    // Read manually curated extra multiword terms
    let manual_path = format!("./generate-data/data/{language_code}/extra_multiword_terms.txt");
    if let Ok(file) = File::open(Path::new(&manual_path)) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?.trim().to_string();
            let line = line
                .replace("...", "")
                .replace("  ", " ")
                .trim()
                .to_string();
            terms.push(line);
        }
    }

    // Terms mined by the sense_discovery binary (embedding-cluster splits,
    // LLM-extracted, corpus-grounded) and committed for adoption. Only the
    // variant surface enters the inventory: a record's `citation` is
    // deliberately *not* pushed here, because making it a term would make it
    // matchable and encodable, which is exactly the competing-gram situation
    // that field exists to avoid.
    terms.extend(
        crate::sense_discovery::load_discovered_terms(language)?
            .into_iter()
            .map(|entry| entry.term.trim().to_string()),
    );

    Ok(terms)
}

async fn download_multiword_terms(language: Language) -> anyhow::Result<Vec<String>> {
    let category = match language {
        Language::French => "French_multiword_terms",
        Language::English => "English_multiword_terms",
        Language::Spanish => "Spanish_multiword_terms",
        Language::Korean => {
            // Korean multiword terms are not supported yet. The wiktionary page seems very barebones.
            return Ok(vec![]);
        }
        Language::German => "German_multiword_terms",
        Language::ChineseSimplified => {
            // No bare Chinese_multiword_terms category exists. Phrases carry the
            // multi-token set expressions (怎么回事-class) that segmentation alone
            // can't teach; idioms/chengyu mostly come out single-token but are
            // teachable items in their own right. Wiktionary interleaves
            // Simplified and Traditional page titles (and the Mandarin
            // subcategory holds pinyin soft-redirects), so keep only pure-Han,
            // Simplified-compatible titles.
            let mut terms = Vec::new();
            for category in ["Chinese_phrases", "Mandarin_phrases", "Chinese_idioms"] {
                terms.extend(download_category(category).await.unwrap_or_default());
            }
            terms.retain(|t| {
                let has_han = t.chars().any(|c| {
                    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c)
                });
                let has_latin = t.chars().any(|c| c.is_ascii_alphabetic());
                has_han && !has_latin && !language.contains_wrong_han_script(t)
            });
            return Ok(terms);
        }
        Language::ChineseTraditional => {
            // No zho-hant pipeline yet (no corpora, no segmentation model).
            return Ok(vec![]);
        }
        Language::Thai => {
            let mut terms = Vec::new();
            for category in ["Thai_phrases", "Thai_idioms", "Thai_proverbs"] {
                terms.extend(download_category(category).await.unwrap_or_default());
            }
            return Ok(terms);
        }
        Language::Japanese => {
            // The Japanese_multiword_terms category only has subcategories, no direct entries.
            // Fetch from the subcategories instead.
            let mut terms = download_category("Japanese_idioms")
                .await
                .unwrap_or_default();
            terms.extend(
                download_category("Japanese_phrases")
                    .await
                    .unwrap_or_default(),
            );
            return Ok(terms);
        }
        Language::Russian => "Russian_multiword_terms",
        Language::Portuguese => "Portuguese_multiword_terms",
        Language::Italian => "Italian_multiword_terms",
        Language::Hindi => "Hindi_multiword_terms",
    };

    let terms = download_category(category)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context(format!("Failed to download {category}"))?;

    Ok(terms)
}

/// For each multiword term, check if its wiktionary page says it's an
/// "alternative form of" or "misconstruction of" another term.
/// Returns a map from alt-form term → canonical term.
pub async fn download_alt_forms(
    terms: &[String],
    cache_dir: &Path,
) -> anyhow::Result<BTreeMap<String, String>> {
    let cache_file = cache_dir.join("multiword_alt_forms.jsonl");

    // Load cached results
    let mut alt_forms: BTreeMap<String, String> = BTreeMap::new();
    let mut already_checked: BTreeSet<String> = BTreeSet::new();
    if cache_file.exists() {
        let file = File::open(&cache_file)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if let Ok(entry) = serde_json::from_str::<Value>(&line)
                && let Some(term) = entry["term"].as_str()
            {
                already_checked.insert(term.to_string());
                if let Some(canonical) = entry["canonical"].as_str() {
                    alt_forms.insert(term.to_string(), canonical.to_string());
                }
            }
        }
    }

    let to_check: Vec<&String> = terms
        .iter()
        .filter(|t| !already_checked.contains(*t))
        .collect();

    if to_check.is_empty() {
        return Ok(alt_forms);
    }

    let pb = ProgressBar::new(to_check.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} alt-form check ({per_sec}, {eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Open cache file in append mode
    let cache_handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cache_file)?;
    let mut writer = std::io::BufWriter::new(cache_handle);

    let client = reqwest::Client::builder()
        .user_agent("YapBot/1.0 (https://yap.town) reqwest/0.11")
        .build()
        .context("Failed to build HTTP client")?;

    // Batch fetch wikitext via MediaWiki API (up to 50 pages per request)
    for batch in to_check.chunks(50) {
        let titles: String = batch
            .iter()
            .map(|t| t.replace(' ', "_"))
            .collect::<Vec<_>>()
            .join("|");

        let response = client
            .get("https://en.wiktionary.org/w/api.php")
            .query(&[
                ("action", "query"),
                ("titles", &titles),
                ("prop", "revisions"),
                ("rvprop", "content"),
                ("format", "json"),
            ])
            .send()
            .await
            .context("Failed to fetch wikitext batch")?;

        let data: Value = response
            .json()
            .await
            .context("Failed to parse wikitext response")?;

        // Build a map from normalized title back to original term
        let title_to_term: BTreeMap<String, &str> = batch
            .iter()
            .map(|t| (t.replace(' ', "_"), t.as_str()))
            .collect();

        // Also handle MediaWiki title normalization (e.g. first letter capitalization)
        let mut normalized_map: BTreeMap<String, String> = BTreeMap::new();
        if let Some(normalizations) = data["query"]["normalized"].as_array() {
            for n in normalizations {
                if let (Some(from), Some(to)) = (n["from"].as_str(), n["to"].as_str()) {
                    normalized_map.insert(to.to_string(), from.to_string());
                }
            }
        }

        if let Some(pages) = data["query"]["pages"].as_object() {
            for (_page_id, page) in pages {
                let page_title = page["title"].as_str().unwrap_or_default();
                // Resolve back to the original term
                let lookup_title = normalized_map
                    .get(page_title)
                    .cloned()
                    .unwrap_or_else(|| page_title.to_string());
                let original_term = title_to_term
                    .get(&lookup_title)
                    .or_else(|| title_to_term.get(page_title));

                let Some(term) = original_term else {
                    continue;
                };

                let wikitext = page
                    .get("revisions")
                    .and_then(|r| r.as_array())
                    .and_then(|r| r.first())
                    .and_then(|r| r["*"].as_str())
                    .unwrap_or_default();

                let canonical = parse_alt_form_wikitext(wikitext);
                let entry = if let Some(ref canonical) = canonical {
                    serde_json::json!({"term": *term, "canonical": canonical})
                } else {
                    serde_json::json!({"term": *term})
                };
                writeln!(writer, "{}", serde_json::to_string(&entry)?)?;

                if let Some(canonical) = canonical {
                    alt_forms.insert(term.to_string(), canonical);
                }

                pb.inc(1);
            }
        }

        // Small delay between batches
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    writer.flush()?;
    pb.finish_and_clear();

    Ok(alt_forms)
}

/// Parse wikitext for {{alternative form of|LANG|TARGET}} or {{misconstruction of|LANG|TARGET}}
fn parse_alt_form_wikitext(wikitext: &str) -> Option<String> {
    for line in wikitext.lines() {
        let trimmed = line.trim().trim_start_matches('#').trim();
        for template in [
            "alternative form of",
            "Alternative form of",
            "alt form of",
            "misconstruction of",
            "Misconstruction of",
        ] {
            // Match {{template|lang|term}} or {{template|lang|term|...}}
            let pattern = format!("{{{{{template}|");
            if let Some(rest) = trimmed
                .strip_prefix(&pattern)
                .or_else(|| trimmed.strip_prefix(&format!("{{{{  {template}|")))
            {
                // Skip the language code (first parameter)
                let after_lang = rest.split_once('|')?.1;
                // Extract the term (up to next | or }})
                let term = after_lang
                    .split(['|', '}'])
                    .next()?
                    .trim()
                    .replace('_', " ");
                if !term.is_empty() {
                    return Some(term);
                }
            }
        }
    }
    None
}

async fn download_category(category_name: &str) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .user_agent("YapBot/1.0 (https://yap.town) reqwest/0.11")
        .build()
        .context("Failed to build HTTP client")?;
    let base_url = "https://en.wiktionary.org/w/api.php";
    let mut all_pages = Vec::new();
    let mut cmcontinue: Option<String> = None;

    loop {
        // Build query parameters
        let mut params = vec![
            ("action", "query"),
            ("list", "categorymembers"),
            ("cmlimit", "500"),
            ("format", "json"),
            ("cmprop", "title"),
        ];

        // Add category title
        let category_title = format!("Category:{category_name}");
        params.push(("cmtitle", &category_title));

        // Build request
        let mut request = client.get(base_url).query(&params);

        // Add continuation token if we have one
        if let Some(ref token) = cmcontinue {
            request = request.query(&[("cmcontinue", token)]);
        }

        // Send request
        let response = request.send().await.context("Failed to send request")?;
        let text = response
            .text()
            .await
            .context("Failed to get response text")?;

        // Parse JSON
        let data: Value =
            serde_json::from_str(&text).context(format!("Failed to parse `{text}` into JSON"))?;

        // Extract page titles
        if let Some(members) = data["query"]["categorymembers"].as_array() {
            for member in members {
                // Only include main namespace pages (ns = 0)
                if member["ns"] == 0
                    && let Some(title) = member["title"].as_str()
                {
                    all_pages.push(title.to_string());
                }
            }
        }

        // Check for continuation
        if let Some(continue_data) = data["continue"].as_object() {
            if let Some(token) = continue_data["cmcontinue"].as_str() {
                cmcontinue = Some(token.to_string());
            }
        } else {
            // No more pages
            break;
        }
    }

    Ok(all_pages)
}

use anyhow::Context as _;
use language_utils::{Course, Language};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead as _, BufReader, Write};
use std::path::{Path, PathBuf};

pub async fn ensure_multiword_terms_file(
    course: &Course,
    base_path: &Path,
) -> anyhow::Result<PathBuf> {
    let Course {
        target_language, ..
    } = course;
    let multiword_terms_file = base_path.join("target_language_multiword_terms.txt");

    if multiword_terms_file.exists() {
        println!("Multiword terms file already exists, skipping download");
        return multiword_terms_file
            .canonicalize()
            .context("Failed to canonicalize multiword terms file path");
    }

    println!("Multiword terms file not found, downloading from Wiktionary...");
    let terms = download_multiword_terms(*target_language)
        .await
        .context("Failed to download multiword terms")?;
    let extra_terms = extra_multiword_terms(*target_language)
        .await
        .context("Failed to get extra multiword terms")?;
    let banned_terms = match target_language {
        Language::French => vec!["de le", "de les", "à le", "à les"],
        Language::Spanish => vec!["de el", "a el"], // Spanish contractions that become "del" and "al"
        Language::English => vec![],
        Language::Korean => vec![],
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

    for term in terms {
        writeln!(file, "{term}")?;
    }

    multiword_terms_file
        .canonicalize()
        .context("Failed to canonicalize multiword terms file path")
}

async fn extra_multiword_terms(language: Language) -> anyhow::Result<Vec<String>> {
    let language_code = language.iso_639_3();
    let file_path = format!("./generate-data/data/{language_code}/extra_multiword_terms.txt");
    let file = File::open(Path::new(&file_path)).context(format!(
        "Failed to open extra multiword terms file at {file_path}"
    ))?;
    let reader = BufReader::new(file);
    let mut terms = Vec::new();
    for line in reader.lines() {
        let line = line?.trim().to_string();
        let line = line
            .replace("...", "")
            .replace("  ", " ")
            .trim()
            .to_string();
        terms.push(line);
    }
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
    };
    println!("Downloading category: {category}");

    let terms = download_category(category)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Downloaded {} terms", terms.len());

    Ok(terms)
}

async fn download_category(category_name: &str) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::new();
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
        let response = request.send().await?;
        let text = response.text().await?;

        // Parse JSON
        let data: Value = serde_json::from_str(&text)?;

        // Extract page titles
        if let Some(members) = data["query"]["categorymembers"].as_array() {
            for member in members {
                // Only include main namespace pages (ns = 0)
                if member["ns"] == 0 {
                    if let Some(title) = member["title"].as_str() {
                        all_pages.push(title.to_string());
                    }
                }
            }
        }

        // Check for continuation
        if let Some(continue_data) = data["continue"].as_object() {
            if let Some(token) = continue_data["cmcontinue"].as_str() {
                cmcontinue = Some(token.to_string());

                // Progress indicator
                print!("\rDownloaded {} terms so far...", all_pages.len());
                std::io::stdout().flush()?;
            }
        } else {
            // No more pages
            println!(); // New line after progress indicator
            break;
        }
    }

    Ok(all_pages)
}

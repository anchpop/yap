use anyhow::Context as _;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use std::path::Path;

/// Get HTML for a Wiktionary page, using cache if available
///
/// This function checks the cache first. If the file is cached, it returns it.
/// Otherwise, it fetches from Wiktionary and saves to cache.
///
/// # Arguments
/// * `word` - The word to fetch
/// * `cache_dir` - Directory to cache HTML files
///
/// # Returns
/// The HTML content of the Wiktionary page
// Global mutex to ensure rate limiting works even with parallel async requests
static WIKTIONARY_FETCH_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

pub async fn get_wiktionary_html(word: &str, cache_dir: &Path) -> anyhow::Result<String> {
    std::fs::create_dir_all(cache_dir)?;

    let cache_file = cache_dir.join(format!("{word}.html"));

    // Check cache first
    if cache_file.exists() {
        match std::fs::read_to_string(&cache_file) {
            Ok(html) => return Ok(html),
            Err(e) => {
                eprintln!("Failed to read cached HTML for {word}: {e}, will re-fetch");
            }
        }
    }

    // Fetch from Wiktionary with rate limiting
    // Use a mutex to ensure only one request hits the server at a time
    let _guard = WIKTIONARY_FETCH_LOCK.lock().await;

    // Rate limit: sleep before making the request to avoid overwhelming the server
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::builder()
        .user_agent("YapBot/1.0 (https://yap.town) reqwest/0.11")
        .build()
        .context("Failed to build HTTP client")?;

    let url = format!("https://en.wiktionary.org/wiki/{word}");
    let response = client
        .get(&url)
        .send()
        .await
        .context(format!("Failed to fetch {word}"))?;

    let html = response
        .text()
        .await
        .context(format!("Failed to read HTML for {word}"))?;

    // Save to cache
    if let Err(e) = std::fs::write(&cache_file, &html) {
        eprintln!("Warning: Failed to write cache file for {word}: {e}");
    }

    Ok(html)
}

pub mod french {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct FrenchVerbConjugation {
        pub infinitive: String,
        pub present_participle: String,
        pub past_participle: String,

        // Indicative mood - simple tenses (6 forms each: je, tu, il, nous, vous, ils)
        pub indicative_present: [String; 6],
        pub indicative_imperfect: [String; 6],
        pub indicative_past_historic: [String; 6],
        pub indicative_future: [String; 6],
        pub indicative_conditional: [String; 6],

        // Subjunctive mood - simple tenses (6 forms each)
        pub subjunctive_present: [String; 6],
        pub subjunctive_imperfect: [String; 6],

        // Imperative mood (3 forms: tu, nous, vous)
        // None for defective verbs that don't have imperative forms (e.g., pouvoir)
        pub imperative: Option<[String; 3]>,

        // Auxiliary verb for compound tenses
        pub auxiliary: Auxiliary,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum Auxiliary {
        Avoir,
        Être,
    }

    /// Extract the French language section from a Wiktionary page
    fn extract_french_section(document: &Html) -> anyhow::Result<Html> {
        // Find the h2 heading with id="French"
        let h2_selector = Selector::parse("h2#French").unwrap();

        let french_heading = document
            .select(&h2_selector)
            .next()
            .context("Could not find French language section")?;

        // Collect all content until the next h2 (language section)
        let mut french_content = String::new();
        let mut current = french_heading.parent();

        while let Some(node) = current {
            current = node.next_sibling();
            if let Some(current_node) = current {
                // Stop if we hit another h2 (next language section)
                if let Some(elem) = ElementRef::wrap(current_node) {
                    if elem.value().name() == "div"
                        && let Some(first_child) = elem.first_child()
                        && let Some(child_elem) = ElementRef::wrap(first_child)
                        && child_elem.value().name() == "h2"
                    {
                        break;
                    }
                    french_content.push_str(&elem.html());
                }
            }
        }

        Ok(Html::parse_fragment(&french_content))
    }

    /// Parse a French verb conjugation table from Wiktionary HTML
    pub fn parse_french_verb_conjugation(
        html: &str,
        verb: &str,
    ) -> anyhow::Result<FrenchVerbConjugation> {
        let document = Html::parse_document(html);

        // Extract only the French language section to avoid dialect tables
        let french_section = extract_french_section(&document)?;

        // Parse infinitive (it's the verb itself)
        let infinitive = verb.to_string();

        // Parse present participle
        let present_participle = parse_present_participle(&french_section)?;

        // Parse past participle
        let past_participle = parse_past_participle(&french_section)?;

        // Parse auxiliary verb from compound tense row
        let auxiliary = parse_auxiliary(&french_section)?;

        // Parse indicative tenses
        let indicative_present = parse_tense(&french_section, "present", "indicative")?;
        let indicative_imperfect = parse_tense(&french_section, "imperfect", "indicative")?;
        let indicative_past_historic = parse_tense(&french_section, "past historic", "indicative")?;
        let indicative_future = parse_tense(&french_section, "future", "indicative")?;
        let indicative_conditional = parse_tense(&french_section, "conditional", "indicative")?;

        // Parse subjunctive tenses
        let subjunctive_present = parse_tense(&french_section, "present", "subjunctive")?;
        let subjunctive_imperfect = parse_tense(&french_section, "imperfect", "subjunctive")?;

        // Parse imperative
        let imperative = parse_imperative(&french_section)?;

        Ok(FrenchVerbConjugation {
            infinitive,
            present_participle,
            past_participle,
            indicative_present,
            indicative_imperfect,
            indicative_past_historic,
            indicative_future,
            indicative_conditional,
            subjunctive_present,
            subjunctive_imperfect,
            imperative,
            auxiliary,
        })
    }

    fn parse_present_participle(document: &Html) -> anyhow::Result<String> {
        // Look for the present participle row in the table
        // Format: <span class="Latn form-of lang-fr ppr-form-of">
        let selector = Selector::parse("span.ppr-form-of a").unwrap();

        document
            .select(&selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find present participle")
    }

    fn parse_past_participle(document: &Html) -> anyhow::Result<String> {
        // Look for the past participle row in the table
        // Format: <span class="Latn form-of lang-fr pp-form-of">
        let selector = Selector::parse("span.pp-form-of a").unwrap();

        document
            .select(&selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find past participle")
    }

    fn parse_auxiliary(document: &Html) -> anyhow::Result<Auxiliary> {
        // Look for the compound tense row which mentions the auxiliary
        // Format: present indicative of <i><a href="/wiki/avoir">avoir</a></i> + past participle
        // or: present indicative of <i><a href="/wiki/être">être</a></i> + past participle

        let selector = Selector::parse("th.roa-compound-row").unwrap();

        for element in document.select(&selector) {
            let text = element.text().collect::<String>();
            if text.contains("avoir") {
                return Ok(Auxiliary::Avoir);
            }
            if text.contains("être") {
                return Ok(Auxiliary::Être);
            }
        }

        anyhow::bail!("Failed to find auxiliary verb")
    }

    fn parse_tense(document: &Html, tense: &str, mood: &str) -> anyhow::Result<[String; 6]> {
        // Find the row with the tense name
        let th_selector =
            Selector::parse("th.roa-indicative-left-rail, th.roa-subjunctive-left-rail").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        let mood_prefix = match mood {
            "indicative" => "roa-indicative-left-rail",
            "subjunctive" => "roa-subjunctive-left-rail",
            _ => anyhow::bail!("Unknown mood: {}", mood),
        };

        // Find the header for this tense
        let mut tense_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains(&tense.to_lowercase())
                && th.attr("class").unwrap_or("").contains(mood_prefix)
            {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    tense_row_ref = Some(parent);
                    break;
                }
            }
        }

        let tense_row =
            tense_row_ref.context(format!("Failed to find tense row for {mood} {tense}"))?;

        // Extract the 6 conjugated forms from the td elements in this row
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in tense_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                // Find the link inside the td
                let td_elem = scraper::ElementRef::wrap(child).unwrap();
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(text) = link.text().next()
                {
                    forms.push(text.to_string());
                }
            }
        }

        if forms.len() != 6 {
            anyhow::bail!(
                "Expected 6 forms for {} {}, found {}",
                mood,
                tense,
                forms.len()
            );
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
            forms[5].clone(),
        ])
    }

    fn parse_imperative(document: &Html) -> anyhow::Result<Option<[String; 3]>> {
        // Find the imperative row (simple)
        let th_selector = Selector::parse("th.roa-imperative-left-rail").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        let mut imperative_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains("simple") {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    imperative_row_ref = Some(parent);
                    break;
                }
            }
        }

        let imperative_row = imperative_row_ref.context("Failed to find imperative row")?;

        // Extract the 3 forms (skip the first and last which are "—")
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in imperative_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                let td_elem = scraper::ElementRef::wrap(child).unwrap();
                let text = td_elem.text().collect::<String>().trim().to_string();

                // Skip empty cells marked with "—"
                if text == "—" {
                    continue;
                }

                // Extract the link text (the conjugated form)
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(text) = link.text().next()
                {
                    forms.push(text.to_string());
                }
            }
        }

        // Defective verbs (like pouvoir) don't have imperative forms
        if forms.is_empty() {
            return Ok(None);
        }

        if forms.len() != 3 {
            anyhow::bail!("Expected 3 forms for imperative, found {}", forms.len());
        }

        Ok(Some([forms[0].clone(), forms[1].clone(), forms[2].clone()]))
    }

    /// Fetch French verb conjugations from Wiktionary with HTML caching
    pub async fn fetch_french_verb_conjugations(
        verbs: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, FrenchVerbConjugation>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(verbs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} French verbs ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<FrenchVerbConjugation, String>)> =
            futures::stream::iter(verbs.iter())
                .map(|verb| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(verb.to_string());

                        let result = match super::get_wiktionary_html(verb, cache_dir).await {
                            Ok(html) => parse_french_verb_conjugation(&html, verb)
                                .map_err(|e| format!("Failed to parse French verb '{verb}': {e}")),
                            Err(e) => {
                                Err(format!("Failed to get HTML for French verb '{verb}': {e}"))
                            }
                        };

                        pb.inc(1);
                        (verb.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (verb, result) in fetch_results {
            match result {
                Ok(conjugation) => {
                    results.insert(verb, conjugation);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            verbs.len(),
            errors.len()
        ));

        Ok(results)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_parse_boire() {
            let html = fs::read_to_string("src/wiktionary-examples/fra/boire.txt")
                .expect("Failed to read boire.txt");

            let conjugation = parse_french_verb_conjugation(&html, "boire")
                .expect("Failed to parse boire conjugation");

            assert_eq!(conjugation.infinitive, "boire");
            assert_eq!(conjugation.present_participle, "buvant");
            assert_eq!(conjugation.past_participle, "bu");
            assert_eq!(conjugation.auxiliary, Auxiliary::Avoir);

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "bois"); // je
            assert_eq!(conjugation.indicative_present[1], "bois"); // tu
            assert_eq!(conjugation.indicative_present[2], "boit"); // il
            assert_eq!(conjugation.indicative_present[3], "buvons"); // nous
            assert_eq!(conjugation.indicative_present[4], "buvez"); // vous
            assert_eq!(conjugation.indicative_present[5], "boivent"); // ils

            // Check future
            assert_eq!(conjugation.indicative_future[0], "boirai");
            assert_eq!(conjugation.indicative_future[1], "boiras");

            // Check imperative
            let imperative = conjugation.imperative.as_ref().unwrap();
            assert_eq!(imperative[0], "bois"); // tu
            assert_eq!(imperative[1], "buvons"); // nous
            assert_eq!(imperative[2], "buvez"); // vous
        }

        #[test]
        fn test_parse_aller() {
            let html = fs::read_to_string("src/wiktionary-examples/fra/aller.txt")
                .expect("Failed to read aller.txt");

            let conjugation = parse_french_verb_conjugation(&html, "aller")
                .expect("Failed to parse aller conjugation");

            assert_eq!(conjugation.infinitive, "aller");
            assert_eq!(conjugation.auxiliary, Auxiliary::Être);

            // Check indicative present (irregular)
            assert_eq!(conjugation.indicative_present[0], "vais"); // je
            assert_eq!(conjugation.indicative_present[1], "vas"); // tu
            assert_eq!(conjugation.indicative_present[2], "va"); // il
            assert_eq!(conjugation.indicative_present[3], "allons"); // nous
            assert_eq!(conjugation.indicative_present[4], "allez"); // vous
            assert_eq!(conjugation.indicative_present[5], "vont"); // ils

            // Check future (irregular stem)
            assert_eq!(conjugation.indicative_future[0], "irai");
            assert_eq!(conjugation.indicative_future[1], "iras");
        }

        #[test]
        fn test_parse_avoir() {
            let html = fs::read_to_string("src/wiktionary-examples/fra/avoir.txt")
                .expect("Failed to read avoir.txt");

            let conjugation = parse_french_verb_conjugation(&html, "avoir")
                .expect("Failed to parse avoir conjugation");

            assert_eq!(conjugation.infinitive, "avoir");
            assert_eq!(conjugation.auxiliary, Auxiliary::Avoir);

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "ai"); // je
            assert_eq!(conjugation.indicative_present[1], "as"); // tu
            assert_eq!(conjugation.indicative_present[2], "a"); // il
        }

        #[test]
        fn test_parse_etre() {
            let html = fs::read_to_string("src/wiktionary-examples/fra/etre.txt")
                .expect("Failed to read etre.txt");

            let conjugation = parse_french_verb_conjugation(&html, "être")
                .expect("Failed to parse être conjugation");

            assert_eq!(conjugation.infinitive, "être");
            assert_eq!(conjugation.auxiliary, Auxiliary::Avoir);

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "suis"); // je
            assert_eq!(conjugation.indicative_present[1], "es"); // tu
            assert_eq!(conjugation.indicative_present[2], "est"); // il

            // Check indicative past historic (passé simple)
            assert_eq!(conjugation.indicative_past_historic[0], "fus"); // je
            assert_eq!(conjugation.indicative_past_historic[1], "fus"); // tu
            assert_eq!(conjugation.indicative_past_historic[2], "fut"); // il
            assert_eq!(conjugation.indicative_past_historic[3], "fûmes"); // nous
            assert_eq!(conjugation.indicative_past_historic[4], "fûtes"); // vous
            assert_eq!(conjugation.indicative_past_historic[5], "furent"); // ils
        }

        #[test]
        fn test_etre_morphology_for_fus() {
            use crate::morphology_analysis::wiktionary_morphology::french::conjugation_to_morphology;
            use language_utils::features::{Number, Person};
            use language_utils::{Heteronym, PartOfSpeech};

            let html = fs::read_to_string("src/wiktionary-examples/fra/etre.txt")
                .expect("Failed to read etre.txt");

            let conjugation = parse_french_verb_conjugation(&html, "être")
                .expect("Failed to parse être conjugation");

            let morphology =
                conjugation_to_morphology("être", &conjugation, language_utils::PartOfSpeech::Verb);

            // Check that "fus" has two morphology entries (je fus, tu fus)
            let fus_heteronym = Heteronym {
                word: "fus".to_string(),
                lemma: "être".to_string(),
                pos: PartOfSpeech::Verb,
            };

            let fus_morphologies = morphology
                .get(&fus_heteronym)
                .expect("Should have morphology for 'fus'");

            assert_eq!(
                fus_morphologies.len(),
                2,
                "Should have 2 morphology entries for 'fus'"
            );

            // Check first person
            let first_person = fus_morphologies
                .iter()
                .find(|m| m.person == Some(Person::First) && m.number == Some(Number::Singular))
                .expect("Should have first person singular entry");
            assert_eq!(
                first_person.tense,
                Some(language_utils::features::Tense::Past)
            );

            // Check second person
            let second_person = fus_morphologies
                .iter()
                .find(|m| m.person == Some(Person::Second) && m.number == Some(Number::Singular))
                .expect("Should have second person singular entry");
            assert_eq!(
                second_person.tense,
                Some(language_utils::features::Tense::Past)
            );
        }

        #[test]
        fn test_parse_pouvoir() {
            let html = fs::read_to_string("src/wiktionary-examples/fra/pouvoir.txt")
                .expect("Failed to read pouvoir.txt");

            let conjugation = parse_french_verb_conjugation(&html, "pouvoir")
                .expect("Failed to parse pouvoir conjugation");

            assert_eq!(conjugation.infinitive, "pouvoir");
            assert_eq!(conjugation.present_participle, "pouvant");
            assert_eq!(conjugation.past_participle, "pu");
            assert_eq!(conjugation.auxiliary, Auxiliary::Avoir);

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "peux"); // je (or puis)
            assert_eq!(conjugation.indicative_present[1], "peux"); // tu
            assert_eq!(conjugation.indicative_present[2], "peut"); // il
            assert_eq!(conjugation.indicative_present[3], "pouvons"); // nous
            assert_eq!(conjugation.indicative_present[4], "pouvez"); // vous
            assert_eq!(conjugation.indicative_present[5], "peuvent"); // ils

            // Check that pouvoir has no imperative forms (defective verb)
            assert!(
                conjugation.imperative.is_none(),
                "pouvoir should not have imperative forms"
            );
        }
    }
}

pub mod spanish {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct SpanishVerbConjugation {
        pub infinitive: String,
        pub gerund: String,
        pub past_participle_masculine_singular: String,
        pub past_participle_feminine_singular: String,

        // Indicative mood - simple tenses (6 forms each: yo, tú, él, nosotros, vosotros, ellos)
        pub indicative_present: [String; 6],
        pub indicative_imperfect: [String; 6],
        pub indicative_preterite: [String; 6],
        pub indicative_future: [String; 6],
        pub indicative_conditional: [String; 6],

        // Subjunctive mood - simple tenses (6 forms each)
        pub subjunctive_present: [String; 6],
        pub subjunctive_imperfect: [String; 6],
        pub subjunctive_future: [String; 6],

        // Imperative mood (5 forms: tú, usted, nosotros, vosotros, ustedes)
        pub imperative: [String; 5],
    }

    /// Extract the Spanish language section from a Wiktionary page
    fn extract_spanish_section(document: &Html) -> anyhow::Result<Html> {
        // Find the h2 heading with id="Spanish"
        let h2_selector = Selector::parse("h2#Spanish").unwrap();

        let spanish_heading = document
            .select(&h2_selector)
            .next()
            .context("Could not find Spanish language section")?;

        // Collect all content until the next h2 (language section)
        let mut spanish_content = String::new();
        let mut current = spanish_heading.parent();

        while let Some(node) = current {
            current = node.next_sibling();
            if let Some(current_node) = current {
                // Stop if we hit another h2 (next language section)
                if let Some(elem) = ElementRef::wrap(current_node) {
                    if elem.value().name() == "div"
                        && let Some(first_child) = elem.first_child()
                        && let Some(child_elem) = ElementRef::wrap(first_child)
                        && child_elem.value().name() == "h2"
                    {
                        break;
                    }
                    spanish_content.push_str(&elem.html());
                }
            }
        }

        Ok(Html::parse_fragment(&spanish_content))
    }

    /// Parse a Spanish verb conjugation table from Wiktionary HTML
    pub fn parse_spanish_verb_conjugation(
        html: &str,
        verb: &str,
    ) -> anyhow::Result<SpanishVerbConjugation> {
        let document = Html::parse_document(html);

        // Extract only the Spanish language section
        let spanish_section = extract_spanish_section(&document)?;

        // Parse infinitive (it's the verb itself)
        let infinitive = verb.to_string();

        // Parse gerund
        let gerund = parse_gerund(&spanish_section)?;

        // Parse past participles (masculine/feminine singular)
        let (past_participle_masculine_singular, past_participle_feminine_singular) =
            parse_past_participles(&spanish_section)?;

        // Parse indicative tenses
        let indicative_present = parse_tense(&spanish_section, "present", "indicative")?;
        let indicative_imperfect = parse_tense(&spanish_section, "imperfect", "indicative")?;
        let indicative_preterite = parse_tense(&spanish_section, "preterite", "indicative")?;
        let indicative_future = parse_tense(&spanish_section, "future", "indicative")?;
        let indicative_conditional = parse_tense(&spanish_section, "conditional", "indicative")?;

        // Parse subjunctive tenses
        let subjunctive_present = parse_tense(&spanish_section, "present", "subjunctive")?;
        let subjunctive_imperfect = parse_tense(&spanish_section, "imperfect", "subjunctive")?;
        let subjunctive_future = parse_tense(&spanish_section, "future", "subjunctive")?;

        // Parse imperative
        let imperative = parse_imperative(&spanish_section)?;

        Ok(SpanishVerbConjugation {
            infinitive,
            gerund,
            past_participle_masculine_singular,
            past_participle_feminine_singular,
            indicative_present,
            indicative_imperfect,
            indicative_preterite,
            indicative_future,
            indicative_conditional,
            subjunctive_present,
            subjunctive_imperfect,
            subjunctive_future,
            imperative,
        })
    }

    fn parse_gerund(document: &Html) -> anyhow::Result<String> {
        // Look for the gerund row in the table
        // Format: <span class="Latn form-of lang-es gerund-...">
        let selector = Selector::parse("span[class*='gerund'] a").unwrap();

        document
            .select(&selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find gerund")
    }

    fn parse_past_participles(document: &Html) -> anyhow::Result<(String, String)> {
        // Look for past participle rows
        // Masculine singular: <span class="Latn form-of lang-es pp-ms-form-of">
        // Feminine singular: <span class="Latn form-of lang-es pp-fs-form-of">
        let ms_selector = Selector::parse("span[class*='pp'][class*='ms'] a").unwrap();
        let fs_selector = Selector::parse("span[class*='pp'][class*='fs'] a").unwrap();

        let masculine = document
            .select(&ms_selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find masculine past participle")?;

        let feminine = document
            .select(&fs_selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find feminine past participle")?;

        Ok((masculine, feminine))
    }

    fn parse_tense(document: &Html, tense: &str, mood: &str) -> anyhow::Result<[String; 6]> {
        // Spanish uses roa-finite-header for tense headers
        let th_selector = Selector::parse("th.roa-finite-header").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        // Map English tense names to Spanish
        let spanish_tense = match tense {
            "present" => "presente",
            "imperfect" => "imperfecto",
            "preterite" => "pretérito",
            "future" => "futuro",
            "conditional" => "condicional",
            _ => tense, // fallback to English name
        };

        // Find the header for this tense
        let tense_keyword = match mood {
            "indicative" => format!("{spanish_tense} de indicativo"),
            "subjunctive" => format!("{spanish_tense} de subjuntivo"),
            _ => anyhow::bail!("Unknown mood: {}", mood),
        };

        let mut tense_row_ref = None;
        for th in document.select(&th_selector) {
            // Check the title attribute which contains the Spanish name
            if let Some(title) = th.value().attr("title") {
                if title.to_lowercase().contains(&tense_keyword.to_lowercase()) {
                    // Get the parent tr element
                    if let Some(parent) = th.parent()
                        && parent.value().as_element().map(|e| e.name()) == Some("tr")
                    {
                        tense_row_ref = Some(parent);
                        break;
                    }
                }
            } else {
                // Fallback: check text content
                let text = th.text().collect::<String>().to_lowercase();
                if text.contains(&tense.to_lowercase()) {
                    // Need to check if this belongs to the right mood by looking at context
                    // For now, just accept it
                    if let Some(parent) = th.parent()
                        && parent.value().as_element().map(|e| e.name()) == Some("tr")
                    {
                        tense_row_ref = Some(parent);
                        break;
                    }
                }
            }
        }

        let tense_row =
            tense_row_ref.context(format!("Failed to find tense row for {mood} {tense}"))?;

        // Extract the 6 conjugated forms from the td elements in this row
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in tense_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                // Find the link inside the td
                let td_elem = scraper::ElementRef::wrap(child).unwrap();
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(text) = link.text().next()
                {
                    forms.push(text.to_string());
                }
            }
        }

        if forms.len() != 6 {
            anyhow::bail!(
                "Expected 6 forms for {} {}, found {}",
                mood,
                tense,
                forms.len()
            );
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
            forms[5].clone(),
        ])
    }

    fn parse_imperative(document: &Html) -> anyhow::Result<[String; 5]> {
        // Find the affirmative imperative row
        // The structure is: th.roa-finite-header with title="imperativo afirmativo" or text "affirmative"
        let th_selector = Selector::parse("th.roa-finite-header").unwrap();
        let a_selector = Selector::parse("a").unwrap();
        let span_selector = Selector::parse("span[lang='es']").unwrap();

        let mut imperative_row_ref = None;
        for th in document.select(&th_selector) {
            // Check title attribute for "imperativo afirmativo" or text for "affirmative"
            let matches = if let Some(title) = th.value().attr("title") {
                title.to_lowercase().contains("afirmativo")
            } else {
                let text = th.text().collect::<String>().to_lowercase();
                text.contains("affirmative")
            };

            if matches {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    imperative_row_ref = Some(parent);
                    break;
                }
            }
        }

        let imperative_row =
            imperative_row_ref.context("Failed to find imperative affirmative row")?;

        // Extract the 5 forms (skip first td which is empty for "yo")
        // Forms are: tú, usted, nosotros, vosotros, ustedes
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in imperative_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                let td_elem = scraper::ElementRef::wrap(child).unwrap();

                // Check if this is an empty cell (for yo form)
                let text = td_elem.text().collect::<String>().trim().to_string();
                if text.is_empty() {
                    continue;
                }

                // Extract the first Spanish link in this cell
                // (tú cell has two forms - tú and vos, we take the first one for tú)
                let mut found = false;
                for span in td_elem.select(&span_selector) {
                    if let Some(link) = span.select(&a_selector).next()
                        && let Some(form_text) = link.text().next()
                    {
                        forms.push(form_text.to_string());
                        found = true;
                        break; // Take only the first form (tú, not vos)
                    }
                }

                // Fallback: try any link if no Spanish span found
                if !found
                    && let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(form_text) = link.text().next()
                {
                    forms.push(form_text.to_string());
                }
            }
        }

        if forms.len() != 5 {
            anyhow::bail!("Expected 5 forms for imperative, found {}", forms.len());
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
        ])
    }

    /// Fetch Spanish verb conjugations from Wiktionary with HTML caching
    ///
    /// # Arguments
    /// * `verbs` - List of verb infinitives to fetch
    /// * `cache_dir` - Directory to cache HTML files (e.g., `.cache/wiktionary/spanish/`)
    ///
    /// # Returns
    /// HashMap mapping verb infinitives to their conjugations
    ///
    /// # Note
    /// For reflexive verbs ending in "se" that don't have conjugation tables on Wiktionary,
    /// this function will automatically try fetching the base verb (without "se") and use
    /// its conjugation instead.
    pub async fn fetch_spanish_verb_conjugations(
        verbs: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, SpanishVerbConjugation>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(verbs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} Spanish verbs ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<SpanishVerbConjugation, String>)> =
            futures::stream::iter(verbs.iter())
                .map(|verb| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(verb.to_string());

                        // Try to parse, with fallback for reflexive verbs (e.g., "moverse" -> "mover")
                        let mut verb_to_try = verb.as_str();
                        let mut tried_fallback = false;

                        let result = loop {
                            match super::get_wiktionary_html(verb_to_try, cache_dir).await {
                                Ok(html) => {
                                    match parse_spanish_verb_conjugation(&html, verb_to_try) {
                                        Ok(conjugation) => {
                                            break Ok(conjugation);
                                        }
                                        Err(e) => {
                                            // Try fallback for reflexive verbs ending in "se"
                                            if !tried_fallback && verb.ends_with("se") {
                                                let base_verb = &verb[..verb.len() - 2];
                                                verb_to_try = base_verb;
                                                tried_fallback = true;
                                                continue;
                                            }
                                            break Err(format!(
                                                "Failed to parse Spanish verb '{verb}': {e}"
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    break Err(format!(
                                        "Failed to get HTML for Spanish verb '{verb_to_try}': {e}"
                                    ));
                                }
                            }
                        };

                        pb.inc(1);
                        (verb.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (verb, result) in fetch_results {
            match result {
                Ok(conjugation) => {
                    results.insert(verb, conjugation);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            verbs.len(),
            errors.len()
        ));

        Ok(results)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_parse_beber() {
            let html = fs::read_to_string("src/wiktionary-examples/spa/beber.txt")
                .expect("Failed to read beber.txt");

            let conjugation = parse_spanish_verb_conjugation(&html, "beber")
                .expect("Failed to parse beber conjugation");

            assert_eq!(conjugation.infinitive, "beber");
            assert_eq!(conjugation.gerund, "bebiendo");
            assert_eq!(conjugation.past_participle_masculine_singular, "bebido");
            assert_eq!(conjugation.past_participle_feminine_singular, "bebida");

            // Check indicative present (yo, tú, él, nosotros, vosotros, ellos)
            assert_eq!(conjugation.indicative_present[0], "bebo");
            assert_eq!(conjugation.indicative_present[1], "bebes");
            assert_eq!(conjugation.indicative_present[2], "bebe");
        }

        #[test]
        fn test_parse_ser() {
            let html = fs::read_to_string("src/wiktionary-examples/spa/ser.txt")
                .expect("Failed to read ser.txt");

            let conjugation = parse_spanish_verb_conjugation(&html, "ser")
                .expect("Failed to parse ser conjugation");

            assert_eq!(conjugation.infinitive, "ser");
            assert_eq!(conjugation.gerund, "siendo");

            // Check indicative present (irregular)
            assert_eq!(conjugation.indicative_present[0], "soy");
            assert_eq!(conjugation.indicative_present[1], "eres");
            assert_eq!(conjugation.indicative_present[2], "es");
        }

        #[test]
        fn test_parse_tener() {
            let html = fs::read_to_string("src/wiktionary-examples/spa/tener.txt")
                .expect("Failed to read tener.txt");

            let conjugation = parse_spanish_verb_conjugation(&html, "tener")
                .expect("Failed to parse tener conjugation");

            assert_eq!(conjugation.infinitive, "tener");
        }

        #[test]
        fn test_parse_venir() {
            let html = fs::read_to_string("src/wiktionary-examples/spa/venir.txt")
                .expect("Failed to read venir.txt");

            let conjugation = parse_spanish_verb_conjugation(&html, "venir")
                .expect("Failed to parse venir conjugation");

            assert_eq!(conjugation.infinitive, "venir");
        }

        #[test]
        fn test_parse_moverse_is_nonlemma() {
            let html = fs::read_to_string("src/wiktionary-examples/spa/moverse.txt")
                .expect("Failed to read moverse.txt");

            // moverse should fail to parse because it's not a lemma form
            // It's just a redirect to "mover"
            let result = parse_spanish_verb_conjugation(&html, "moverse");
            assert!(result.is_err());

            // The error should be about not finding the gerund
            assert!(result.unwrap_err().to_string().contains("gerund"));
        }
    }
}

pub mod german {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct GermanVerbConjugation {
        pub infinitive: String,
        pub present_participle: String,
        pub past_participle: String,

        // Auxiliary verb (sein or haben)
        pub auxiliary: GermanAuxiliary,

        // Indicative present (6 forms: ich, du, er, wir, ihr, sie)
        pub indicative_present: [String; 6],
        // Indicative preterite (6 forms)
        pub indicative_preterite: [String; 6],

        // Subjunctive I (Konjunktiv I) - 6 forms
        pub subjunctive_i: [String; 6],
        // Subjunctive II (Konjunktiv II) - 6 forms
        pub subjunctive_ii: [String; 6],

        // Imperative (2 forms: du, ihr)
        pub imperative: [String; 2],
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum GermanAuxiliary {
        Haben,
        Sein,
    }

    /// Extract the German language section from a Wiktionary page
    fn extract_german_section(document: &Html) -> anyhow::Result<Html> {
        // Find the h2 heading with id="German"
        let h2_selector = Selector::parse("h2#German").unwrap();

        let german_heading = document
            .select(&h2_selector)
            .next()
            .context("Could not find German language section")?;

        // Collect all content until the next h2 (language section)
        let mut german_content = String::new();
        let mut current = german_heading.parent();

        while let Some(node) = current {
            current = node.next_sibling();
            if let Some(current_node) = current {
                // Stop if we hit another h2 (next language section)
                if let Some(elem) = ElementRef::wrap(current_node) {
                    if elem.value().name() == "div"
                        && let Some(first_child) = elem.first_child()
                        && let Some(child_elem) = ElementRef::wrap(first_child)
                        && child_elem.value().name() == "h2"
                    {
                        break;
                    }
                    german_content.push_str(&elem.html());
                }
            }
        }

        Ok(Html::parse_fragment(&german_content))
    }

    /// Parse a German verb conjugation table from Wiktionary HTML
    pub fn parse_german_verb_conjugation(
        html: &str,
        verb: &str,
    ) -> anyhow::Result<GermanVerbConjugation> {
        let document = Html::parse_document(html);

        // Extract only the German language section
        let german_section = extract_german_section(&document)?;

        // Parse infinitive (it's the verb itself)
        let infinitive = verb.to_string();

        // Parse present participle
        let present_participle = parse_participle(&german_section, "pres|part")?;

        // Parse past participle
        let past_participle = parse_participle(&german_section, "past|part")?;

        // Parse auxiliary verb
        let auxiliary = parse_auxiliary(&german_section)?;

        // Parse indicative present
        let indicative_present = parse_tense(&german_section, "pres")?;

        // Parse indicative preterite
        let indicative_preterite = parse_tense(&german_section, "pret")?;

        // Parse subjunctive I
        let subjunctive_i = parse_tense(&german_section, "sub:I")?;

        // Parse subjunctive II
        let subjunctive_ii = parse_tense(&german_section, "sub:II")?;

        // Parse imperative
        let imperative = parse_imperative(&german_section)?;

        Ok(GermanVerbConjugation {
            infinitive,
            present_participle,
            past_participle,
            auxiliary,
            indicative_present,
            indicative_preterite,
            subjunctive_i,
            subjunctive_ii,
            imperative,
        })
    }

    fn parse_participle(document: &Html, participle_type: &str) -> anyhow::Result<String> {
        // Look for span with class containing the participle type
        // e.g., "pres|part-form-of" or "past|part-form-of"
        let selector_str = format!("span[class*='{participle_type}-form-of']");
        let selector = Selector::parse(&selector_str).unwrap();

        for element in document.select(&selector) {
            // Get the text from the link inside
            let a_selector = Selector::parse("a").unwrap();
            if let Some(link) = element.select(&a_selector).next()
                && let Some(text) = link.text().next()
            {
                return Ok(text.to_string());
            }
            // Fallback: check if it's a selflink (bold text)
            let strong_selector = Selector::parse("strong.selflink").unwrap();
            if let Some(strong) = element.select(&strong_selector).next()
                && let Some(text) = strong.text().next()
            {
                return Ok(text.to_string());
            }
        }

        anyhow::bail!("Failed to find {} participle", participle_type)
    }

    fn parse_auxiliary(document: &Html) -> anyhow::Result<GermanAuxiliary> {
        // Look for the auxiliary row in the NavHead or table
        // The NavHead contains text like "auxiliary sein" or "auxiliary haben"
        let navhead_selector = Selector::parse("div.NavHead").unwrap();

        for navhead in document.select(&navhead_selector) {
            let text = navhead.text().collect::<String>().to_lowercase();
            if text.contains("auxiliary") {
                if text.contains("sein") {
                    return Ok(GermanAuxiliary::Sein);
                }
                if text.contains("haben") {
                    return Ok(GermanAuxiliary::Haben);
                }
            }
        }

        // Fallback: look in the table itself
        let td_selector = Selector::parse("td").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        for td in document.select(&td_selector) {
            // Check previous sibling for "auxiliary" header
            if let Some(prev) = td.prev_sibling()
                && let Some(prev_elem) = ElementRef::wrap(prev)
            {
                let prev_text = prev_elem.text().collect::<String>().to_lowercase();
                if prev_text.contains("auxiliary") {
                    let td_text = td.text().collect::<String>().to_lowercase();
                    if td_text.contains("sein") {
                        return Ok(GermanAuxiliary::Sein);
                    }
                    if td_text.contains("haben") {
                        return Ok(GermanAuxiliary::Haben);
                    }
                }
            }

            // Also check links in the cell
            for link in td.select(&a_selector) {
                if let Some(href) = link.value().attr("href") {
                    if href.contains("sein#German") {
                        // Check if this is in the auxiliary row by looking at the row
                        if let Some(parent) = td.parent() {
                            let parent_text = ElementRef::wrap(parent)
                                .map(|e| e.text().collect::<String>())
                                .unwrap_or_default()
                                .to_lowercase();
                            if parent_text.contains("auxiliary") {
                                return Ok(GermanAuxiliary::Sein);
                            }
                        }
                    }
                    if href.contains("haben#German")
                        && let Some(parent) = td.parent()
                    {
                        let parent_text = ElementRef::wrap(parent)
                            .map(|e| e.text().collect::<String>())
                            .unwrap_or_default()
                            .to_lowercase();
                        if parent_text.contains("auxiliary") {
                            return Ok(GermanAuxiliary::Haben);
                        }
                    }
                }
            }
        }

        anyhow::bail!("Failed to find auxiliary verb")
    }

    fn parse_tense(document: &Html, tense_marker: &str) -> anyhow::Result<[String; 6]> {
        // German conjugation forms are marked with classes like:
        // "1|s|pres-form-of", "2|s|pres-form-of", "3|s|pres-form-of"
        // "1|p|pres-form-of", "2|p|pres-form-of", "3|p|pres-form-of"

        let persons = ["1", "2", "3"];
        let numbers = ["s", "p"];

        let mut forms = Vec::new();

        for person in &persons {
            for number in &numbers {
                let class_pattern = format!("{person}|{number}|{tense_marker}-form-of");
                let selector_str = format!("span[class*='{class_pattern}']");

                if let Ok(selector) = Selector::parse(&selector_str) {
                    let mut found = false;
                    for element in document.select(&selector) {
                        // Get the text from the link inside, or selflink
                        let a_selector = Selector::parse("a").unwrap();
                        if let Some(link) = element.select(&a_selector).next()
                            && let Some(text) = link.text().next()
                        {
                            forms.push(text.to_string());
                            found = true;
                            break;
                        }
                        // Check for selflink
                        let strong_selector = Selector::parse("strong.selflink").unwrap();
                        if let Some(strong) = element.select(&strong_selector).next()
                            && let Some(text) = strong.text().next()
                        {
                            forms.push(text.to_string());
                            found = true;
                            break;
                        }
                        // Fallback: direct text
                        if !found {
                            let text = element.text().collect::<String>().trim().to_string();
                            if !text.is_empty() {
                                forms.push(text);
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        anyhow::bail!("Failed to find {} {} {} form", person, number, tense_marker);
                    }
                } else {
                    anyhow::bail!(
                        "Invalid selector for {} {} {}",
                        person,
                        number,
                        tense_marker
                    );
                }
            }
        }

        // Reorder from [1s, 1p, 2s, 2p, 3s, 3p] to [1s, 2s, 3s, 1p, 2p, 3p]
        // Actually the loop gives us: 1s, 1p, 2s, 2p, 3s, 3p
        // We want: ich, du, er, wir, ihr, sie = 1s, 2s, 3s, 1p, 2p, 3p
        if forms.len() != 6 {
            anyhow::bail!(
                "Expected 6 forms for {}, found {}",
                tense_marker,
                forms.len()
            );
        }

        Ok([
            forms[0].clone(), // 1s (ich)
            forms[2].clone(), // 2s (du)
            forms[4].clone(), // 3s (er)
            forms[1].clone(), // 1p (wir)
            forms[3].clone(), // 2p (ihr)
            forms[5].clone(), // 3p (sie)
        ])
    }

    fn parse_imperative(document: &Html) -> anyhow::Result<[String; 2]> {
        // Imperative forms are marked with "s|imp-form-of" and "p|imp-form-of"
        let mut forms = Vec::new();

        // Singular imperative (du)
        let s_selector = Selector::parse("span[class*='s|imp-form-of']").unwrap();
        let mut found_s = false;
        for element in document.select(&s_selector) {
            let a_selector = Selector::parse("a").unwrap();
            if let Some(link) = element.select(&a_selector).next()
                && let Some(text) = link.text().next()
            {
                forms.push(text.to_string());
                found_s = true;
                break;
            }
        }
        if !found_s {
            anyhow::bail!("Failed to find singular imperative");
        }

        // Plural imperative (ihr)
        let p_selector = Selector::parse("span[class*='p|imp-form-of']").unwrap();
        let mut found_p = false;
        for element in document.select(&p_selector) {
            let a_selector = Selector::parse("a").unwrap();
            if let Some(link) = element.select(&a_selector).next()
                && let Some(text) = link.text().next()
            {
                forms.push(text.to_string());
                found_p = true;
                break;
            }
        }
        if !found_p {
            anyhow::bail!("Failed to find plural imperative");
        }

        Ok([forms[0].clone(), forms[1].clone()])
    }

    /// Fetch German verb conjugations from Wiktionary with HTML caching
    pub async fn fetch_german_verb_conjugations(
        verbs: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, GermanVerbConjugation>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(verbs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} German verbs ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<GermanVerbConjugation, String>)> =
            futures::stream::iter(verbs.iter())
                .map(|verb| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(verb.to_string());

                        let result = match super::get_wiktionary_html(verb, cache_dir).await {
                            Ok(html) => parse_german_verb_conjugation(&html, verb)
                                .map_err(|e| format!("Failed to parse German verb '{verb}': {e}")),
                            Err(e) => {
                                Err(format!("Failed to get HTML for German verb '{verb}': {e}"))
                            }
                        };

                        pb.inc(1);
                        (verb.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (verb, result) in fetch_results {
            match result {
                Ok(conjugation) => {
                    results.insert(verb, conjugation);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            verbs.len(),
            errors.len()
        ));

        Ok(results)
    }

    // =====================================
    // German Noun Declension
    // =====================================

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct GermanNounDeclension {
        pub lemma: String,
        pub gender: GermanGender,

        // Singular forms (nominative, genitive, dative, accusative)
        pub nominative_singular: String,
        pub genitive_singular: String,
        pub dative_singular: String,
        pub accusative_singular: String,

        // Plural forms (nominative, genitive, dative, accusative)
        // These are Option because some nouns are uncountable (sg-only)
        pub nominative_plural: Option<String>,
        pub genitive_plural: Option<String>,
        pub dative_plural: Option<String>,
        pub accusative_plural: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum GermanGender {
        Masculine,
        Feminine,
        Neuter,
    }

    /// Parse a German noun declension table from Wiktionary HTML
    pub fn parse_german_noun_declension(
        html: &str,
        noun: &str,
    ) -> anyhow::Result<GermanNounDeclension> {
        let document = Html::parse_document(html);

        // Extract only the German language section
        let german_section = extract_german_section(&document)?;

        let lemma = noun.to_string();

        // Parse gender from NavHead
        let gender = parse_noun_gender(&german_section)?;

        // Parse singular forms (required)
        let nominative_singular = parse_noun_form(&german_section, "nom", "s")?;
        let genitive_singular = parse_noun_form(&german_section, "gen", "s")?;
        let dative_singular = parse_noun_form(&german_section, "dat", "s")?;
        let accusative_singular = parse_noun_form(&german_section, "acc", "s")?;

        // Parse plural forms (optional - some nouns are uncountable/sg-only)
        let nominative_plural = parse_noun_form(&german_section, "nom", "p").ok();
        let genitive_plural = parse_noun_form(&german_section, "gen", "p").ok();
        let dative_plural = parse_noun_form(&german_section, "dat", "p").ok();
        let accusative_plural = parse_noun_form(&german_section, "acc", "p").ok();

        Ok(GermanNounDeclension {
            lemma,
            gender,
            nominative_singular,
            genitive_singular,
            dative_singular,
            accusative_singular,
            nominative_plural,
            genitive_plural,
            dative_plural,
            accusative_plural,
        })
    }

    fn parse_noun_gender(document: &Html) -> anyhow::Result<GermanGender> {
        // Look for the NavHead which contains gender info like:
        // "Declension of Frau [feminine]"
        // "Declension of Mann [masculine, strong // mixed]"
        // "Declension of Kind [neuter, strong // mixed]"
        let navhead_selector = Selector::parse("div.NavHead").unwrap();

        for navhead in document.select(&navhead_selector) {
            let text = navhead.text().collect::<String>().to_lowercase();
            if text.contains("declension") {
                if text.contains("feminine") {
                    return Ok(GermanGender::Feminine);
                }
                if text.contains("masculine") {
                    return Ok(GermanGender::Masculine);
                }
                if text.contains("neuter") {
                    return Ok(GermanGender::Neuter);
                }
            }
        }

        anyhow::bail!("Failed to find noun gender")
    }

    fn parse_noun_form(document: &Html, case: &str, number: &str) -> anyhow::Result<String> {
        // German noun forms use classes like:
        // "nom|s-form-of", "gen|s-form-of", "dat|s-form-of", "acc|s-form-of"
        // "nom|p-form-of", "gen|p-form-of", "dat|p-form-of", "acc|p-form-of"
        let class_pattern = format!("{case}|{number}-form-of");
        let selector_str = format!("span[class*='{class_pattern}']");

        if let Ok(selector) = Selector::parse(&selector_str) {
            for element in document.select(&selector) {
                // Get the text from the link inside
                let a_selector = Selector::parse("a").unwrap();
                if let Some(link) = element.select(&a_selector).next()
                    && let Some(text) = link.text().next()
                {
                    return Ok(text.to_string());
                }
                // Check for selflink (bold text)
                let strong_selector = Selector::parse("strong.selflink").unwrap();
                if let Some(strong) = element.select(&strong_selector).next()
                    && let Some(text) = strong.text().next()
                {
                    return Ok(text.to_string());
                }
            }
        }

        anyhow::bail!("Failed to find {} {} form", case, number)
    }

    /// Fetch German noun declensions from Wiktionary with HTML caching
    pub async fn fetch_german_noun_declensions(
        nouns: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, GermanNounDeclension>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(nouns.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} German nouns ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<GermanNounDeclension, String>)> =
            futures::stream::iter(nouns.iter())
                .map(|noun| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(noun.to_string());

                        let result = match super::get_wiktionary_html(noun, cache_dir).await {
                            Ok(html) => parse_german_noun_declension(&html, noun)
                                .map_err(|e| format!("Failed to parse German noun '{noun}': {e}")),
                            Err(e) => {
                                Err(format!("Failed to get HTML for German noun '{noun}': {e}"))
                            }
                        };

                        pb.inc(1);
                        (noun.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (noun, result) in fetch_results {
            match result {
                Ok(declension) => {
                    results.insert(noun, declension);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            nouns.len(),
            errors.len()
        ));

        Ok(results)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_parse_gehen() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/gehen.txt")
                .expect("Failed to read gehen.txt");

            let conjugation = parse_german_verb_conjugation(&html, "gehen")
                .expect("Failed to parse gehen conjugation");

            assert_eq!(conjugation.infinitive, "gehen");
            assert_eq!(conjugation.present_participle, "gehend");
            assert_eq!(conjugation.past_participle, "gegangen");
            assert_eq!(conjugation.auxiliary, GermanAuxiliary::Sein);

            // Check indicative present (ich, du, er, wir, ihr, sie)
            assert_eq!(conjugation.indicative_present[0], "gehe"); // ich
            assert_eq!(conjugation.indicative_present[1], "gehst"); // du
            assert_eq!(conjugation.indicative_present[2], "geht"); // er
            assert_eq!(conjugation.indicative_present[3], "gehen"); // wir
            assert_eq!(conjugation.indicative_present[4], "geht"); // ihr
            assert_eq!(conjugation.indicative_present[5], "gehen"); // sie

            // Check preterite
            assert_eq!(conjugation.indicative_preterite[0], "ging"); // ich
            assert_eq!(conjugation.indicative_preterite[1], "gingst"); // du

            // Check imperative
            assert_eq!(conjugation.imperative[0], "geh"); // du
            assert_eq!(conjugation.imperative[1], "geht"); // ihr
        }

        #[test]
        fn test_parse_haben() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/haben.txt")
                .expect("Failed to read haben.txt");

            let conjugation = parse_german_verb_conjugation(&html, "haben")
                .expect("Failed to parse haben conjugation");

            assert_eq!(conjugation.infinitive, "haben");
            assert_eq!(conjugation.auxiliary, GermanAuxiliary::Haben);

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "habe"); // ich
            assert_eq!(conjugation.indicative_present[1], "hast"); // du
            assert_eq!(conjugation.indicative_present[2], "hat"); // er
        }

        #[test]
        fn test_parse_sein() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/sein.txt")
                .expect("Failed to read sein.txt");

            let conjugation = parse_german_verb_conjugation(&html, "sein")
                .expect("Failed to parse sein conjugation");

            assert_eq!(conjugation.infinitive, "sein");
            assert_eq!(conjugation.auxiliary, GermanAuxiliary::Sein);

            // Check indicative present (highly irregular)
            assert_eq!(conjugation.indicative_present[0], "bin"); // ich
            assert_eq!(conjugation.indicative_present[1], "bist"); // du
            assert_eq!(conjugation.indicative_present[2], "ist"); // er
            assert_eq!(conjugation.indicative_present[3], "sind"); // wir
            assert_eq!(conjugation.indicative_present[4], "seid"); // ihr
            assert_eq!(conjugation.indicative_present[5], "sind"); // sie
        }

        #[test]
        fn test_parse_trinken() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/trinken.txt")
                .expect("Failed to read trinken.txt");

            let conjugation = parse_german_verb_conjugation(&html, "trinken")
                .expect("Failed to parse trinken conjugation");

            assert_eq!(conjugation.infinitive, "trinken");
            assert_eq!(conjugation.auxiliary, GermanAuxiliary::Haben);
            assert_eq!(conjugation.past_participle, "getrunken");
        }

        // Noun declension tests

        #[test]
        fn test_parse_mann() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/Mann.txt")
                .expect("Failed to read Mann.txt");

            let declension = parse_german_noun_declension(&html, "Mann")
                .expect("Failed to parse Mann declension");

            assert_eq!(declension.lemma, "Mann");
            assert_eq!(declension.gender, GermanGender::Masculine);
            assert_eq!(declension.nominative_singular, "Mann");
            assert_eq!(declension.nominative_plural, Some("Männer".to_string()));
            assert_eq!(declension.dative_plural, Some("Männern".to_string()));
        }

        #[test]
        fn test_parse_frau() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/Frau.txt")
                .expect("Failed to read Frau.txt");

            let declension = parse_german_noun_declension(&html, "Frau")
                .expect("Failed to parse Frau declension");

            assert_eq!(declension.lemma, "Frau");
            assert_eq!(declension.gender, GermanGender::Feminine);
            assert_eq!(declension.nominative_singular, "Frau");
            assert_eq!(declension.nominative_plural, Some("Frauen".to_string()));
        }

        #[test]
        fn test_parse_kind() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/Kind.txt")
                .expect("Failed to read Kind.txt");

            let declension = parse_german_noun_declension(&html, "Kind")
                .expect("Failed to parse Kind declension");

            assert_eq!(declension.lemma, "Kind");
            assert_eq!(declension.gender, GermanGender::Neuter);
            assert_eq!(declension.nominative_singular, "Kind");
            assert_eq!(declension.nominative_plural, Some("Kinder".to_string()));
        }

        #[test]
        fn test_parse_hund() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/Hund.txt")
                .expect("Failed to read Hund.txt");

            let declension = parse_german_noun_declension(&html, "Hund")
                .expect("Failed to parse Hund declension");

            assert_eq!(declension.lemma, "Hund");
            assert_eq!(declension.gender, GermanGender::Masculine);
            assert_eq!(declension.nominative_singular, "Hund");
            assert_eq!(declension.nominative_plural, Some("Hunde".to_string()));
        }

        #[test]
        fn test_parse_kreativitaet() {
            let html = fs::read_to_string("src/wiktionary-examples/deu/Kreativität.txt")
                .expect("Failed to read Kreativität.txt");

            let declension = parse_german_noun_declension(&html, "Kreativität")
                .expect("Failed to parse Kreativität declension");

            assert_eq!(declension.lemma, "Kreativität");
            assert_eq!(declension.gender, GermanGender::Feminine);
            assert_eq!(declension.nominative_singular, "Kreativität");
            assert_eq!(declension.genitive_singular, "Kreativität");
            assert_eq!(declension.dative_singular, "Kreativität");
            assert_eq!(declension.accusative_singular, "Kreativität");
            // Uncountable noun - no plural forms
            assert_eq!(declension.nominative_plural, None);
            assert_eq!(declension.genitive_plural, None);
            assert_eq!(declension.dative_plural, None);
            assert_eq!(declension.accusative_plural, None);
        }
    }
}

pub mod portuguese {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct PortugueseVerbConjugation {
        pub infinitive: String,
        pub gerund: String,
        pub past_participle: String,

        // Indicative mood - simple tenses (6 forms each: eu, tu, ele/você, nós, vós, eles/vocês)
        pub indicative_present: [String; 6],
        pub indicative_imperfect: [String; 6],
        pub indicative_preterite: [String; 6],
        pub indicative_pluperfect: [String; 6],
        pub indicative_future: [String; 6],
        pub indicative_conditional: [String; 6],

        // Subjunctive mood - simple tenses (6 forms each)
        pub subjunctive_present: [String; 6],
        pub subjunctive_imperfect: [String; 6],
        pub subjunctive_future: [String; 6],

        // Imperative mood - affirmative (5 forms: tu, você, nós, vós, vocês)
        pub imperative_affirmative: [String; 5],
    }

    /// Extract the Portuguese language section from a Wiktionary page
    fn extract_portuguese_section(document: &Html) -> anyhow::Result<Html> {
        // Find the h2 heading with id="Portuguese"
        let h2_selector = Selector::parse("h2#Portuguese").unwrap();

        let portuguese_heading = document
            .select(&h2_selector)
            .next()
            .context("Could not find Portuguese language section")?;

        // Collect all content until the next h2 (language section)
        let mut portuguese_content = String::new();
        let mut current = portuguese_heading.parent();

        while let Some(node) = current {
            current = node.next_sibling();
            if let Some(current_node) = current {
                // Stop if we hit another h2 (next language section)
                if let Some(elem) = ElementRef::wrap(current_node) {
                    if elem.value().name() == "div"
                        && let Some(first_child) = elem.first_child()
                        && let Some(child_elem) = ElementRef::wrap(first_child)
                        && child_elem.value().name() == "h2"
                    {
                        break;
                    }
                    portuguese_content.push_str(&elem.html());
                }
            }
        }

        Ok(Html::parse_fragment(&portuguese_content))
    }

    /// Parse a Portuguese verb conjugation table from Wiktionary HTML
    pub fn parse_portuguese_verb_conjugation(
        html: &str,
        verb: &str,
    ) -> anyhow::Result<PortugueseVerbConjugation> {
        let document = Html::parse_document(html);

        // Extract only the Portuguese language section
        let portuguese_section = extract_portuguese_section(&document)?;

        // Parse infinitive (it's the verb itself)
        let infinitive = verb.to_string();

        // Parse gerund
        let gerund = parse_gerund(&portuguese_section)?;

        // Parse past participle
        let past_participle = parse_past_participle(&portuguese_section)?;

        // Parse indicative tenses
        let indicative_present = parse_tense(&portuguese_section, "present", "indicative")?;
        let indicative_imperfect = parse_tense(&portuguese_section, "imperfect", "indicative")?;
        let indicative_preterite = parse_tense(&portuguese_section, "preterite", "indicative")?;
        let indicative_pluperfect = parse_tense(&portuguese_section, "pluperfect", "indicative")?;
        let indicative_future = parse_tense(&portuguese_section, "future", "indicative")?;
        let indicative_conditional = parse_tense(&portuguese_section, "conditional", "indicative")?;

        // Parse subjunctive tenses
        let subjunctive_present = parse_tense(&portuguese_section, "present", "subjunctive")?;
        let subjunctive_imperfect = parse_tense(&portuguese_section, "imperfect", "subjunctive")?;
        let subjunctive_future = parse_tense(&portuguese_section, "future", "subjunctive")?;

        // Parse imperative
        let imperative_affirmative = parse_imperative(&portuguese_section)?;

        Ok(PortugueseVerbConjugation {
            infinitive,
            gerund,
            past_participle,
            indicative_present,
            indicative_imperfect,
            indicative_preterite,
            indicative_pluperfect,
            indicative_future,
            indicative_conditional,
            subjunctive_present,
            subjunctive_imperfect,
            subjunctive_future,
            imperative_affirmative,
        })
    }

    fn parse_gerund(document: &Html) -> anyhow::Result<String> {
        // Look for the gerund row in the table
        // Format: <span class="Latn form-of lang-pt gerund-...">
        let selector =
            Selector::parse("span.gerund-ser-form-of a, span[class*='gerund'] a").unwrap();

        document
            .select(&selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find gerund")
    }

    fn parse_past_participle(document: &Html) -> anyhow::Result<String> {
        // Look for the past participle row (masculine singular form)
        // Portuguese only uses one form of past participle (unlike Spanish)
        let selector =
            Selector::parse("span[class*='pp'][class*='ms'] a, span.pp￰ms-form-of a").unwrap();

        document
            .select(&selector)
            .next()
            .and_then(|el| el.text().next())
            .map(|s| s.to_string())
            .context("Failed to find past participle")
    }

    fn parse_tense(document: &Html, tense: &str, mood: &str) -> anyhow::Result<[String; 6]> {
        // Portuguese uses roa-indicative-left-rail and roa-subjunctive-left-rail for tense headers
        let th_selector = match mood {
            "indicative" => Selector::parse("th.roa-indicative-left-rail").unwrap(),
            "subjunctive" => Selector::parse("th.roa-subjunctive-left-rail").unwrap(),
            _ => anyhow::bail!("Unknown mood: {}", mood),
        };
        let a_selector = Selector::parse("a").unwrap();

        // Find the header for this tense
        let mut tense_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains(&tense.to_lowercase()) {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    tense_row_ref = Some(parent);
                    break;
                }
            }
        }

        let tense_row =
            tense_row_ref.context(format!("Failed to find tense row for {mood} {tense}"))?;

        // Extract the 6 conjugated forms from the td elements in this row
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in tense_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                // Find the link inside the td
                let td_elem = scraper::ElementRef::wrap(child).unwrap();
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(text) = link.text().next()
                {
                    forms.push(text.to_string());
                }
            }
        }

        if forms.len() != 6 {
            anyhow::bail!(
                "Expected 6 forms for {} {}, found {}",
                mood,
                tense,
                forms.len()
            );
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
            forms[5].clone(),
        ])
    }

    fn parse_imperative(document: &Html) -> anyhow::Result<[String; 5]> {
        // Find the affirmative imperative row
        let th_selector = Selector::parse("th.roa-imperative-left-rail").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        let mut imperative_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains("affirmative") {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    imperative_row_ref = Some(parent);
                    break;
                }
            }
        }

        let imperative_row =
            imperative_row_ref.context("Failed to find imperative affirmative row")?;

        // Extract the 5 forms (skip the first td which is empty for "eu")
        // Forms are: tu, você, nós, vós, vocês
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in imperative_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                let td_elem = scraper::ElementRef::wrap(child).unwrap();

                // Check if this is an empty cell or rowspan cell (for "eu" form)
                let text = td_elem.text().collect::<String>().trim().to_string();
                if text.is_empty() || td_elem.value().attr("rowspan").is_some() {
                    continue;
                }

                // Extract the link text (the conjugated form)
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(form_text) = link.text().next()
                {
                    forms.push(form_text.to_string());
                }
            }
        }

        if forms.len() != 5 {
            anyhow::bail!("Expected 5 forms for imperative, found {}", forms.len());
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
        ])
    }

    /// Fetch Portuguese verb conjugations from Wiktionary with HTML caching
    pub async fn fetch_portuguese_verb_conjugations(
        verbs: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, PortugueseVerbConjugation>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(verbs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} Portuguese verbs ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<PortugueseVerbConjugation, String>)> =
            futures::stream::iter(verbs.iter())
                .map(|verb| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(verb.to_string());

                        let result = match super::get_wiktionary_html(verb, cache_dir).await {
                            Ok(html) => {
                                parse_portuguese_verb_conjugation(&html, verb).map_err(|e| {
                                    format!("Failed to parse Portuguese verb '{verb}': {e}")
                                })
                            }
                            Err(e) => Err(format!(
                                "Failed to get HTML for Portuguese verb '{verb}': {e}"
                            )),
                        };

                        pb.inc(1);
                        (verb.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (verb, result) in fetch_results {
            match result {
                Ok(conjugation) => {
                    results.insert(verb, conjugation);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            verbs.len(),
            errors.len()
        ));

        Ok(results)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_parse_ser() {
            let html = fs::read_to_string("src/wiktionary-examples/por/ser.txt")
                .expect("Failed to read ser.txt");

            let conjugation = parse_portuguese_verb_conjugation(&html, "ser")
                .expect("Failed to parse ser conjugation");

            assert_eq!(conjugation.infinitive, "ser");
            assert_eq!(conjugation.gerund, "sendo");
            assert_eq!(conjugation.past_participle, "sido");

            // Check indicative present (eu, tu, ele, nós, vós, eles)
            assert_eq!(conjugation.indicative_present[0], "sou"); // eu
            assert_eq!(conjugation.indicative_present[1], "és"); // tu
            assert_eq!(conjugation.indicative_present[2], "é"); // ele/você
            assert_eq!(conjugation.indicative_present[3], "somos"); // nós
            assert_eq!(conjugation.indicative_present[4], "sois"); // vós
            assert_eq!(conjugation.indicative_present[5], "são"); // eles/vocês

            // Check preterite (irregular)
            assert_eq!(conjugation.indicative_preterite[0], "fui");
            assert_eq!(conjugation.indicative_preterite[1], "foste");
            assert_eq!(conjugation.indicative_preterite[2], "foi");

            // Check imperative affirmative (tu, você, nós, vós, vocês)
            assert_eq!(conjugation.imperative_affirmative[0], "sê"); // tu
            assert_eq!(conjugation.imperative_affirmative[1], "seja"); // você
            assert_eq!(conjugation.imperative_affirmative[2], "sejamos"); // nós
            assert_eq!(conjugation.imperative_affirmative[3], "sede"); // vós
            assert_eq!(conjugation.imperative_affirmative[4], "sejam"); // vocês
        }

        #[test]
        fn test_parse_ter() {
            let html = fs::read_to_string("src/wiktionary-examples/por/ter.txt")
                .expect("Failed to read ter.txt");

            let conjugation = parse_portuguese_verb_conjugation(&html, "ter")
                .expect("Failed to parse ter conjugation");

            assert_eq!(conjugation.infinitive, "ter");
            assert_eq!(conjugation.gerund, "tendo");
            assert_eq!(conjugation.past_participle, "tido");

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "tenho"); // eu
            assert_eq!(conjugation.indicative_present[1], "tens"); // tu
            assert_eq!(conjugation.indicative_present[2], "tem"); // ele
        }

        #[test]
        fn test_parse_fazer() {
            let html = fs::read_to_string("src/wiktionary-examples/por/fazer.txt")
                .expect("Failed to read fazer.txt");

            let conjugation = parse_portuguese_verb_conjugation(&html, "fazer")
                .expect("Failed to parse fazer conjugation");

            assert_eq!(conjugation.infinitive, "fazer");
        }

        #[test]
        fn test_parse_ir() {
            let html = fs::read_to_string("src/wiktionary-examples/por/ir.txt")
                .expect("Failed to read ir.txt");

            let conjugation = parse_portuguese_verb_conjugation(&html, "ir")
                .expect("Failed to parse ir conjugation");

            assert_eq!(conjugation.infinitive, "ir");
        }
    }
}

pub mod italian {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct ItalianVerbConjugation {
        pub infinitive: String,
        pub gerund: String,
        pub past_participle: String,

        // Indicative mood - simple tenses (6 forms each: io, tu, lui/lei, noi, voi, loro)
        pub indicative_present: [String; 6],
        pub indicative_imperfect: [String; 6],
        pub indicative_past_historic: [String; 6],
        pub indicative_future: [String; 6],
        pub indicative_conditional: [String; 6],

        // Subjunctive mood - simple tenses (6 forms each)
        pub subjunctive_present: [String; 6],
        pub subjunctive_imperfect: [String; 6],

        // Imperative mood (5 forms: tu, Lei, noi, voi, Loro)
        pub imperative: [String; 5],
    }

    /// Extract the Italian language section from a Wiktionary page
    fn extract_italian_section(document: &Html) -> anyhow::Result<Html> {
        // Find the h2 heading with id="Italian"
        let h2_selector = Selector::parse("h2#Italian").unwrap();

        let italian_heading = document
            .select(&h2_selector)
            .next()
            .context("Could not find Italian language section")?;

        // Collect all content until the next h2 (language section)
        let mut italian_content = String::new();
        let mut current = italian_heading.parent();

        while let Some(node) = current {
            current = node.next_sibling();
            if let Some(current_node) = current {
                // Stop if we hit another h2 (next language section)
                if let Some(elem) = ElementRef::wrap(current_node) {
                    if elem.value().name() == "div"
                        && let Some(first_child) = elem.first_child()
                        && let Some(child_elem) = ElementRef::wrap(first_child)
                        && child_elem.value().name() == "h2"
                    {
                        break;
                    }
                    italian_content.push_str(&elem.html());
                }
            }
        }

        Ok(Html::parse_fragment(&italian_content))
    }

    /// Parse an Italian verb conjugation table from Wiktionary HTML
    pub fn parse_italian_verb_conjugation(
        html: &str,
        verb: &str,
    ) -> anyhow::Result<ItalianVerbConjugation> {
        let document = Html::parse_document(html);

        // Extract only the Italian language section
        let italian_section = extract_italian_section(&document)?;

        // Parse infinitive (it's the verb itself)
        let infinitive = verb.to_string();

        // Parse gerund
        let gerund = parse_gerund(&italian_section)?;

        // Parse past participle
        let past_participle = parse_past_participle(&italian_section)?;

        // Parse indicative tenses
        let indicative_present = parse_tense(&italian_section, "present", "indicative")?;
        let indicative_imperfect = parse_tense(&italian_section, "imperfect", "indicative")?;
        let indicative_past_historic =
            parse_tense(&italian_section, "past historic", "indicative")?;
        let indicative_future = parse_tense(&italian_section, "future", "indicative")?;
        let indicative_conditional = parse_tense(&italian_section, "conditional", "indicative")?;

        // Parse subjunctive tenses
        let subjunctive_present = parse_tense(&italian_section, "present", "subjunctive")?;
        let subjunctive_imperfect = parse_tense(&italian_section, "imperfect", "subjunctive")?;

        // Parse imperative
        let imperative = parse_imperative(&italian_section)?;

        Ok(ItalianVerbConjugation {
            infinitive,
            gerund,
            past_participle,
            indicative_present,
            indicative_imperfect,
            indicative_past_historic,
            indicative_future,
            indicative_conditional,
            subjunctive_present,
            subjunctive_imperfect,
            imperative,
        })
    }

    fn parse_gerund(document: &Html) -> anyhow::Result<String> {
        // Look for the gerund row in the table
        // Format: <th>gerund</th> followed by <td> with links
        let th_selector = Selector::parse("th.roa-nonfinite-header").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains("gerund") {
                // Get the next td sibling (skip text nodes)
                let mut current = th.next_sibling();
                while let Some(node) = current {
                    if let Some(elem) = ElementRef::wrap(node)
                        && elem.value().name() == "td"
                    {
                        // Get first link - extract word from href to get actual spelling
                        if let Some(link) = elem.select(&a_selector).next()
                            && let Some(href) = link.value().attr("href")
                        {
                            // href is like "/wiki/essendo#Italian"
                            if let Some(word) = href.strip_prefix("/wiki/")
                                && let Some(word_clean) = word.split('#').next()
                            {
                                // URL decode in case of special characters (like %C3%A8 → è)
                                let decoded = percent_encoding::percent_decode_str(word_clean)
                                    .decode_utf8_lossy()
                                    .to_string();
                                return Ok(decoded);
                            }
                        }
                    }
                    current = node.next_sibling();
                }
            }
        }

        anyhow::bail!("Failed to find gerund")
    }

    fn parse_past_participle(document: &Html) -> anyhow::Result<String> {
        // Look for the past participle row
        let th_selector = Selector::parse("th.roa-nonfinite-header").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains("past participle") {
                // Get the next td sibling (skip text nodes)
                let mut current = th.next_sibling();
                while let Some(node) = current {
                    if let Some(elem) = ElementRef::wrap(node)
                        && elem.value().name() == "td"
                    {
                        // Get first link - extract word from href to get actual spelling
                        if let Some(link) = elem.select(&a_selector).next()
                            && let Some(href) = link.value().attr("href")
                        {
                            // href is like "/wiki/stato#Italian"
                            if let Some(word) = href.strip_prefix("/wiki/")
                                && let Some(word_clean) = word.split('#').next()
                            {
                                // URL decode in case of special characters (like %C3%A8 → è)
                                let decoded = percent_encoding::percent_decode_str(word_clean)
                                    .decode_utf8_lossy()
                                    .to_string();
                                return Ok(decoded);
                            }
                        }
                    }
                    current = node.next_sibling();
                }
            }
        }

        anyhow::bail!("Failed to find past participle")
    }

    fn parse_tense(document: &Html, tense: &str, mood: &str) -> anyhow::Result<[String; 6]> {
        // Italian uses roa-indicative-left-rail and roa-subjunctive-left-rail for tense headers
        let th_selector = match mood {
            "indicative" => Selector::parse("th.roa-indicative-left-rail").unwrap(),
            "subjunctive" => Selector::parse("th.roa-subjunctive-left-rail").unwrap(),
            _ => anyhow::bail!("Unknown mood: {}", mood),
        };
        let a_selector = Selector::parse("a").unwrap();

        // Find the header for this tense
        let mut tense_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            if text.contains(&tense.to_lowercase()) {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    tense_row_ref = Some(parent);
                    break;
                }
            }
        }

        let tense_row =
            tense_row_ref.context(format!("Failed to find tense row for {mood} {tense}"))?;

        // Extract the 6 conjugated forms from the td elements in this row
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in tense_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                let td_elem = scraper::ElementRef::wrap(child).unwrap();

                // Get the first link - extract word from href to get actual spelling
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(href) = link.value().attr("href")
                {
                    // href is like "/wiki/sono#Italian"
                    if let Some(word) = href.strip_prefix("/wiki/")
                        && let Some(word_clean) = word.split('#').next()
                    {
                        // URL decode in case of special characters (like %C3%A8 → è)
                        let decoded = percent_encoding::percent_decode_str(word_clean)
                            .decode_utf8_lossy()
                            .to_string();
                        forms.push(decoded);
                    }
                }
            }
        }

        if forms.len() != 6 {
            anyhow::bail!(
                "Expected 6 forms for {} {}, found {}",
                mood,
                tense,
                forms.len()
            );
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
            forms[5].clone(),
        ])
    }

    fn parse_imperative(document: &Html) -> anyhow::Result<[String; 5]> {
        // Find the imperative row (not the negative imperative)
        let th_selector = Selector::parse("th.roa-imperative-left-rail").unwrap();
        let a_selector = Selector::parse("a").unwrap();

        let mut imperative_row_ref = None;
        for th in document.select(&th_selector) {
            let text = th.text().collect::<String>().to_lowercase();
            // Look for "imperative" but not "negative imperative"
            if text.contains("imperative") && !text.contains("negative") {
                // Get the parent tr element
                if let Some(parent) = th.parent()
                    && parent.value().as_element().map(|e| e.name()) == Some("tr")
                {
                    // Skip text nodes to find the next tr sibling
                    let mut current = parent.next_sibling();
                    while let Some(node) = current {
                        if let Some(next_elem) = ElementRef::wrap(node)
                            && next_elem.value().name() == "tr"
                        {
                            imperative_row_ref = Some(next_elem);
                            break;
                        }
                        current = node.next_sibling();
                    }
                    if imperative_row_ref.is_some() {
                        break;
                    }
                }
            }
        }

        let imperative_row = imperative_row_ref.context("Failed to find imperative row")?;

        // Extract the 5 forms (skip the first td which is empty)
        // Forms are: tu, Lei, noi, voi, Loro
        let mut forms = Vec::new();

        // Iterate through children of the tr element
        for child in imperative_row.children() {
            if let Some(element) = child.value().as_element()
                && element.name() == "td"
            {
                let td_elem = scraper::ElementRef::wrap(child).unwrap();

                // Check if this is an empty cell
                let text = td_elem.text().collect::<String>().trim().to_string();
                if text.is_empty() {
                    continue;
                }

                // Extract from href to get actual spelling (not pronunciation marks)
                if let Some(link) = td_elem.select(&a_selector).next()
                    && let Some(href) = link.value().attr("href")
                {
                    // href is like "/wiki/sii#Italian"
                    if let Some(word) = href.strip_prefix("/wiki/")
                        && let Some(word_clean) = word.split('#').next()
                    {
                        // URL decode in case of special characters (like %C3%A8 → è)
                        let decoded = percent_encoding::percent_decode_str(word_clean)
                            .decode_utf8_lossy()
                            .to_string();
                        forms.push(decoded);
                    }
                }
            }
        }

        if forms.len() != 5 {
            anyhow::bail!("Expected 5 forms for imperative, found {}", forms.len());
        }

        Ok([
            forms[0].clone(),
            forms[1].clone(),
            forms[2].clone(),
            forms[3].clone(),
            forms[4].clone(),
        ])
    }

    /// Fetch Italian verb conjugations from Wiktionary with HTML caching
    pub async fn fetch_italian_verb_conjugations(
        verbs: &[String],
        cache_dir: &Path,
    ) -> anyhow::Result<HashMap<String, ItalianVerbConjugation>> {
        use futures::StreamExt;

        let pb = indicatif::ProgressBar::new(verbs.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} Italian verbs ({per_sec}, {msg}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let fetch_results: Vec<(String, Result<ItalianVerbConjugation, String>)> =
            futures::stream::iter(verbs.iter())
                .map(|verb| {
                    let pb = pb.clone();
                    async move {
                        pb.set_message(verb.to_string());

                        let result = match super::get_wiktionary_html(verb, cache_dir).await {
                            Ok(html) => parse_italian_verb_conjugation(&html, verb)
                                .map_err(|e| format!("Failed to parse Italian verb '{verb}': {e}")),
                            Err(e) => {
                                Err(format!("Failed to get HTML for Italian verb '{verb}': {e}"))
                            }
                        };

                        pb.inc(1);
                        (verb.clone(), result)
                    }
                })
                .buffered(50)
                .collect()
                .await;

        // Process results
        let mut results = HashMap::new();
        let mut errors = Vec::new();

        for (verb, result) in fetch_results {
            match result {
                Ok(conjugation) => {
                    results.insert(verb, conjugation);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        pb.finish_with_message(format!(
            "Finished: {}/{} parsed ({} errors)",
            results.len(),
            verbs.len(),
            errors.len()
        ));

        Ok(results)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        #[test]
        fn test_parse_essere() {
            let html = fs::read_to_string("src/wiktionary-examples/ita/essere.txt")
                .expect("Failed to read essere.txt");

            let conjugation = parse_italian_verb_conjugation(&html, "essere")
                .expect("Failed to parse essere conjugation");

            assert_eq!(conjugation.infinitive, "essere");
            assert_eq!(conjugation.gerund, "essendo");
            assert_eq!(conjugation.past_participle, "stato");

            // Check indicative present (io, tu, lui/lei, noi, voi, loro)
            assert_eq!(conjugation.indicative_present[0], "sono"); // io
            assert_eq!(conjugation.indicative_present[1], "sei"); // tu
            assert_eq!(conjugation.indicative_present[2], "è"); // lui/lei
            assert_eq!(conjugation.indicative_present[3], "siamo"); // noi
            assert_eq!(conjugation.indicative_present[4], "siete"); // voi
            assert_eq!(conjugation.indicative_present[5], "sono"); // loro

            // Check past historic (irregular)
            assert_eq!(conjugation.indicative_past_historic[0], "fui");
            assert_eq!(conjugation.indicative_past_historic[1], "fosti");
            assert_eq!(conjugation.indicative_past_historic[2], "fu");

            // Check imperative (tu, Lei, noi, voi, Loro)
            assert_eq!(conjugation.imperative[0], "sii"); // tu
            assert_eq!(conjugation.imperative[1], "sia"); // Lei
            assert_eq!(conjugation.imperative[2], "siamo"); // noi
            assert_eq!(conjugation.imperative[3], "siate"); // voi
            assert_eq!(conjugation.imperative[4], "siano"); // Loro
        }

        #[test]
        fn test_parse_avere() {
            let html = fs::read_to_string("src/wiktionary-examples/ita/avere.txt")
                .expect("Failed to read avere.txt");

            let conjugation = parse_italian_verb_conjugation(&html, "avere")
                .expect("Failed to parse avere conjugation");

            assert_eq!(conjugation.infinitive, "avere");
            assert_eq!(conjugation.gerund, "avendo");
            assert_eq!(conjugation.past_participle, "avuto");

            // Check indicative present
            assert_eq!(conjugation.indicative_present[0], "ho"); // io
            assert_eq!(conjugation.indicative_present[1], "hai"); // tu
            assert_eq!(conjugation.indicative_present[2], "ha"); // lui/lei
        }

        #[test]
        fn test_parse_fare() {
            let html = fs::read_to_string("src/wiktionary-examples/ita/fare.txt")
                .expect("Failed to read fare.txt");

            let conjugation = parse_italian_verb_conjugation(&html, "fare")
                .expect("Failed to parse fare conjugation");

            assert_eq!(conjugation.infinitive, "fare");
        }

        #[test]
        fn test_parse_andare() {
            let html = fs::read_to_string("src/wiktionary-examples/ita/andare.txt")
                .expect("Failed to read andare.txt");

            let conjugation = parse_italian_verb_conjugation(&html, "andare")
                .expect("Failed to parse andare conjugation");

            assert_eq!(conjugation.infinitive, "andare");
        }
    }
}

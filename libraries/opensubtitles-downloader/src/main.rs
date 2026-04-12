use anyhow::{anyhow, Context, Result};
use clap::Parser;
use language_utils::{Language, MovieMetadataBasic};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use tysm::chat_completions::ChatClient;

static QUALITY_CHECK_CLIENT: LazyLock<ChatClient> = LazyLock::new(|| {
    ChatClient::from_env("gpt-5.4")
        .unwrap()
        .with_cache_directory("./.cache")
        .with_service_tier("flex")
});

/// Fetch an image from a URL and return the bytes
async fn fetch_image_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

const EXTRA_MOVIES: &[&str] = &[
    // English classics & blockbusters
    "tt6751668",  // Parasite
    "tt20215234", // Oppenheimer
    "tt2584384",  // Jojo Rabbit
    "tt4468740",  // The Dead Don't Die
    "tt1856101",  // Blade Runner 2049
    "tt4263482",  // The Handmaiden
    "tt28607951", // Wicked
    "tt2582802",  // Whiplash
    "tt0382932",  // Ratatouille
    "tt0347149",  // Howl's Moving Castle
    "tt0299658",  // Chicago
    "tt0245429",  // Spirited Away
    "tt0230011",  // Moulin Rouge!
    "tt2396224",  // Song of the Sea
    "tt0805564",  // Lars and the Real Girl
    "tt0460989",  // Juno
    "tt0320661",  // Kung Fu Hustle
    "tt0264464",  // Catch Me If You Can
    "tt0120737",  // LOTR: Fellowship
    "tt0167261",  // LOTR: Two Towers
    "tt0167260",  // LOTR: Return of the King
    "tt0265666",  // The Royal Tenenbaums
    "tt0137523",  // Fight Club
    "tt0128445",  // Rushmore
    "tt0120338",  // Titanic
    "tt0119698",  // Princess Mononoke
    "tt0104797",  // A Few Good Men
    "tt0105236",  // Reservoir Dogs
    "tt3783958",  // La La Land
    "tt0099685",  // Goodfellas
    "tt0097499",  // Field of Dreams
    "tt0097165",  // Dead Poets Society
    "tt0097576",  // Indiana Jones and the Last Crusade
    "tt0097216",  // Die Hard
    "tt0093779",  // The Princess Bride
    "tt0096018",  // Cinema Paradiso
    "tt0181875",  // Almost Famous
    "tt2194499",  // Moonrise Kingdom
    "tt0780504",  // Drive
    "tt7131622",  // Once Upon a Time... in Hollywood
    // French
    "tt1675434",  // The Intouchables
    "tt0110413",  // Léon: The Professional
    "tt0211915",  // Amélie
    "tt0119116",  // The Fifth Element
    "tt2278871",  // Blue Is the Warmest Color
    "tt0113247",  // La Haine
    "tt0250223",  // Astérix & Obélix: Mission Cléopâtre
    "tt1655442",  // The Artist
    "tt4954522",  // Raw
    "tt0290673",  // Irréversible
    "tt1255953",  // Incendies
    "tt17009710", // Anatomy of a Fall
    "tt8613070",  // Portrait of a Lady on Fire
    "tt3612616",  // Mommy
    "tt0372824",  // The Chorus
    "tt0053198",  // The 400 Blows
    "tt7458762",  // The Wolf's Call
    "tt5078204",  // Two Is a Family
    "tt0092593",  // Au Revoir Les Enfants
    "tt0101765",  // Delicatessen
    "tt0106856",  // Three Colors: Blue
    "tt0338135",  // Caché (Hidden)
    "tt0363589",  // The Diving Bell and the Butterfly
    "tt0401711",  // Tell No One (Ne le dis à personne)
    "tt0756683",  // A Prophet (Un prophète)
    "tt14444726", // Titane
    "tt0070460",  // Day for Night (La Nuit américaine)
    "tt0048491",  // Les Diaboliques
    "tt0046268",  // The Wages of Fear (Le Salaire de la peur)
    "tt0082085",  // Diva
    "tt0120202",  // Taxi
    "tt0091288",  // Jean de Florette
    "tt0091480",  // Manon des Sources
    "tt0060474",  // La Grande Vadrouille
    "tt0108500",  // Les Visiteurs
    // Spanish
    "tt0457430",  // Pan's Labyrinth
    "tt4857264",  // The Invisible Guest
    "tt1038988",  // [REC]
    "tt1189073",  // The Skin I Live In
    "tt6155172",  // Roma
    "tt3011894",  // Wild Tales
    "tt16277242", // Society of the Snow
    "tt0464141",  // The Orphanage
    "tt0245712",  // Amores Perros
    "tt1305806",  // The Secret in Their Eyes
    "tt0185125",  // All About My Mother
    "tt6908274",  // Mirage
    "tt8291806",  // Pain and Glory
    "tt0441909",  // Volver
    "tt0245574",  // Y Tu Mamá También
    "tt0117093",  // Open Your Eyes (Abre los ojos)
    "tt0091670",  // Women on the Verge of a Nervous Breakdown
    "tt0234853",  // The Devil's Backbone
    "tt5765280",  // A Fantastic Woman (Una mujer fantástica)
    "tt7549996",  // The Platform (El hoyo)
    "tt0314331",  // Mondays in the Sun (Los lunes al sol)
    "tt1530509",  // No (2012, Chilean)
    "tt1650048",  // Even the Rain (También la lluvia)
    "tt0327056",  // The Motorcycle Diaries
    "tt0070040",  // The Spirit of the Beehive (El espíritu de la colmena)
    "tt0287467",  // Talk to Her (Hable con ella)
    "tt0117883",  // Tesis
    // German
    "tt1016150", // All Quiet on the Western Front
    "tt0088323", // The NeverEnding Story
    "tt0405094", // The Lives of Others
    "tt1063669", // The Wave
    "tt0017136", // Metropolis
    "tt0301357", // Good Bye, Lenin!
    "tt0130827", // Run Lola Run
    "tt0082096", // Das Boot
    "tt0022100", // M
    "tt0013442", // Nosferatu
    "tt0119167", // Funny Games
    "tt2987732", // Suck Me Shakespeer
    "tt3042408", // Who Am I
    "tt0093191", // Wings of Desire
    "tt4226388", // Victoria
    "tt0068182", // Aguirre, the Wrath of God
    "tt3104988", // Toni Erdmann
    "tt3615160", // Look Who's Back (Er ist wieder da)
    "tt4530422", // The Captain (Der Hauptmann)
    "tt0421082", // The Counterfeiters (Die Fälscher)
    "tt1186830", // The White Ribbon (Das weiße Band)
    "tt0076085", // The Tin Drum (Die Blechtrommel)
    "tt6710474", // System Crasher (Systemsprenger)
    "tt0250258", // The Experiment (Das Experiment)
    "tt0363163", // Downfall (Der Untergang)
    "tt0765432", // The Baader Meinhof Complex (Der Baader Meinhof Komplex)
    "tt0347048", // Head-On (Gegen die Wand)
    "tt1954701", // A Coffee in Berlin (Oh Boy)
    // Korean
    "tt0364569",  // Oldboy
    "tt5700672",  // Train to Busan
    "tt0353969",  // Memories of Murder
    "tt4016934",  // The Handmaiden
    "tt1588170",  // I Saw the Devil
    "tt5215952",  // The Wailing
    "tt0451094",  // Lady Vengeance
    "tt7282468",  // Burning
    "tt1216496",  // Mother
    "tt1190539",  // The Chaser
    "tt1527788",  // The Man from Nowhere
    "tt12477480", // Decision to Leave
    "tt0365376",  // A Tale of Two Sisters
    "tt0423866",  // 3-Iron
    "tt0468492",  // The Host
    "tt6644200",  // A Taxi Driver
    "tt0469903",  // Thirst
    "tt1133985",  // Spring, Summer, Fall, Winter... and Spring
    "tt1278060",  // Secret Sunshine (Milyang)
    "tt4334266",  // The Age of Shadows
    "tt13384586", // Broker
    "tt3622120",  // Assassination
    "tt0310775",  // Sympathy for Mr. Vengeance
    // Chinese
    "tt0190332",  // Crouching Tiger, Hidden Dragon
    "tt0299977",  // Hero
    "tt0385004",  // House of Flying Daggers
    "tt0446059",  // Fearless
    "tt0425637",  // Red Cliff
    "tt1410063",  // The Flowers of War
    "tt10627720", // Ne Zha
    "tt0101640",  // Raise the Red Lantern
    "tt0808357",  // Lust, Caution
    "tt0106332",  // Farewell My Concubine
    "tt0118694",  // In the Mood for Love
    "tt0109424",  // Chungking Express
    "tt0338564",  // Infernal Affairs
    "tt0112725",  // Eat Drink Man Woman
    "tt0765429",  // Ip Man
    "tt0460780",  // Curse of the Golden Flower
    "tt3810626",  // The Mermaid (Mei ren yu)
    "tt0112913",  // To Live (Huozhe)
    "tt0115857",  // Happy Together
    "tt0093389",  // A Chinese Ghost Story
    "tt0408664",  // The World (Shìjiè)
    "tt0859765",  // Still Life (Sānxiá hǎorén)
    // Japanese
    "tt5311514",  // Your Name.
    "tt0096283",  // My Neighbor Totoro
    "tt0095327",  // Grave of the Fireflies
    "tt0876563",  // Ponyo
    "tt0094625",  // Akira
    "tt0092067",  // Castle in the Sky
    "tt0097814",  // Kiki's Delivery Service
    "tt5323662",  // A Silent Voice
    "tt0047478",  // Seven Samurai
    "tt0087544",  // Nausicaä of the Valley of the Wind
    "tt0113568",  // Ghost in the Shell
    "tt0266308",  // Battle Royale
    "tt0104652",  // Porco Rosso
    "tt2013293",  // The Wind Rises
    "tt6587046",  // Shoplifters
    "tt1568921",  // The Tale of the Princess Kaguya
    "tt5462602",  // Weathering with You
    "tt14564098", // Suzume
    "tt12593682", // Demon Slayer: Mugen Train
    "tt0831887",  // Departures (Okuribito)
    "tt0044741",  // Ikiru
    "tt0032976",  // Rashomon
    "tt0054215",  // Yojimbo
    "tt0092048",  // Tampopo
    "tt0166924",  // Audition
    "tt0046438",  // Tokyo Story
    "tt0156887",  // Perfect Blue
    // Hindi
    "tt0264235",  // Lagaan
    "tt0169102",  // Dil Chahta Hai
    "tt0986264",  // 3 Idiots
    "tt1187043",  // Gangs of Wasseypur
    "tt2338151",  // Queen
    "tt6439020",  // Andhadhun
    "tt0374887",  // Rang De Basanti
    "tt0347304",  // Swades
    "tt2631186",  // PK
    "tt4559006",  // Dangal
    "tt3322420",  // Masaan
    "tt2390150",  // Haider
    "tt15745892", // RRR
    "tt3767372",  // Article 15
    "tt0150992",  // Dil Se..
    "tt8239946",  // Gully Boy
    "tt6644630",  // Tumbbad
    "tt0348730",  // Kal Ho Naa Ho
    "tt6766834",  // Super 30
    "tt3495026",  // Bajrangi Bhaijaan
    "tt0251075",  // Devdas
    "tt0319736",  // Black
    "tt2094990",  // Barfi!
    "tt2356180",  // Bhaag Milkha Bhaag
    "tt1562872",  // Zindagi Na Milegi Dobara
    // Russian
    "tt0079944",  // Stalker
    "tt0069293",  // Solaris
    "tt0091251",  // Come and See
    "tt0072443",  // Mirror
    "tt2802154",  // Leviathan
    "tt0060107",  // Andrei Rublev
    "tt6304162",  // Loveless
    "tt0376968",  // The Return
    "tt0318034",  // Russian Ark
    "tt6537238",  // Salyut-7
    "tt0118767",  // Brother
    "tt0050986",  // The Cranes Are Flying
    "tt0112883",  // Burnt by the Sun
    "tt0084726",  // Kin-dza-dza!
    "tt0079579",  // Moscow Does Not Believe in Tears
    "tt1234530",  // Elena
    "tt0363187",  // Night Watch
    "tt0488074",  // Day Watch
    "tt0015648",  // Battleship Potemkin
    "tt0056111",  // Ivan's Childhood (Ivanovo detstvo)
    "tt0063794",  // War and Peace (1966, Bondarchuk)
    "tt0062759",  // The Diamond Arm (1969, Gaidai)
    "tt0073179",  // The Irony of Fate (1976, Ryazanov)
    "tt0076727",  // Office Romance (1977, Ryazanov)
    "tt0084345",  // My Friend Ivan Lapshin (1985, German)
    "tt0097561",  // The Needle (1988, Nugmanov)
    "tt0095574",  // Little Vera (1988, Pichul)
    "tt0093754",  // Repentance (1987, Abuladze)
    "tt0101003",  // Freeze, Die, Come to Life (1990, Kanevsky)
    "tt0100757",  // Taxi Blues (1990, Lungin)
    "tt0096841",  // The Asthenic Syndrome (1989, Muratova)
    "tt0126711",  // Is It Easy to Be Young? (1986, Podnieks)
    "tt0238883",  // Brother 2 (2000, Balabanov)
    "tt0156849",  // Of Freaks and Men (1998, Balabanov)
    "tt0124207",  // The Thief (1997, Chukhray)
    "tt0116754",  // Prisoner of the Mountains (1996, Bodrov Sr.)
    "tt0156701",  // Khrustalyov, My Car! (1998, German)
    "tt1588875",  // How I Ended This Summer (2010, Popogrebsky)
    "tt10199640", // Beanpole (2019, Balagov)
    "tt0847880",  // Cargo 200 (2007, Balabanov)
    // Portuguese
    "tt0317248",  // City of God
    "tt0861739",  // Elite Squad
    "tt1555149",  // Elite Squad: The Enemy Within
    "tt0271383",  // A Dog's Will
    "tt2762506",  // Bacurau
    "tt0140888",  // Central Station
    "tt14961016", // I'm Still Here
    "tt3742378",  // The Second Mother
    "tt5221584",  // Aquarius
    "tt0293007",  // Carandiru
    "tt0082912",  // Pixote
    "tt1424432",  // Neighboring Sounds (O Som ao Redor)
    "tt3398268",  // The Way He Looks (Hoje Eu Quero Voltar Sozinho)
    "tt0361862",  // City of Men (Cidade dos Homens)
    "tt0212985",  // Behind the Sun (Abril Despedaçado)
    "tt0367110",  // Madame Satã
    // Italian
    "tt0118799", // Life Is Beautiful
    "tt0060196", // The Good, the Bad and the Ugly
    "tt0064116", // Once Upon a Time in the West
    "tt0095765", // Cinema Paradiso
    "tt0058461", // A Fistful of Dollars
    "tt4901306", // Perfect Strangers
    "tt2358891", // The Great Beauty
    "tt0076786", // Suspiria
    "tt0040522", // Bicycle Thieves
    "tt0056801", // 8½
    "tt0213847", // Malèna
    "tt0120731", // The Legend of 1900
    "tt0053779", // La Dolce Vita
    "tt0048673", // La Strada
    "tt0036775", // Rome, Open City
    "tt0116209", // Il Postino
    "tt5164214", // Happy as Lazzaro (Lazzaro felice)
    "tt7304534", // The Hand of God (È stata la mano di Dio)
    "tt0050783", // Nights of Cabiria
    "tt0057091", // The Leopard (Il Gattopardo)
    "tt7026672", // Pinocchio (2019, Garrone)
    "tt0065571", // The Conformist (Il conformista)
    "tt0071129", // Amarcord
    "tt0055913", // Divorce Italian Style (Divorzio all'italiana)
    "tt0065889", // Investigation of a Citizen Above Suspicion
];

/// OMDB API response
#[derive(Debug, Deserialize)]
struct OmdbResponse {
    #[serde(rename = "Ratings", default)]
    ratings: Vec<OmdbRating>,
}

#[derive(Debug, Deserialize)]
struct OmdbRating {
    #[serde(rename = "Source")]
    source: String,
    #[serde(rename = "Value")]
    value: String,
}

struct OmdbClient {
    api_key: String,
    client: reqwest::Client,
}

impl OmdbClient {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    async fn get_rotten_tomatoes_score(&self, imdb_id: &str) -> Option<u8> {
        let url = format!(
            "https://www.omdbapi.com/?i={}&apikey={}",
            imdb_id, self.api_key
        );
        let response = self.client.get(&url).send().await.ok()?;
        let omdb: OmdbResponse = response.json().await.ok()?;
        for rating in &omdb.ratings {
            if rating.source == "Rotten Tomatoes" {
                return rating.value.trim_end_matches('%').parse().ok();
            }
        }
        None
    }
}

/// Response from /discover/popular endpoint
#[derive(Debug, Deserialize)]
struct PopularMoviesResponse {
    data: Vec<PopularMovie>,
}

#[derive(Debug, Deserialize)]
struct PopularMovie {
    attributes: PopularMovieAttributes,
}

#[derive(Debug, Deserialize)]
struct PopularMovieAttributes {
    title: String,
    #[serde(rename = "imdb_id")]
    imdb_id: Option<u64>,
    year: Option<String>,
}

/// Response from /subtitles search endpoint
#[derive(Debug, Deserialize)]
struct SubtitleSearchResponse {
    data: Vec<SubtitleResult>,
}

#[derive(Debug, Deserialize)]
struct SubtitleResult {
    attributes: SubtitleAttributes,
}

#[derive(Debug, Deserialize)]
struct SubtitleAttributes {
    #[allow(dead_code)]
    #[serde(rename = "feature_details")]
    feature_details: FeatureDetails,
    files: Vec<SubtitleFile>,
    download_count: Option<u64>,
    #[serde(default)]
    from_trusted: Option<bool>,
    #[serde(default)]
    ai_translated: bool,
    #[serde(default)]
    machine_translated: bool,
    ratings: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FeatureDetails {
    #[allow(dead_code)]
    #[serde(rename = "imdb_id")]
    imdb_id: u64,
    #[allow(dead_code)]
    title: String,
    #[allow(dead_code)]
    year: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct SubtitleFile {
    #[serde(rename = "file_id")]
    file_id: u64,
}

/// Download link response
#[derive(Debug, Deserialize)]
struct DownloadResponse {
    link: String,
    #[allow(dead_code)]
    #[serde(rename = "file_name")]
    file_name: String,
}

/// Subtitle line for JSON output
#[derive(Debug, Serialize)]
struct SubtitleLineJson {
    sentence: String,
    start_ms: u32,
    end_ms: u32,
}

/// TMDB API Movie Response
#[derive(Debug, Deserialize)]
struct TmdbMovie {
    title: String,
    release_date: Option<String>,
    poster_path: Option<String>,
    original_language: Option<String>,
}

/// TMDB Find API Response
#[derive(Debug, Deserialize)]
struct TmdbFindResponse {
    movie_results: Vec<TmdbMovie>,
}

struct TmdbClient {
    api_key: String,
    client: reqwest::Client,
}

impl TmdbClient {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    async fn get_movie(&self, imdb_id: &str, language: &str) -> Result<TmdbMovie> {
        // Use the find endpoint to search by IMDB ID
        let url = format!(
            "https://api.themoviedb.org/3/find/{}?api_key={}&external_source=imdb_id&language={}",
            imdb_id, self.api_key, language
        );

        let response = self.client.get(&url).send().await?;
        let response_text = response.text().await?;
        let find_response: TmdbFindResponse = serde_json::from_str(&response_text)?;

        if find_response.movie_results.is_empty() {
            return Err(anyhow!("No movie found for IMDB ID {imdb_id}"));
        }

        // Rate limiting: wait 300ms between requests (TMDB allows ~40 req/10s)
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        Ok(find_response.movie_results.into_iter().next().unwrap())
    }
}

struct OpenSubtitlesClient {
    api_key: String,
    client: reqwest::Client,
    access_token: Option<String>,
}

impl OpenSubtitlesClient {
    fn new(api_key: String) -> Self {
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
    async fn login(&mut self, username: &str, password: &str) -> Result<()> {
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
    async fn get_popular_movies(&self, language: &str, limit: usize) -> Result<Vec<PopularMovie>> {
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
            return Err(anyhow!("API error ({status}): {error_text}"));
        }

        let popular_response: PopularMoviesResponse = response.json().await?;

        println!("Found {} popular movies", popular_response.data.len());

        // Take only the first `limit` results
        Ok(popular_response.data.into_iter().take(limit).collect())
    }

    /// Search for subtitles for a specific movie by IMDB ID
    async fn search_subtitles_for_movie(
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
            .await?
            .error_for_status()?;

        let search_response = response
            .text()
            .await
            .context("Failed to get subtitle search response")?;
        let search_response: SubtitleSearchResponse = serde_json::from_str(&search_response)
            .context(format!(
                "Failed to parse subtitle search response: {search_response}"
            ))
            .unwrap();

        // Return all results for filtering
        Ok(search_response.data)
    }

    /// Download a subtitle file
    async fn download_subtitle(&self, file_id: u64) -> Result<String> {
        let url = "https://api.opensubtitles.com/api/v1/download";

        let mut body = HashMap::new();
        body.insert("file_id", file_id);

        let mut request = self.client.post(url).header("Api-Key", &self.api_key);

        // Add Authorization header if we have a token
        if let Some(token) = &self.access_token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }

        let response = request.json(&body).send().await?.error_for_status()?;

        let download_response: DownloadResponse = response.json().await?;

        // Download the actual SRT file from the link
        let srt_response = self.client.get(&download_response.link).send().await?;

        let srt_content = srt_response.text().await?;

        // Rate limiting: wait 500ms between requests
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Ok(srt_content)
    }
}

/// Parse SRT content and extract cleaned sentences with timestamps
fn parse_srt(srt_content: &str) -> Result<Vec<SubtitleLineJson>> {
    use subparse::SubtitleFormat;

    let subtitle_file = subparse::parse_str(
        SubtitleFormat::SubRip,
        srt_content,
        25.0, // fps (not used for SRT but required parameter)
    )
    .map_err(|e| anyhow!("Failed to parse SRT: {e:?}"))?;

    let mut lines = Vec::new();

    for entry in subtitle_file
        .get_subtitle_entries()
        .map_err(|e| anyhow!("Failed to get subtitle entries: {e:?}"))?
    {
        // entry.line is Option<String>
        let text = match &entry.line {
            Some(line) => cleanup_subtitle_text(line),
            None => continue,
        };

        // Skip empty lines or very short lines
        if text.len() < 3 {
            continue;
        }

        // secs() returns i64, multiply by i64
        let start_ms = entry.timespan.start.secs() * 1000;
        let end_ms = entry.timespan.end.secs() * 1000;

        lines.push(SubtitleLineJson {
            sentence: text,
            start_ms: start_ms as u32,
            end_ms: end_ms as u32,
        });
    }

    Ok(lines)
}

/// Clean up subtitle text
fn cleanup_subtitle_text(text: &str) -> String {
    let mut result = text.to_string();

    // Remove HTML tags
    result = strip_html_tags(&result);

    // Remove hearing-impaired annotations
    result = result
        .replace("[MUSIC]", "")
        .replace("(MUSIC)", "")
        .replace("[music]", "")
        .replace("(music)", "")
        .replace("[DOOR SLAMS]", "")
        .replace("(DOOR SLAMS)", "")
        .replace("[PHONE RINGS]", "")
        .replace("(PHONE RINGS)", "");

    // Remove bracketed content (hearing impaired)
    let re_brackets = regex::Regex::new(r"\[.*?\]").unwrap();
    result = re_brackets.replace_all(&result, "").to_string();

    let re_parens = regex::Regex::new(r"\(.*?\)").unwrap();
    result = re_parens.replace_all(&result, "").to_string();

    // Remove speaker names like "JOHN:"
    let re_speaker = regex::Regex::new(r"^[A-Z][A-Z\s]+:\s*").unwrap();
    result = re_speaker.replace_all(&result, "").to_string();

    // Trim whitespace
    result = result.trim().to_string();

    // Remove multiple spaces
    let re_spaces = regex::Regex::new(r"\s+").unwrap();
    result = re_spaces.replace_all(&result, " ").to_string();

    result
}

/// Strip HTML tags from text
fn strip_html_tags(text: &str) -> String {
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(text, "").to_string()
}

/// Check if subtitle lines pass the language sanity check.
fn passes_language_sanity_check(lines: &[SubtitleLineJson], language: Language) -> bool {
    match language.check_subtitle_sanity(lines.iter().map(|l| l.sentence.as_str()), &[]) {
        Ok(()) => true,
        Err(reason) => {
            eprintln!("  Sanity check failed: {reason}");
            false
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SubtitleQualityResponse {
    /// Whether the subtitles look like clean, properly-encoded text
    clean: bool,
    /// Brief explanation if not clean
    reason: String,
}

/// Use an LLM to check if subtitle samples look clean and properly encoded.
/// Samples 3 blocks from the subtitle file (beginning, middle, end).
async fn passes_llm_quality_check(lines: &[SubtitleLineJson], language: Language) -> bool {
    if lines.len() < 10 {
        return true; // Too short to meaningfully check
    }

    // Sample 8 lines from 5 evenly-spaced blocks for broader coverage
    let block_size = 8;
    let len = lines.len();
    let offsets = [
        5.min(len),
        len / 4,
        len / 2,
        3 * len / 4,
        len.saturating_sub(block_size),
    ];

    let mut sample = String::new();
    for (i, &start) in offsets.iter().enumerate() {
        sample.push_str(&format!("--- BLOCK {i} ---\n"));
        for line in lines.iter().skip(start).take(block_size) {
            sample.push_str(&line.sentence);
            sample.push('\n');
        }
        sample.push('\n');
    }

    let prompt = format!(
        "These are samples from a {language} movie subtitle file. \
         Are these subtitles clean and properly encoded? \
         Look for any SYSTEMIC problems:\n\
         - OCR errors: 'rhe'/'rhat' for 'the'/'that', 'II' for 'Il', 'l' (lowercase L) for 'I' \
           (e.g. 'lo' for 'Io' in Italian, 'ln' for 'In', 'l'' for 'l''), \
           'I' (capital I) for 'l' (e.g. 'I'' for 'l'' in French/Italian)\n\
         - Missing diacritics: text in ASCII when the language requires accents \
           (e.g. Spanish without á/é/í/ó/ú/ñ, French without é/è/ê/à/ç)\n\
         - Wrong language or bilingual: subtitles in a different language, or two languages interleaved\n\
         - Encoding corruption: mojibake, garbled accents, control characters, \
           Greek lookalike characters mixed into Latin text\n\
         - Formatting artifacts: {{\\an8}}, SSA/ASS tags, HTML tags\n\n\
         A few minor issues in individual lines are OK — flag it only if there's a \
         SYSTEMIC problem affecting many lines.\n\n{sample}"
    );

    match QUALITY_CHECK_CLIENT
        .chat::<SubtitleQualityResponse>(prompt)
        .await
    {
        Ok(response) => {
            if !response.clean {
                println!("  ✗ LLM quality check failed: {}", response.reason);
            }
            response.clean
        }
        Err(e) => {
            println!("  ⚠ LLM quality check error (allowing): {e}");
            true // Don't block on LLM errors
        }
    }
}

/// Download subtitles for a single movie and return metadata
#[allow(clippy::too_many_arguments)]
async fn download_movie_subtitles(
    opensub_client: &OpenSubtitlesClient,
    tmdb_client: &TmdbClient,
    omdb_client: &OmdbClient,
    imdb_id: u64,
    imdb_id_str: &str,
    language_iso639_1: &str,
    tmdb_language: &str,
    subtitle_path: &std::path::Path,
    posters_dir: &std::path::Path,
    language: Language,
) -> Result<Option<(Vec<SubtitleLineJson>, MovieMetadataBasic)>> {
    // Search for subtitles
    let mut subtitle_results = opensub_client
        .search_subtitles_for_movie(imdb_id, language_iso639_1)
        .await?;

    if subtitle_results.is_empty() {
        return Ok(None);
    }

    // Filter and sort by quality
    subtitle_results.retain(|s| !s.attributes.ai_translated && !s.attributes.machine_translated);
    if subtitle_results.is_empty() {
        println!("  ✗ No human-translated subtitles available");
        return Ok(None);
    }

    subtitle_results.sort_by(|a, b| {
        match (a.attributes.from_trusted, b.attributes.from_trusted) {
            (Some(true), _) => return std::cmp::Ordering::Less,
            (_, Some(true)) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        match (a.attributes.download_count, b.attributes.download_count) {
            (Some(a_count), Some(b_count)) => {
                if a_count != b_count {
                    return b_count.cmp(&a_count);
                }
            }
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        match (a.attributes.ratings, b.attributes.ratings) {
            (Some(a_rating), Some(b_rating)) => b_rating
                .partial_cmp(&a_rating)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });

    println!("  Found {} subtitle options", subtitle_results.len());

    // Try each subtitle in order until one succeeds
    for subtitle_result in subtitle_results {
        println!(
            "  Trying subtitle: {} downloads, trusted: {}, rating: {:.1}",
            subtitle_result.attributes.download_count.unwrap_or(0),
            subtitle_result.attributes.from_trusted.unwrap_or(false),
            subtitle_result.attributes.ratings.unwrap_or(0.0)
        );

        let Some(file_id) = subtitle_result.attributes.files.first().map(|f| f.file_id) else {
            println!("  ✗ No files found for this subtitle, trying next...");
            continue;
        };

        println!("  Downloading subtitle (file_id: {file_id})...");
        let srt_content = match opensub_client.download_subtitle(file_id).await {
            Ok(content) => content,
            Err(e) => {
                println!("  ✗ Download failed: {e}, trying next...");
                continue;
            }
        };

        println!("  Parsing SRT...");
        let subtitle_lines = match parse_srt(&srt_content) {
            Ok(lines) => lines,
            Err(e) => {
                println!("  ✗ Parse failed: {e}, trying next...");
                continue;
            }
        };

        if subtitle_lines.is_empty() {
            continue;
        }

        // Sanity check: verify subtitles are actually in the target language
        if !passes_language_sanity_check(&subtitle_lines, language) {
            println!(
                "  ✗ Subtitles failed language sanity check (wrong language?), trying next..."
            );
            continue;
        }

        // LLM quality check: sample blocks and verify they look clean
        if !passes_llm_quality_check(&subtitle_lines, language).await {
            println!("  ✗ Subtitles failed LLM quality check, trying next...");
            continue;
        }

        println!("  Extracted {} dialogue lines", subtitle_lines.len());

        // Save subtitle file
        let subtitle_file = match fs::File::create(subtitle_path) {
            Ok(file) => file,
            Err(e) => {
                println!("  ✗ Failed to create file: {e}, trying next...");
                continue;
            }
        };

        for line in &subtitle_lines {
            if let Err(e) = serde_json::to_writer(&subtitle_file, &line) {
                println!("  ✗ Failed to write subtitle: {e}");
                break;
            }
            if let Err(e) = writeln!(&subtitle_file) {
                println!("  ✗ Failed to write newline: {e}");
                break;
            }
        }

        println!("  ✓ Saved to {}", subtitle_path.display());

        // Fetch metadata from TMDB
        println!("  Fetching metadata from TMDB...");
        let (title, year, original_language) =
            match tmdb_client.get_movie(imdb_id_str, tmdb_language).await {
                Ok(tmdb_data) => {
                    let title = tmdb_data.title;
                    let year = tmdb_data
                        .release_date
                        .and_then(|d| d.split('-').next().and_then(|y| y.parse::<u16>().ok()));
                    let original_language = tmdb_data.original_language;

                    // Fetch and save poster if available (skip if already exists)
                    let poster_file = posters_dir.join(format!("{imdb_id_str}.jpg"));
                    if !poster_file.exists() {
                        if let Some(poster_path) = tmdb_data.poster_path {
                            println!("  Fetching poster image...");
                            let poster_url =
                                format!("https://image.tmdb.org/t/p/w500{poster_path}");
                            match fetch_image_bytes(&opensub_client.client, &poster_url).await {
                                Ok(bytes) => {
                                    if let Err(e) = fs::write(&poster_file, &bytes) {
                                        println!("  ⚠ Failed to save poster: {e}");
                                    } else {
                                        println!("  ✓ Saved poster to {}", poster_file.display());
                                    }
                                }
                                Err(e) => {
                                    println!("  ⚠ Failed to fetch poster: {e}");
                                }
                            }
                        }
                    }

                    (title, year, original_language)
                }
                Err(e) => {
                    println!("  ⚠ Could not fetch TMDB metadata: {e:?}");
                    ("Unknown".to_string(), None, None)
                }
            };

        // Fetch Rotten Tomatoes score from OMDB
        let rotten_tomatoes_score = omdb_client.get_rotten_tomatoes_score(imdb_id_str).await;
        if let Some(score) = rotten_tomatoes_score {
            println!("  ✓ Rotten Tomatoes: {score}%");
        }

        let movie = MovieMetadataBasic {
            id: imdb_id_str.to_string(),
            title,
            year,
            original_language,
            rotten_tomatoes_score,
        };

        return Ok(Some((subtitle_lines, movie)));
    }

    Ok(None)
}

/// Fetch movie metadata from TMDB
async fn fetch_tmdb_metadata(
    tmdb_client: &TmdbClient,
    omdb_client: &OmdbClient,
    imdb_id_str: &str,
    tmdb_language: &str,
    opensub_client: &OpenSubtitlesClient,
    posters_dir: &std::path::Path,
) -> Result<MovieMetadataBasic> {
    let (tmdb_title, tmdb_year, tmdb_original_language) =
        match tmdb_client.get_movie(imdb_id_str, tmdb_language).await {
            Ok(tmdb_data) => {
                let tmdb_title = tmdb_data.title;
                let tmdb_year = tmdb_data
                    .release_date
                    .and_then(|d| d.split('-').next().and_then(|y| y.parse::<u16>().ok()));
                let tmdb_original_language = tmdb_data.original_language;

                // Fetch and save poster if available (skip if already exists)
                let poster_file = posters_dir.join(format!("{imdb_id_str}.jpg"));
                if !poster_file.exists() {
                    if let Some(poster_path) = tmdb_data.poster_path {
                        println!("  Fetching poster image...");
                        let poster_url = format!("https://image.tmdb.org/t/p/w500{poster_path}");
                        match fetch_image_bytes(&opensub_client.client, &poster_url).await {
                            Ok(bytes) => {
                                if let Err(e) = fs::write(&poster_file, &bytes) {
                                    println!("  ⚠ Failed to save poster: {e}");
                                } else {
                                    println!("  ✓ Saved poster to {}", poster_file.display());
                                }
                            }
                            Err(e) => {
                                println!("  ⚠ Failed to fetch poster: {e}");
                            }
                        }
                    }
                }

                (tmdb_title, tmdb_year, tmdb_original_language)
            }
            Err(e) => {
                println!("  ⚠ Could not fetch TMDB metadata: {e}");
                return Err(anyhow!("Failed to fetch TMDB metadata: {e}"));
            }
        };

    // Fetch Rotten Tomatoes score from OMDB
    let rotten_tomatoes_score = omdb_client.get_rotten_tomatoes_score(imdb_id_str).await;
    if let Some(score) = rotten_tomatoes_score {
        println!("  ✓ Rotten Tomatoes: {score}%");
    }

    Ok(MovieMetadataBasic {
        id: imdb_id_str.to_string(),
        title: tmdb_title,
        year: tmdb_year,
        original_language: tmdb_original_language,
        rotten_tomatoes_score,
    })
}

/// Process a single movie: download subtitle if needed, fetch metadata if needed
/// Returns (metadata, is_new_download)
#[allow(clippy::too_many_arguments)]
async fn process_movie(
    imdb_id_str: &str,
    opensub_client: &OpenSubtitlesClient,
    tmdb_client: &TmdbClient,
    omdb_client: &OmdbClient,
    existing_metadata: &FxHashMap<String, MovieMetadataBasic>,
    language_iso639_1: &str,
    tmdb_language: &str,
    output_dir: &std::path::Path,
    posters_dir: &std::path::Path,
    language: Language,
) -> Result<(MovieMetadataBasic, bool)> {
    let subtitle_path = output_dir.join(format!("subtitles/{imdb_id_str}.jsonl"));
    let imdb_id = imdb_id_str.strip_prefix("tt").unwrap().parse::<u64>()?;

    let (is_new_download, maybe_metadata) = if subtitle_path.exists() {
        println!("  ✓ Subtitle already downloaded");
        (false, None)
    } else {
        println!("  Searching for subtitles...");
        match download_movie_subtitles(
            opensub_client,
            tmdb_client,
            omdb_client,
            imdb_id,
            imdb_id_str,
            language_iso639_1,
            tmdb_language,
            &subtitle_path,
            posters_dir,
            language,
        )
        .await?
        {
            Some((_, movie)) => {
                println!("  ✓ Downloaded successfully");
                (true, Some(movie))
            }
            None => {
                println!("  ✗ Failed to download subtitles");
                return Err(anyhow!("No subtitles available"));
            }
        }
    };

    // If we got metadata from download, use it. Otherwise check existing or fetch from TMDB
    let metadata = if let Some(meta) = maybe_metadata {
        meta
    } else if let Some(existing) = existing_metadata
        .get(imdb_id_str)
        .filter(|m| m.original_language.is_some() && m.rotten_tomatoes_score.is_some())
    {
        println!("  ✓ Using existing metadata");
        existing.clone()
    } else {
        println!("  Fetching metadata from TMDB...");
        fetch_tmdb_metadata(
            tmdb_client,
            omdb_client,
            imdb_id_str,
            tmdb_language,
            opensub_client,
            posters_dir,
        )
        .await?
    };

    Ok((metadata, is_new_download))
}

/// Parse a Language from an ISO 639-3 code for clap
fn parse_language(s: &str) -> Result<Language, String> {
    Language::from_iso_639_3(s).ok_or_else(|| {
        format!(
            "unsupported language code '{s}'. Supported: fra, eng, spa, deu, kor, zho, jpn, rus, por, ita, hin"
        )
    })
}

/// Download movie subtitles from OpenSubtitles
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Language codes (ISO 639-3: fra, eng, spa, deu, kor, zho, jpn, rus, por, ita, hin)
    #[arg(short, long, num_args = 1.., value_parser = parse_language)]
    language: Vec<Language>,

    /// Number of movies to download per language
    #[arg(short, long, default_value_t = 5)]
    count: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    dotenv::dotenv().ok();

    // Get API keys from environment
    let opensub_api_key = std::env::var("OPENSUBTITLES_API_KEY")
        .context("OPENSUBTITLES_API_KEY environment variable not set")?;
    let tmdb_api_key =
        std::env::var("TMDB_API_KEY").context("TMDB_API_KEY environment variable not set")?;

    // Get optional login credentials
    let username = std::env::var("OPENSUBTITLES_USERNAME").ok();
    let password = std::env::var("OPENSUBTITLES_PASSWORD").ok();

    // Parse command line arguments
    let args = Args::parse();
    let languages = args.language;
    let count = args.count;

    let omdb_api_key =
        std::env::var("OMDB_API_KEY").context("OMDB_API_KEY environment variable not set")?;

    // Create clients once for all languages
    let mut opensub_client = OpenSubtitlesClient::new(opensub_api_key);
    let tmdb_client = TmdbClient::new(tmdb_api_key);
    let omdb_client = OmdbClient::new(omdb_api_key);

    // Login if credentials are provided
    if let (Some(user), Some(pass)) = (username, password) {
        println!("Logging in to OpenSubtitles...");
        opensub_client.login(&user, &pass).await?;
    } else {
        println!("No login credentials provided - using unauthenticated mode (limited downloads)");
        println!("Set OPENSUBTITLES_USERNAME and OPENSUBTITLES_PASSWORD in .env to authenticate");
    }

    // Process each language
    for language in languages {
        let language_iso639_3 = language.iso_639_3();
        let language_iso639_1 = language.opensubtitles_language_code();
        let tmdb_language = language.tmdb_language_code();

        println!(
            "\n========================================\nDownloading {count} subtitles for language: {language_iso639_3}\n========================================"
        );

        // Create output directory using ISO 639-3 to match generate-data pipeline
        let output_dir = PathBuf::from(format!(
            "./generate-data/data/{language_iso639_3}/sentence-sources/movies"
        ));
        fs::create_dir_all(&output_dir)?;
        fs::create_dir_all(output_dir.join("subtitles"))?;
        let posters_dir = output_dir.join("posters");
        fs::create_dir_all(&posters_dir)?;

        // Read existing metadata to avoid re-fetching OMDB data
        let metadata_path = output_dir.join("metadata.jsonl");
        let mut existing_metadata: FxHashMap<String, MovieMetadataBasic> = FxHashMap::default();
        if metadata_path.exists() {
            let metadata_content = fs::read_to_string(&metadata_path)?;
            for line in metadata_content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(movie) = serde_json::from_str::<MovieMetadataBasic>(line) {
                    existing_metadata.insert(movie.id.clone(), movie);
                }
            }
            println!("Loaded metadata for {} movies", existing_metadata.len());
        }

        // Count already downloaded movies
        let subtitles_dir = output_dir.join("subtitles");
        let existing_count = if subtitles_dir.exists() {
            fs::read_dir(&subtitles_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
                .count()
        } else {
            0
        };

        if existing_count > 0 {
            println!("Found {existing_count} already downloaded movies");
        }

        // Get popular movies using ISO 639-1 for OpenSubtitles API
        // Request more than needed to account for already-downloaded movies and low-quality subtitles
        let fetch_count = count * 3 + existing_count;
        println!("Searching for popular movies...");
        let popular_movies = opensub_client
            .get_popular_movies(language_iso639_1, fetch_count)
            .await?;

        println!("Found {} popular movies", popular_movies.len());

        let mut movies = Vec::new();
        let mut downloaded_count = 0;

        for popular_movie in popular_movies.iter() {
            // Stop if we've downloaded enough new movies
            if downloaded_count >= count {
                break;
            }
            let attrs = &popular_movie.attributes;
            let Some(imdb_id) = attrs.imdb_id else {
                println!("  ✗ Skipping movie with no IMDB ID: {}", attrs.title);
                continue;
            };
            let imdb_id_str = format!("tt{imdb_id:07}");

            println!(
                "\n[Downloaded: {}/{}] {} ({})",
                downloaded_count,
                count,
                attrs.title,
                attrs.year.as_deref().unwrap_or("Unknown")
            );

            match process_movie(
                &imdb_id_str,
                &opensub_client,
                &tmdb_client,
                &omdb_client,
                &existing_metadata,
                language_iso639_1,
                tmdb_language,
                &output_dir,
                &posters_dir,
                language,
            )
            .await
            {
                Ok((movie, is_new)) => {
                    movies.push(movie);
                    if is_new {
                        downloaded_count += 1;
                    }
                }
                Err(e) => {
                    println!("  ✗ Error: {e}");
                }
            }
        }

        // Warn if we couldn't download enough movies
        if downloaded_count < count {
            println!(
                "\n⚠ Warning: Only found {downloaded_count} movies with subtitles (requested {count})"
            );
        }

        // Also download EXTRA_MOVIES if not already downloaded
        println!("\nProcessing extra movies list...");
        for &imdb_id_str in EXTRA_MOVIES {
            println!("\n  Processing {imdb_id_str}...");

            match process_movie(
                imdb_id_str,
                &opensub_client,
                &tmdb_client,
                &omdb_client,
                &existing_metadata,
                language_iso639_1,
                tmdb_language,
                &output_dir,
                &posters_dir,
                language,
            )
            .await
            {
                Ok((movie, _)) => {
                    movies.push(movie);
                }
                Err(e) => {
                    println!("  ✗ Error: {e}");
                }
            }
        }

        // Ensure metadata exists for all movies with downloaded subtitles
        let processed_ids: rustc_hash::FxHashSet<String> =
            movies.iter().map(|m| m.id.clone()).collect();
        let subtitle_files: Vec<_> = fs::read_dir(&subtitles_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .and_then(|s| s.to_str().map(|s| s.to_string()))
            })
            .filter(|id| !processed_ids.contains(id))
            .collect();

        if !subtitle_files.is_empty() {
            println!(
                "\nFetching metadata for {} movies with existing subtitles...",
                subtitle_files.len()
            );
            for imdb_id_str in &subtitle_files {
                println!("  Processing {imdb_id_str}...");
                if let Some(existing) = existing_metadata
                    .get(imdb_id_str)
                    .filter(|m| m.original_language.is_some() && m.rotten_tomatoes_score.is_some())
                {
                    println!("  ✓ Using existing metadata");
                    movies.push(existing.clone());
                } else {
                    match fetch_tmdb_metadata(
                        &tmdb_client,
                        &omdb_client,
                        imdb_id_str,
                        tmdb_language,
                        &opensub_client,
                        &posters_dir,
                    )
                    .await
                    {
                        Ok(movie) => {
                            println!("  ✓ Fetched metadata: {}", movie.title);
                            movies.push(movie);
                        }
                        Err(e) => {
                            println!("  ✗ Error: {e}");
                        }
                    }
                }
            }
        }

        // Save metadata
        let metadata_path = output_dir.join("metadata.jsonl");
        let metadata_file = fs::File::create(&metadata_path)?;
        for movie in &movies {
            serde_json::to_writer(&metadata_file, &movie)?;
            writeln!(&metadata_file)?;
        }

        println!("\nMetadata saved to {}", metadata_path.display());
        println!(
            "Done! Downloaded {} new movies for {} (total: {} movies)",
            downloaded_count,
            language_iso639_3,
            movies.len()
        );
    }

    println!("\n========================================");
    println!("All languages processed successfully!");
    println!("========================================");

    Ok(())
}

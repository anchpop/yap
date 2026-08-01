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

/// Hand-picked movies to fetch subtitles for, on top of whatever
/// `discover/popular` returns. Every ID here was resolved against TMDB and the
/// comment is the title and year that ID *actually* points at — so a mistyped or
/// stale ID shows up as a wrong-looking comment instead of silently downloading
/// the wrong film. Grouping is by the movie's original language; the list itself
/// is flat and every entry is tried for every target language.
const EXTRA_MOVIES: &[&str] = &[
    // English classics & blockbusters
    "tt20215234", // Conclave (2024)
    "tt15398776", // Oppenheimer (2023)
    "tt2584384",  // Jojo Rabbit (2019)
    "tt4468740",  // Paddington 2 (2017)
    "tt1856101",  // Blade Runner 2049 (2017)
    "tt4263482",  // The Witch (2016)
    "tt28607951", // Anora (2024)
    "tt1262426",  // Wicked (2024)
    "tt2582802",  // Whiplash (2014)
    "tt0382932",  // Ratatouille (2007)
    "tt0299658",  // Chicago (2002)
    "tt0230011",  // Atlantis: The Lost Empire (2001)
    "tt0203009",  // Moulin Rouge! (2001)
    "tt2396224",  // It's Such a Beautiful Day (2012)
    "tt1865505",  // Song of the Sea (2014)
    "tt0805564",  // Lars and the Real Girl (2007)
    "tt0460989",  // The Wind That Shakes the Barley (2006)
    "tt0467406",  // Juno (2007)
    "tt0264464",  // Catch Me If You Can (2002)
    "tt0120737",  // The Lord of the Rings: The Fellowship of the Ring (2001)
    "tt0167261",  // The Lord of the Rings: The Two Towers (2002)
    "tt0167260",  // The Lord of the Rings: The Return of the King (2003)
    "tt0265666",  // The Royal Tenenbaums (2001)
    "tt0137523",  // Fight Club (1999)
    "tt0128445",  // Rushmore (1998)
    "tt0120338",  // Titanic (1997)
    "tt0104797",  // Malcolm X (1992)
    "tt0104257",  // A Few Good Men (1992)
    "tt0105236",  // Reservoir Dogs (1992)
    "tt3783958",  // La La Land (2016)
    "tt0099685",  // Goodfellas (1990)
    "tt0097499",  // Henry V (1989)
    "tt0097351",  // Field of Dreams (1989)
    "tt0097165",  // Dead Poets Society (1989)
    "tt0097576",  // Indiana Jones and the Last Crusade (1989)
    "tt0097216",  // Do the Right Thing (1989)
    "tt0095016",  // Die Hard (1988)
    "tt0093779",  // The Princess Bride (1987)
    "tt0096018",  // Running on Empty (1988)
    "tt0181875",  // Almost Famous (2000)
    "tt2194499",  // About Time (2013)
    "tt1748122",  // Moonrise Kingdom (2012)
    "tt0780504",  // Drive (2011)
    "tt7131622",  // Once Upon a Time... in Hollywood (2019)
    "tt0106856",  // Falling Down (1993)
    "tt0363589",  // Elephant (2003)
    "tt0756683",  // The Man from Earth (2007)
    "tt14444726", // TÁR (2022)
    "tt0082085",  // Blow Out (1981)
    "tt0234853",  // The Tao of Steve (2000)
    "tt0314331",  // Love Actually (2003)
    "tt0327056",  // Mystic River (2003)
    "tt3104988",  // Crazy Rich Asians (2018)
    "tt0421082",  // Control (2007)
    "tt6710474",  // Everything Everywhere All at Once (2022)
    "tt6644200",  // A Quiet Place (2018)
    "tt0765429",  // American Gangster (2007)
    "tt0093389",  // The Last Emperor (1987)
    "tt5462602",  // The Big Sick (2017)
    "tt12593682", // Bullet Train (2022)
    "tt0032976",  // Rebecca (1940)
    "tt0054215",  // Psycho (1960)
    "tt0166924",  // Mulholland Drive (2001)
    "tt0084726",  // Star Trek II: The Wrath of Khan (1982)
    "tt1424432",  // Senna (2010)
    "tt0361862",  // The Machinist (2004)
    "tt0036775",  // Double Indemnity (1944)
    "tt0116209",  // The English Patient (1996)
    // French
    "tt1675434",  // The Intouchables (2011)
    "tt0110413",  // Léon: The Professional (1994)
    "tt0211915",  // Amélie (2001)
    "tt0119116",  // The Fifth Element (1997)
    "tt2278871",  // Blue Is the Warmest Color (2013)
    "tt0113247",  // La Haine (1995)
    "tt0250223",  // Astérix & Obélix: Mission Cléopâtre (2002)
    "tt1655442",  // The Artist (2011)
    "tt4954522",  // Raw (2017)
    "tt1255953",  // Incendies (2010)
    "tt17009710", // Anatomy of a Fall (2023)
    "tt8613070",  // Portrait of a Lady on Fire (2019)
    "tt3612616",  // Mommy (2014)
    "tt0372824",  // The Chorus (2004)
    "tt0053198",  // The 400 Blows (1959)
    "tt0092593",  // Au Revoir les Enfants (1987)
    "tt0101765",  // The Double Life of Véronique (1991)
    "tt0101700",  // Delicatessen (1991)
    "tt0108394",  // Three Colors: Blue (1993)
    "tt0338135",  // The Barbarian Invasions (2003)
    "tt0387898",  // Caché (2005)
    "tt0401383",  // The Diving Bell and the Butterfly (2007)
    "tt0362225",  // Tell No One (2006)
    "tt1235166",  // A Prophet (2009)
    "tt10944760", // Titane (2021)
    "tt0070460",  // Day for Night (1973)
    "tt0046911",  // Les Diaboliques (1955)
    "tt0046268",  // The Wages of Fear (1953)
    "tt0082269",  // Diva (1981)
    "tt0152930",  // Taxi (1998)
    "tt0091288",  // Jean de Florette (1986)
    "tt0091480",  // Manon des Sources (1986)
    "tt0060474",  // La Grande Vadrouille (1966)
    "tt0108500",  // Les Visiteurs (1993)
    "tt1650048",  // Laurence Anyways (2012)
    // Spanish
    "tt0457430",  // Pan's Labyrinth (2006)
    "tt4857264",  // The Invisible Guest (2017)
    "tt1038988",  // [REC] (2007)
    "tt1189073",  // The Skin I Live In (2011)
    "tt6155172",  // Roma (2018)
    "tt3011894",  // Wild Tales (2014)
    "tt16277242", // Society of the Snow (2023)
    "tt0464141",  // The Orphanage (2007)
    "tt0245712",  // Amores Perros (2000)
    "tt1305806",  // The Secret in Their Eyes (2009)
    "tt0185125",  // All About My Mother (1999)
    "tt8291806",  // Pain and Glory (2019)
    "tt0441909",  // Volver (2006)
    "tt0245574",  // Y Tu Mamá También (2001)
    "tt0125659",  // Open Your Eyes (1997)
    "tt0095675",  // Women on the Verge of a Nervous Breakdown (1988)
    "tt0256009",  // The Devil's Backbone (2001)
    "tt5639354",  // A Fantastic Woman (2017)
    "tt8228288",  // The Platform (2019)
    "tt0319769",  // Mondays in the Sun (2002)
    "tt2059255",  // No (2012)
    "tt1422032",  // Even the Rain (2011)
    "tt0318462",  // The Motorcycle Diaries (2004)
    "tt0070040",  // The Spirit of the Beehive (1973)
    "tt0287467",  // Talk to Her (2002)
    "tt0117883",  // Tesis (1996)
    // German
    "tt1016150", // All Quiet on the Western Front (2022)
    "tt0088323", // The NeverEnding Story (1984)
    "tt0405094", // The Lives of Others (2006)
    "tt1063669", // The Wave (2008)
    "tt0017136", // Metropolis (1927)
    "tt0301357", // Good Bye, Lenin! (2003)
    "tt0130827", // Run Lola Run (1998)
    "tt0082096", // Das Boot (1981)
    "tt0022100", // M (1931)
    "tt0013442", // Nosferatu (1922)
    "tt0119167", // Funny Games (1997)
    "tt2987732", // Suck Me Shakespeer (2013)
    "tt3042408", // Who Am I (2014)
    "tt0093191", // Wings of Desire (1987)
    "tt4226388", // Victoria (2015)
    "tt0068182", // Aguirre, the Wrath of God (1972)
    "tt4048272", // Toni Erdmann (2016)
    "tt4176826", // Look Who's Back (2015)
    "tt6763252", // The Captain (2018)
    "tt0813547", // The Counterfeiters (2007)
    "tt1149362", // The White Ribbon (2009)
    "tt0078875", // The Tin Drum (1979)
    "tt8535968", // System Crasher (2019)
    "tt0250258", // The Experiment (2001)
    "tt0363163", // Downfall (2004)
    "tt0765432", // The Baader Meinhof Complex (2008)
    "tt0347048", // Head-On (2004)
    "tt1954701", // A Coffee in Berlin (2012)
    // Korean
    "tt6751668",  // Parasite (2019)
    "tt4016934",  // The Handmaiden (2016)
    "tt0364569",  // Oldboy (2003)
    "tt5700672",  // Train to Busan (2016)
    "tt0353969",  // Memories of Murder (2003)
    "tt1588170",  // I Saw the Devil (2010)
    "tt5215952",  // The Wailing (2016)
    "tt0451094",  // Lady Vengeance (2005)
    "tt7282468",  // Burning (2018)
    "tt1216496",  // Mother (2009)
    "tt1190539",  // The Chaser (2008)
    "tt1527788",  // The Man from Nowhere (2010)
    "tt12477480", // Decision to Leave (2022)
    "tt0365376",  // A Tale of Two Sisters (2003)
    "tt0423866",  // 3-Iron (2004)
    "tt0468492",  // The Host (2006)
    "tt6878038",  // A Taxi Driver (2017)
    "tt0762073",  // Thirst (2009)
    "tt0374546",  // Spring, Summer, Fall, Winter... and Spring (2003)
    "tt0817225",  // Secret Sunshine (2007)
    "tt13056052", // Broker (2022)
    "tt0310775",  // Sympathy for Mr. Vengeance (2002)
    // Chinese (Mandarin & Cantonese)
    "tt0373074",  // Kung Fu Hustle (2004)
    "tt0190332",  // Crouching Tiger, Hidden Dragon (2000)
    "tt0299977",  // Hero (2002)
    "tt0385004",  // House of Flying Daggers (2004)
    "tt0446059",  // Fearless (2006)
    "tt0425637",  // Red Cliff (2008)
    "tt10627720", // Ne Zha (2019)
    "tt0101640",  // Raise the Red Lantern (1991)
    "tt0808357",  // Lust, Caution (2007)
    "tt0106332",  // Farewell My Concubine (1993)
    "tt0118694",  // In the Mood for Love (2000)
    "tt0109424",  // Chungking Express (1994)
    "tt0338564",  // Infernal Affairs (2002)
    "tt0111797",  // Eat Drink Man Woman (1994)
    "tt1220719",  // Ip Man (2008)
    "tt4701660",  // The Mermaid (2016)
    "tt0112913",  // Fallen Angels (1995)
    "tt0110081",  // To Live (1994)
    "tt0118845",  // Happy Together (1997)
    "tt0093978",  // A Chinese Ghost Story (1987)
    "tt0423176",  // The World (2004)
    "tt0859765",  // Still Life (2006)
    "tt0286112",  // Shaolin Soccer (2001)
    "tt0188766",  // King of Comedy (1999)
    "tt0116426",  // The God of Cookery (1996)
    "tt0092263",  // A Better Tomorrow (1986)
    "tt0089374",  // Police Story (1985)
    "tt0103285",  // Once Upon a Time in China (1991)
    "tt0244316",  // Yi Yi (2000)
    "tt0101258",  // Days of Being Wild (1990)
    "tt0212712",  // 2046 (2004)
    "tt1462900",  // The Grandmaster (2013)
    "tt0117905",  // Comrades: Almost a Love Story (1996)
    "tt0209189",  // Not One Less (1999)
    "tt0235060",  // The Road Home (1999)
    "tt0215369",  // Shower (1999)
    "tt0107156",  // The Wedding Banquet (1993)
    "tt0234837",  // Suzhou River (2000)
    "tt0276501",  // Beijing Bicycle (2001)
    "tt0111786",  // In the Heat of the Sun (1994)
    "tt0434008",  // Election (2005)
    "tt1267160",  // Cape No. 7 (2008)
    "tt2036416",  // You Are the Apple of My Eye (2011)
    "tt4967094",  // Our Times (2015)
    "tt6054290",  // Soulmate (2016)
    "tt8033592",  // Us and Them (2018)
    "tt10883506", // A Sun (2019)
    "tt5290882",  // Detective Chinatown (2015)
    "tt2459022",  // Lost in Thailand (2012)
    "tt7362036",  // Dying to Survive (2018)
    "tt9586294",  // Better Days (2019)
    "tt7131870",  // Wolf Warrior 2 (2017)
    "tt7605074",  // The Wandering Earth (2019)
    "tt13539646", // The Wandering Earth II (2023)
    "tt13364790", // Hi, Mom (2021)
    "tt28151876", // Yolo (2024)
    "tt21148018", // Full River Red (2023)
    "tt34956443", // Ne Zha 2 (2025)
    "tt25434854", // Deep Sea (2023)
    "tt9288776",  // White Snake (2019)
    "tt1920885",  // Big Fish & Begonia (2016)
    // Japanese
    "tt0347149",  // Howl's Moving Castle (2004)
    "tt0245429",  // Spirited Away (2001)
    "tt0119698",  // Princess Mononoke (1997)
    "tt1278060",  // The Garden of Sinners: Paradox Spiral (2008)
    "tt0408664",  // Nobody Knows (2004)
    "tt5311514",  // Your Name. (2016)
    "tt0096283",  // My Neighbor Totoro (1988)
    "tt0095327",  // Grave of the Fireflies (1988)
    "tt0876563",  // Ponyo (2008)
    "tt0094625",  // Akira (1988)
    "tt0092067",  // Castle in the Sky (1986)
    "tt0097814",  // Kiki's Delivery Service (1989)
    "tt5323662",  // A Silent Voice (2016)
    "tt0047478",  // Seven Samurai (1954)
    "tt0087544",  // Nausicaä of the Valley of the Wind (1984)
    "tt0113568",  // Ghost in the Shell (1995)
    "tt0266308",  // Battle Royale (2000)
    "tt0104652",  // Porco Rosso (1992)
    "tt2013293",  // The Wind Rises (2013)
    "tt6587046",  // The Boy and the Heron (2023)
    "tt8075192",  // Shoplifters (2018)
    "tt1568921",  // The Secret World of Arrietty (2010)
    "tt2576852",  // The Tale of The Princess Kaguya (2013)
    "tt9426210",  // Weathering with You (2019)
    "tt16428256", // Suzume (2022)
    "tt11032374", // Demon Slayer: Mugen Train (2020)
    "tt1069238",  // Departures (2008)
    "tt0044741",  // Ikiru (1952)
    "tt0042876",  // Rashomon (1950)
    "tt0055630",  // Yojimbo (1961)
    "tt0092048",  // Tampopo (1985)
    "tt0235198",  // Audition (2000)
    "tt0046438",  // Tokyo Story (1953)
    "tt0156887",  // Perfect Blue (1998)
    "tt3398268",  // When Marnie Was There (2014)
    // Hindi (plus Telugu films with Hindi releases)
    "tt0169102",  // Lagaan: Once Upon a Time in India (2001)
    "tt0292490",  // Dil Chahta Hai (2001)
    "tt0986264",  // Like Stars on Earth (2007)
    "tt1187043",  // 3 Idiots (2009)
    "tt1954470",  // Gangs of Wasseypur - Part 1 (2012)
    "tt2338151",  // PK (2014)
    "tt3322420",  // Queen (2014)
    "tt8108198",  // Andhadhun (2018)
    "tt0374887",  // Munna Bhai M.B.B.S. (2003)
    "tt0405508",  // Rang De Basanti (2006)
    "tt0347304",  // Kal Ho Naa Ho (2003)
    "tt0367110",  // Swades (2004)
    "tt2631186",  // Bāhubali: The Beginning (2015)
    "tt5074352",  // Dangal (2016)
    "tt4635372",  // Masaan (2015)
    "tt3390572",  // Haider (2014)
    "tt8178634",  // RRR (2022)
    "tt3767372",  // Piku (2015)
    "tt10324144", // Article 15 (2019)
    "tt0150992",  // Hum Dil De Chuke Sanam (1999)
    "tt0164538",  // Dil Se.. (1998)
    "tt8239946",  // Tumbbad (2018)
    "tt2395469",  // Gully Boy (2019)
    "tt3863552",  // Bajrangi Bhaijaan (2015)
    "tt0238936",  // Devdas (2002)
    "tt0375611",  // Black (2005)
    "tt2082197",  // Barfi! (2012)
    "tt2356180",  // Bhaag Milkha Bhaag (2013)
    "tt1562872",  // Zindagi Na Milegi Dobara (2011)
    // Thai
    "tt1588895",  // Uncle Boonmee Who Can Recall His Past Lives (2010)
    "tt0381668",  // Tropical Malady (2004)
    "tt0477731",  // Syndromes and a Century (2006)
    "tt2818654",  // Cemetery of Splendor (2015)
    "tt0217680",  // Nang Nak (1999)
    "tt0440803",  // Shutter (2004)
    "tt31392609", // How to Make Millions Before Grandma Dies (2024)
    "tt2776344",  // Pee Mak (2013)
    "tt6788942",  // Bad Genius (2017)
    "tt0368909",  // Ong-Bak: The Thai Warrior (2003)
    "tt0345549",  // Last Life in the Universe (2003)
    "tt0269217",  // Tears of the Black Tiger (2000)
    "tt0269587",  // Mysterious Object at Noon (2000)
    "tt0415046",  // The Overture (2004)
    "tt1844735",  // Mekong Hotel (2012)
    // Russian & Soviet
    "tt0079944",  // Stalker (1979)
    "tt0069293",  // Solaris (1972)
    "tt0091251",  // Come and See (1985)
    "tt0072443",  // Mirror (1975)
    "tt2802154",  // Leviathan (2014)
    "tt0060107",  // Andrei Rublev (1966)
    "tt6304162",  // Loveless (2017)
    "tt0376968",  // The Return (2003)
    "tt0318034",  // Russian Ark (2002)
    "tt0118767",  // Brother (1997)
    "tt0050634",  // The Cranes Are Flying (1957)
    "tt0111579",  // Burnt by the Sun (1994)
    "tt0091341",  // Kin-dza-dza! (1986)
    "tt0079579",  // Moscow Does Not Believe in Tears (1980)
    "tt1925421",  // Elena (2011)
    "tt0403358",  // Night Watch (2004)
    "tt0015648",  // Battleship Potemkin (1925)
    "tt0056111",  // Ivan's Childhood (1962)
    "tt0063794",  // War and Peace (1968)
    "tt0062759",  // The Diamond Arm (1969)
    "tt0073179",  // The Irony of Fate (1976)
    "tt0076727",  // Office Romance (1977)
    "tt0084345",  // My Friend Ivan Lapshin (1985)
    "tt0097561",  // The Needle (1989)
    "tt0095574",  // Little Vera (1988)
    "tt0093754",  // Repentance (1987)
    "tt0101003",  // Freeze, Die, Come to Life (1990)
    "tt0100757",  // Taxi Blues (1990)
    "tt0096841",  // The Asthenic Syndrome (1989)
    "tt0238883",  // Brother 2 (2000)
    "tt0124207",  // The Thief (1997)
    "tt0116754",  // Prisoner of the Mountains (1996)
    "tt0156701",  // Khrustalyov, My Car! (1999)
    "tt10199640", // Beanpole (2019)
    // Portuguese
    "tt0317248",  // City of God (2002)
    "tt0861739",  // Elite Squad (2007)
    "tt1555149",  // Elite Squad: The Enemy Within (2010)
    "tt0271383",  // A Dog's Will (2000)
    "tt2762506",  // Bacurau (2019)
    "tt0140888",  // Central Station (1998)
    "tt14961016", // I'm Still Here (2024)
    "tt3742378",  // The Second Mother (2015)
    "tt5221584",  // Aquarius (2016)
    "tt0293007",  // Carandiru (2003)
    "tt0082912",  // Pixote (1980)
    "tt1702014",  // The Way He Looks (2014)
    "tt0317887",  // Madame Satã (2002)
    // Italian
    "tt0095765",  // Cinema Paradiso (1988)
    "tt0076085",  // A Special Day (1977)
    "tt0118799",  // Life Is Beautiful (1997)
    "tt0060196",  // The Good, the Bad and the Ugly (1966)
    "tt0064116",  // Once Upon a Time in the West (1968)
    "tt0058461",  // A Fistful of Dollars (1964)
    "tt4901306",  // Perfect Strangers (2016)
    "tt2358891",  // The Great Beauty (2013)
    "tt0076786",  // Suspiria (1977)
    "tt0040522",  // Bicycle Thieves (1948)
    "tt0056801",  // 8½ (1963)
    "tt0213847",  // Malèna (2000)
    "tt0120731",  // The Legend of 1900 (1998)
    "tt0053779",  // La Dolce Vita (1960)
    "tt0047528",  // La Strada (1954)
    "tt0038890",  // Rome, Open City (1945)
    "tt0110877",  // Il Postino (1994)
    "tt6752992",  // Happy as Lazzaro (2018)
    "tt12680684", // The Hand of God (2021)
    "tt0050783",  // Nights of Cabiria (1957)
    "tt0057091",  // The Leopard (1963)
    "tt0065571",  // The Conformist (1971)
    "tt0071129",  // Amarcord (1973)
    "tt0055913",  // Divorce Italian Style (1961)
    "tt0065889",  // Investigation of a Citizen Above Suspicion (1970)
    // Swedish
    "tt0091670", // The Sacrifice (1986)
    "tt0050986", // Wild Strawberries (1957)
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

    // Remove SSA/ASS override tags ({\an8}, {\i1}, {\pos(200,100)}, …) and
    // ASS escapes for line break / hard space
    let re_ssa = regex::Regex::new(r"\{\\[^}]*\}").unwrap();
    result = re_ssa.replace_all(&result, "").to_string();
    result = result.replace("\\N", " ").replace("\\h", " ");

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

    // Rank best-first: trusted, then most-downloaded, then best-rated. Tuple
    // ordering is ascending, so each key is negated to put the desirable value
    // first. Written as a key function rather than a chain of early returns
    // because the latter is easy to get subtly wrong — the previous version
    // reported `Less` when *both* sides were trusted, which is not a valid
    // ordering and left the sort arbitrary among trusted subtitles.
    subtitle_results.sort_by(|a, b| {
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
    Language::from_code(s).ok_or_else(|| {
        format!(
            "unsupported language code '{s}'. Supported: fra, eng, spa, deu, kor, zho, jpn, rus, por, ita, hin, tha"
        )
    })
}

/// Download movie subtitles from OpenSubtitles
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Language codes (ISO 639-3: fra, eng, spa, deu, kor, zho, jpn, rus, por, ita, hin, tha)
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
        let language_iso639_3 = language.code();
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

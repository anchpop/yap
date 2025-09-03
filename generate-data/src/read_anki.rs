use html2text::from_read;
use indexmap::IndexSet;
use rusqlite::{Connection, Result as SqlResult};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::{fmt, fs};
use tempfile::NamedTempFile;
use zip::ZipArchive;

#[derive(serde::Serialize, serde::Deserialize, Hash, Eq, PartialEq)]
pub struct CardOutput {
    pub target: Vec<String>,
    pub native: String,
}

#[derive(Debug)]
pub enum AnkiError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Sqlite(rusqlite::Error),
    InvalidDeck(String),
}

impl fmt::Display for AnkiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnkiError::Io(e) => write!(f, "IO error: {e}"),
            AnkiError::Zip(e) => write!(f, "ZIP error: {e}"),
            AnkiError::Sqlite(e) => write!(f, "SQLite error: {e}"),
            AnkiError::InvalidDeck(msg) => write!(f, "Invalid deck: {msg}"),
        }
    }
}

impl Error for AnkiError {}

impl From<std::io::Error> for AnkiError {
    fn from(err: std::io::Error) -> Self {
        AnkiError::Io(err)
    }
}

impl From<zip::result::ZipError> for AnkiError {
    fn from(err: zip::result::ZipError) -> Self {
        AnkiError::Zip(err)
    }
}

impl From<rusqlite::Error> for AnkiError {
    fn from(err: rusqlite::Error) -> Self {
        AnkiError::Sqlite(err)
    }
}

#[derive(Debug, Clone)]
pub struct Card {
    pub id: i64,
    pub note_id: i64,
    pub deck_id: i64,
    pub question: String,
    pub answer: String,
    pub question_html: String,
    pub answer_html: String,
    pub tags: Vec<String>,
    pub modified: i64,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: i64,
    pub guid: String,
    pub model_id: i64,
    pub fields: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug)]
pub struct Deck {
    pub id: i64,
    pub name: String,
    pub description: String,
}

pub struct AnkiReader {
    connection: Connection,
    _temp_file: NamedTempFile, // Keep the temp file alive
}

impl AnkiReader {
    /// Create a new AnkiReader from an .apkg file path
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, AnkiError> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        // Extract the SQLite database (usually named "collection.anki2")
        let mut db_data = Vec::new();
        let mut found_db = false;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_lowercase();

            if name.ends_with(".anki2") || name == "collection.db" {
                file.read_to_end(&mut db_data)?;
                found_db = true;
                break;
            }
        }

        if !found_db {
            return Err(AnkiError::InvalidDeck(
                "No Anki database found in archive".to_string(),
            ));
        }

        // Create a temporary file for the SQLite database
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&db_data)?;
        temp_file.flush()?;

        // Open the connection to the temporary file
        let connection = Connection::open(temp_file.path())?;

        Ok(AnkiReader {
            connection,
            _temp_file: temp_file, // Store the temp file to keep it alive
        })
    }

    /// Get all cards from the deck
    pub fn get_cards(&self) -> Result<Vec<Card>, AnkiError> {
        let mut stmt = self.connection.prepare(
            "SELECT c.id, c.nid, c.did, n.flds, n.tags, c.mod, n.mid
             FROM cards c
             JOIN notes n ON c.nid = n.id",
        )?;

        let cards = stmt.query_map([], |row| {
            let card_id: i64 = row.get(0)?;
            let note_id: i64 = row.get(1)?;
            let deck_id: i64 = row.get(2)?;
            let fields: String = row.get(3)?;
            let tags: String = row.get(4)?;
            let modified: i64 = row.get(5)?;
            let model_id: i64 = row.get(6)?;

            Ok((card_id, note_id, deck_id, fields, tags, modified, model_id))
        })?;

        let mut result = Vec::new();

        for card in cards {
            let (card_id, note_id, deck_id, fields, tags, modified, model_id) = card?;

            // Split fields by separator (ASCII 31)
            let field_vec: Vec<String> = fields.split('\x1f').map(|s| s.to_string()).collect();

            // Get the template for this card
            let (question, answer) = self.get_card_content(card_id, model_id, &field_vec)?;

            // Convert HTML to plain text
            let question_text =
                from_read(question.as_bytes(), 80).unwrap_or_else(|_| question.clone());
            let answer_text = from_read(answer.as_bytes(), 80).unwrap_or_else(|_| answer.clone());

            // Parse tags
            let tag_vec: Vec<String> = if tags.is_empty() {
                Vec::new()
            } else {
                tags.split(' ')
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .collect()
            };

            result.push(Card {
                id: card_id,
                note_id,
                deck_id,
                question: question_text,
                answer: answer_text,
                question_html: question,
                answer_html: answer,
                tags: tag_vec,
                modified,
            });
        }

        Ok(result)
    }

    /// Get card content with templates applied
    fn get_card_content(
        &self,
        _card_id: i64,
        _model_id: i64,
        fields: &[String],
    ) -> SqlResult<(String, String)> {
        // This is a simplified version - real Anki template processing is more complex
        // For now, we'll just return the first two fields as question/answer
        let question = fields.first().cloned().unwrap_or_default();
        let answer = fields.get(1).cloned().unwrap_or_default();

        Ok((question, answer))
    }

    /// Get all notes from the deck
    pub fn get_notes(&self) -> Result<Vec<Note>, AnkiError> {
        let mut stmt = self
            .connection
            .prepare("SELECT id, guid, mid, flds, tags FROM notes")?;

        let notes = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let guid: String = row.get(1)?;
            let model_id: i64 = row.get(2)?;
            let fields: String = row.get(3)?;
            let tags: String = row.get(4)?;

            let field_vec: Vec<String> = fields.split('\x1f').map(|s| s.to_string()).collect();
            let tag_vec: Vec<String> = if tags.is_empty() {
                Vec::new()
            } else {
                tags.split(' ')
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .collect()
            };

            Ok(Note {
                id,
                guid,
                model_id,
                fields: field_vec,
                tags: tag_vec,
            })
        })?;

        let mut result = Vec::new();
        for note in notes {
            result.push(note?);
        }

        Ok(result)
    }

    /// Get all decks
    pub fn get_decks(&self) -> Result<Vec<Deck>, AnkiError> {
        let mut stmt = self.connection.prepare("SELECT decks FROM col")?;
        let _decks_json: String = stmt.query_row([], |row| row.get(0))?;

        // Parse the JSON (simplified - you might want to use serde_json for real implementation)
        // For now, we'll return a simple default
        Ok(vec![Deck {
            id: 1,
            name: "Default".to_string(),
            description: "".to_string(),
        }])
    }

    /// Get cards by deck ID
    pub fn get_cards_by_deck(&self, deck_id: i64) -> Result<Vec<Card>, AnkiError> {
        let all_cards = self.get_cards()?;
        Ok(all_cards
            .into_iter()
            .filter(|c| c.deck_id == deck_id)
            .collect())
    }

    /// Get cards by tag
    pub fn get_cards_by_tag(&self, tag: &str) -> Result<Vec<Card>, AnkiError> {
        let all_cards = self.get_cards()?;
        Ok(all_cards
            .into_iter()
            .filter(|c| c.tags.iter().any(|t| t == tag))
            .collect())
    }

    /// Get a specific card by ID
    pub fn get_card(&self, card_id: i64) -> Result<Option<Card>, AnkiError> {
        let cards = self.get_cards()?;
        Ok(cards.into_iter().find(|c| c.id == card_id))
    }

    /// Get a specific note by ID
    pub fn get_note(&self, note_id: i64) -> Result<Option<Note>, AnkiError> {
        let notes = self.get_notes()?;
        Ok(notes.into_iter().find(|n| n.id == note_id))
    }
}

// Example usage:
//
// use anki_reader::AnkiReader;
//
// fn main() -> Result<(), Box<dyn std::error::Error>> {
//     let reader = AnkiReader::from_file("my_deck.apkg")?;
//
//     // Get all cards
//     let cards = reader.get_cards()?;
//     for card in &cards {
//         println!("Q: {}", card.question);
//         println!("A: {}", card.answer);
//         println!("Tags: {:?}", card.tags);
//         println!("---");
//     }
//
//     // Get cards by tag
//     let tagged_cards = reader.get_cards_by_tag("vocabulary")?;
//     println!("Found {} cards with 'vocabulary' tag", tagged_cards.len());
//
//     Ok(())
// }
//
// Cargo.toml dependencies:
// [dependencies]
// zip = "0.6"
// rusqlite = { version = "0.29", features = ["bundled"] }
// html2text = "0.4"
// tempfile = "3.8"

// This test is a bit goofy because it actually updates the docs/anki-schema.md file (if it needs updating).
// This means that it will fail the first time and pass the second time.
#[test]
fn test_db_schema_docs_up_to_date() -> Result<(), String> {
    // print current working directory
    println!(
        "Current working directory: {}",
        std::env::current_dir().unwrap().display()
    );

    // Get a test Anki file path
    let test_file_path = Path::new("./data/fra/sentence-sources/anki-decks/French_Sentences.apkg");

    // Create an AnkiReader instance
    let reader = AnkiReader::from_file(test_file_path)
        .map_err(|e| format!("Failed to open Anki file: {e}"))?;

    // Get database info
    let db_info = crate::db_info::get_db_info(&reader.connection)
        .map_err(|e| format!("Failed to get DB info: {e}"))?;

    let doc_file_dir =
        Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../docs/anki-schema.md");

    let current_doc_file_contents = std::fs::read_to_string(&doc_file_dir).unwrap_or_default();

    let tables = format!(
        "**Do not edit this file manually. It is automatically generated from the database schema.**\n\n# Anki Database Schema\n{db_info}"
    );

    if current_doc_file_contents != tables {
        // Create docs directory if it doesn't exist
        if let Some(parent) = doc_file_dir.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create docs directory: {e}"))?;
        }

        let mut doc_file = File::create(&doc_file_dir)
            .map_err(|e| format!("Failed to create {}: {}", doc_file_dir.display(), e))?;
        doc_file
            .write_all(tables.as_bytes())
            .map_err(|e| format!("Failed to write to file: {e}"))?;
        return Err("Schema has changed. The `docs/anki-schema.md` file has been updated. This test should pass if you run it again.".to_string());
    }
    Ok(())
}

pub fn get_all_cards(source_data_path: &Path) -> IndexSet<CardOutput> {
    let anki_decks_dir = source_data_path
        .join("sentence-sources/anki-decks")
        .canonicalize()
        .unwrap();

    // Check if directory exists
    if !anki_decks_dir.exists() {
        panic!("Directory '{}' not found!", anki_decks_dir.display());
    }

    let mut found_cards = Vec::new();

    // Read all .apkg files in the directory
    match fs::read_dir(anki_decks_dir.clone()) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();

                // Check if it's an .apkg file
                if path.extension().and_then(|s| s.to_str()) == Some("apkg") {
                    let deck_filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    println!("Processing deck: {deck_filename}");

                    match AnkiReader::from_file(&path) {
                        Ok(reader) => match reader.get_cards() {
                            Ok(cards) => {
                                println!("Found {} cards in {}", cards.len(), deck_filename);

                                for card in cards.iter() {
                                    if !card.question.is_empty() && !card.answer.is_empty() {
                                        let card_output = CardOutput {
                                            target: card
                                                .question
                                                .trim()
                                                .split("\n")
                                                .map(|s| s.to_string())
                                                .collect(),
                                            native: card.answer.trim().to_string(),
                                        };

                                        found_cards.push(card_output);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading cards from {deck_filename}: {e}")
                            }
                        },
                        Err(e) => eprintln!("Error opening deck {deck_filename}: {e}"),
                    }
                }
            }
        }
        Err(e) => eprintln!(
            "Error reading directory {}: {}",
            anki_decks_dir.display(),
            e
        ),
    }

    found_cards.into_iter().collect()
}

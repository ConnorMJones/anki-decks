//! End-to-end checks: parsed entries -> deck definition -> .apkg, verified by
//! opening the collection database inside the package rather than trusting that
//! the write returned Ok.

use ptx::deck;
use ptx::module::Module;
use ptx::parse::{ArchReq, Entry, Qualifier};
use rusqlite::Connection;
use std::io::Read;

fn entry(id: &str, section: &str, name: &str, gloss: &str) -> Entry {
    Entry {
        id: id.into(),
        section: section.into(),
        name: name.into(),
        gloss: gloss.into(),
        syntax: Some(format!("{name}.type d, a, b;")),
        qualifiers: vec![Qualifier {
            name: "type".into(),
            values: vec![".u32".into(), ".s32".into()],
        }],
        description: vec![],
        semantics: None,
        since_ptx: Some("2.0".into()),
        arch: ArchReq::Min("sm_20".into()),
        examples: None,
    }
}

fn inputs() -> Vec<deck::Input> {
    vec![
        deck::Input {
            module: Module::Core,
            category: "ptx::integer-arithmetic".into(),
            entry: entry("integer-arithmetic-instructions-bfe", "9.7.1.20", "bfe", "Bit Field Extract."),
        },
        deck::Input {
            module: Module::Concepts,
            category: "ptx::special-registers".into(),
            entry: entry("special-registers-tid", "10.1", "%tid", "Thread identifier within a CTA."),
        },
    ]
}

/// Build the deck, write a .apkg, and hand back an open connection to the
/// collection database extracted from it.
fn build_and_open(dir: &std::path::Path) -> Connection {
    let definition = deck::build(&inputs(), "9.3");
    let toml_text = toml::to_string_pretty(&definition).expect("serialise deck definition");

    let apkg = dir.join("test.apkg");
    ankit_builder::DeckBuilder::parse(&toml_text)
        .expect("ankit-builder should accept our TOML")
        .write_apkg(&apkg)
        .expect("write apkg");

    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&apkg).expect("open apkg")).expect("read zip");
    let mut bytes = Vec::new();
    archive
        .by_name("collection.anki2")
        .expect("apkg must contain collection.anki2")
        .read_to_end(&mut bytes)
        .expect("read collection");

    let db_path = dir.join("collection.anki2");
    std::fs::write(&db_path, &bytes).expect("write collection");
    Connection::open(&db_path).expect("open collection")
}

/// Deck names out of the collection, spanning both Anki schemas: the legacy
/// one keeps a JSON blob in `col.decks`, newer ones use a `decks` table.
fn deck_names(db: &Connection) -> Vec<String> {
    let names: Vec<String> = match db.prepare("SELECT name FROM decks") {
        Ok(mut stmt) => stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect(),
        Err(_) => {
            let json: String =
                db.query_row("SELECT decks FROM col", [], |r| r.get(0)).expect("col.decks");
            // Pull each "name":"..." out without taking a JSON dependency.
            json.split("\"name\":\"")
                .skip(1)
                .filter_map(|rest| rest.split('"').next())
                .map(|s| s.replace("\\u001f", "::"))
                .collect()
        }
    };
    // Nested deck names use \x1f separators in newer schemas, "::" in older.
    names.into_iter().map(|n| n.replace('\u{1f}', "::")).collect()
}

#[test]
fn apkg_contains_every_generated_note() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_and_open(dir.path());

    let expected = deck::build(&inputs(), "9.3").notes.len();
    let actual: i64 =
        db.query_row("SELECT count(*) FROM notes", [], |r| r.get(0)).unwrap();
    assert_eq!(actual as usize, expected, "every note should reach the package");

    let cards: i64 = db.query_row("SELECT count(*) FROM cards", [], |r| r.get(0)).unwrap();
    assert!(cards > actual, "multi-template models should yield more cards than notes");
}

#[test]
fn apkg_preserves_our_stable_guids() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_and_open(dir.path());

    // The whole update story depends on our GUIDs surviving into the package
    // unchanged -- if ankit-builder regenerated them, re-imports would create
    // duplicates instead of updating in place.
    let mut stmt = db.prepare("SELECT guid FROM notes").unwrap();
    let in_db: std::collections::BTreeSet<String> =
        stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();

    let expected: std::collections::BTreeSet<String> =
        deck::build(&inputs(), "9.3").notes.into_iter().map(|n| n.guid).collect();

    assert_eq!(in_db, expected);
}

#[test]
fn apkg_builds_the_subdeck_tree() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_and_open(dir.path());

    let names = deck_names(&db);

    for expected in [
        "PTX ISA::Core Instructions::Recall",
        "PTX ISA::Core Instructions::Syntax",
        "PTX ISA::Core Instructions::Qualifiers",
        "PTX ISA::Core Instructions::Arch",
        "PTX ISA::Concepts::Recall",
    ] {
        assert!(names.iter().any(|n| n == expected), "missing deck {expected}; have {names:?}");
    }
}

#[test]
fn apkg_field_content_survives_the_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db = build_and_open(dir.path());

    // Notes store fields joined by \x1f. Just check the text made it through.
    let mut stmt = db.prepare("SELECT flds FROM notes").unwrap();
    let all: Vec<String> =
        stmt.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
    let joined = all.join("\n");

    assert!(joined.contains("Bit Field Extract."));
    assert!(joined.contains("bfe"));
    assert!(joined.contains("%tid"));
    assert!(joined.contains("sm_20"));
}

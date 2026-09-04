use ptx::{deck, module, parse};

use std::path::PathBuf;

const USAGE: &str = "usage: ptx <ptx-isa.html> [--json | --toml | --apkg <out>] [--version <n>]

  --json          dump the parsed intermediate representation (default)
  --toml          emit an ankit-builder deck definition on stdout
  --apkg <out>    build an importable Anki package at <out>
  --version <n>   PTX ISA version to label the deck with (default 9.3)
  --modules <a,b> limit to some of core,instructions,concepts (default: all)
";

enum Output {
    Json,
    Toml,
    Apkg(PathBuf),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input: Option<PathBuf> = None;
    let mut output = Output::Json;
    let mut ptx_version = "9.3".to_string();
    let mut modules: Option<Vec<module::Module>> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--toml" => output = Output::Toml,
            "--json" => output = Output::Json,
            "--apkg" => {
                output = Output::Apkg(args.next().ok_or("--apkg needs an output path")?.into());
            }
            "--version" => {
                ptx_version = args.next().ok_or("--version needs a value")?;
            }
            "--modules" => {
                let value = args.next().ok_or("--modules needs a value")?;
                let mut selected = Vec::new();
                for slug in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    selected.push(
                        module::Module::parse_slug(slug)
                            .ok_or_else(|| format!("unknown module: {slug}"))?,
                    );
                }
                modules = Some(selected);
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other if input.is_none() => input = Some(other.into()),
            other => return Err(format!("unexpected argument: {other}").into()),
        }
    }
    let Some(input) = input else {
        eprint!("{USAGE}");
        std::process::exit(2);
    };

    let html = std::fs::read_to_string(&input)?;
    let entries = parse::parse(&html);

    let classified: Vec<deck::Input> = entries
        .into_iter()
        .filter_map(|entry| {
            module::classify(&entry.section)
                .map(|(module, category)| deck::Input { module, category, entry })
        })
        // Filtering drops whole subdecks but never renumbers the rest: GUIDs are
        // per-entry, so adding a module later leaves existing notes untouched.
        .filter(|i| modules.as_ref().is_none_or(|want| want.contains(&i.module)))
        .collect();

    if classified.is_empty() {
        return Err("no entries matched; check --modules".into());
    }

    report_coverage(&classified);

    match output {
        Output::Json => {
            let out: Vec<_> = classified
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "module": i.module, "category": i.category, "entry": i.entry
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Output::Toml => {
            let definition = deck::build(&classified, &ptx_version);
            report_notes(&definition);
            println!("{}", toml::to_string_pretty(&definition)?);
        }
        Output::Apkg(path) => {
            let definition = deck::build(&classified, &ptx_version);
            report_notes(&definition);
            // Round-tripping through TOML rather than constructing
            // ankit_builder's types directly keeps the checked-in deck.toml and
            // the .apkg provably the same deck.
            let toml_text = toml::to_string_pretty(&definition)?;
            ankit_builder::DeckBuilder::parse(&toml_text)?.write_apkg(&path)?;
            eprintln!("\nwrote {}", path.display());
        }
    }
    Ok(())
}

/// Per-module field coverage, written to stderr so stdout stays pipeable.
/// Each column is a field a card kind depends on, so this doubles as a report
/// of how many cards of each kind the deck can actually produce.
fn report_coverage(classified: &[deck::Input]) {
    use module::Module::*;

    eprintln!(
        "{:<18} {:>6} {:>7} {:>7} {:>7} {:>7} {:>10} {:>9}",
        "module", "count", "gloss", "syntax", "quals", "sm", "semantics", "examples"
    );
    for m in [Core, Instructions, Concepts] {
        let rows: Vec<_> = classified.iter().filter(|i| i.module == m).collect();
        if rows.is_empty() {
            continue;
        }
        let n = |f: fn(&parse::Entry) -> bool| rows.iter().filter(|i| f(&i.entry)).count();
        eprintln!(
            "{:<18} {:>6} {:>7} {:>7} {:>7} {:>7} {:>10} {:>9}",
            m.deck_name(),
            rows.len(),
            n(|e| !e.gloss.is_empty()),
            n(|e| e.syntax.is_some()),
            n(|e| !e.qualifiers.is_empty()),
            n(|e| e.arch != parse::ArchReq::Unknown),
            n(|e| e.semantics.is_some()),
            n(|e| e.examples.is_some()),
        );
    }
}

/// Note and card counts per subdeck, so the size of each family is visible
/// before importing.
fn report_notes(d: &deck::DeckDefinition) {
    use std::collections::BTreeMap;

    // Every model emits one card per template unconditionally -- no template is
    // gated behind a `{{#Field}}` conditional, precisely so this count matches
    // what Anki ends up with.
    let cards_per_note: BTreeMap<&str, usize> =
        d.models.iter().map(|m| (m.name.as_str(), m.templates.len())).collect();

    let mut per_deck: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for n in &d.notes {
        let e = per_deck.entry(n.deck.as_str()).or_default();
        e.0 += 1;
        e.1 += cards_per_note.get(n.model.as_str()).copied().unwrap_or(1);
    }

    eprintln!("\n{:<44} {:>6} {:>6}", "deck", "notes", "cards");
    let (mut tn, mut tc) = (0, 0);
    for (deck, (notes, cards)) in &per_deck {
        eprintln!("{deck:<44} {notes:>6} {cards:>6}");
        tn += notes;
        tc += cards;
    }
    eprintln!("{:<44} {:>6} {:>6}", "TOTAL", tn, tc);
}

//! Deck generation: turns parsed entries into an ankit-builder deck definition.
//!
//! # Stability contract
//!
//! Regenerating this deck must never cost you review history. Two rules make
//! that work, and both are load-bearing:
//!
//! 1. **Note GUIDs are derived from identity, not content.** A GUID is a hash of
//!    the doc's section id plus the card kind (plus the qualifier name, where a
//!    note is per-qualifier) -- never of the field text. Reword a gloss, fix a
//!    parser bug, or bump to a newer PTX ISA release, and Anki still recognises
//!    the note and updates it in place.
//! 2. **Model IDs and field lists are frozen.** Anki only matches notes by GUID
//!    when the note type is unchanged, so the constants below must not be edited
//!    once you have studied the deck. Adding a *new* card kind is safe; changing
//!    the fields of an existing one is not.

use crate::module::Module;
use crate::parse::{ArchReq, Entry};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

// Frozen model IDs. Never change these; see the stability contract above.
const MODEL_RECALL: i64 = 1_723_400_001;
const MODEL_SYNTAX: i64 = 1_723_400_002;
const MODEL_QUALIFIERS: i64 = 1_723_400_003;
const MODEL_ARCH: i64 = 1_723_400_004;
const MODEL_VERSION: i64 = 1_723_400_005;

const DECK_ROOT: &str = "PTX ISA";

/// The card families. Each maps to a subdeck under every scope module, so an
/// unwanted family can be suspended or skipped wholesale.
///
/// `Arch` and `Version` share the "Arch" subdeck but are separate models with a
/// single template each. Splitting them matters: ankit-builder writes one card
/// per template without evaluating `{{#Field}}` conditionals, so a combined
/// two-template model would emit a blank-fronted card for every entry that has
/// only one of the two facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Recall,
    Syntax,
    Qualifiers,
    Arch,
    Version,
}

impl CardKind {
    pub fn deck_name(self) -> &'static str {
        match self {
            CardKind::Recall => "Recall",
            CardKind::Syntax => "Syntax",
            CardKind::Qualifiers => "Qualifiers",
            CardKind::Arch | CardKind::Version => "Arch",
        }
    }

    fn model_name(self) -> &'static str {
        match self {
            CardKind::Recall => "PTX Recall",
            CardKind::Syntax => "PTX Syntax",
            CardKind::Qualifiers => "PTX Qualifiers",
            CardKind::Arch => "PTX Arch",
            CardKind::Version => "PTX Version",
        }
    }

    /// Short tag used in the GUID seed. Frozen alongside the model IDs.
    fn seed(self) -> &'static str {
        match self {
            CardKind::Recall => "recall",
            CardKind::Syntax => "syntax",
            CardKind::Qualifiers => "qual",
            CardKind::Arch => "arch",
            CardKind::Version => "version",
        }
    }
}

// --- ankit-builder deck definition schema -----------------------------------
// Mirrors ankit_builder::DeckDefinition. Field order matters: the TOML
// serialiser must emit scalars before tables, so map-valued fields come last.

#[derive(Debug, Serialize)]
pub struct DeckDefinition {
    pub package: PackageInfo,
    pub models: Vec<ModelDef>,
    pub decks: Vec<DeckDef>,
    pub notes: Vec<NoteDef>,
}

#[derive(Debug, Serialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct ModelDef {
    pub name: String,
    pub id: i64,
    pub fields: Vec<String>,
    pub css: String,
    pub templates: Vec<TemplateDef>,
}

#[derive(Debug, Serialize)]
pub struct TemplateDef {
    pub name: String,
    pub front: String,
    pub back: String,
}

#[derive(Debug, Serialize)]
pub struct DeckDef {
    pub name: String,
    pub id: i64,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct NoteDef {
    pub deck: String,
    pub model: String,
    pub tags: Vec<String>,
    pub guid: String,
    pub fields: BTreeMap<String, String>,
}

// --- GUID and id derivation -------------------------------------------------

/// Stable note GUID. Seeded only from identity, so field edits never orphan a
/// note's scheduling history.
fn guid(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    h.update(b"ptx-cards/v1");
    for p in parts {
        h.update(b"\x1f");
        h.update(p.as_bytes());
    }
    let digest = h.finalize();
    // Anki accepts any stable string; 16 hex chars is ample against collision
    // across a few thousand notes.
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministic deck id, so re-imports land in the same deck rather than
/// creating a duplicate tree.
fn deck_id(name: &str) -> i64 {
    let mut h = Sha256::new();
    h.update(b"ptx-cards/deck/v1");
    h.update(name.as_bytes());
    let d = h.finalize();
    let n = i64::from_be_bytes(d[..8].try_into().unwrap());
    // Anki deck ids are positive epoch-millis-like integers.
    (n.abs() % 1_000_000_000_000) + 1_600_000_000_000
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// --- Models -----------------------------------------------------------------

const CSS: &str = r#".card {
  font-family: -apple-system, Segoe UI, Roboto, sans-serif;
  font-size: 18px;
  text-align: center;
  color: #1a1a1a;
  background: #fdfdfd;
}
.nightMode.card, .night_mode .card { color: #e8e8e8; background: #2c2c2c; }
.mnemonic { font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 26px; font-weight: 600; }
.gloss { font-size: 20px; }
pre.ptx { font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 15px; text-align: left; display: inline-block;
  background: #f2f2f2; border-radius: 6px; padding: 10px 14px; margin: 8px 0;
  white-space: pre; overflow-x: auto; max-width: 100%; }
.nightMode pre.ptx, .night_mode pre.ptx { background: #3a3a3a; }
.meta { font-size: 13px; color: #888; margin-top: 10px; }
.prompt { font-size: 16px; color: #666; }
.nightMode .prompt, .night_mode .prompt { color: #aaa; }
hr { border: none; border-top: 1px solid #ccc; margin: 14px 0; }
"#;

fn models() -> Vec<ModelDef> {
    vec![
        // Recall: both directions off one note. Recognition teaches you to read
        // PTX; production -- the reverse card -- is what teaches you to write it.
        ModelDef {
            name: CardKind::Recall.model_name().into(),
            id: MODEL_RECALL,
            fields: vec!["Name".into(), "Gloss".into(), "Kind".into(), "Section".into()],
            css: CSS.into(),
            templates: vec![
                TemplateDef {
                    name: "Recognition".into(),
                    front: "<div class=\"mnemonic\">{{Name}}</div>".into(),
                    back: "{{FrontSide}}<hr><div class=\"gloss\">{{Gloss}}</div>\
                           <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                        .into(),
                },
                TemplateDef {
                    name: "Production".into(),
                    front: "<div class=\"prompt\">Which PTX {{Kind}}?</div>\
                            <div class=\"gloss\">{{Gloss}}</div>"
                        .into(),
                    back: "{{FrontSide}}<hr><div class=\"mnemonic\">{{Name}}</div>\
                           <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                        .into(),
                },
            ],
        },
        ModelDef {
            name: CardKind::Syntax.model_name().into(),
            id: MODEL_SYNTAX,
            fields: vec!["Name".into(), "Syntax".into(), "Gloss".into(), "Section".into()],
            css: CSS.into(),
            templates: vec![TemplateDef {
                name: "Syntax".into(),
                front: "<div class=\"prompt\">Give the syntax of</div>\
                        <div class=\"mnemonic\">{{Name}}</div>\
                        <div class=\"meta\">{{Gloss}}</div>"
                    .into(),
                back: "{{FrontSide}}<hr><pre class=\"ptx\">{{Syntax}}</pre>\
                       <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                    .into(),
            }],
        },
        // One note per (entry, qualifier) rather than one card listing every
        // qualifier: each card then asks a single question.
        ModelDef {
            name: CardKind::Qualifiers.model_name().into(),
            id: MODEL_QUALIFIERS,
            fields: vec![
                "Name".into(),
                "Qualifier".into(),
                "Values".into(),
                "Gloss".into(),
                "Section".into(),
            ],
            css: CSS.into(),
            templates: vec![TemplateDef {
                name: "Qualifier values".into(),
                front: "<div class=\"prompt\">Which <code>.{{Qualifier}}</code> \
                        values does this accept?</div>\
                        <div class=\"mnemonic\">{{Name}}</div>\
                        <div class=\"meta\">{{Gloss}}</div>"
                    .into(),
                back: "{{FrontSide}}<hr><pre class=\"ptx\">{{Values}}</pre>\
                       <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                    .into(),
            }],
        },
        // Architecture floor and introducing ISA version are separate models
        // with one template each, so an entry that has only one of the two
        // facts produces exactly one card and never a blank.
        ModelDef {
            name: CardKind::Arch.model_name().into(),
            id: MODEL_ARCH,
            fields: vec!["Name".into(), "Sm".into(), "Gloss".into(), "Section".into()],
            css: CSS.into(),
            templates: vec![TemplateDef {
                name: "Target arch".into(),
                front: "<div class=\"prompt\">Minimum target architecture for</div>\
                        <div class=\"mnemonic\">{{Name}}</div>\
                        <div class=\"meta\">{{Gloss}}</div>"
                    .into(),
                back: "{{FrontSide}}<hr><div class=\"mnemonic\">{{Sm}}</div>\
                       <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                    .into(),
            }],
        },
        ModelDef {
            name: CardKind::Version.model_name().into(),
            id: MODEL_VERSION,
            fields: vec!["Name".into(), "PtxVersion".into(), "Gloss".into(), "Section".into()],
            css: CSS.into(),
            templates: vec![TemplateDef {
                name: "ISA version".into(),
                front: "<div class=\"prompt\">Which PTX ISA version introduced</div>\
                        <div class=\"mnemonic\">{{Name}}</div>\
                        <div class=\"meta\">{{Gloss}}</div>"
                    .into(),
                back: "{{FrontSide}}<hr><div class=\"mnemonic\">PTX ISA {{PtxVersion}}</div>\
                       <div class=\"meta\">PTX ISA &sect;{{Section}}</div>"
                    .into(),
            }],
        },
    ]
}

/// What to call an entry on a production card: "Which PTX <kind>?"
fn kind_word(section: &str) -> &'static str {
    if section.starts_with("10.") {
        "special register"
    } else if section.starts_with("11.") {
        "directive"
    } else {
        "instruction"
    }
}

// --- Build ------------------------------------------------------------------

pub struct Input {
    pub module: Module,
    pub category: String,
    pub entry: Entry,
}

pub fn build(inputs: &[Input], ptx_version: &str) -> DeckDefinition {
    let mut decks: Vec<DeckDef> = Vec::new();
    let mut seen_decks = std::collections::BTreeSet::new();
    let mut notes = Vec::new();

    let push_deck = |decks: &mut Vec<DeckDef>,
                         seen: &mut std::collections::BTreeSet<String>,
                         name: String,
                         desc: String| {
        if seen.insert(name.clone()) {
            decks.push(DeckDef { id: deck_id(&name), name, description: desc });
        }
    };

    push_deck(
        &mut decks,
        &mut seen_decks,
        DECK_ROOT.to_string(),
        format!("NVIDIA PTX ISA {ptx_version}, generated from the official documentation."),
    );

    for input in inputs {
        let e = &input.entry;
        let module_deck = format!("{DECK_ROOT}::{}", input.module.deck_name());
        push_deck(&mut decks, &mut seen_decks, module_deck.clone(), String::new());

        let base_tags = vec![input.module.tag().to_string(), input.category.clone()];

        let add = |kind: CardKind,
                       guid_extra: Option<&str>,
                       fields: BTreeMap<String, String>,
                       notes: &mut Vec<NoteDef>,
                       decks: &mut Vec<DeckDef>,
                       seen: &mut std::collections::BTreeSet<String>| {
            let deck = format!("{module_deck}::{}", kind.deck_name());
            push_deck(decks, seen, deck.clone(), String::new());

            let mut seed = vec![e.id.as_str(), kind.seed()];
            if let Some(x) = guid_extra {
                seed.push(x);
            }
            let mut tags = base_tags.clone();
            match &e.arch {
                ArchReq::Min(sm) => tags.push(format!("ptx::{sm}")),
                ArchReq::All => tags.push("ptx::all-architectures".into()),
                ArchReq::Unknown => {}
            }
            notes.push(NoteDef {
                deck,
                model: kind.model_name().to_string(),
                tags,
                guid: guid(&seed),
                fields,
            });
        };

        let name = esc(&e.name);
        let gloss = esc(&e.gloss);
        let kind_w = kind_word(&e.section);

        // Recall -- every entry has a gloss, so this is universal.
        add(
            CardKind::Recall,
            None,
            BTreeMap::from([
                ("Name".into(), name.clone()),
                ("Gloss".into(), gloss.clone()),
                ("Kind".into(), kind_w.into()),
                ("Section".into(), e.section.clone()),
            ]),
            &mut notes,
            &mut decks,
            &mut seen_decks,
        );

        if let Some(syntax) = &e.syntax {
            add(
                CardKind::Syntax,
                None,
                BTreeMap::from([
                    ("Name".into(), name.clone()),
                    ("Syntax".into(), esc(syntax)),
                    ("Gloss".into(), gloss.clone()),
                    ("Section".into(), e.section.clone()),
                ]),
                &mut notes,
                &mut decks,
                &mut seen_decks,
            );
        }

        for q in &e.qualifiers {
            add(
                CardKind::Qualifiers,
                Some(&q.name),
                BTreeMap::from([
                    ("Name".into(), name.clone()),
                    ("Qualifier".into(), esc(&q.name)),
                    ("Values".into(), esc(&q.values.join("\n"))),
                    ("Gloss".into(), gloss.clone()),
                    ("Section".into(), e.section.clone()),
                ]),
                &mut notes,
                &mut decks,
                &mut seen_decks,
            );
        }

        // An Unknown requirement produces no card at all, rather than asserting
        // something the doc does not actually say.
        let sm_text = match &e.arch {
            ArchReq::Min(sm) => Some(sm.clone()),
            ArchReq::All => Some("all target architectures".into()),
            ArchReq::Unknown => None,
        };
        if let Some(sm_text) = sm_text {
            add(
                CardKind::Arch,
                None,
                BTreeMap::from([
                    ("Name".into(), name.clone()),
                    ("Sm".into(), sm_text),
                    ("Gloss".into(), gloss.clone()),
                    ("Section".into(), e.section.clone()),
                ]),
                &mut notes,
                &mut decks,
                &mut seen_decks,
            );
        }

        if let Some(version) = &e.since_ptx {
            add(
                CardKind::Version,
                None,
                BTreeMap::from([
                    ("Name".into(), name.clone()),
                    ("PtxVersion".into(), version.clone()),
                    ("Gloss".into(), gloss.clone()),
                    ("Section".into(), e.section.clone()),
                ]),
                &mut notes,
                &mut decks,
                &mut seen_decks,
            );
        }
    }

    DeckDefinition {
        package: PackageInfo {
            name: "PTX ISA".into(),
            version: ptx_version.into(),
            description: format!(
                "NVIDIA PTX ISA {ptx_version} flashcards, generated from the official docs."
            ),
        },
        models: models(),
        decks,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Qualifier;

    fn entry() -> Entry {
        Entry {
            id: "integer-arithmetic-instructions-bfe".into(),
            section: "9.7.1.20".into(),
            name: "bfe".into(),
            gloss: "Bit Field Extract.".into(),
            syntax: Some("bfe.type  d, a, b, c;".into()),
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

    fn input(e: Entry) -> Vec<Input> {
        vec![Input { module: Module::Core, category: "ptx::integer-arithmetic".into(), entry: e }]
    }

    #[test]
    fn guid_survives_content_changes() {
        // The whole point of the stability contract: reword every field and the
        // note must keep its identity so scheduling is preserved on re-import.
        let before = build(&input(entry()), "9.3");
        let mut changed = entry();
        changed.gloss = "Completely different wording.".into();
        changed.syntax = Some("bfe.type d, a, b, c; // reworded".into());
        changed.arch = ArchReq::Min("sm_50".into());
        let after = build(&input(changed), "9.4");

        let g = |d: &DeckDefinition| d.notes.iter().map(|n| n.guid.clone()).collect::<Vec<_>>();
        assert_eq!(g(&before), g(&after));
    }

    #[test]
    fn guid_differs_per_card_kind_and_qualifier() {
        let mut e = entry();
        e.qualifiers.push(Qualifier { name: "rnd".into(), values: vec![".rn".into()] });
        let d = build(&input(e), "9.3");
        let guids: std::collections::BTreeSet<_> = d.notes.iter().map(|n| &n.guid).collect();
        assert_eq!(guids.len(), d.notes.len(), "every note needs a distinct guid");
    }

    #[test]
    fn builds_the_expected_subdeck_tree() {
        let d = build(&input(entry()), "9.3");
        let names: Vec<_> = d.decks.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"PTX ISA"));
        assert!(names.contains(&"PTX ISA::Core Instructions"));
        assert!(names.contains(&"PTX ISA::Core Instructions::Recall"));
        assert!(names.contains(&"PTX ISA::Core Instructions::Qualifiers"));
        assert!(names.contains(&"PTX ISA::Core Instructions::Arch"));
    }

    #[test]
    fn one_qualifier_note_per_qualifier() {
        let mut e = entry();
        e.qualifiers.push(Qualifier { name: "rnd".into(), values: vec![".rn".into()] });
        let d = build(&input(e), "9.3");
        let n = d.notes.iter().filter(|n| n.model == "PTX Qualifiers").count();
        assert_eq!(n, 2);
    }

    #[test]
    fn entries_without_optional_data_skip_those_card_kinds() {
        let mut e = entry();
        e.syntax = None;
        e.qualifiers.clear();
        e.arch = ArchReq::Unknown;
        e.since_ptx = None;
        let d = build(&input(e), "9.3");
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0].model, "PTX Recall");
    }

    #[test]
    fn no_template_depends_on_a_conditional_field() {
        // ankit-builder writes one card per template without evaluating
        // {{#Field}} sections, so a gated template would import as a blank
        // card. Every model must therefore be unconditional.
        for m in models() {
            for t in &m.templates {
                assert!(
                    !t.front.contains("{{#") && !t.front.contains("{{^"),
                    "template {}/{} is conditional and would produce blank cards",
                    m.name,
                    t.name
                );
            }
        }
    }

    #[test]
    fn every_note_fills_every_field_of_its_model() {
        // A note missing a field its template renders would show an empty card.
        let by_name: BTreeMap<_, _> = models().into_iter().map(|m| (m.name.clone(), m)).collect();
        let mut e = entry();
        e.qualifiers.push(Qualifier { name: "rnd".into(), values: vec![".rn".into()] });
        for n in build(&input(e), "9.3").notes {
            let model = &by_name[&n.model];
            for f in &model.fields {
                assert!(
                    n.fields.get(f).is_some_and(|v| !v.is_empty()),
                    "note for model {} has empty field {f}",
                    n.model
                );
            }
        }
    }

    #[test]
    fn arch_and_version_are_independent_cards() {
        // An entry documented with an ISA version but no architecture floor
        // should yield the version card only.
        let mut e = entry();
        e.arch = ArchReq::Unknown;
        let d = build(&input(e), "9.3");
        let models: Vec<_> = d.notes.iter().map(|n| n.model.as_str()).collect();
        assert!(models.contains(&"PTX Version"));
        assert!(!models.contains(&"PTX Arch"));
    }

    #[test]
    fn deck_ids_are_stable_and_positive() {
        let a = build(&input(entry()), "9.3");
        let b = build(&input(entry()), "9.3");
        assert_eq!(
            a.decks.iter().map(|d| d.id).collect::<Vec<_>>(),
            b.decks.iter().map(|d| d.id).collect::<Vec<_>>()
        );
        assert!(a.decks.iter().all(|d| d.id > 0));
    }

    #[test]
    fn field_text_is_html_escaped() {
        let mut e = entry();
        e.syntax = Some("d = (a < b) && (c > 0);".into());
        let d = build(&input(e), "9.3");
        let note = d.notes.iter().find(|n| n.model == "PTX Syntax").unwrap();
        let s = &note.fields["Syntax"];
        assert!(!s.contains('<') && !s.contains('>'));
        assert!(s.contains("&lt;") && s.contains("&gt;") && s.contains("&amp;&amp;"));
    }

    #[test]
    fn production_card_names_the_entry_kind() {
        let mut sreg = entry();
        sreg.id = "special-registers-tid".into();
        sreg.section = "10.1".into();
        sreg.name = "%tid".into();
        let d = build(&input(sreg), "9.3");
        let recall = d.notes.iter().find(|n| n.model == "PTX Recall").unwrap();
        assert_eq!(recall.fields["Kind"], "special register");
    }
}

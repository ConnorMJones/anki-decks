//! Parser for the PTX ISA HTML document.
//!
//! The document is Sphinx-generated and highly regular. Every documented entry
//! (instruction, special register, directive) lives in its own `<section id=...>`
//! and opens with a "name rubric" -- a `<p class="rubric">` containing only the
//! entry name in a `<code>` -- immediately followed by a one-line gloss `<p>`.
//! The rest of the entry is a flat run of siblings delimited by further
//! `<p class="rubric">` labels: Syntax, Description, Semantics, Notes,
//! PTX ISA Notes, Target ISA Notes, Examples.

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One documented entry: an instruction, special register, or directive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// The HTML section id, e.g. `integer-arithmetic-instructions-bfe`.
    /// Stable across doc revisions, so it anchors note GUIDs.
    pub id: String,
    /// Dotted section number, e.g. `9.7.1.20`.
    pub section: String,
    /// Entry name as written in the doc, e.g. `bfe`, `%tid`, `.maxntid`.
    pub name: String,
    /// The one-line gloss NVIDIA gives right under the name, e.g. "Bit Field Extract."
    pub gloss: String,
    /// Contents of the Syntax code block, verbatim.
    pub syntax: Option<String>,
    /// Qualifier lists declared in the syntax block, in document order:
    /// `.type = { .u32, .s32 }` becomes `("type", [".u32", ".s32"])`. Covers
    /// `.type` but also `.sem`, `.scope`, `.ss`, `.rnd`, `.shape`, and friends.
    pub qualifiers: Vec<Qualifier>,
    /// Description prose, as plain text paragraphs.
    pub description: Vec<String>,
    /// Contents of the Semantics code block, verbatim.
    pub semantics: Option<String>,
    /// PTX ISA version the entry was introduced in, e.g. `2.0`.
    pub since_ptx: Option<String>,
    /// Target architecture requirement, from the Target ISA Notes.
    pub arch: ArchReq,
    /// Contents of the Examples code block, verbatim.
    pub examples: Option<String>,
}

/// What the Target ISA Notes say about architecture support.
///
/// Only the *first* paragraph is authoritative for the base requirement. Later
/// paragraphs restrict individual variants -- `add` is "supported on all target
/// architectures" even though a later line says `add.u16x2` needs `sm_90` -- so
/// scanning the whole section would manufacture false requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchReq {
    /// "Supported on all target architectures."
    All,
    /// "Requires sm_NN or higher."
    Min(String),
    /// Anything else, e.g. "Supported on following architectures:" plus a list.
    /// Left unknown rather than guessed; these produce no architecture card.
    Unknown,
}

/// One `.name = { .a, .b }` qualifier list from a syntax block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qualifier {
    /// Qualifier name without the leading dot, e.g. `type`, `sem`, `scope`.
    pub name: String,
    /// Permitted values, written as they appear, e.g. `.u32`.
    pub values: Vec<String>,
}

/// Rubric labels that introduce a code block rather than prose.
fn is_syntax_label(label: &str) -> bool {
    // Special registers use "Syntax (predefined)".
    label == "Syntax" || label.starts_with("Syntax (")
}

pub fn parse(html: &str) -> Vec<Entry> {
    let doc = Html::parse_document(html);
    let section_sel = Selector::parse("section[id]").unwrap();

    let mut entries = Vec::new();
    for section in doc.select(&section_sel) {
        if let Some(entry) = parse_section(section) {
            entries.push(entry);
        }
    }
    entries
}

fn parse_section(section: ElementRef) -> Option<Entry> {
    let id = section.value().attr("id")?.to_string();

    // Walk only direct element children: nested subsections are separate entries.
    let children: Vec<ElementRef> = section
        .children()
        .filter_map(ElementRef::wrap)
        .collect();

    let section_number = children
        .iter()
        .find(|e| matches!(e.value().name(), "h1" | "h2" | "h3" | "h4" | "h5"))
        .and_then(|h| {
            let sel = Selector::parse("span.section-number").unwrap();
            h.select(&sel).next().map(|s| text_of(s))
        })
        .map(|s| s.trim().trim_end_matches('.').to_string())?;

    // Locate the name rubric: a rubric paragraph whose text is exactly the
    // contents of a single <code>. Prose rubrics ("Syntax", "Description")
    // carry no <code> child, which is what distinguishes them.
    let code_sel = Selector::parse("code").unwrap();
    let (name_idx, name) = children.iter().enumerate().find_map(|(i, e)| {
        if e.value().name() != "p" || !has_class(*e, "rubric") {
            return None;
        }
        let code = e.select(&code_sel).next()?;
        let code_text = text_of(code);
        // Guard against rubrics that merely mention code inside a longer label.
        if code_text.trim() == text_of(*e).trim() && !code_text.trim().is_empty() {
            Some((i, code_text.trim().to_string()))
        } else {
            None
        }
    })?;

    // The gloss is the paragraph immediately after the name rubric.
    let gloss = children
        .get(name_idx + 1)
        .filter(|e| e.value().name() == "p" && !has_class(**e, "rubric"))
        .map(|e| text_of(*e).trim().to_string())?;

    // Split the remaining siblings into labelled buckets.
    let mut buckets: BTreeMap<String, Vec<ElementRef>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for e in &children[name_idx + 2..] {
        if e.value().name() == "p" && has_class(*e, "rubric") {
            current = Some(text_of(*e).trim().to_string());
            buckets.entry(current.clone().unwrap()).or_default();
        } else if let Some(label) = &current {
            buckets.get_mut(label).unwrap().push(*e);
        }
    }

    let syntax = buckets
        .iter()
        .find(|(label, _)| is_syntax_label(label))
        .and_then(|(_, nodes)| first_code_block(nodes));
    let qualifiers = syntax.as_deref().map(parse_qualifiers).unwrap_or_default();

    let description = buckets
        .get("Description")
        .map(|nodes| {
            nodes
                .iter()
                .filter(|e| e.value().name() == "p")
                .map(|e| text_of(*e).trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let semantics = buckets.get("Semantics").and_then(|n| first_code_block(n));
    let examples = buckets
        .get("Examples")
        .or_else(|| buckets.get("Example"))
        .and_then(|n| first_code_block(n));

    let since_ptx = buckets
        .get("PTX ISA Notes")
        .map(|n| joined_text(n))
        .and_then(|t| extract_after(&t, "Introduced in PTX ISA version"));
    let arch = buckets
        .get("Target ISA Notes")
        .and_then(|nodes| nodes.iter().find(|e| e.value().name() == "p"))
        .map(|p| parse_arch(&text_of(*p)))
        .unwrap_or(ArchReq::Unknown);

    Some(Entry {
        id,
        section: section_number,
        name,
        gloss,
        syntax,
        qualifiers,
        description,
        semantics,
        since_ptx,
        arch,
        examples,
    })
}

fn has_class(e: ElementRef, class: &str) -> bool {
    e.value().classes().any(|c| c == class)
}

fn text_of(e: ElementRef) -> String {
    // Sphinx splits inline code across <span class="pre"> tokens with no
    // whitespace between them in the source, so concatenating text nodes
    // directly would glue words together. Join on the whitespace that the
    // rendered page shows, then collapse runs.
    let raw: String = e.text().collect::<Vec<_>>().join("");
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn joined_text(nodes: &[ElementRef]) -> String {
    nodes
        .iter()
        .map(|e| text_of(*e))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Code blocks are `<div class="highlight"><pre>`; take the raw text so
/// indentation and line breaks survive.
fn first_code_block(nodes: &[ElementRef]) -> Option<String> {
    let pre_sel = Selector::parse("pre").unwrap();
    for e in nodes {
        if let Some(pre) = e.select(&pre_sel).next() {
            let text: String = pre.text().collect::<Vec<_>>().join("");
            let trimmed = text.trim_matches('\n').trim_end();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Pull every qualifier list out of a syntax block. The doc's shape is:
///
/// ```text
/// bfe.type  d, a, b, c;
///
/// .type = { .u32, .u64,
///           .s32, .s64 };
/// ```
///
/// A block may declare several lists (`.sem`, `.scope`, `.ss`, ...), and the
/// values wrap across lines, so this scans for each `.name = { ... }` in turn.
fn parse_qualifiers(syntax: &str) -> Vec<Qualifier> {
    let mut out: Vec<Qualifier> = Vec::new();
    let bytes = syntax.as_bytes();
    let mut i = 0;

    while i < syntax.len() {
        let Some(dot) = syntax[i..].find('.') else { break };
        let start = i + dot;
        let name: String = syntax[start + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            i = start + 1;
            continue;
        }

        // Require `= {` after the name, allowing whitespace.
        let mut j = start + 1 + name.len();
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'=' {
            i = start + 1 + name.len();
            continue;
        }
        j += 1;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'{' {
            i = start + 1 + name.len();
            continue;
        }
        // Values may themselves contain brace groups, as in
        // `.space = { .global, .shared{::cta, ::cluster} }`, so track depth
        // rather than stopping at the first `}`.
        let Some(close) = matching_brace(syntax, j) else {
            break;
        };

        let values: Vec<String> = split_top_level(&syntax[j + 1..close])
            .into_iter()
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
            .collect();

        // The doc repeats a qualifier list when an instruction has several
        // syntax forms; keep the first, which is the general one.
        if !values.is_empty() && !out.iter().any(|q| q.name == name) {
            out.push(Qualifier { name, values });
        }
        i = close + 1;
    }
    out
}

/// Index of the `}` matching the `{` at `open`, or `None` if unbalanced.
fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split on commas that sit outside any nested brace group.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

fn extract_after(text: &str, prefix: &str) -> Option<String> {
    let idx = text.find(prefix)?;
    let tail = &text[idx + prefix.len()..];
    let token: String = tail
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let token = token.trim_end_matches('.').to_string();
    (!token.is_empty()).then_some(token)
}

/// Classify the opening paragraph of the Target ISA Notes.
///
/// Only its first sentence carries the base requirement; a paragraph may go on
/// to name architectures for specific variants.
fn parse_arch(paragraph: &str) -> ArchReq {
    let sentence = paragraph.split_once(". ").map_or(paragraph, |(head, _)| head);
    let lowered = sentence.to_ascii_lowercase();

    if lowered.contains("all target architectures") {
        return ArchReq::All;
    }
    match lowest_sm(sentence) {
        Some(sm) => ArchReq::Min(sm),
        None => ArchReq::Unknown,
    }
}

/// Lowest `sm_NN` named in a sentence, preserving family suffixes like `sm_90a`.
fn lowest_sm(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut best: Option<(u32, String)> = None;
    let mut i = 0;
    while let Some(pos) = text[i..].find("sm_") {
        let start = i + pos;
        let digits: String =
            text[start + 3..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let mut end = start + 3 + digits.len();
            if end < bytes.len() && (bytes[end] as char).is_ascii_alphabetic() {
                end += 1;
            }
            if let Ok(n) = digits.parse::<u32>()
                && best.as_ref().is_none_or(|(b, _)| n < *b)
            {
                best = Some((n, text[start..end].to_string()));
            }
        }
        i = start + 3;
    }
    best.map(|(_, s)| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quals(syntax: &str) -> Vec<(String, Vec<String>)> {
        parse_qualifiers(syntax)
            .into_iter()
            .map(|q| (q.name, q.values))
            .collect()
    }

    #[test]
    fn parses_type_qualifier_list_across_lines() {
        let syntax = "bfe.type  d, a, b, c;\n\n.type = { .u32, .u64,\n          .s32, .s64 };";
        assert_eq!(
            quals(syntax),
            vec![("type".to_string(), vec![".u32".into(), ".u64".into(), ".s32".into(), ".s64".into()])]
        );
    }

    #[test]
    fn parses_multiple_qualifier_lists_in_order() {
        let syntax = "ld.sem.scope.ss.type d, [a];\n\n.sem =   { .relaxed, .acquire };\n\
                      .scope = { .cta, .gpu, .sys };\n.ss =    { .global, .shared };\n\
                      .type =  { .b32, .b64 };";
        let q = quals(syntax);
        assert_eq!(q.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["sem", "scope", "ss", "type"]);
        assert_eq!(q[1].1, vec![".cta", ".gpu", ".sys"]);
    }

    #[test]
    fn ignores_dotted_tokens_that_are_not_lists() {
        // Bare mnemonic qualifiers and operands must not be mistaken for lists.
        assert_eq!(quals("ret;").len(), 0);
        assert_eq!(quals("add.s32 d, a, b;").len(), 0);
    }

    #[test]
    fn nested_brace_groups_stay_one_value() {
        // atom's .space list nests a sub-state-space group inside a value.
        let syntax = ".space = { .global, .shared{::cta, ::cluster} };";
        assert_eq!(
            quals(syntax),
            vec![(
                "space".to_string(),
                vec![".global".to_string(), ".shared{::cta, ::cluster}".to_string()]
            )]
        );
    }

    #[test]
    fn keeps_first_definition_when_a_list_repeats() {
        // Instructions with several syntax forms redeclare the same qualifier.
        let syntax = ".type = { .b32 };\nfoo.type d;\n.type = { .b32, .b64 };";
        assert_eq!(quals(syntax), vec![("type".to_string(), vec![".b32".to_string()])]);
    }

    #[test]
    fn extracts_ptx_version() {
        let t = "Introduced in PTX ISA version 2.0.";
        assert_eq!(extract_after(t, "Introduced in PTX ISA version").as_deref(), Some("2.0"));
    }

    #[test]
    fn reads_a_plain_minimum_architecture() {
        assert_eq!(parse_arch("bfe requires sm_20 or higher."), ArchReq::Min("sm_20".into()));
        assert_eq!(parse_arch("Requires sm_90a."), ArchReq::Min("sm_90a".into()));
    }

    #[test]
    fn universal_support_is_not_narrowed_by_later_variant_notes() {
        // The bug this guards: `add` is supported everywhere, but a later
        // sentence mentions sm_90 for add.u16x2. Reporting sm_90 as the floor
        // would make the card assert something false.
        let notes = "Supported on all target architectures. \
                     add.u16x2 and add.s16x2 require sm_90 or higher.";
        assert_eq!(parse_arch(notes), ArchReq::All);
    }

    #[test]
    fn a_variant_sentence_does_not_override_the_base_requirement() {
        let notes = "Requires sm_70 or higher. The .u8 form requires sm_90 or higher.";
        assert_eq!(parse_arch(notes), ArchReq::Min("sm_70".into()));
    }

    #[test]
    fn family_specific_lists_are_left_unknown_rather_than_guessed() {
        assert_eq!(parse_arch("Supported on following architectures:"), ArchReq::Unknown);
        assert_eq!(parse_arch("No architecture mentioned."), ArchReq::Unknown);
    }
}

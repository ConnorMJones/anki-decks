//! Scope classification: which of the three study modules an entry belongs to.
//!
//! Classification is driven purely by the doc's own section numbering, which is
//! stable and needs no name heuristics. Chapter 9.7.<n> groups instructions into
//! twenty categories; chapters 10 and 11 cover special registers and directives.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Module {
    /// The instructions you reach for writing ordinary PTX by hand.
    Core,
    /// Everything else in chapter 9: specialised precision, tensor cores,
    /// texture/surface, video SIMD.
    Instructions,
    /// Special registers and directives -- the surrounding language, not opcodes.
    Concepts,
}

impl Module {
    pub fn deck_name(self) -> &'static str {
        match self {
            Module::Core => "Core Instructions",
            Module::Instructions => "Instructions",
            Module::Concepts => "Concepts",
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Module::Core => "ptx::core",
            Module::Instructions => "ptx::instructions",
            Module::Concepts => "ptx::concepts",
        }
    }

    /// Short name accepted on the command line.
    pub fn slug(self) -> &'static str {
        match self {
            Module::Core => "core",
            Module::Instructions => "instructions",
            Module::Concepts => "concepts",
        }
    }

    pub fn parse_slug(s: &str) -> Option<Module> {
        [Module::Core, Module::Instructions, Module::Concepts]
            .into_iter()
            .find(|m| m.slug() == s)
    }
}

/// Human-readable category name for a 9.7.<n> instruction group, used for tags.
pub fn instruction_category(n: u32) -> &'static str {
    match n {
        1 => "integer-arithmetic",
        2 => "extended-precision",
        3 => "floating-point",
        4 => "half-precision",
        5 => "mixed-precision",
        6 => "comparison-selection",
        7 => "half-comparison",
        8 => "logic-shift",
        9 => "data-movement",
        10 => "fabric",
        11 => "texture",
        12 => "surface",
        13 => "control-flow",
        14 => "synchronization",
        15 => "wmma",
        16 => "wgmma",
        17 => "tcgen05",
        18 => "stack",
        19 => "video",
        20 => "miscellaneous",
        _ => "other",
    }
}

/// Instruction categories that make up the Core module. The rest of chapter 9
/// is specialised enough that it belongs in the wider Instructions module.
fn is_core_category(n: u32) -> bool {
    matches!(n, 1 | 2 | 3 | 6 | 8 | 9 | 13 | 14 | 18 | 20)
}

/// Classify by dotted section number, e.g. `9.7.1.20` or `10.1`.
/// Returns `None` for sections that aren't card-bearing entries.
pub fn classify(section: &str) -> Option<(Module, String)> {
    let parts: Vec<u32> = section.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;

    match parts.as_slice() {
        // 9.7.<category>.<entry> -- an instruction.
        [9, 7, category, _rest @ ..] if !_rest.is_empty() => {
            let module = if is_core_category(*category) {
                Module::Core
            } else {
                Module::Instructions
            };
            Some((module, format!("ptx::{}", instruction_category(*category))))
        }
        // 10.<n> -- a special register.
        [10, _] => Some((Module::Concepts, "ptx::special-registers".to_string())),
        // 11.<n>[.<m>] -- a directive.
        [11, ..] if parts.len() >= 2 => Some((Module::Concepts, "ptx::directives".to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_split_core_from_specialised() {
        // bfe, integer arithmetic
        assert_eq!(classify("9.7.1.20").unwrap().0, Module::Core);
        // a tcgen05 instruction
        assert_eq!(classify("9.7.17.3").unwrap().0, Module::Instructions);
        // texture
        assert_eq!(classify("9.7.11.1").unwrap().0, Module::Instructions);
    }

    #[test]
    fn category_tags_follow_section_number() {
        assert_eq!(classify("9.7.8.4").unwrap().1, "ptx::logic-shift");
        assert_eq!(classify("9.7.19.2").unwrap().1, "ptx::video");
    }

    #[test]
    fn special_registers_and_directives_are_concepts() {
        assert_eq!(classify("10.1").unwrap(), (Module::Concepts, "ptx::special-registers".into()));
        assert_eq!(classify("11.4.2").unwrap(), (Module::Concepts, "ptx::directives".into()));
    }

    #[test]
    fn category_headers_and_prose_chapters_are_skipped() {
        // 9.7.1 is the category heading itself, not an instruction.
        assert_eq!(classify("9.7.1"), None);
        // Chapter 8 is the memory consistency model: prose, no card entries.
        assert_eq!(classify("8.2"), None);
        assert_eq!(classify("intro"), None);
    }
}

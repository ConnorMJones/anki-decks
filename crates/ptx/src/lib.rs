//! Generates an Anki deck for the NVIDIA PTX ISA from the official
//! documentation.
//!
//! The pipeline is three stages:
//!
//! 1. [`parse`] turns the Sphinx-generated HTML into a structured [`parse::Entry`]
//!    per documented instruction, special register, and directive.
//! 2. [`module`] assigns each entry to a study module using the doc's own
//!    section numbering.
//! 3. [`deck`] renders those entries into an ankit-builder deck definition,
//!    which becomes a `.apkg`.
//!
//! See [`deck`] for the stability contract that lets the deck be regenerated
//! without losing review history.

pub mod deck;
pub mod module;
pub mod parse;

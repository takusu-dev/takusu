//! Built-in skills bundled at compile time.
//!
//! The `build.rs` script writes `OUT_DIR/bundled_skills_contents.rs` from the
//! `skills/` directory. This module re-exports those contents as a slice of
//! `(slug, markdown)` pairs.

pub fn built_in_skill_contents() -> &'static [(&'static str, &'static str)] {
    include!(concat!(env!("OUT_DIR"), "/bundled_skills_contents.rs"))
}

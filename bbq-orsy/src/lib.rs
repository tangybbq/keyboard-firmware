//! Orsy, an orthographic syllabic chord layout.
//!
//! One stroke spells one syllable: an onset and a second character on the
//! left hand, a vowel and a coda on the right.  There is no dictionary; the
//! spelling follows from the chord by rule.  The rules are those of the
//! Midi4Text theory, recovered from its shipped dictionary and written down in
//! `midi4text-analysis/m4t/theory.py`, which this crate is a port of and must
//! agree with exactly.  The chord assignment is the frozen mapping in
//! `docs/orsy/orsy-mapping.json`; see `docs/orsy/` for the design.
//!
//! The crate is pure logic with no dependencies, so that it can be tested on
//! the host and used by the host tools.  [`translate`] turns a [`Chord`] into a
//! [`Translation`], and [`Output`] turns a sequence of translations into
//! characters and backspaces, handling the spacing between words, capitals and
//! undo.  Chord accumulation and the keyboard side of things live in
//! `bbq-keyboard`.

#![cfg_attr(not(any(feature = "std", test)), no_std)]

pub mod chord;
pub mod compose;
pub mod tables;

pub use chord::Chord;
pub use compose::{translate, Translation};

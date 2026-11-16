//! Lesson text, ported from keybr.com. The scheduler picks the unlocked
//! letters and the one in focus, the filter turns them into the rule every
//! word must satisfy, and the generator draws words from the language's
//! word list (`dictionary`) or its order-4 phonetic model (`transition`,
//! read by `model`), using the xorshift32 PRNG in `rng`. `language` holds
//! what differs per language, `layout` keybr.com's keyboard layouts.

pub mod dictionary;
pub mod filter;
pub mod generator;
pub mod language;
pub mod layout;
pub mod model;
pub mod rng;
pub mod scheduler;
pub mod transition;

pub use filter::LetterFilter;
pub use generator::WordGenerator;
pub use language::Language;
pub use scheduler::LetterScheduler;

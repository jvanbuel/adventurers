// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Turning a line of player input into words.
//!
//! The game understands at most two words per command.  Each is looked
//! up in the vocabulary: motions first, then objects, then actions,
//! then the reservoir magic word, then numbers.  Only the first five
//! letters of a word matter.

use crate::dungeon::{IGNORE, Motion, Obj, PART, Verb};

/// Number of significant characters in a vocabulary word.
const TOKLEN: usize = 5;

/// What a word was recognised as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WordKind {
    /// No word was given.
    Empty,
    /// Not in the vocabulary.
    Unknown,
    Motion(Motion),
    Object(Obj),
    Action(Verb),
    /// A number, used as the argument of `seed` and `waste`.
    Numeric,
}

/// One word of a command, as typed and as recognised.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Word {
    pub raw: String,
    pub kind: WordKind,
}

impl Word {
    pub fn empty() -> Word {
        Word {
            raw: String::new(),
            kind: WordKind::Empty,
        }
    }

    /// The word's vocabulary number regardless of its kind, as the C
    /// code compared them: `Empty` and `Numeric` are 0, `Unknown` is -1.
    ///
    /// A few command rewrites compare numbers across kinds (an object
    /// number against a motion number, say).  They use this so that
    /// odd inputs behave exactly as in the original.
    pub fn id(&self) -> i32 {
        match self.kind {
            WordKind::Empty | WordKind::Numeric => 0,
            WordKind::Unknown => -1,
            WordKind::Motion(m) => m.raw(),
            WordKind::Object(o) => o.raw(),
            WordKind::Action(v) => v.raw(),
        }
    }
}

/// Parser settings that change how words are read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dialect<'a> {
    /// Emulate the original's uppercasing, truncation, and refusal of
    /// some single-letter abbreviations.
    pub oldstyle: bool,
    /// This game's reservoir magic word.
    pub magic_word: &'a str,
}

/// `strncasecmp(a, b, TOKLEN) == 0`: equal ignoring case over the first
/// five characters, where a shorter word must match exactly.
fn same_token(a: &str, b: &str) -> bool {
    let a = a.bytes().map(Some).chain(std::iter::repeat(None));
    let b = b.bytes().map(Some).chain(std::iter::repeat(None));
    for (x, y) in a.zip(b).take(TOKLEN) {
        match (x, y) {
            (None, None) => return true,
            (Some(x), Some(y)) if x.eq_ignore_ascii_case(&y) => {}
            _ => return false,
        }
    }
    true
}

impl Dialect<'_> {
    /// In oldstyle mode some single letters ("i", "l", ...) are not
    /// abbreviations.
    fn allows_abbreviation(&self, word: &str) -> bool {
        word.len() > 1 || !self.oldstyle || !IGNORE.as_bytes().contains(&word.as_bytes()[0])
    }

    fn find<T: Copy>(
        &self,
        word: &str,
        mut entries: impl Iterator<Item = (T, &'static [&'static str])>,
    ) -> Option<T> {
        entries
            .find(|(_, words)| words.iter().any(|w| same_token(word, w)))
            .map(|(id, _)| id)
    }

    /// Recognise a single word.
    pub fn lookup(&self, word: &str) -> WordKind {
        if word.is_empty() {
            return WordKind::Empty;
        }
        if let Some(m) = self.find(word, Motion::all().map(|m| (m, m.info().words))) {
            if self.allows_abbreviation(word) {
                return WordKind::Motion(m);
            }
        }
        if let Some(o) = self.find(word, Obj::all().map(|o| (o, o.info().words))) {
            return WordKind::Object(o);
        }
        if let Some(v) = self.find(word, Verb::all().map(|v| (v, v.info().words))) {
            // PART is only reachable through this game's magic word.
            if v != PART && self.allows_abbreviation(word) {
                return WordKind::Action(v);
            }
        }
        if word.eq_ignore_ascii_case(self.magic_word) {
            return WordKind::Action(PART);
        }
        if is_integer(word) {
            return WordKind::Numeric;
        }
        WordKind::Unknown
    }

    /// Split a line into its first two words and recognise them.
    pub fn tokenize(&self, line: &str) -> [Word; 2] {
        // In oldstyle mode, simulate the effect of the FORTRAN version
        // packing tokens into sixbit words: uppercase and truncate.
        const TRUNCLEN: usize = TOKLEN + TOKLEN;
        let mut tokens = c_words(line).map(|t| {
            if self.oldstyle {
                truncate_chars(t, TRUNCLEN).to_ascii_uppercase()
            } else {
                t.to_string()
            }
        });
        [(); 2].map(|()| {
            let raw = tokens.next().unwrap_or_default();
            let kind = self.lookup(&raw);
            Word { raw, kind }
        })
    }
}

/// Split on C `isspace()` characters, as `sscanf("%s")` does.
fn c_words(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t', '\n', '\x0b', '\x0c', '\r'])
        .filter(|t| !t.is_empty())
}

fn truncate_chars(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// Is this an optionally negative run of digits?
fn is_integer(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())
}

/// Number of blank- or tab-separated words; commands have at most two.
pub(crate) fn word_count(s: &str) -> usize {
    s.split([' ', '\t']).filter(|w| !w.is_empty()).count()
}

/// Read a yes/no reply: any first word starting with "y" or "n".
pub(crate) fn yes_or_no(reply: &str) -> Option<bool> {
    match c_words(reply).next()?.bytes().next()?.to_ascii_lowercase() {
        b'y' => Some(true),
        b'n' => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::*;

    const MODERN: Dialect = Dialect {
        oldstyle: false,
        magic_word: "Z'ZZZ",
    };
    const OLD: Dialect = Dialect {
        oldstyle: true,
        magic_word: "Z'ZZZ",
    };

    #[test]
    fn only_five_letters_matter() {
        assert_eq!(MODERN.lookup("lantern"), WordKind::Object(LAMP));
        assert_eq!(MODERN.lookup("LANTERNS"), WordKind::Object(LAMP));
        assert_eq!(MODERN.lookup("lan"), WordKind::Unknown);
    }

    #[test]
    fn motions_win_over_objects_and_actions() {
        assert_eq!(MODERN.lookup("north"), WordKind::Motion(NORTH));
        assert_eq!(MODERN.lookup("take"), WordKind::Action(CARRY));
    }

    #[test]
    fn magic_word_and_numbers() {
        assert_eq!(MODERN.lookup("z'zzz"), WordKind::Action(PART));
        assert_eq!(MODERN.lookup("-42"), WordKind::Numeric);
        assert_eq!(MODERN.lookup("-"), WordKind::Unknown);
    }

    #[test]
    fn oldstyle_refuses_some_single_letters() {
        assert_eq!(MODERN.lookup("i"), WordKind::Action(INVENTORY));
        // Oldstyle input is uppercased before lookup.
        assert_eq!(OLD.lookup("I"), WordKind::Unknown);
        assert_eq!(OLD.lookup("INVEN"), WordKind::Action(INVENTORY));
    }

    #[test]
    fn tokenize_takes_two_words() {
        let [a, b] = MODERN.tokenize("  get\tlamp  ");
        assert_eq!((a.raw.as_str(), a.kind), ("get", WordKind::Action(CARRY)));
        assert_eq!((b.raw.as_str(), b.kind), ("lamp", WordKind::Object(LAMP)));
        let [a, b] = MODERN.tokenize("");
        assert_eq!((a.kind, b.kind), (WordKind::Empty, WordKind::Empty));
    }

    #[test]
    fn oldstyle_uppercases_and_truncates() {
        let [a, _] = OLD.tokenize("abcdefghijklmnop");
        assert_eq!(a.raw, "ABCDEFGHIJ");
    }

    #[test]
    fn ids_follow_c_numbering() {
        assert_eq!(Word::empty().id(), 0);
        let w = Word {
            raw: "xyzzy".into(),
            kind: MODERN.lookup("xyzzy"),
        };
        assert_eq!(w.id(), XYZZY.raw());
    }

    #[test]
    fn replies() {
        assert_eq!(yes_or_no("Yes please"), Some(true));
        assert_eq!(yes_or_no(" n"), Some(false));
        assert_eq!(yes_or_no("maybe"), None);
        assert_eq!(yes_or_no("   "), None);
    }

    #[test]
    fn counting_words() {
        assert_eq!(word_count("a b\tc"), 3);
        assert_eq!(word_count("   "), 0);
    }
}

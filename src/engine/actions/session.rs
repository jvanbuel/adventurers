// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Verbs about the game rather than the cave: brevity, quitting,
//! saving, resuming, and the testing aids `seed` and `waste`.

use super::super::turn::Phase;
use super::super::{Console, Engine, Flow, GameOver, SaveMode};
use crate::dungeon::*;
use std::fs::File;
use std::io::{Read as _, Write as _};

/// Read a number the way C's `strtol` does: optional leading space and
/// sign, then digits; 0 if there are none; saturating on overflow.  The
/// result is then truncated to 32 bits, as the original stored it.
///
/// The argument is usually a validated number, but command rewriting can
/// leave `seed` or `waste` with any word, or none.
fn numeric_arg(word: &str) -> i32 {
    let s = word.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (negative, rest) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return 0;
    }
    let n = match (digits.parse::<i64>(), negative) {
        (Ok(n), false) => n,
        (Ok(n), true) => -n,
        (Err(_), false) => i64::MAX,
        (Err(_), true) => i64::MIN,
    };
    n as i32
}

impl<C: Console> Engine<C> {
    /// Brief.  Only give long descriptions on the first visit.
    pub(super) fn brief(&mut self) -> Phase {
        self.game.long_desc_interval = 10000;
        self.game.detail = 3;
        self.say(BRIEF_CONFIRM);
        Phase::ClearObj
    }

    /// Quit, if the player confirms.
    pub(super) fn quit(&mut self) -> Flow<Phase> {
        if self.yes_or_no(REALLY_QUIT, OK_MAN, OK_MAN)? {
            return Err(self.terminate(super::super::score::Ending::Quit));
        }
        Ok(Phase::ClearObj)
    }

    /// Save.  Saving costs points, so it can't be used to retry battles
    /// or to start over after learning the magic word.
    pub(super) fn suspend(&mut self) -> Flow<Phase> {
        if !matches!(self.config.saves, SaveMode::Manual) {
            self.say(SAVERESUME_DISABLED);
            return Ok(Phase::Top);
        }
        self.say(SUSPEND_WARNING);
        if !self.yes_or_no(THIS_ACCEPTABLE, OK_MAN, OK_MAN)? {
            return Ok(Phase::ClearObj);
        }
        self.game.save_penalty += 5;

        let Some((mut file, _)) = self.prompt_for_file(|name| File::create(name)) else {
            return Ok(Phase::Top);
        };
        // There's no good way to report a failed write; the player was
        // told it was saved either way in the original too.
        let _ = file.write_all(&self.save_bytes());
        self.say(RESUME_HELP);
        Err(GameOver::Saved)
    }

    /// Resume a saved game, abandoning this one.
    pub(super) fn resume(&mut self) -> Flow<Phase> {
        if !matches!(self.config.saves, SaveMode::Manual) {
            self.say(SAVERESUME_DISABLED);
            return Ok(Phase::Top);
        }
        if self.game.loc != LOC_START || self.game.visits[LOC_START.index()] != 1 {
            self.say(RESUME_ABANDON);
            if !self.yes_or_no(THIS_ACCEPTABLE, OK_MAN, OK_MAN)? {
                return Ok(Phase::ClearObj);
            }
        }
        let Some((mut file, _)) = self.prompt_for_file(|name| File::open(name)) else {
            return Ok(Phase::Top);
        };
        let mut bytes = Vec::new();
        // An unreadable file restores as an empty one: "not a save".
        let _ = file.read_to_end(&mut bytes);
        self.restore(&bytes)?;
        Ok(Phase::Top)
    }

    /// Ask for a file name until `open` succeeds with one.  `None` if the
    /// player gives no name or input runs out.
    pub(super) fn prompt_for_file(
        &mut self,
        open: impl Fn(&str) -> std::io::Result<File>,
    ) -> Option<(File, String)> {
        loop {
            let line = self.console.read_line("\nFile name: ")?;
            let name = line.trim_matches(|c: char| c.is_ascii_whitespace());
            if name.is_empty() {
                return None;
            }
            match open(name) {
                Ok(file) => return Some((file, name.to_string())),
                Err(_) => self
                    .console
                    .print(&format!("Can't open file {name}, try again.\n")),
            }
        }
    }

    /// Reseed the random number generator (for testing).
    pub(super) fn reseed_command(&mut self, verb: Verb, arg: &str) -> Phase {
        let seed = numeric_arg(arg);
        self.speak(verb.info().message, &[seed.into()]);
        self.set_seed(seed);
        self.game.turns -= 1;
        Phase::Top
    }

    /// Burn lamp turns (for testing).
    pub(super) fn waste(&mut self, verb: Verb, arg: &str) -> Phase {
        self.game.lamp_life = self.game.lamp_life.wrapping_sub(numeric_arg(arg));
        self.speak(verb.info().message, &[self.game.lamp_life.into()]);
        Phase::Top
    }
}

#[cfg(test)]
mod tests {
    use super::numeric_arg;

    #[test]
    fn numeric_arguments_follow_strtol() {
        assert_eq!(numeric_arg("42"), 42);
        assert_eq!(numeric_arg("-7"), -7);
        assert_eq!(numeric_arg(""), 0);
        assert_eq!(numeric_arg("lantern"), 0);
        assert_eq!(numeric_arg("12abc"), 12);
        // Saturates to i64::MAX, then truncates to 32 bits.
        assert_eq!(numeric_arg("99999999999999999999"), -1);
        assert_eq!(numeric_arg("4294967296"), 0);
    }
}

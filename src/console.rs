// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The seam between the game and its player.
//!
//! The engine never touches stdin or stdout.  It writes text and asks
//! for lines of input through a [`Console`].  The `advent` binary
//! provides a terminal implementation; [`ScriptedConsole`] feeds a
//! fixed script and records the transcript, for tests.

use std::collections::VecDeque;

/// How the engine talks to the player.
pub trait Console {
    /// Show some text exactly as given (it includes any newlines).
    fn print(&mut self, text: &str);

    /// Prompt for and read one line of input, without its trailing
    /// newline.  `None` means input has run out, which ends the game.
    fn read_line(&mut self, prompt: &str) -> Option<String>;

    /// Called once a line has been accepted as a command or answer
    /// (rather than a comment or a file name), so the console can echo
    /// or log it.  Does nothing by default.
    fn accepted(&mut self, _prompt: &str, _line: &str) {}
}

/// A console that plays a fixed script and records everything shown.
///
/// The transcript looks like a non-interactive terminal session: each
/// accepted line is echoed after its prompt.
///
/// ```
/// use adventurers::{Config, Engine, ScriptedConsole, Start};
///
/// let console = ScriptedConsole::new(["no", "quit", "yes"]);
/// let mut engine = Engine::new(console, Config::default(), 1);
/// let _ = engine.play(Start::New);
/// assert!(engine.console().transcript().contains("You scored"));
/// ```
#[derive(Debug, Default, Clone)]
pub struct ScriptedConsole {
    input: VecDeque<String>,
    transcript: String,
}

impl ScriptedConsole {
    /// A console that will answer with these lines, in order.
    pub fn new<I, S>(lines: I) -> ScriptedConsole
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ScriptedConsole {
            input: lines.into_iter().map(Into::into).collect(),
            transcript: String::new(),
        }
    }

    /// Everything printed so far, with accepted input echoed.
    pub fn transcript(&self) -> &str {
        &self.transcript
    }

    /// Lines of the script not yet read.
    pub fn remaining(&self) -> usize {
        self.input.len()
    }
}

impl Console for ScriptedConsole {
    fn print(&mut self, text: &str) {
        self.transcript.push_str(text);
    }

    fn read_line(&mut self, _prompt: &str) -> Option<String> {
        self.input.pop_front()
    }

    fn accepted(&mut self, prompt: &str, line: &str) {
        self.transcript.push_str(prompt);
        self.transcript.push_str(line);
        self.transcript.push('\n');
    }
}

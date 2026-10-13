// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The terminal console: standard input and output, optional script
//! files, and an optional log of the player's commands.

use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};

use adventurers::Console;

/// Where input is currently coming from, when playing scripts.
enum Source {
    Stdin,
    File(BufReader<File>),
}

/// A console on the process's terminal.
///
/// Without scripts, commands come from standard input.  The prompt is
/// shown only when standard input is a terminal; otherwise each command
/// is echoed after its prompt, so a transcript reads naturally.
///
/// With scripts, commands come from each script file in turn (`-` is
/// standard input), and every line read is shown after the prompt.
pub struct Terminal {
    scripts: Vec<String>,
    next_script: usize,
    source: Option<Source>,
    log: Option<File>,
    oldstyle: bool,
    stdout: io::Stdout,
}

impl Terminal {
    pub fn new(scripts: Vec<String>, log: Option<File>, oldstyle: bool) -> Terminal {
        Terminal {
            scripts,
            next_script: 0,
            source: None,
            log,
            oldstyle,
            stdout: io::stdout(),
        }
    }

    /// Record the game's seed in the log, so it can be replayed.
    pub fn log_seed(&mut self, seed: i32) {
        self.log_line(&format!("seed {seed}"));
    }

    fn log_line(&mut self, line: &str) {
        if let Some(log) = &mut self.log {
            // The log is a convenience; a failed write shouldn't stop play.
            let _ = writeln!(log, "{line}").and_then(|()| log.flush());
        }
    }

    pub fn flush(&mut self) {
        let _ = self.stdout.flush();
    }

    /// Read a line from standard input like readline(): prompt only on
    /// a terminal, and drop the newline.
    fn read_stdin(&mut self, prompt: &str) -> Option<String> {
        if io::stdin().is_terminal() {
            self.print(prompt);
        }
        self.flush();
        let mut line = read_raw(&mut io::stdin().lock())?;
        if line.ends_with('\n') {
            line.pop();
        }
        Some(line)
    }

    /// The next line from the scripts, with its newline.
    fn read_script(&mut self, prompt: &str) -> Option<String> {
        loop {
            if self.source.is_none() {
                let name = self.scripts.get(self.next_script)?.clone();
                self.next_script += 1;
                self.source = if name == "-" {
                    Some(Source::Stdin)
                } else {
                    match File::open(&name) {
                        Ok(f) => Some(Source::File(BufReader::new(f))),
                        Err(_) => {
                            eprintln!("Can't open script {name}");
                            continue;
                        }
                    }
                };
            }
            let line = match self.source.as_mut()? {
                Source::Stdin if io::stdin().is_terminal() && !self.oldstyle => {
                    return self.read_stdin(prompt);
                }
                Source::Stdin => read_raw(&mut io::stdin().lock()),
                Source::File(reader) => read_raw(reader),
            };
            match line {
                Some(line) => {
                    self.print(prompt);
                    self.print(&line);
                    return Some(line);
                }
                None => self.source = None,
            }
        }
    }
}

/// One line including its newline, or `None` at end of input.
fn read_raw(reader: &mut impl BufRead) -> Option<String> {
    let mut buf = Vec::new();
    match reader.read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(String::from_utf8_lossy(&buf).into_owned()),
    }
}

impl Console for Terminal {
    fn print(&mut self, text: &str) {
        // If stdout is gone there's nobody to tell.
        let _ = self.stdout.write_all(text.as_bytes());
    }

    fn read_line(&mut self, prompt: &str) -> Option<String> {
        if !self.scripts.is_empty() {
            return self.read_script(prompt);
        }
        let line = self.read_stdin(prompt);
        if line.is_none() {
            self.print(prompt);
        }
        line
    }

    fn accepted(&mut self, prompt: &str, line: &str) {
        if !io::stdin().is_terminal() {
            self.print(&format!("{prompt}{line}\n"));
        }
        self.log_line(line);
    }
}

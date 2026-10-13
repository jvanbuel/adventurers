// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The game engine: the rules of Adventure, driven one command at a
//! time through a [`Console`].
//!
//! [`Engine`] is the crate's one deep module.  Its interface is small:
//! build it with a console, a [`Config`] and a seed, then [`Engine::play`]
//! until the game is over.  Everything else is private to this module
//! tree:
//!
//! * `turn` — the main loop: moving, describing, reading commands.
//! * `movement` — travel rules and the special movements.
//! * `dwarves` — the dwarves and the pirate.
//! * `hints` — offering hints.
//! * `closing` — the lamp, the closing of the cave, and the end game.
//! * `actions` — what each verb does.
//! * `score` — scoring and the end of the game.

mod actions;
mod closing;
mod dwarves;
mod hints;
mod movement;
mod score;
mod turn;

use std::fs::File;
use std::io::Write as _;

use crate::console::Console;
use crate::dungeon::{
    BOTTLE, Cond, LAMP, LAMP_DARK, LOC_BUILDING, Loc, Msg, OIL, OIL_BOTTLE, Obj, Text, WATER,
    WATER_BOTTLE,
};
use crate::parser::Dialect;
use crate::save::{self, RestoreError, SAVE_VERSION};
use crate::state::Game;
use crate::text::{self, Arg};
use turn::Command;

/// How saving works in this build.
#[derive(Debug, Default)]
pub enum SaveMode {
    /// The `save` and `resume` commands work.
    #[default]
    Manual,
    /// `save` and `resume` are disabled.
    Disabled,
    /// `save` and `resume` are disabled; instead the game is written to
    /// this file when it starts and when it ends.
    Auto(File),
}

/// Settings that are not part of the game state.
#[derive(Debug, Default)]
pub struct Config {
    /// Behave like the original: no prompt, uppercase and truncate
    /// words, refuse some abbreviations, print "Initialising...".
    pub oldstyle: bool,
    /// Trace every random number drawn.
    pub debug: bool,
    /// How saving works.
    pub saves: SaveMode,
}

/// How a game begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// A new game: ask whether the player wants instructions.
    New,
    /// Resume the game in this save file.
    Restore(Vec<u8>),
}

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum GameOver {
    /// The game ended normally and the final score was shown.
    Finished {
        /// The final score.
        score: i32,
    },
    /// Input ran out while a yes/no answer was needed.
    InputExhausted,
    /// The player saved the game to resume later.
    Saved,
    /// A restored save file held impossible state.
    TamperedSave,
}

/// Result of anything that can end the game.
type Flow<T = ()> = Result<T, GameOver>;

/// The game.  See the [module documentation](self).
pub struct Engine<C: Console> {
    console: C,
    config: Config,
    game: Game,
    seed: i32,
    command: Command,
    /// The maximum possible score, computed with the score.
    max_score: i32,
}

/// Which object's description to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Aspect {
    /// The inventory line.
    Inventory,
    /// What it looks like in a given state.
    Look,
    /// What it sounds like in a given state.
    Sound,
    /// What is written on it in a given state.
    Writing,
    /// The message for changing to a given state.
    Change,
}

/// Whether, and which, dwarf is at a location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DwarfAt {
    /// No dwarf here, or the dwarves aren't active yet.
    None,
    /// This dwarf (never the pirate) is here.
    Dwarf(usize),
    /// Every dwarf is dead.
    AllDead,
}

impl<C: Console> Engine<C> {
    /// Set up a new game seeded with `seed`.
    pub fn new(console: C, config: Config, seed: i32) -> Engine<C> {
        let mut engine = Engine {
            console,
            config,
            game: Game::new(),
            seed,
            command: Command::default(),
            max_score: 0,
        };
        if engine.config.oldstyle {
            engine.console.print("Initialising...\n");
        }
        engine.set_seed(seed);
        engine
    }

    /// Play the game from the start, until it ends.
    pub fn play(&mut self, start: Start) -> GameOver {
        match self.start(start) {
            Ok(()) => self.run(),
            Err(over) => over,
        }
    }

    /// Begin the game: ask the opening question, or restore a save.
    ///
    /// # Errors
    ///
    /// The game may end before it begins: input may run out, or the
    /// save may be tampered with.
    pub fn start(&mut self, start: Start) -> Result<(), GameOver> {
        match start {
            Start::New => {
                self.game.novice = self.yes_or_no(
                    crate::dungeon::WELCOME_YOU,
                    crate::dungeon::CAVE_NEARBY,
                    crate::dungeon::NO_MESSAGE,
                )?;
                if self.game.novice {
                    self.game.lamp_life = NOVICELIMIT;
                }
            }
            Start::Restore(bytes) => {
                self.restore(&bytes)?;
                if matches!(self.config.saves, SaveMode::Auto(_)) {
                    self.score(score::Ending::Scoring);
                }
            }
        }
        self.autosave();
        Ok(())
    }

    /// Play the game until it ends.  Call after [`start`](Self::start).
    pub fn run(&mut self) -> GameOver {
        loop {
            if let Err(over) = self.turn() {
                return over;
            }
        }
    }

    /// The seed this game started with.
    pub fn seed(&self) -> i32 {
        self.seed
    }

    /// The console.
    pub fn console(&self) -> &C {
        &self.console
    }

    /// The console, mutably.
    pub fn console_mut(&mut self) -> &mut C {
        &mut self.console
    }

    // ---- Output -------------------------------------------------------------

    /// Print a message with its arguments filled in, after a blank line.
    /// Missing or empty messages print nothing.
    fn speak(&mut self, text: Text, args: &[Arg]) {
        self.speak_with_blank(text, true, args);
    }

    fn speak_with_blank(&mut self, text: Text, blank: bool, args: &[Arg]) {
        let Some(template) = text.filter(|t| !t.is_empty()) else {
            return;
        };
        let outdoors = !self.inside(self.game.loc);
        let mut out = String::new();
        if blank {
            out.push('\n');
        }
        out.push_str(&text::render(template, args, outdoors));
        out.push('\n');
        self.console.print(&out);
    }

    /// Print an arbitrary message.
    fn say(&mut self, msg: Msg) {
        self.speak(msg.text(), &[]);
    }

    /// Print an arbitrary message with arguments.
    fn say_with(&mut self, msg: Msg, args: &[Arg]) {
        self.speak(msg.text(), args);
    }

    /// Print an arbitrary message that echoes the player's words, with
    /// `printf` semantics.
    fn say_echo(&mut self, msg: Msg, word: &str) {
        let line = text::printf(msg.text().unwrap_or(""), &[Arg::Str(word)]);
        self.console.print(&format!("\n{line}\n"));
    }

    /// Print one of an object's descriptions.
    fn describe(&mut self, obj: Obj, aspect: Aspect, state: i32) {
        self.describe_with(obj, aspect, state, true, &[]);
    }

    fn describe_with(&mut self, obj: Obj, aspect: Aspect, state: i32, blank: bool, args: &[Arg]) {
        let info = obj.info();
        let pick = |list: &'static [Text]| {
            usize::try_from(state)
                .ok()
                .and_then(|i| list.get(i).copied().flatten())
        };
        let text = match aspect {
            Aspect::Inventory => info.inventory,
            Aspect::Look => pick(info.descriptions),
            Aspect::Sound => pick(info.sounds),
            Aspect::Writing => pick(info.texts),
            Aspect::Change => pick(info.changes),
        };
        self.speak_with_blank(text, blank, args);
    }

    /// Change an object's state and print the message for the change.
    fn state_change(&mut self, obj: Obj, state: i32) {
        self.game.objects.set_prop(obj, state);
        self.describe(obj, Aspect::Change, state);
    }

    // ---- Input ----------------------------------------------------------------

    fn prompt(&self) -> &'static str {
        if self.config.oldstyle { "" } else { "> " }
    }

    /// Read a line of player input, skipping comments.
    fn get_input(&mut self) -> Option<String> {
        let prompt = self.prompt();
        self.console.print("\n");
        let mut input = loop {
            let line = self.console.read_line(prompt)?;
            if !line.starts_with('#') {
                break line;
            }
        };
        if let Some(pos) = input.find('\n') {
            input.truncate(pos);
        }
        self.console.accepted(prompt, &input);
        Some(input)
    }

    /// Wait for a yes or no answer, without asking a question.
    fn silent_yes_or_no(&mut self) -> Flow<bool> {
        loop {
            let reply = self.get_input().ok_or(GameOver::InputExhausted)?;
            match crate::parser::yes_or_no(&reply) {
                Some(answer) => return Ok(answer),
                None => self.say(crate::dungeon::PLEASE_ANSWER),
            }
        }
    }

    /// Ask a question and print the matching response to the answer.
    fn yes_or_no(&mut self, question: Msg, yes: Msg, no: Msg) -> Flow<bool> {
        self.ask(question.text(), yes.text(), no.text())
    }

    fn ask(&mut self, question: Text, yes: Text, no: Text) -> Flow<bool> {
        loop {
            self.speak(question, &[]);
            let reply = self.get_input().ok_or(GameOver::InputExhausted)?;
            match crate::parser::yes_or_no(&reply) {
                Some(answer) => {
                    self.speak(if answer { yes } else { no }, &[]);
                    return Ok(answer);
                }
                None => self.say(crate::dungeon::PLEASE_ANSWER),
            }
        }
    }

    fn dialect(&self) -> (bool, String) {
        (self.config.oldstyle, self.game.magic_word())
    }

    fn with_dialect<T>(&self, f: impl FnOnce(&Dialect) -> T) -> T {
        let (oldstyle, magic_word) = self.dialect();
        f(&Dialect {
            oldstyle,
            magic_word: &magic_word,
        })
    }

    // ---- Randomness -------------------------------------------------------------

    /// A random number in `[0, range)`.
    fn randrange(&mut self, range: i32) -> i32 {
        let raw = self.game.rng.next_raw();
        self.trace_random(raw);
        crate::rng::Lcg::scale(raw, range)
    }

    /// True `n` percent of the time.
    fn pct(&mut self, n: i32) -> bool {
        self.randrange(100) < n
    }

    fn trace_random(&mut self, raw: i32) {
        if self.config.debug {
            self.console.print(&format!("# random {raw}\n"));
        }
    }

    /// Reseed the generator; this also picks a new magic word.
    fn set_seed(&mut self, seed: i32) {
        let mut draws = Vec::with_capacity(5);
        self.game.reseed(seed, |raw| draws.push(raw));
        for raw in draws {
            self.trace_random(raw);
        }
    }

    // ---- Where things are -------------------------------------------------------------

    fn prop(&self, obj: Obj) -> i32 {
        self.game.objects.prop(obj)
    }

    fn toting(&self, obj: Obj) -> bool {
        self.game.objects.is_carried(obj)
    }

    /// Either end of the object is at the player's location.
    fn at(&self, obj: Obj) -> bool {
        self.game.objects.is_at(obj, self.game.loc)
    }

    /// The object is at the player's location or carried.
    fn here(&self, obj: Obj) -> bool {
        self.game.objects.is_here(obj, self.game.loc)
    }

    fn forced(loc: Loc) -> bool {
        loc.has(Cond::Forced)
    }

    fn outside(loc: Loc) -> bool {
        loc.has(Cond::Above) || loc.has(Cond::Forest)
    }

    /// In the cave, or in the building.
    fn inside(&self, loc: Loc) -> bool {
        !Self::outside(loc) || loc == LOC_BUILDING
    }

    fn is_dark_here(&self) -> bool {
        !self.game.loc.has(Cond::Lit) && (self.prop(LAMP) == LAMP_DARK || !self.here(LAMP))
    }

    /// The liquid in the bottle, if any.
    fn liquid(&self) -> Option<Obj> {
        match self.prop(BOTTLE) {
            WATER_BOTTLE => Some(WATER),
            OIL_BOTTLE => Some(OIL),
            _ => None,
        }
    }

    /// The liquid in the bottle, or `NO_OBJECT`.  For the comparisons
    /// the original makes against objects that may themselves be
    /// `NO_OBJECT` (a verb applied to a number, say).
    fn liquid_or_nothing(&self) -> Obj {
        self.liquid().unwrap_or(crate::dungeon::NO_OBJECT)
    }

    /// The liquid available at a location, if any.
    fn liquid_at(loc: Loc) -> Option<Obj> {
        loc.has(Cond::Fluid)
            .then(|| if loc.has(Cond::Oily) { OIL } else { WATER })
    }

    /// The dwarf at `loc`, ignoring the pirate.
    fn dwarf_at(&self, loc: Loc) -> DwarfAt {
        if self.game.dwarf_activity < 2 {
            return DwarfAt::None;
        }
        let mut result = DwarfAt::AllDead;
        for (i, d) in self
            .game
            .dwarves
            .iter()
            .enumerate()
            .take(crate::dungeon::NDWARVES)
            .skip(1)
        {
            if d.loc == loc {
                return DwarfAt::Dwarf(i);
            }
            if d.loc != crate::dungeon::LOC_NOWHERE {
                result = DwarfAt::None;
            }
        }
        result
    }

    fn dwarf_is_here(&self) -> bool {
        matches!(self.dwarf_at(self.game.loc), DwarfAt::Dwarf(_))
    }

    // ---- Saving -------------------------------------------------------------------

    fn save_bytes(&self) -> Vec<u8> {
        save::encode(&self.game, SAVE_VERSION)
    }

    /// Replace the game with a saved one.
    fn restore(&mut self, bytes: &[u8]) -> Flow {
        match save::decode(bytes) {
            Ok(game) => self.game = game,
            Err(RestoreError::NotASave) => self.say(crate::dungeon::BAD_SAVE),
            Err(RestoreError::VersionSkew { found }) => self.say_with(
                crate::dungeon::VERSION_SKEW,
                &[
                    Arg::Int(found / 10),
                    Arg::Int(found % 10),
                    Arg::Int(SAVE_VERSION / 10),
                    Arg::Int(SAVE_VERSION % 10),
                ],
            ),
            Err(RestoreError::Tampered) => {
                self.say(crate::dungeon::SAVE_TAMPERING);
                return Err(GameOver::TamperedSave);
            }
        }
        Ok(())
    }

    /// In autosave mode, write the game to the autosave file.
    fn autosave(&mut self) {
        let bytes = self.save_bytes();
        if let SaveMode::Auto(file) = &mut self.config.saves {
            use std::io::Seek as _;
            // Autosaving is best-effort, as in the original.
            let _ = file.rewind();
            let _ = file.write_all(&bytes);
            let _ = file.flush();
        }
    }
}

/// Turn limit for players who asked for instructions.
const NOVICELIMIT: i32 = 1000;

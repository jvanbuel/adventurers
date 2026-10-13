// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The main loop: move the player, describe where they are, then read
//! and carry out commands until one of them moves the player again.

use super::{Aspect, Console, Engine, Flow};
use crate::dungeon::*;
use crate::parser::{Word, WordKind, word_count};
use crate::state::Knife;

/// Percent chance of falling into a pit when moving in the dark.
const PIT_KILL_PROB: i32 = 35;
/// Turns left after trying to leave once the cave starts closing.
pub(super) const PANICTIME: i32 = 15;
/// Longest command line considered, as in the C version.
const LINESIZE: usize = 1024;

/// How a verb is being used.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum SpeechPart {
    /// Not yet known (an object word was given on its own).
    #[default]
    Unknown,
    /// No object.
    Intransitive,
    /// With an object.
    Transitive,
}

/// How far a command has got.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub(super) enum CommandState {
    #[default]
    Empty,
    Given,
    Preprocessed,
    Processing,
    Executed,
}

/// The command being worked on.  It persists across inputs, so that
/// "get" followed by "lamp" means "get lamp".
#[derive(Clone, Debug)]
pub(super) struct Command {
    pub part: SpeechPart,
    pub words: [Word; 2],
    pub verb: Verb,
    pub obj: Obj,
    pub state: CommandState,
}

impl Default for Command {
    fn default() -> Self {
        Command {
            part: SpeechPart::Unknown,
            words: [Word::empty(), Word::empty()],
            verb: ACT_NULL,
            obj: NO_OBJECT,
            state: CommandState::Empty,
        }
    }
}

/// What to do after an action.  These were once FORTRAN line numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Phase {
    /// The action moved the player (or will have): end the turn.
    Terminate,
    /// Do a null move, to describe the (possibly new) location.
    Move,
    /// Describe the location and read a new command.
    Top,
    /// Forget the object and read a new command.
    ClearObj,
    /// Read another word to complete the command.
    CheckHint,
    /// Process the second word as a command of its own.
    Word2,
    /// Ask what the verb should apply to.
    Unknown,
    /// The player woke the sleeping dwarves: game over.
    DwarfWake,
}

impl<C: Console> Engine<C> {
    /// One turn: move if the last command said to, then read and carry
    /// out commands until the player moves again.
    pub(super) fn turn(&mut self) -> Flow {
        if !self.do_move()? {
            return Ok(());
        }
        if !self.do_command()? {
            return Err(self.terminate(super::score::Ending::Quit));
        }
        Ok(())
    }

    /// Actually move to the new location, then let the dwarves move.
    /// Returns false if the player died on the way.
    fn do_move(&mut self) -> Flow<bool> {
        let g = &mut self.game;
        // Can't leave the cave once it's closing (except by the main
        // office).
        if Self::outside(g.newloc) && g.newloc != LOC_NOWHERE && g.closing {
            self.say(EXIT_CLOSED);
            let g = &mut self.game;
            g.newloc = g.loc;
            if !g.panicked {
                g.closed_clock = PANICTIME;
            }
            g.panicked = true;
        }

        // A dwarf who has seen the player and came from where they
        // want to go blocks the way, unless the player is somewhere the
        // pirate can't go (the dwarves are rooted in place).
        let g = &self.game;
        if g.newloc != g.loc && !Self::forced(g.loc) && !g.loc.has(Cond::NoArrr) {
            let blocked = g.dwarves[1..NDWARVES]
                .iter()
                .any(|d| d.oldloc == g.newloc && d.seen);
            if blocked {
                self.game.newloc = self.game.loc;
                self.say(DWARF_BLOCK);
            }
        }
        self.game.loc = self.game.newloc;

        if !self.dwarves_move() {
            self.croak()?;
        }

        if self.game.loc == LOC_NOWHERE {
            self.croak()?;
        }

        // The easiest way to get killed is to fall into a pit in pitch
        // darkness.
        if !Self::forced(self.game.loc)
            && self.is_dark_here()
            && self.game.was_dark
            && self.pct(PIT_KILL_PROB)
        {
            self.say(PIT_FALL);
            self.game.oldloc2 = self.game.loc;
            self.croak()?;
            return Ok(false);
        }
        Ok(true)
    }

    fn describe_location(&mut self) {
        let loc = self.game.loc;
        let info = loc.info();
        let mut text = info.small;
        if self.game.visits[loc.index()] % self.game.long_desc_interval == 0 || text.is_none() {
            text = info.big;
        }
        if !Self::forced(loc) && self.is_dark_here() {
            text = PITCH_DARK.text();
        }
        if self.toting(BEAR) {
            self.say(TAME_BEAR);
        }
        self.speak(text, &[]);
        if loc == LOC_Y2 && self.pct(25) && !self.game.closing {
            self.say(SAYS_PLUGH);
        }
    }

    /// Describe the objects here.  Seeing a treasure for the first time
    /// marks it found.
    fn list_objects(&mut self) {
        if self.is_dark_here() {
            return;
        }
        let loc = self.game.loc;
        self.game.visits[loc.index()] += 1;
        let present: Vec<Obj> = self.game.objects.at(loc).collect();
        for obj in present {
            if obj == STEPS && self.toting(NUGGET) {
                continue;
            }
            let objects = &mut self.game.objects;
            if objects.is_stashed(obj) || objects.is_not_found(obj) {
                if self.game.closed {
                    continue;
                }
                objects.set_found(obj);
                // These start in special states once seen; state 0 is
                // needed for the full score.
                match obj {
                    RUG => objects.set_prop(RUG, RUG_DRAGON),
                    CHAIN => objects.set_prop(CHAIN, CHAINING_BEAR),
                    EGGS => self.game.seen_big_words = true,
                    _ => {}
                }
                self.game.treasures_unseen -= 1;
            }
            let state = if obj == STEPS {
                if self.game.objects.fixed(STEPS) == Fixed::At(loc) {
                    STEPS_UP
                } else {
                    STEPS_DOWN
                }
            } else {
                self.prop(obj)
            };
            self.describe(obj, Aspect::Look, state);
        }
    }

    /// Read a command; false if input has run out.
    fn get_command_input(&mut self) -> bool {
        let mut input = loop {
            let Some(line) = self.get_input() else {
                return false;
            };
            if word_count(&line) > 2 {
                self.say(TWO_WORDS);
                continue;
            }
            if !line.is_empty() {
                break line;
            }
        };
        if input.len() >= LINESIZE {
            let mut cut = LINESIZE - 1;
            while !input.is_char_boundary(cut) {
                cut -= 1;
            }
            input.truncate(cut);
        }
        self.command.words = self.with_dialect(|d| d.tokenize(&input));
        self.command.state = CommandState::Given;
        true
    }

    pub(super) fn clear_command(&mut self) {
        let cmd = &mut self.command;
        cmd.verb = ACT_NULL;
        cmd.part = SpeechPart::Unknown;
        self.game.last_object = cmd.obj;
        cmd.obj = NO_OBJECT;
        cmd.state = CommandState::Empty;
    }

    /// Rewrite a few irregular commands into regular ones.  Returns false
    /// if the command was handled here and another is needed.
    ///
    /// The comparisons are on raw vocabulary numbers regardless of word
    /// kind, as in the original, so that odd inputs behave identically.
    fn preprocess_command(&mut self) -> bool {
        let loc = self.game.loc;
        let [w0, w1] = &self.command.words;
        if w0.kind == WordKind::Motion(ENTER) && (w1.id() == STREAM.raw() || w1.id() == WATER.raw())
        {
            // "enter water": gets the player wet.
            self.say(if Self::liquid_at(loc) == Some(WATER) {
                FEET_WET
            } else {
                WHERE_QUERY
            });
            return false;
        }

        let cage_and_bird_here = self.here(CAGE) && self.here(BIRD);
        let words = &mut self.command.words;
        if let WordKind::Object(_) = words[0].kind {
            // "lamp get" means "get lamp".
            if let WordKind::Action(_) = words[1].kind {
                words.swap(0, 1);
            }
            // "grate" means go to the grate, from nearby.
            if words[0].id() == GRATE.raw() {
                let target = match loc {
                    LOC_START | LOC_VALLEY | LOC_SLIT => DEPRESSION.raw(),
                    LOC_COBBLE | LOC_DEBRIS | LOC_AWKWARD | LOC_BIRDCHAMBER | LOC_PITTOP => {
                        ENTRANCE.raw()
                    }
                    // Elsewhere the grate's object number is used as a
                    // motion number, as in the original.
                    _ => words[0].id(),
                };
                words[0].kind = WordKind::Motion(Motion::from_raw(target));
            }
            // "water plant" means "pour water", and so on.
            let (id0, id1) = (words[0].id(), words[1].id());
            if (id0 == WATER.raw() || id0 == OIL.raw())
                && (id1 == PLANT.raw() || id1 == DOOR.raw())
                && self.game.objects.is_at(Obj::from_raw(id1), loc)
            {
                words[1] = words[0].clone();
                words[0] = Word {
                    raw: "pour".to_string(),
                    kind: WordKind::Action(POUR),
                };
            }
            // "cage bird" means "take bird".
            let (id0, id1) = (words[0].id(), words[1].id());
            if id0 == CAGE.raw() && id1 == BIRD.raw() && cage_and_bird_here {
                words[0].kind = WordKind::Action(CARRY);
            }
        }

        // A word of no kind is taken as a motion.
        if words[0].kind == WordKind::Empty {
            words[0].kind = WordKind::Motion(MOT_0);
        }
        self.command.state = CommandState::Preprocessed;
        true
    }

    /// Describe the location, then read and carry out commands until the
    /// player moves.  Returns false if input runs out.
    fn do_command(&mut self) -> Flow<bool> {
        self.clear_command();
        while self.command.state != CommandState::Executed {
            self.describe_location();

            if Self::forced(self.game.loc) {
                self.playermove(HERE)?;
                return Ok(true);
            }

            self.list_objects();

            // Keep reading until a command is both given and executed.
            self.clear_command();
            while self.command.state <= CommandState::Given {
                if self.game.closed {
                    self.unstash_carried();
                }
                self.game.was_dark = self.is_dark_here();

                // A knife thrown elsewhere disappears.
                if let Knife::ThrownAt(at) = self.game.knife {
                    if at != self.game.loc {
                        self.game.knife = Knife::Absent;
                    }
                }

                while self.command.state < CommandState::Preprocessed {
                    self.check_hints()?;
                    if !self.get_command_input() {
                        return Ok(false);
                    }
                    // Saying anything but the next magic word breaks
                    // the FEE FIE FOE FOO sequence.
                    let g = &mut self.game;
                    g.fee_fie_progress = if g.fee_fie_progress > 0 {
                        -g.fee_fie_progress
                    } else {
                        0
                    };
                    g.turns += 1;
                    self.preprocess_command();
                }

                if self.close_check() {
                    return Ok(true);
                }

                while self.command.state == CommandState::Preprocessed {
                    self.process_command()?;
                }
            }
        }
        Ok(true)
    }

    /// Once the cave is closed, objects picked up from the end-game
    /// piles stop being stashed (so they are described once put down).
    fn unstash_carried(&mut self) {
        let objects = &self.game.objects;
        if (objects.is_not_found(OYSTER) || objects.is_stashed(OYSTER)) && self.toting(OYSTER) {
            self.describe(OYSTER, Aspect::Look, 1);
        }
        for obj in Obj::all() {
            let objects = &mut self.game.objects;
            if objects.is_carried(obj) && (objects.is_not_found(obj) || objects.is_stashed(obj)) {
                let prop = objects.prop(obj);
                objects.stash(obj, prop);
            }
        }
    }

    /// Carry out the first word of a preprocessed command.
    fn process_command(&mut self) -> Flow {
        self.command.state = CommandState::Processing;
        let [w0, w1] = &self.command.words;

        if w0.kind == WordKind::Unknown {
            let raw = w0.raw.clone();
            self.say_echo(DONT_KNOW, &raw);
            self.clear_command();
            return Ok(());
        }

        // Hint at shortcuts.
        let says_go_somewhere = w0.raw.eq_ignore_ascii_case("go") && w1.id() != 0;
        if w0.raw.eq_ignore_ascii_case("west") {
            self.game.west_count += 1;
            if self.game.west_count == 10 {
                self.say(W_IS_WEST);
            }
        }
        if says_go_somewhere {
            self.game.go_count += 1;
            if self.game.go_count == 10 {
                self.say(GO_UNNEEDED);
            }
        }

        let [w0, w1] = &self.command.words;
        match w0.kind {
            WordKind::Motion(motion) => {
                self.playermove(motion)?;
                self.command.state = CommandState::Executed;
                return Ok(());
            }
            WordKind::Object(obj) => {
                self.command.part = SpeechPart::Unknown;
                self.command.obj = obj;
            }
            WordKind::Action(verb) => {
                self.command.part = if w1.kind == WordKind::Numeric {
                    SpeechPart::Transitive
                } else {
                    SpeechPart::Intransitive
                };
                self.command.verb = verb;
            }
            WordKind::Numeric => {
                if !self.config.oldstyle {
                    let raw = w0.raw.clone();
                    self.say_echo(DONT_KNOW, &raw);
                    self.clear_command();
                    return Ok(());
                }
                // In oldstyle mode a number falls through to the
                // previous verb, as in the original.
            }
            WordKind::Empty | WordKind::Unknown => unreachable!("handled by preprocess_command"),
        }

        match self.action(self.command.clone())? {
            Phase::Terminate => self.command.state = CommandState::Executed,
            Phase::Move => {
                self.playermove(NUL)?;
                self.command.state = CommandState::Executed;
            }
            Phase::Word2 => {
                let second = std::mem::replace(&mut self.command.words[1], Word::empty());
                self.command.words[0] = second;
                self.command.state = CommandState::Preprocessed;
            }
            Phase::Unknown => {
                // Ask "Verb what?" and read another word.
                let raw = &mut self.command.words[0].raw;
                if let Some(first) = raw.get(0..1) {
                    let upper = first.to_ascii_uppercase();
                    raw.replace_range(0..1, &upper);
                }
                let raw = raw.clone();
                self.say_echo(DO_WHAT, &raw);
                self.command.obj = NO_OBJECT;
                self.command.state = CommandState::Given;
            }
            Phase::CheckHint => self.command.state = CommandState::Given,
            Phase::DwarfWake => {
                self.say(DWARVES_AWAKEN);
                return Err(self.terminate(super::score::Ending::Died));
            }
            Phase::ClearObj => self.clear_command(),
            Phase::Top => {}
        }
        Ok(())
    }
}

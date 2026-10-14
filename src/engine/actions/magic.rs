// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Magic words and the end-game dynamite.

use super::super::turn::{Command, Phase};
use super::super::{Aspect, Console, Engine, Flow};
use crate::dungeon::*;
use crate::objects::Place;
use crate::parser::WordKind;
use crate::state::Bonus;

impl<C: Console> Engine<C> {
    /// FEE FIE FOE FOO (and FUM).  Said in order, the last word brings
    /// the golden eggs back to the giant's room.  `id` is the raw number
    /// of the word said.
    pub(super) fn big_words(&mut self, id: i32) -> Phase {
        let progress = self.game.fee_fie_progress.abs();
        let word = |v: Verb| v.raw();

        // Only FEE can start the sequence.
        if progress == 0 && [FIE, FOE, FOO, FUM].map(word).contains(&id) {
            self.say(NOTHING_HAPPENS);
            return Phase::ClearObj;
        }

        let steps = [
            (0, word(FEE)),
            (word(FEE), word(FIE)),
            (word(FIE), word(FOE)),
            (word(FOE), word(FOO)),
        ];
        if !steps.contains(&(progress, id)) {
            let msg = if self.config.oldstyle || self.game.seen_big_words {
                START_OVER
            } else {
                WELL_POINTLESS
            };
            self.say(msg);
            self.game.fee_fie_progress = 0;
            return Phase::ClearObj;
        }

        self.game.fee_fie_progress = id;
        if id != word(FOO) {
            self.say(OK_MAN);
            return Phase::ClearObj;
        }
        self.game.fee_fie_progress = 0;
        let home = EGGS.info().plac;
        let objects = &mut self.game.objects;
        if objects.place(EGGS) == Place::At(home)
            || (objects.is_carried(EGGS) && self.game.loc == home)
        {
            self.say(NOTHING_HAPPENS);
            return Phase::ClearObj;
        }
        // Bring back the troll if the eggs are stolen back from him
        // before crossing.
        if objects.place(EGGS) == Place::NOWHERE
            && objects.place(TROLL) == Place::NOWHERE
            && objects.prop(TROLL) == TROLL_UNPAID
        {
            objects.set_prop(TROLL, TROLL_PAIDONCE);
        }
        let state = if self.here(EGGS) {
            EGGS_VANISHED
        } else if self.game.loc == home {
            EGGS_HERE
        } else {
            EGGS_DONE
        };
        self.describe(EGGS, Aspect::Look, state);
        self.game.objects.move_to(EGGS, Place::At(home));
        Phase::ClearObj
    }

    /// Say.  Magic words work as if said alone; anything else is echoed.
    pub(super) fn say_word(&mut self, command: &Command) -> Phase {
        let word = &command.words[1];
        match word.kind {
            WordKind::Motion(XYZZY | PLUGH | PLOVER) => Phase::Word2,
            WordKind::Action(PART) => self.reservoir(),
            WordKind::Action(v @ (FEE | FIE | FOE | FOO | FUM)) => self.big_words(v.raw()),
            _ => {
                self.say_echo(OKEY_DOKEY, &word.raw);
                Phase::ClearObj
            }
        }
    }

    /// The magic word parts the reservoir (or drowns the player at its
    /// bottom).
    pub(super) fn reservoir(&mut self) -> Phase {
        if !self.at(RESER) && self.game.loc != LOC_RESBOTTOM {
            self.say(NOTHING_HAPPENS);
            return Phase::ClearObj;
        }
        let state = if self.prop(RESER) == WATERS_PARTED {
            WATERS_UNPARTED
        } else {
            WATERS_PARTED
        };
        self.state_change(RESER, state);
        if self.at(RESER) {
            return Phase::ClearObj;
        }
        self.game.oldloc2 = self.game.loc;
        self.game.newloc = LOC_NOWHERE;
        self.say(NOT_BRIGHT);
        Phase::Terminate
    }

    /// Blast.  Only works with the dynamite in the repository, and ends
    /// the game one way or another.
    pub(super) fn blast(&mut self) -> Flow<Phase> {
        if self.game.objects.is_not_found(ROD2) || !self.game.closed {
            self.say(REQUIRES_DYNAMITE);
            return Ok(Phase::ClearObj);
        }
        let (bonus, msg) = if self.here(ROD2) {
            (Bonus::Splatter, SPLATTER_MESSAGE)
        } else if self.game.loc == LOC_NE {
            (Bonus::Defeat, DEFEAT_MESSAGE)
        } else {
            (Bonus::Victory, VICTORY_MESSAGE)
        };
        self.game.bonus = bonus;
        self.say(msg);
        Err(self.terminate(super::super::score::Ending::Died))
    }
}

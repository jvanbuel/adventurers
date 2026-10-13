// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Offering hints to players who linger where a hint applies.

use super::{Console, DwarfAt, Engine, Flow};
use crate::dungeon::*;
use crate::objects::Place;
use crate::state::WARNTIME;

/// What a hint's situation check decided.
enum Verdict {
    /// Offer the hint now.
    Offer,
    /// Not yet; keep counting, and look at no further hints this turn.
    Wait,
    /// Not applicable; start counting again, and look at no further
    /// hints this turn.
    Reset,
}

impl<C: Console> Engine<C> {
    /// Count turns spent where each unused hint applies, and offer one
    /// once the player has been stuck long enough.
    pub(super) fn check_hints(&mut self) -> Flow {
        let loc = self.game.loc;
        if i64::from(loc.info().condition_bits()) < i64::from(self.game.hint_threshold) {
            return Ok(());
        }
        for (n, hint) in HINTS.iter().enumerate() {
            let state = &mut self.game.hints[n];
            if state.used {
                continue;
            }
            if !loc.info().has_bit(HINT_BASE + n as u32 + 1) {
                state.turns_here = -1;
            }
            state.turns_here += 1;
            if state.turns_here < hint.turns {
                continue;
            }

            match self.hint_verdict(n) {
                Verdict::Offer => {}
                Verdict::Wait => return Ok(()),
                Verdict::Reset => {
                    self.game.hints[n].turns_here = 0;
                    return Ok(());
                }
            }

            self.game.hints[n].turns_here = 0;
            if !self.ask(hint.question, NO_MESSAGE.text(), OK_MAN.text())? {
                return Ok(());
            }
            self.say_with(HINT_COST, &[hint.penalty.into(), hint.penalty.into()]);
            let used = self.ask(WANT_HINT.text(), hint.hint, OK_MAN.text())?;
            self.game.hints[n].used = used;
            if used && self.game.lamp_life > WARNTIME {
                self.game.lamp_life += WARNTIME * hint.penalty;
            }
        }
        Ok(())
    }

    /// Whether the situation for hint `n` really calls for it.
    fn hint_verdict(&self, n: usize) -> Verdict {
        use Verdict::{Offer, Reset, Wait};
        let g = &self.game;
        let empty = |loc: Loc| g.objects.at(loc).next().is_none();
        let lost = empty(g.loc) && empty(g.oldloc) && empty(g.oldloc2);
        let offer_or = |cond: bool, otherwise: Verdict| if cond { Offer } else { otherwise };
        match n {
            // Trying to get into the cave.
            0 => offer_or(self.prop(GRATE) == GRATE_CLOSED && !self.here(KEYS), Reset),
            // Trying to catch the bird.
            1 => offer_or(
                g.objects.place(BIRD) == Place::At(g.loc)
                    && self.toting(ROD)
                    && g.last_object == BIRD,
                Wait,
            ),
            // Trying to deal with the snake.
            2 => offer_or(self.here(SNAKE) && !self.here(BIRD), Reset),
            // Lost in the maze.
            3 => offer_or(lost && g.objects.holding() > 1, Reset),
            // Pondering the dark room.
            4 => offer_or(
                !g.objects.is_not_found(EMERALD) && g.objects.is_not_found(PYRAMID),
                Reset,
            ),
            // At Witt's End.
            5 => Offer,
            // At the cliff with the urn.
            6 => offer_or(g.dwarf_activity == 0, Reset),
            // Lost in the forest.
            7 => offer_or(lost, Wait),
            // Trying to deal with the ogre.
            8 => match self.dwarf_at(g.loc) {
                DwarfAt::AllDead => Reset,
                DwarfAt::None if self.here(OGRE) => Offer,
                _ => Wait,
            },
            // Found all treasures except the jade.
            9 => offer_or(
                g.treasures_unseen == 1
                    && (g.objects.is_stashed(JADE) || g.objects.is_not_found(JADE)),
                Reset,
            ),
            _ => unreachable!("no situation check for hint {n}"),
        }
    }
}

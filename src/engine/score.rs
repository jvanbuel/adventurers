// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Death, scoring, and the end of the game.

use super::{Console, Engine, Flow, GameOver};
use crate::dungeon::*;
use crate::objects::Place;
use crate::state::Bonus;

/// Why the score is being worked out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ending {
    /// The player died or won: they get the points for not quitting.
    Died,
    /// The player quit, or input ran out.
    Quit,
    /// The `score` command: show the score and carry on.
    Scoring,
}

impl<C: Console> Engine<C> {
    /// Work out the score, and the maximum possible.
    ///
    /// | Objective                | Points            | Max |
    /// |--------------------------|-------------------|-----|
    /// | Getting well into cave   | 25                | 25  |
    /// | Each treasure < chest    | 12                | 60  |
    /// | Treasure chest itself    | 14                | 14  |
    /// | Each treasure > chest    | 16                | 224 |
    /// | Surviving                | (max - deaths)*10 | 30  |
    /// | Not quitting             | 4                 | 4   |
    /// | Reaching closing time    | 25                | 25  |
    /// | Closed: quit or killed   | 10                |     |
    /// | Closed: klutzed          | 25                |     |
    /// | Closed: wrong way        | 30                |     |
    /// | Closed: success          | 45                | 45  |
    /// | Came to Witt's End       | 1                 | 1   |
    /// | Round out the total      | 2                 | 2   |
    ///
    /// Points are deducted for hints, for too many turns, and for saves.
    pub(super) fn score(&mut self, ending: Ending) -> i32 {
        let g = &self.game;
        let mut score = 0;
        let mut max = 0;

        // Treasures: 2 points for finding each, the rest for getting it
        // to the building intact.
        for o in Obj::all().filter(|o| o.is_treasure() && o.info().inventory.is_some()) {
            let worth = match o.cmp(&CHEST) {
                std::cmp::Ordering::Less => 12,
                std::cmp::Ordering::Equal => 14,
                std::cmp::Ordering::Greater => 16,
            };
            if !g.objects.is_stashed(o) && !g.objects.is_not_found(o) {
                score += 2;
            }
            if g.objects.place(o) == Place::At(LOC_BUILDING) && g.objects.is_found(o) {
                score += worth - 2;
            }
            max += worth;
        }

        score += (NDEATHS as i32 - g.deaths) * 10;
        max += NDEATHS as i32 * 10;
        if ending == Ending::Died {
            score += 4;
        }
        max += 4;
        if g.dwarf_activity != 0 {
            score += 25;
        }
        max += 25;
        if g.closing {
            score += 25;
        }
        max += 25;
        if g.closed {
            score += match g.bonus {
                Bonus::None => 10,
                Bonus::Splatter => 25,
                Bonus::Defeat => 30,
                Bonus::Victory => 45,
            };
        }
        max += 45;
        if g.objects.place(MAGAZINE) == Place::At(LOC_WITTSEND) {
            score += 1;
        }
        max += 1;
        score += 2;
        max += 2;

        for (state, hint) in g.hints.iter().zip(&HINTS) {
            if state.used {
                score -= hint.penalty;
            }
        }
        if g.novice {
            score -= 5;
        }
        if g.clue_read {
            score -= 10;
        }
        score -= g.turn_penalty + g.save_penalty;
        self.max_score = max;

        if ending == Ending::Scoring {
            let turns = g.turns;
            self.say_with(
                GARNERED_POINTS,
                &[score.into(), max.into(), turns.into(), turns.into()],
            );
        }
        score
    }

    /// End the game: show the final score and rank.
    pub(super) fn terminate(&mut self, ending: Ending) -> GameOver {
        let points = self.score(ending);
        self.autosave();
        let (max, g) = (self.max_score, &self.game);
        let (turn_penalty, save_penalty, turns) = (g.turn_penalty, g.save_penalty, g.turns);

        if points + turn_penalty + 1 >= max && turn_penalty != 0 {
            self.say(TOOK_LONG);
        }
        if points + save_penalty + 1 >= max && save_penalty != 0 {
            self.say(WITHOUT_SUSPENDS);
        }
        self.say_with(
            TOTAL_SCORE,
            &[points.into(), max.into(), turns.into(), turns.into()],
        );
        match CLASSES[1..].iter().position(|c| c.threshold >= points) {
            Some(i) => {
                let class = &CLASSES[1 + i];
                self.speak(class.message, &[]);
                if 1 + i < NCLASSES - 1 {
                    let next = class.threshold + 1 - points;
                    self.say_with(NEXT_HIGHER, &[next.into(), next.into()]);
                } else {
                    self.say(NO_HIGHER);
                }
            }
            None => self.say(OFF_SCALE),
        }
        GameOver::Finished { score: points }
    }

    /// The player has been killed.  Offer reincarnation (a limited
    /// number of times).  If accepted, everything carried is left where
    /// they last were safe, except the lamp, which is turned off and
    /// left outside the building; the player wakes up in the building.
    pub(super) fn croak(&mut self) -> Flow {
        // Saves can record negative deaths; use the first obituary then.
        let obituary = &OBITUARIES[self.game.deaths.clamp(0, NDEATHS as i32 - 1) as usize];
        self.game.deaths += 1;

        if self.game.closing {
            // No resurrection during closing time.
            self.say(DEATH_CLOSING);
            return Err(self.terminate(Ending::Died));
        }
        let again = self.ask(obituary.query, obituary.yes_response, OK_MAN.text())?;
        if !again || self.game.deaths == NDEATHS as i32 {
            return Err(self.terminate(Ending::Died));
        }

        let objects = &mut self.game.objects;
        objects.set_place(WATER, Place::NOWHERE);
        objects.set_place(OIL, Place::NOWHERE);
        if objects.is_carried(LAMP) {
            objects.set_prop(LAMP, LAMP_DARK);
        }
        // Backwards, so the bird is dropped before the cage.
        let safe = Place::At(self.game.oldloc2);
        for o in Obj::all().rev() {
            if objects.is_carried(o) {
                objects.drop(
                    o,
                    if o == LAMP {
                        Place::At(LOC_START)
                    } else {
                        safe
                    },
                );
            }
        }
        let g = &mut self.game;
        g.oldloc = LOC_BUILDING;
        g.loc = LOC_BUILDING;
        g.newloc = LOC_BUILDING;
        Ok(())
    }
}

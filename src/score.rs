// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Scoring and wrap-up.

use crate::advent::*;
use crate::dungeon::*;
use crate::misc::exit;

impl Advent {
    /// mode is ScoreGame if scoring, QuitGame if quitting, EndGame if
    /// died or won.
    pub fn score(&mut self, mode: Termination) -> i32 {
        let mut score = 0;

        //  The present scoring algorithm is as follows:
        //     Objective:          Points:        Present total possible:
        //  Getting well into cave   25                    25
        //  Each treasure < chest    12                    60
        //  Treasure chest itself    14                    14
        //  Each treasure > chest    16                   224
        //  Surviving             (MAX-NUM)*10             30
        //  Not quitting              4                     4
        //  Reaching "game.closng"   25                    25
        //  "Closed": Quit/Killed    10
        //            Klutzed        25
        //            Wrong way      30
        //            Success        45                    45
        //  Came to Witt's End        1                     1
        //  Round out the total       2                     2
        //                                       TOTAL:   430
        //  Points can also be deducted for using hints or too many turns,
        //  or for saving intermediate positions.

        //  First tally up the treasures.  Must be in building and not
        //  broken.  Give the poor guy 2 points just for finding each
        //  treasure.
        self.mxscor = 0;
        for i in 1..NOBJECTS as i32 {
            if !OBJECTS[i as usize].is_treasure {
                continue;
            }
            if OBJECTS[i as usize].inventory.is_some() {
                let mut k = 12;
                if i == CHEST {
                    k = 14;
                }
                if i > CHEST {
                    k = 16;
                }
                if !self.object_is_stashed(i) && !self.object_is_notfound(i) {
                    score += 2;
                }
                if self.place(i) == LOC_BUILDING && self.object_is_found(i) {
                    score += k - 2;
                }
                self.mxscor += k;
            }
        }

        //  Now look at how he finished and how far he got.  NDEATHS and
        //  game.numdie tell us how well he survived.  game.dflag will tell
        //  us if he ever got suitably deep into the cave.  game.closng
        //  still indicates whether he reached the endgame.  And if he got
        //  as far as "cave closed" (indicated by "game.closed"), then
        //  bonus is zero for mundane exits or 133, 134, 135 if he blew it
        //  (so to speak).
        score += (NDEATHS as i32 - self.game.numdie) * 10;
        self.mxscor += NDEATHS as i32 * 10;
        if mode == Termination::EndGame {
            score += 4;
        }
        self.mxscor += 4;
        if self.game.dflag != 0 {
            score += 25;
        }
        self.mxscor += 25;
        if self.game.closng {
            score += 25;
        }
        self.mxscor += 25;
        if self.game.closed {
            match self.game.bonus {
                BONUS_NONE => score += 10,
                BONUS_SPLATTER => score += 25,
                BONUS_DEFEAT => score += 30,
                BONUS_VICTORY => score += 45,
                _ => {}
            }
        }
        self.mxscor += 45;

        // Did he come to Witt's End as he should?
        if self.place(MAGAZINE) == LOC_WITTSEND {
            score += 1;
        }
        self.mxscor += 1;

        // Round it off.
        score += 2;
        self.mxscor += 2;

        // Deduct for hints/turns/saves. Hints < 4 are special; see
        // database desc.
        for i in 0..NHINTS {
            if self.game.hints[i].used {
                score -= HINTS[i].penalty;
            }
        }
        if self.game.novice {
            score -= 5;
        }
        if self.game.clshnt {
            score -= 10;
        }
        score = score - self.game.trnluz - self.game.saved;

        // Return to score command if that's where we came from.
        if mode == Termination::ScoreGame {
            let t = self.game.turns;
            self.rspeak(
                GARNERED_POINTS,
                &[Arg::I(score), Arg::I(self.mxscor), Arg::I(t), Arg::I(t)],
            );
        }

        score
    }

    /// End of game.  Let's tell him all about it.
    pub fn terminate(&mut self, mode: Termination) -> ! {
        let points = self.score(mode);
        #[cfg(feature = "autosave")]
        self.autosave();

        if points + self.game.trnluz + 1 >= self.mxscor && self.game.trnluz != 0 {
            self.rspeak(TOOK_LONG, &[]);
        }
        if points + self.game.saved + 1 >= self.mxscor && self.game.saved != 0 {
            self.rspeak(WITHOUT_SUSPENDS, &[]);
        }
        let t = self.game.turns;
        self.rspeak(
            TOTAL_SCORE,
            &[Arg::I(points), Arg::I(self.mxscor), Arg::I(t), Arg::I(t)],
        );
        for i in 1..NCLASSES {
            if CLASSES[i].threshold >= points {
                self.speak(CLASSES[i].message, &[]);
                if i < NCLASSES - 1 {
                    let nxt = CLASSES[i].threshold + 1 - points;
                    self.rspeak(NEXT_HIGHER, &[Arg::I(nxt), Arg::I(nxt)]);
                } else {
                    self.rspeak(NO_HIGHER, &[]);
                }
                exit(0);
            }
        }
        self.rspeak(OFF_SCALE, &[]);
        exit(0);
    }
}

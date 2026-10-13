// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The clocks: turn penalties, the lamp running down, and the closing of
//! the cave.
//!
//! The cave starts closing a while after the last treasure has been
//! seen (the clock only runs while the player is deep inside).  Then
//! the player cannot leave; a little later, or sooner if they try, the
//! cave closes and they are moved into the end-game repository.

use super::{Console, Engine};
use crate::dungeon::*;
use crate::objects::{Place, STATE_FOUND};
use crate::state::WARNTIME;

/// Turn limit increment from fresh batteries.
const BATTERYLIFE: i32 = 2500;

impl<C: Console> Engine<C> {
    /// Advance the clocks for a turn.  Returns true if the cave has just
    /// closed, which ends the turn.
    pub(super) fn close_check(&mut self) -> bool {
        for t in &TURN_THRESHOLDS {
            if self.game.turns == t.threshold + 1 {
                self.game.turn_penalty += t.point_loss;
                self.speak(t.message, &[]);
            }
        }

        let g = &mut self.game;
        if g.treasures_unseen == 0 && g.loc.has(Cond::Deep) && g.loc != LOC_Y2 {
            g.closing_clock -= 1;
        }

        if self.game.closing_clock == 0 {
            self.begin_closing();
            return self.game.closed;
        } else if self.game.closing_clock < 0 {
            self.game.closed_clock -= 1;
        }
        if self.game.closed_clock == 0 {
            self.close_cave();
            return true;
        }

        self.lamp_check();
        false
    }

    /// The first warning.  Lock the grate, destroy the bridge, kill the
    /// dwarves and the pirate, and remove the troll and bear (unless
    /// dead).  The dragon stays: too much trouble to move.  From now on
    /// the player can't leave the cave or be resurrected.
    fn begin_closing(&mut self) {
        let objects = &mut self.game.objects;
        objects.set_prop(GRATE, GRATE_CLOSED);
        objects.set_prop(FISSURE, UNBRIDGED);
        for d in &mut self.game.dwarves[1..] {
            d.seen = false;
            d.loc = LOC_NOWHERE;
        }
        let troll = TROLL.info();
        objects.destroy(TROLL);
        objects.move_fixed(TROLL, Fixed::Free);
        objects.move_to(TROLL2, Place::At(troll.plac));
        objects.move_fixed(TROLL2, troll.fixd);
        objects.juggle(CHASM);
        if objects.prop(BEAR) != BEAR_DEAD {
            objects.destroy(BEAR);
        }
        objects.set_prop(CHAIN, CHAIN_HEAP);
        objects.set_fixed(CHAIN, Fixed::Free);
        objects.set_prop(AXE, AXE_HERE);
        objects.set_fixed(AXE, Fixed::Free);
        self.say(CAVE_CLOSING);
        self.game.closing_clock = -1;
        self.game.closing = true;
    }

    /// The blinding flash.  Set up the repository: at the northeast end
    /// bottles, plants, oysters, lamps, rods with stars, sleeping
    /// dwarves and the player; at the southwest end the grate over the
    /// treasures, the snake pit, caged birds, more rods and pillows; a
    /// mirror along one wall.  Anything else the player carried is gone.
    fn close_cave(&mut self) {
        let objects = &mut self.game.objects;
        objects.put(BOTTLE, LOC_NE, EMPTY_BOTTLE);
        objects.put(PLANT, LOC_NE, PLANT_THIRSTY);
        objects.put(OYSTER, LOC_NE, STATE_FOUND);
        objects.put(LAMP, LOC_NE, LAMP_DARK);
        objects.put(ROD, LOC_NE, STATE_FOUND);
        objects.put(DWARF, LOC_NE, STATE_FOUND);
        // The grate keeps its normal state, and the sign is reused.
        objects.move_to(GRATE, Place::At(LOC_SW));
        objects.move_to(SIGN, Place::At(LOC_SW));
        objects.set_prop(SIGN, ENDGAME_SIGN);
        objects.put(SNAKE, LOC_SW, SNAKE_CHASED);
        objects.put(BIRD, LOC_SW, BIRD_CAGED);
        objects.put(CAGE, LOC_SW, STATE_FOUND);
        objects.put(ROD2, LOC_SW, STATE_FOUND);
        objects.put(PILLOW, LOC_SW, STATE_FOUND);
        objects.put(MIRROR, LOC_NE, STATE_FOUND);
        objects.set_fixed(MIRROR, Fixed::At(LOC_SW));
        for o in Obj::all() {
            if objects.is_carried(o) {
                objects.destroy(o);
            }
        }

        let g = &mut self.game;
        g.loc = LOC_NE;
        g.oldloc = LOC_NE;
        g.newloc = LOC_NE;
        self.say(CAVE_CLOSED);
        self.game.closed = true;
    }

    /// Run the lamp down.  Near the end, replace the batteries if fresh
    /// ones are here, or warn the player once.
    fn lamp_check(&mut self) {
        if self.prop(LAMP) == LAMP_BRIGHT {
            self.game.lamp_life -= 1;
        }

        if self.game.lamp_life <= WARNTIME {
            if self.here(BATTERY) && self.prop(BATTERY) == FRESH_BATTERIES && self.here(LAMP) {
                self.say(REPLACE_BATTERIES);
                self.game.objects.set_prop(BATTERY, DEAD_BATTERIES);
                self.game.lamp_life += BATTERYLIFE;
                self.game.lamp_warned = false;
            } else if !self.game.lamp_warned && self.here(LAMP) {
                self.game.lamp_warned = true;
                let msg = if self.prop(BATTERY) == DEAD_BATTERIES {
                    MISSING_BATTERIES
                } else if self.game.objects.place(BATTERY) == Place::NOWHERE {
                    LAMP_DIM
                } else {
                    GET_BATTERIES
                };
                self.say(msg);
            }
        }
        if self.game.lamp_life == 0 {
            self.game.lamp_life = -1;
            self.game.objects.set_prop(LAMP, LAMP_DARK);
            if self.here(LAMP) {
                self.say(LAMP_OUT);
            }
        }
    }
}

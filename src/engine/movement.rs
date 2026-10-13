// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Moving the player: travel rules, "back", "look", and the three
//! special movements (the plover passage, the plover transport, and the
//! troll bridge).

use super::{Aspect, Console, Engine, Flow};
use crate::dungeon::*;
use crate::objects::Place;

/// The index of the first rule after `i` that goes somewhere different
/// or under a different condition.  Used when a rule's condition fails.
fn next_different(rules: &[TravelOp], i: usize) -> usize {
    rules[i + 1..]
        .iter()
        .position(|r| !r.same_target(&rules[i]))
        .map(|p| i + 1 + p)
        .unwrap_or_else(|| panic!("conditional travel rule {i} has no alternative"))
}

impl<C: Console> Engine<C> {
    /// Try to move the player by `motion`, setting `newloc`.  The old
    /// location is remembered in `oldloc` (for "back"), and the one
    /// before in `oldloc2`, the last place the player was safe.
    pub(super) fn playermove(&mut self, mut motion: Motion) -> Flow {
        let here = self.game.loc;
        let rules = here.info().travel;
        self.game.newloc = here;
        assert!(!rules.is_empty(), "location {here:?} has no travel rules");

        if motion == NUL {
            return Ok(());
        } else if motion == BACK {
            match self.find_way_back() {
                Some(m) => motion = m,
                None => return Ok(()),
            }
        } else if motion == LOOK {
            // Can't give more detail.  Pretend it wasn't dark (though it
            // may now be dark) so the player won't fall into a pit while
            // staring into the gloom.
            if self.game.detail < 3 {
                self.say(NO_MORE_DETAIL);
            }
            self.game.detail += 1;
            self.game.was_dark = false;
            self.game.visits[here.index()] = 0;
            return Ok(());
        } else if motion == CAVE {
            let outside = Self::outside(here) && here != LOC_GRATE;
            self.say(if outside { FOLLOW_STREAM } else { NEED_DETAIL });
            return Ok(());
        } else {
            self.game.oldloc2 = self.game.oldloc;
            self.game.oldloc = here;
        }

        let Some(mut entry) = rules
            .iter()
            .position(|r| r.motion == HERE || r.motion == motion)
        else {
            let msg = match motion {
                EAST | WEST | SOUTH | NORTH | NE | NW | SW | SE | UP | DOWN => BAD_DIRECTION,
                FORWARD | LEFT | RIGHT => UNSURE_FACING,
                OUTSIDE | INSIDE => NO_INOUT_HERE,
                XYZZY | PLUGH => NOTHING_HAPPENS,
                CRAWL => WHICH_WAY,
                _ => CANT_APPLY,
            };
            self.say(msg);
            return Ok(());
        };

        loop {
            // Find the first rule from here on whose condition holds.
            loop {
                let rule = rules[entry];
                let ok = match rule.cond {
                    TravelCond::Always => true,
                    TravelCond::Pct(n) => self.pct(n),
                    TravelCond::Carry(obj) => self.toting(obj),
                    TravelCond::With(obj) => self.toting(obj) || self.at(obj),
                    TravelCond::Not(obj, state) => self.prop(obj) != state,
                };
                if ok {
                    break;
                }
                entry = next_different(rules, entry);
            }

            match rules[entry].dest {
                Dest::Goto(loc) => {
                    self.game.newloc = loc;
                    return Ok(());
                }
                Dest::Speak(msg) => {
                    self.say(msg);
                    return Ok(());
                }
                Dest::Special(1) => {
                    self.plover_passage();
                    return Ok(());
                }
                Dest::Special(2) => {
                    // The plover transport: drop the emerald (this rule
                    // only applies when carrying it), so it has to come
                    // out through the passage, then carry on as if it
                    // hadn't been carried.
                    self.game.objects.drop(EMERALD, Place::At(here));
                    entry = next_different(rules, entry);
                }
                Dest::Special(3) => return self.cross_troll_bridge(),
                Dest::Special(n) => panic!("unknown special travel {n}"),
            }
        }
    }

    /// Work out which motion leads back to where the player was.
    /// Returns `None` (having said why) if there isn't one.
    fn find_way_back(&mut self) -> Option<Motion> {
        let g = &mut self.game;
        let here = g.loc;
        let mut target = g.oldloc;
        if Self::forced(target) {
            target = g.oldloc2;
        }
        g.oldloc2 = g.oldloc;
        g.oldloc = here;
        if here.has(Cond::NoBack) {
            self.say(TWIST_TURN);
            return None;
        }
        if target == here {
            self.say(FORGOT_PATH);
            return None;
        }
        // A rule straight there, or else (the last) one through a forced
        // location that leads there.
        let rules = here.info().travel;
        let leads_back_via = |dest: Dest| match dest {
            Dest::Goto(via) => Self::forced(via) && via.info().travel[0].dest.raw() == target.raw(),
            _ => false,
        };
        let rule = rules
            .iter()
            .find(|r| r.dest == Dest::Goto(target))
            .or_else(|| rules.iter().rev().find(|r| leads_back_via(r.dest)));
        match rule {
            Some(rule) => Some(rule.motion),
            None => {
                self.say(NOT_CONNECTED);
                None
            }
        }
    }

    /// The passage between the plover room and the alcove admits only
    /// the player and the emerald.
    fn plover_passage(&mut self) {
        let g = &mut self.game;
        g.newloc = if g.loc == LOC_PLOVER {
            LOC_ALCOVE
        } else {
            LOC_PLOVER
        };
        let holding = g.objects.holding();
        if holding > 1 || (holding == 1 && !self.toting(EMERALD)) {
            self.game.newloc = self.game.loc;
            self.say(MUST_DROP);
        }
    }

    /// The troll bridge.  The troll blocks the way back after each
    /// crossing until paid again; the bridge can't bear the bear.
    ///
    /// This is a special movement so that dwarves never wander across
    /// and meet the bear.
    fn cross_troll_bridge(&mut self) -> Flow {
        let troll = TROLL.info();
        if self.prop(TROLL) == TROLL_PAIDONCE {
            // Crossed since paying: the troll steps out and blocks.
            self.describe(TROLL, Aspect::Look, TROLL_PAIDONCE);
            let objects = &mut self.game.objects;
            objects.set_prop(TROLL, TROLL_UNPAID);
            objects.destroy(TROLL2);
            objects.move_fixed(TROLL2, Fixed::Free);
            objects.move_to(TROLL, Place::At(troll.plac));
            objects.move_fixed(TROLL, troll.fixd);
            objects.juggle(CHASM);
            self.game.newloc = self.game.loc;
            return Ok(());
        }

        let Fixed::At(far_end) = troll.fixd else {
            unreachable!("the troll guards both ends of the bridge")
        };
        self.game.newloc = Loc::from_raw(troll.plac.raw() + far_end.raw() - self.game.loc.raw());
        if self.prop(TROLL) == TROLL_UNPAID {
            self.game.objects.set_prop(TROLL, TROLL_PAIDONCE);
        }
        if !self.toting(BEAR) {
            return Ok(());
        }
        self.state_change(CHASM, BRIDGE_WRECKED);
        let newloc = self.game.newloc;
        let objects = &mut self.game.objects;
        objects.set_prop(TROLL, TROLL_GONE);
        objects.drop(BEAR, Place::At(newloc));
        objects.set_fixed(BEAR, Fixed::Immovable);
        objects.set_prop(BEAR, BEAR_DEAD);
        self.game.oldloc2 = newloc;
        self.croak()
    }
}

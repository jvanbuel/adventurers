// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Taking, dropping, finding and listing what the player carries.

use super::super::turn::Phase;
use super::super::{Aspect, Console, Engine};
use crate::dungeon::*;
use crate::objects::{Place, STATE_IN_CAVITY};

/// Most objects the player can carry at once.
const INVLIMIT: i32 = 7;

impl<C: Console> Engine<C> {
    /// Take.  With no object, takes the only thing here.  The bird and
    /// cage go together when the bird is caged; liquids go in the
    /// bottle.
    pub(super) fn carry(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let loc = self.game.loc;
        let obj = match obj {
            Some(obj) => obj,
            None => {
                let mut present = self.game.objects.at(loc);
                match (present.next(), present.next()) {
                    (Some(only), None) if !self.dwarf_is_here() => only,
                    _ => return Phase::Unknown,
                }
            }
        };

        if self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }

        if obj == MESSAG {
            self.say(REMOVE_MESSAGE);
            self.game.objects.destroy(MESSAG);
            return Phase::ClearObj;
        }

        if self.game.objects.fixed(obj) != Fixed::Free {
            let msg = match obj {
                PLANT
                    if self.prop(PLANT) == PLANT_THIRSTY || self.game.objects.is_stashed(PLANT) =>
                {
                    DEEP_ROOTS
                }
                BEAR if self.prop(BEAR) == SITTING_BEAR => BEAR_CHAINED,
                CHAIN if self.prop(BEAR) != UNTAMED_BEAR => STILL_LOCKED,
                RUG if self.prop(RUG) == RUG_HOVER => RUG_HOVERS,
                URN => URN_NOBUDGE,
                CAVITY => DOUGHNUT_HOLES,
                BLOOD => FEW_DROPS,
                SIGN => HAND_PASSTHROUGH,
                _ => YOU_JOKING,
            };
            self.say(msg);
            return Phase::ClearObj;
        }

        let mut obj = obj;
        if obj == WATER || obj == OIL {
            if !self.here(BOTTLE) || self.liquid() != Some(obj) {
                if !self.toting(BOTTLE) {
                    self.say(NO_CONTAINER);
                } else if self.prop(BOTTLE) == EMPTY_BOTTLE {
                    return self.fill(verb, Some(BOTTLE));
                } else {
                    self.say(BOTTLE_FULL);
                }
                return Phase::ClearObj;
            }
            obj = BOTTLE;
        }

        if self.game.objects.holding() >= INVLIMIT {
            self.say(CARRY_LIMIT);
            return Phase::ClearObj;
        }

        if obj == BIRD && self.prop(BIRD) != BIRD_CAGED && !self.game.objects.is_stashed(BIRD) {
            if self.prop(BIRD) == BIRD_FOREST_UNCAGED {
                self.game.objects.destroy(BIRD);
                self.say(BIRD_CRAP);
                return Phase::ClearObj;
            }
            if !self.toting(CAGE) {
                self.say(CANNOT_CARRY);
                return Phase::ClearObj;
            }
            if self.toting(ROD) {
                self.say(BIRD_EVADES);
                return Phase::ClearObj;
            }
            self.game.objects.set_prop(BIRD, BIRD_CAGED);
        }
        let objects = &mut self.game.objects;
        if (obj == BIRD || obj == CAGE) && objects.in_state(BIRD, BIRD_CAGED) {
            // Take the other one too.  (Unlike the original, only if it
            // is here: a caged bird that was killed is gone, and trying
            // to take it from here hung the C version.)
            let other = if obj == BIRD { CAGE } else { BIRD };
            if objects.is_at(other, loc) {
                objects.carry(other, loc);
            }
        }
        objects.carry(obj, loc);

        if obj == BOTTLE {
            if let Some(liquid) = self.liquid() {
                self.game.objects.set_place(liquid, Place::Carried);
            }
        }
        if obj.is_gemstone() && !self.game.objects.is_found(obj) {
            self.game.objects.set_found(obj);
            self.game.objects.set_prop(CAVITY, CAVITY_EMPTY);
        }
        self.say(OK_MAN);
        Phase::ClearObj
    }

    /// Drop.  "Throw" comes here for most objects.  Gems go in the
    /// cavity; coins buy batteries; the bird may attack the snake or be
    /// burnt by the dragon; the vase breaks unless there's a pillow.
    pub(super) fn discard(&mut self, verb: Verb, obj: Obj) -> Phase {
        let loc = self.game.loc;
        let here = Place::At(loc);
        let obj = if obj == ROD && !self.toting(ROD) && self.toting(ROD2) {
            ROD2
        } else {
            obj
        };

        if !self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }

        if obj.is_gemstone() && self.at(CAVITY) && self.prop(CAVITY) != CAVITY_FULL {
            self.place_gem(obj);
            return Phase::ClearObj;
        }

        if obj == COINS && self.here(VEND) {
            self.game.objects.destroy(COINS);
            self.game.objects.drop(BATTERY, here);
            self.describe(BATTERY, Aspect::Look, FRESH_BATTERIES);
            return Phase::ClearObj;
        }

        // (With no liquid, this also matches NO_OBJECT, as in the original.)
        let obj = if self.liquid_or_nothing() == obj {
            BOTTLE
        } else {
            obj
        };
        if obj == BOTTLE {
            if let Some(liquid) = self.liquid() {
                self.game.objects.set_place(liquid, Place::NOWHERE);
            }
        }

        if obj == BEAR && self.at(TROLL) {
            // The troll flees from the bear.
            self.state_change(TROLL, TROLL_GONE);
            let troll = TROLL.info();
            let objects = &mut self.game.objects;
            objects.move_to(TROLL, Place::NOWHERE);
            objects.move_fixed(TROLL, Fixed::Free);
            objects.move_to(TROLL2, Place::At(troll.plac));
            objects.move_fixed(TROLL2, troll.fixd);
            objects.juggle(CHASM);
            objects.drop(obj, here);
            return Phase::ClearObj;
        }

        if obj == VASE && loc != PILLOW.info().plac {
            let state = if self.at(PILLOW) {
                VASE_WHOLE
            } else {
                VASE_DROPPED
            };
            self.state_change(VASE, state);
            if self.prop(VASE) != VASE_WHOLE {
                self.game.objects.set_fixed(VASE, Fixed::Immovable);
            }
            self.game.objects.drop(obj, here);
            return Phase::ClearObj;
        }

        if obj == CAGE && self.prop(BIRD) == BIRD_CAGED {
            self.game.objects.drop(BIRD, here);
        }

        if obj == BIRD {
            if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
                self.say(BIRD_BURNT);
                self.game.objects.destroy(BIRD);
                return Phase::ClearObj;
            }
            if self.here(SNAKE) {
                self.say(BIRD_ATTACKS);
                if self.game.closed {
                    return Phase::DwarfWake;
                }
                self.game.objects.destroy(SNAKE);
                // The travel rules check the snake's state.
                self.game.objects.set_prop(SNAKE, SNAKE_CHASED);
            } else {
                self.say(OK_MAN);
            }
            let state = if loc.has(Cond::Forest) {
                BIRD_FOREST_UNCAGED
            } else {
                BIRD_UNCAGED
            };
            self.game.objects.set_prop(BIRD, state);
            self.game.objects.drop(obj, here);
            return Phase::ClearObj;
        }

        self.say(OK_MAN);
        self.game.objects.drop(obj, here);
        Phase::ClearObj
    }

    /// Put a gem in the cavity.  The emerald makes the rug rise; the
    /// ruby makes it settle.
    pub(super) fn place_gem(&mut self, gem: Obj) {
        self.say(GEM_FITS);
        self.game.objects.set_prop(gem, STATE_IN_CAVITY);
        self.game.objects.set_prop(CAVITY, CAVITY_FULL);
        let rug_hovers = self.prop(RUG) == RUG_HOVER;
        if self.here(RUG) && ((gem == EMERALD && !rug_hovers) || (gem == RUBY && rug_hovers)) {
            let msg = if gem == RUBY {
                RUG_SETTLES
            } else if self.toting(RUG) {
                RUG_WIGGLES
            } else {
                RUG_RISES
            };
            self.say(msg);
            if !self.toting(RUG) || gem == RUBY {
                let (state, far_end) = if rug_hovers {
                    (RUG_FLOOR, Fixed::Free)
                } else {
                    (RUG_HOVER, Fixed::At(SAPPH.info().plac))
                };
                self.game.objects.set_prop(RUG, state);
                self.game.objects.move_fixed(RUG, far_end);
            }
        }
        self.game.objects.drop(gem, Place::At(self.game.loc));
    }

    /// Find.  It may be carried or here; otherwise, no help.
    pub(super) fn find(&mut self, verb: Verb, obj: Obj) -> Phase {
        let msg = if self.toting(obj) {
            ALREADY_CARRYING
        } else if self.game.closed {
            NEEDED_NEARBY
        } else if self.at(obj)
            || (self.liquid_or_nothing() == obj && self.at(BOTTLE))
            || Self::liquid_at(self.game.loc).unwrap_or(NO_OBJECT) == obj
            || (obj == DWARF && self.dwarf_is_here())
        {
            YOU_HAVEIT
        } else {
            self.verb_message(verb);
            return Phase::ClearObj;
        };
        self.say(msg);
        Phase::ClearObj
    }

    /// Inventory.
    pub(super) fn inventory(&mut self) -> Phase {
        let carried: Vec<Obj> = Obj::all()
            .filter(|&o| o != BEAR && self.toting(o))
            .collect();
        if !carried.is_empty() {
            self.say(NOW_HOLDING);
        }
        for o in carried.iter().copied() {
            self.describe_with(o, Aspect::Inventory, 0, false, &[]);
        }
        if self.toting(BEAR) {
            self.say(TAME_BEAR);
        }
        if carried.is_empty() {
            self.say(NO_CARRY);
        }
        Phase::ClearObj
    }
}

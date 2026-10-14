// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Eating, drinking, and moving liquids between bottle, urn and plant.

use super::super::turn::Phase;
use super::super::{Console, Engine};
use crate::dungeon::*;
use crate::objects::Place;

impl<C: Console> Engine<C> {
    /// Eat.  With no object, eat the food if it's here.
    pub(super) fn eat(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        match obj {
            None if !self.here(FOOD) => return Phase::Unknown,
            None | Some(FOOD) => {
                self.game.objects.destroy(FOOD);
                self.say(THANKS_DELICIOUS);
            }
            Some(BIRD | SNAKE | CLAM | OYSTER | DWARF | DRAGON | TROLL | BEAR | OGRE) => {
                self.say(LOST_APPETITE)
            }
            Some(_) => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Drink.  With no object, drink water from the bottle or stream.
    pub(super) fn drink(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let water_here = Self::liquid_at(self.game.loc) == Some(WATER)
            || (self.liquid() == Some(WATER) && self.here(BOTTLE));
        if obj.is_none() && !water_here {
            return Phase::Unknown;
        }
        match obj {
            Some(BLOOD) => {
                self.game.objects.destroy(BLOOD);
                self.state_change(DRAGON, DRAGON_BLOODLESS);
                self.game.blooded = true;
            }
            Some(o) if o != WATER => self.say(RIDICULOUS_ATTEMPT),
            _ if self.liquid() == Some(WATER) && self.here(BOTTLE) => {
                self.game.objects.set_place(WATER, Place::NOWHERE);
                self.state_change(BOTTLE, EMPTY_BOTTLE);
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Fill the bottle or the urn from a liquid here.  (The vase is
    /// nasty.)
    pub(super) fn fill(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let loc = self.game.loc;
        match obj {
            Some(VASE) => {
                if Self::liquid_at(loc).is_none() {
                    self.say(FILL_INVALID);
                } else if !self.toting(VASE) {
                    self.say(ARENT_CARRYING);
                } else {
                    self.say(SHATTER_VASE);
                    let objects = &mut self.game.objects;
                    objects.set_prop(VASE, VASE_BROKEN);
                    objects.set_fixed(VASE, Fixed::Immovable);
                    objects.drop(VASE, Place::At(loc));
                }
                return Phase::ClearObj;
            }
            Some(URN) => {
                self.fill_urn();
                return Phase::ClearObj;
            }
            Some(o) if o != BOTTLE => {
                self.verb_message(verb);
                return Phase::ClearObj;
            }
            None if !self.here(BOTTLE) => return Phase::Unknown,
            _ => {}
        }

        if self.here(URN) && self.prop(URN) != URN_EMPTY {
            self.say(URN_NOPOUR);
        } else if self.liquid().is_some() {
            self.say(BOTTLE_FULL);
        } else if let Some(liquid) = Self::liquid_at(loc) {
            self.state_change(
                BOTTLE,
                if liquid == OIL {
                    OIL_BOTTLE
                } else {
                    WATER_BOTTLE
                },
            );
            if self.toting(BOTTLE) {
                self.game.objects.set_place(liquid, Place::Carried);
            }
        } else {
            self.say(NO_LIQUID);
        }
        Phase::ClearObj
    }

    /// Pour the bottle into the urn.  Oil can be lit; water just sits.
    pub(super) fn fill_urn(&mut self) {
        if self.prop(URN) != URN_EMPTY {
            self.say(FULL_URN);
            return;
        }
        if !self.here(BOTTLE) {
            self.say(FILL_INVALID);
            return;
        }
        let Some(liquid) = self.liquid() else {
            self.say(FILL_INVALID);
            return;
        };
        if liquid == OIL {
            self.game.objects.set_prop(URN, URN_DARK);
        }
        self.game.objects.set_prop(BOTTLE, EMPTY_BOTTLE);
        self.say(if liquid == OIL { OIL_URN } else { WATER_URN });
        self.game.objects.set_place(liquid, Place::NOWHERE);
    }

    /// Pour.  With no object, or the bottle, pour what's in the bottle.
    /// Water on the plant makes it grow; oil frees the rusty door.
    pub(super) fn pour(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let obj = match obj {
            None | Some(BOTTLE) => self.liquid(),
            Some(obj) => Some(obj).filter(|&o| o != NO_OBJECT),
        };
        let Some(obj) = obj else {
            return Phase::Unknown;
        };
        if !self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }
        if obj != OIL && obj != WATER {
            self.say(CANT_POUR);
            return Phase::ClearObj;
        }
        if self.here(URN) && self.prop(URN) == URN_EMPTY {
            return self.fill(verb, Some(URN));
        }
        self.game.objects.set_prop(BOTTLE, EMPTY_BOTTLE);
        self.game.objects.set_place(obj, Place::NOWHERE);
        if self.at(DOOR) {
            self.state_change(
                DOOR,
                if obj == OIL {
                    DOOR_UNRUSTED
                } else {
                    DOOR_RUSTED
                },
            );
            Phase::ClearObj
        } else if !self.at(PLANT) {
            self.say(GROUND_WET);
            Phase::ClearObj
        } else if obj == WATER {
            // Watering cycles the plant through its three states.
            self.state_change(PLANT, (self.prop(PLANT) + 1) % 3);
            self.game.objects.set_prop(PLANT2, self.prop(PLANT));
            Phase::Move
        } else {
            self.say(SHAKING_LEAVES);
            Phase::ClearObj
        }
    }
}

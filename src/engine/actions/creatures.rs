// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Verbs aimed at creatures: attacking, throwing, feeding and waking.

use super::super::turn::Phase;
use super::super::{Console, DwarfAt, Engine, Flow};
use crate::dungeon::*;
use crate::objects::Place;

impl<C: Console> Engine<C> {
    /// Attack.  With no object, pick the target if there's only one
    /// candidate: enemies (dwarf, snake, dragon, troll, ogre, bear)
    /// first, then others (bird, vending machine, clam).  "Throw" with
    /// the axe comes here too, but can't target the bird or machine.
    pub(super) fn attack(&mut self, verb: Verb, obj: Option<Obj>) -> Flow<Phase> {
        let obj = match obj {
            Some(obj) => obj,
            None => {
                let mut targets = Vec::new();
                if self.dwarf_is_here() {
                    targets.push(DWARF);
                }
                if self.here(SNAKE) {
                    targets.push(SNAKE);
                }
                if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
                    targets.push(DRAGON);
                }
                if self.at(TROLL) {
                    targets.push(TROLL);
                }
                if self.at(OGRE) {
                    targets.push(OGRE);
                }
                if self.here(BEAR) && self.prop(BEAR) == UNTAMED_BEAR {
                    targets.push(BEAR);
                }
                if targets.is_empty() {
                    if self.here(BIRD) && verb != THROW {
                        targets.push(BIRD);
                    }
                    if self.here(VEND) && verb != THROW {
                        targets.push(VEND);
                    }
                    // Clam and oyster are both treated as the clam.
                    if self.here(CLAM) || self.here(OYSTER) {
                        targets.push(CLAM);
                    }
                }
                match targets[..] {
                    [] => {
                        self.say(NO_TARGET);
                        return Ok(Phase::ClearObj);
                    }
                    [only] => only,
                    _ => return Ok(Phase::Unknown),
                }
            }
        };

        match obj {
            BIRD => {
                if self.game.closed {
                    self.say(UNHAPPY_BIRD);
                } else {
                    self.game.objects.destroy(BIRD);
                    self.say(BIRD_DEAD);
                }
            }
            VEND => {
                let state = if self.prop(VEND) == VEND_BLOCKS {
                    VEND_UNBLOCKS
                } else {
                    VEND_BLOCKS
                };
                self.state_change(VEND, state);
            }
            BEAR => match self.prop(BEAR) {
                UNTAMED_BEAR => self.say(BEAR_HANDS),
                SITTING_BEAR | CONTENTED_BEAR => self.say(BEAR_CONFUSED),
                BEAR_DEAD => self.say(ALREADY_DEAD),
                _ => {}
            },
            DRAGON if self.prop(DRAGON) == DRAGON_BARS => return self.kill_dragon(),
            DRAGON => self.say(ALREADY_DEAD),
            OGRE => self.attack_ogre(),
            CLAM | OYSTER => self.say(SHELL_IMPERVIOUS),
            SNAKE => self.say(SNAKE_WARNING),
            DWARF => {
                if self.game.closed {
                    return Ok(Phase::DwarfWake);
                }
                self.say(BARE_HANDS_QUERY);
            }
            TROLL => self.say(ROCKY_TROLL),
            _ => self.verb_message(verb),
        }
        Ok(Phase::ClearObj)
    }

    /// Attacking the dragon with bare hands: if the player insists, it
    /// works.  The dragon dies on the rug, which can then be taken, and
    /// the player is moved to the middle of the dragon's cave.
    pub(super) fn kill_dragon(&mut self) -> Flow<Phase> {
        self.say(BARE_HANDS_QUERY);
        if !self.silent_yes_or_no()? {
            self.say(NASTY_DRAGON);
            return Ok(Phase::Move);
        }
        self.state_change(DRAGON, DRAGON_DEAD);
        let objects = &mut self.game.objects;
        objects.set_prop(RUG, RUG_FLOOR);
        objects.move_fixed(DRAGON, Fixed::Immovable);
        objects.move_fixed(RUG, Fixed::Free);
        objects.move_to(DRAGON, Place::At(LOC_SECRET5));
        objects.move_to(RUG, Place::At(LOC_SECRET5));
        objects.drop(BLOOD, Place::At(LOC_SECRET5));
        let dragon = DRAGON.info();
        for o in Obj::all() {
            let place = objects.place(o).raw();
            if place == dragon.plac.raw() || place == dragon.fixd.raw() {
                objects.move_to(o, Place::At(LOC_SECRET5));
            }
        }
        self.game.loc = LOC_SECRET5;
        Ok(Phase::Move)
    }

    /// The ogre dodges; if dwarves are here, one throws a knife at it and
    /// they all flee.
    pub(super) fn attack_ogre(&mut self) {
        self.say(OGRE_DODGE);
        if self.dwarf_at(self.game.loc) == DwarfAt::None {
            return;
        }
        self.say(KNIFE_THROWN);
        self.game.objects.destroy(OGRE);
        let loc = self.game.loc;
        let mut fled = 0;
        for d in &mut self.game.dwarves[1..super::super::dwarves::PIRATE] {
            if d.loc == loc {
                fled += 1;
                d.loc = LOC_LONGWEST;
                d.seen = false;
            }
        }
        self.say(if fled > 1 { OGRE_PANIC1 } else { OGRE_PANIC2 });
    }

    /// Throw.  The axe attacks; a treasure pays the troll; food goes to
    /// the bear; anything else is dropped.
    pub(super) fn throw(&mut self, verb: Verb, obj: Obj) -> Flow<Phase> {
        if !self.toting(obj) {
            self.verb_message(verb);
            return Ok(Phase::ClearObj);
        }
        if obj.is_treasure() && self.at(TROLL) {
            // The troll takes the treasure as toll.
            let troll = TROLL.info();
            let objects = &mut self.game.objects;
            objects.drop(obj, Place::NOWHERE);
            objects.move_to(TROLL, Place::NOWHERE);
            objects.move_fixed(TROLL, Fixed::Free);
            objects.drop(TROLL2, Place::At(troll.plac));
            objects.drop_fixed(TROLL2, troll.fixd);
            objects.juggle(CHASM);
            self.say(TROLL_SATISFIED);
            return Ok(Phase::ClearObj);
        }
        if obj == FOOD && self.here(BEAR) {
            return Ok(self.feed(verb, BEAR));
        }
        if obj != AXE {
            return Ok(self.discard(verb, obj));
        }
        if let DwarfAt::Dwarf(i) = self.dwarf_at(self.game.loc) {
            if self.randrange(crate::dungeon::NDWARVES as i32 + 1) < self.game.dwarf_activity {
                return Ok(self.axe_lands(DWARF_DODGES));
            }
            let d = &mut self.game.dwarves[i];
            d.seen = false;
            d.loc = LOC_NOWHERE;
            self.game.dwarves_killed += 1;
            let msg = if self.game.dwarves_killed == 1 {
                DWARF_SMOKE
            } else {
                KILLED_DWARF
            };
            return Ok(self.axe_lands(msg));
        }
        if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
            return Ok(self.axe_lands(DRAGON_SCALES));
        }
        if self.at(TROLL) {
            return Ok(self.axe_lands(TROLL_RETURNS));
        }
        if self.at(OGRE) {
            return Ok(self.axe_lands(OGRE_DODGE));
        }
        if self.here(BEAR) && self.prop(BEAR) == UNTAMED_BEAR {
            // This'll teach the player to throw the axe at the bear!
            let loc = Place::At(self.game.loc);
            self.game.objects.drop(AXE, loc);
            self.game.objects.set_fixed(AXE, Fixed::Immovable);
            self.game.objects.juggle(BEAR);
            self.state_change(AXE, AXE_LOST);
            return Ok(Phase::ClearObj);
        }
        self.attack(verb, None)
    }

    /// The thrown axe falls to the ground here.
    pub(super) fn axe_lands(&mut self, msg: Msg) -> Phase {
        self.say(msg);
        self.game.objects.drop(AXE, Place::At(self.game.loc));
        Phase::Move
    }

    /// Feed.  The bear is the only creature that can be fed usefully.
    pub(super) fn feed(&mut self, verb: Verb, obj: Obj) -> Phase {
        match obj {
            BIRD => self.say(BIRD_PINING),
            DRAGON => self.say(if self.prop(DRAGON) == DRAGON_BARS {
                NOTHING_EDIBLE
            } else {
                RIDICULOUS_ATTEMPT
            }),
            SNAKE => {
                if !self.game.closed && self.here(BIRD) {
                    self.game.objects.destroy(BIRD);
                    self.say(BIRD_DEVOURED);
                } else {
                    self.say(NOTHING_EDIBLE);
                }
            }
            TROLL => self.say(TROLL_VICES),
            DWARF if self.here(FOOD) => {
                self.game.dwarf_activity += 2;
                self.say(REALLY_MAD);
            }
            BEAR if self.prop(BEAR) == BEAR_DEAD => self.say(RIDICULOUS_ATTEMPT),
            BEAR if self.prop(BEAR) == UNTAMED_BEAR => {
                if self.here(FOOD) {
                    let objects = &mut self.game.objects;
                    objects.destroy(FOOD);
                    objects.set_fixed(AXE, Fixed::Free);
                    objects.set_prop(AXE, AXE_HERE);
                    self.state_change(BEAR, SITTING_BEAR);
                } else {
                    self.say(NOTHING_EDIBLE);
                }
            }
            OGRE if self.here(FOOD) => self.say(OGRE_FULL),
            DWARF | BEAR | OGRE => self.verb_message(verb),
            _ => self.say(AM_GAME),
        }
        Phase::ClearObj
    }

    /// Wake.  Only the sleeping dwarves, with dire consequences.
    pub(super) fn wake(&mut self, verb: Verb, obj: Obj) -> Phase {
        if obj != DWARF || !self.game.closed {
            self.verb_message(verb);
            return Phase::ClearObj;
        }
        self.say(PROD_DWARF);
        Phase::DwarfWake
    }
}

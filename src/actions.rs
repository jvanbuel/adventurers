// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Actions for the dungeon-running code.

use crate::advent::*;
use crate::dungeon::*;
use crate::misc::{atol, bug, strtol};

const NOBJ: i32 = NOBJECTS as i32;

impl Advent {
    /// The default message for a verb.
    fn verb_message(&self, verb: i32) {
        self.speak(ACTIONS[verb as usize].message, &[]);
    }

    fn arbitrary(msg: i32) -> Msg {
        ARBITRARY_MESSAGES[msg as usize]
    }

    ///  Attack.  Assume target if unambiguous.  "Throw" also links here.
    ///  Attackable objects fall into two categories: enemies (snake,
    ///  dwarf, etc.)  and others (bird, clam, machine).  Ambiguous if 2
    ///  enemies, or no enemies but 2 others.
    fn attack(&mut self, command: &Command) -> Phase {
        let verb = command.verb;
        let mut obj = command.obj;

        if obj == INTRANSITIVE {
            let mut changes = 0;
            if self.atdwrf(self.game.loc) > 0 {
                obj = DWARF;
                changes += 1;
            }
            if self.here(SNAKE) {
                obj = SNAKE;
                changes += 1;
            }
            if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
                obj = DRAGON;
                changes += 1;
            }
            if self.at(TROLL) {
                obj = TROLL;
                changes += 1;
            }
            if self.at(OGRE) {
                obj = OGRE;
                changes += 1;
            }
            if self.here(BEAR) && self.prop(BEAR) == UNTAMED_BEAR {
                obj = BEAR;
                changes += 1;
            }
            // check for low-priority targets
            if obj == INTRANSITIVE {
                // Can't attack bird or machine by throwing axe.
                if self.here(BIRD) && verb != THROW {
                    obj = BIRD;
                    changes += 1;
                }
                if self.here(VEND) && verb != THROW {
                    obj = VEND;
                    changes += 1;
                }
                // Clam and oyster both treated as clam for intransitive
                // case; no harm done.
                if self.here(CLAM) || self.here(OYSTER) {
                    obj = CLAM;
                    changes += 1;
                }
            }
            if changes >= 2 {
                return Phase::Unknown;
            }
        }

        if obj == BIRD {
            if self.game.closed {
                self.rspeak(UNHAPPY_BIRD, &[]);
            } else {
                self.destroy(BIRD);
                self.rspeak(BIRD_DEAD, &[]);
            }
            return Phase::ClearObj;
        }
        if obj == VEND {
            let st = if self.prop(VEND) == VEND_BLOCKS {
                VEND_UNBLOCKS
            } else {
                VEND_BLOCKS
            };
            self.state_change(VEND, st);
            return Phase::ClearObj;
        }

        if obj == BEAR {
            match self.prop(BEAR) {
                UNTAMED_BEAR => self.rspeak(BEAR_HANDS, &[]),
                SITTING_BEAR => self.rspeak(BEAR_CONFUSED, &[]),
                CONTENTED_BEAR => self.rspeak(BEAR_CONFUSED, &[]),
                BEAR_DEAD => self.rspeak(ALREADY_DEAD, &[]),
                _ => {}
            }
            return Phase::ClearObj;
        }
        if obj == DRAGON && self.prop(DRAGON) == DRAGON_BARS {
            //  Fun stuff for dragon.  If he insists on attacking it, win!
            //  Set game.prop to dead, move dragon to central loc (still
            //  fixed), move rug there (not fixed), and move him there,
            //  too.  Then do a null motion to get new description.
            self.rspeak(BARE_HANDS_QUERY, &[]);
            if !self.silent_yes_or_no() {
                self.speak(Self::arbitrary(NASTY_DRAGON), &[]);
                return Phase::Move;
            }
            self.state_change(DRAGON, DRAGON_DEAD);
            self.set_prop(RUG, RUG_FLOOR);
            // Hardcoding LOC_SECRET5 as the dragon's death location is
            // ugly. The way it was computed before was worse; it
            // depended on the two dragon locations being LOC_SECRET4 and
            // LOC_SECRET6 and LOC_SECRET5 being right between them.
            self.move_obj(DRAGON + NOBJ, IS_FIXED);
            self.move_obj(RUG + NOBJ, IS_FREE);
            self.move_obj(DRAGON, LOC_SECRET5);
            self.move_obj(RUG, LOC_SECRET5);
            self.drop(BLOOD, LOC_SECRET5);
            for i in 1..NOBJ {
                if self.place(i) == OBJECTS[DRAGON as usize].plac
                    || self.place(i) == OBJECTS[DRAGON as usize].fixd
                {
                    self.move_obj(i, LOC_SECRET5);
                }
            }
            self.game.loc = LOC_SECRET5;
            return Phase::Move;
        }

        if obj == OGRE {
            self.rspeak(OGRE_DODGE, &[]);
            if self.atdwrf(self.game.loc) == 0 {
                return Phase::ClearObj;
            }
            self.rspeak(KNIFE_THROWN, &[]);
            self.destroy(OGRE);
            let mut dwarves = 0;
            for i in 1..PIRATE {
                if self.game.dwarves[i].loc == self.game.loc {
                    dwarves += 1;
                    self.game.dwarves[i].loc = LOC_LONGWEST;
                    self.game.dwarves[i].seen = false;
                }
            }
            self.rspeak(
                if dwarves > 1 {
                    OGRE_PANIC1
                } else {
                    OGRE_PANIC2
                },
                &[],
            );
            return Phase::ClearObj;
        }

        match obj {
            INTRANSITIVE => self.rspeak(NO_TARGET, &[]),
            CLAM | OYSTER => self.rspeak(SHELL_IMPERVIOUS, &[]),
            SNAKE => self.rspeak(SNAKE_WARNING, &[]),
            DWARF => {
                if self.game.closed {
                    return Phase::DwarfWake;
                }
                self.rspeak(BARE_HANDS_QUERY, &[]);
            }
            DRAGON => self.rspeak(ALREADY_DEAD, &[]),
            TROLL => self.rspeak(ROCKY_TROLL, &[]),
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Only called on FEE FIE FOE FOO (AND FUM).  Advance to next state
    /// if given in proper order. Look up foo in special section of vocab
    /// to determine which word we've got. Last word zips the eggs back
    /// to the giant room (unless already there).
    fn bigwords(&mut self, id: i32) -> Phase {
        let foobar = self.game.foobar.abs();

        // Only FEE can start a magic-word sequence.
        if foobar == WORD_EMPTY && (id == FIE || id == FOE || id == FOO || id == FUM) {
            self.rspeak(NOTHING_HAPPENS, &[]);
            return Phase::ClearObj;
        }

        if (foobar == WORD_EMPTY && id == FEE)
            || (foobar == FEE && id == FIE)
            || (foobar == FIE && id == FOE)
            || (foobar == FOE && id == FOO)
        {
            self.game.foobar = id;
            if id != FOO {
                self.rspeak(OK_MAN, &[]);
                return Phase::ClearObj;
            }
            self.game.foobar = WORD_EMPTY;
            let eggs_plac = OBJECTS[EGGS as usize].plac;
            if self.place(EGGS) == eggs_plac || (self.toting(EGGS) && self.game.loc == eggs_plac) {
                self.rspeak(NOTHING_HAPPENS, &[]);
                Phase::ClearObj
            } else {
                // Bring back troll if we steal the eggs back from him
                // before crossing.
                if self.place(EGGS) == LOC_NOWHERE
                    && self.place(TROLL) == LOC_NOWHERE
                    && self.prop(TROLL) == TROLL_UNPAID
                {
                    self.set_prop(TROLL, TROLL_PAIDONCE);
                }
                if self.here(EGGS) {
                    self.pspeak(EGGS, SpeakType::Look, true, EGGS_VANISHED, &[]);
                } else if self.game.loc == eggs_plac {
                    self.pspeak(EGGS, SpeakType::Look, true, EGGS_HERE, &[]);
                } else {
                    self.pspeak(EGGS, SpeakType::Look, true, EGGS_DONE, &[]);
                }
                self.move_obj(EGGS, eggs_plac);

                Phase::ClearObj
            }
        } else {
            // Magic-word sequence was started but is incorrect
            if self.settings.oldstyle || self.game.seenbigwords {
                self.rspeak(START_OVER, &[]);
            } else {
                self.rspeak(WELL_POINTLESS, &[]);
            }
            self.game.foobar = WORD_EMPTY;
            Phase::ClearObj
        }
    }

    /// Blast.  No effect unless you've got dynamite, which is a neat
    /// trick!
    fn blast(&mut self) {
        if self.object_is_notfound(ROD2) || !self.game.closed {
            self.rspeak(REQUIRES_DYNAMITE, &[]);
        } else {
            if self.here(ROD2) {
                self.game.bonus = BONUS_SPLATTER;
                self.rspeak(SPLATTER_MESSAGE, &[]);
            } else if self.game.loc == LOC_NE {
                self.game.bonus = BONUS_DEFEAT;
                self.rspeak(DEFEAT_MESSAGE, &[]);
            } else {
                self.game.bonus = BONUS_VICTORY;
                self.rspeak(VICTORY_MESSAGE, &[]);
            }
            self.terminate(Termination::EndGame);
        }
    }

    /// Break.  Only works for mirror in repository and, of course, the
    /// vase.
    fn vbreak(&mut self, verb: i32, obj: i32) -> Phase {
        match obj {
            MIRROR => {
                if self.game.closed {
                    self.state_change(MIRROR, MIRROR_BROKEN);
                    return Phase::DwarfWake;
                }
                self.rspeak(TOO_FAR, &[]);
            }
            VASE if self.prop(VASE) == VASE_WHOLE => {
                if self.toting(VASE) {
                    self.drop(VASE, self.game.loc);
                }
                self.state_change(VASE, VASE_BROKEN);
                self.set_fixed(VASE, IS_FIXED);
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Brief.  Intransitive only.  Suppress full descriptions after
    /// first time.
    fn brief(&mut self) -> Phase {
        self.game.abbnum = 10000;
        self.game.detail = 3;
        self.rspeak(BRIEF_CONFIRM, &[]);
        Phase::ClearObj
    }

    /// Carry an object.  Special cases for bird and cage (if bird in
    /// cage, can't take one without the other).  Liquids also special,
    /// since they depend on status of bottle.  Also various side
    /// effects, etc.
    fn vcarry(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == INTRANSITIVE {
            // Carry, no object given yet.  OK if only one object present.
            let atloc = self.atloc(self.game.loc);
            if atloc == NO_OBJECT
                || self.game.link[atloc as usize] != 0
                || self.atdwrf(self.game.loc) > 0
            {
                return Phase::Unknown;
            }
            // The C code uses atloc as-is here, which indexes past the
            // end of the object table when the only thing present is the
            // far end of a two-placed object.  Map it back to the object
            // itself, which is what listobjects() shows the player.
            obj = if atloc >= NOBJ { atloc - NOBJ } else { atloc };
        }

        if self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }

        if obj == MESSAG {
            self.rspeak(REMOVE_MESSAGE, &[]);
            self.destroy(MESSAG);
            return Phase::ClearObj;
        }

        if self.fixed(obj) != IS_FREE {
            match obj {
                PLANT => {
                    let m = if self.prop(PLANT) == PLANT_THIRSTY || self.object_is_stashed(PLANT) {
                        DEEP_ROOTS
                    } else {
                        YOU_JOKING
                    };
                    self.rspeak(m, &[]);
                }
                BEAR => {
                    let m = if self.prop(BEAR) == SITTING_BEAR {
                        BEAR_CHAINED
                    } else {
                        YOU_JOKING
                    };
                    self.rspeak(m, &[]);
                }
                CHAIN => {
                    let m = if self.prop(BEAR) != UNTAMED_BEAR {
                        STILL_LOCKED
                    } else {
                        YOU_JOKING
                    };
                    self.rspeak(m, &[]);
                }
                RUG => {
                    let m = if self.prop(RUG) == RUG_HOVER {
                        RUG_HOVERS
                    } else {
                        YOU_JOKING
                    };
                    self.rspeak(m, &[]);
                }
                URN => self.rspeak(URN_NOBUDGE, &[]),
                CAVITY => self.rspeak(DOUGHNUT_HOLES, &[]),
                BLOOD => self.rspeak(FEW_DROPS, &[]),
                SIGN => self.rspeak(HAND_PASSTHROUGH, &[]),
                _ => self.rspeak(YOU_JOKING, &[]),
            }
            return Phase::ClearObj;
        }

        if obj == WATER || obj == OIL {
            if !self.here(BOTTLE) || self.liquid() != obj {
                if !self.toting(BOTTLE) {
                    self.rspeak(NO_CONTAINER, &[]);
                    return Phase::ClearObj;
                }
                if self.prop(BOTTLE) == EMPTY_BOTTLE {
                    return self.fill(verb, BOTTLE);
                } else {
                    self.rspeak(BOTTLE_FULL, &[]);
                }
                return Phase::ClearObj;
            }
            obj = BOTTLE;
        }

        if self.game.holdng >= INVLIMIT {
            self.rspeak(CARRY_LIMIT, &[]);
            return Phase::ClearObj;
        }

        if obj == BIRD && self.prop(BIRD) != BIRD_CAGED && !self.object_is_stashed(BIRD) {
            if self.prop(BIRD) == BIRD_FOREST_UNCAGED {
                self.destroy(BIRD);
                self.rspeak(BIRD_CRAP, &[]);
                return Phase::ClearObj;
            }
            if !self.toting(CAGE) {
                self.rspeak(CANNOT_CARRY, &[]);
                return Phase::ClearObj;
            }
            if self.toting(ROD) {
                self.rspeak(BIRD_EVADES, &[]);
                return Phase::ClearObj;
            }
            self.set_prop(BIRD, BIRD_CAGED);
        }
        if (obj == BIRD || obj == CAGE) && self.object_state_equals(BIRD, BIRD_CAGED) {
            // expression maps BIRD to CAGE and CAGE to BIRD
            self.carry(BIRD + CAGE - obj, self.game.loc);
        }

        self.carry(obj, self.game.loc);

        if obj == BOTTLE && self.liquid() != NO_OBJECT {
            let l = self.liquid();
            self.set_place(l, CARRIED);
        }

        if gstone(obj) && !self.object_is_found(obj) {
            self.object_set_found(obj);
            self.set_prop(CAVITY, CAVITY_EMPTY);
        }
        self.rspeak(OK_MAN, &[]);
        Phase::ClearObj
    }

    /// Do something to the bear's chain.
    fn chain(&mut self, verb: i32) -> Phase {
        if verb != LOCK {
            if self.prop(BEAR) == UNTAMED_BEAR {
                self.rspeak(BEAR_BLOCKS, &[]);
                return Phase::ClearObj;
            }
            if self.prop(CHAIN) == CHAIN_HEAP {
                self.rspeak(ALREADY_UNLOCKED, &[]);
                return Phase::ClearObj;
            }
            self.set_prop(CHAIN, CHAIN_HEAP);
            self.set_fixed(CHAIN, IS_FREE);
            if self.prop(BEAR) != BEAR_DEAD {
                self.set_prop(BEAR, CONTENTED_BEAR);
            }

            if self.prop(BEAR) == BEAR_DEAD {
                // Can't be reached until the bear can die in some way
                // other than a bridge collapse. Leave in in case this
                // changes.
                self.set_fixed(BEAR, IS_FIXED);
            } else {
                self.set_fixed(BEAR, IS_FREE);
            }
            self.rspeak(CHAIN_UNLOCKED, &[]);
            return Phase::ClearObj;
        }

        if self.prop(CHAIN) != CHAIN_HEAP {
            self.rspeak(ALREADY_LOCKED, &[]);
            return Phase::ClearObj;
        }
        if self.game.loc != OBJECTS[CHAIN as usize].plac {
            self.rspeak(NO_LOCKSITE, &[]);
            return Phase::ClearObj;
        }

        self.set_prop(CHAIN, CHAIN_FIXED);

        if self.toting(CHAIN) {
            self.drop(CHAIN, self.game.loc);
        }
        self.set_fixed(CHAIN, IS_FIXED);

        self.rspeak(CHAIN_LOCKED, &[]);
        Phase::ClearObj
    }

    /// Discard object.  "Throw" also comes here for most objects.
    /// Special cases for bird (might attack snake or dragon) and cage
    /// (might contain bird) and vase. Drop coins at vending machine for
    /// extra batteries.
    fn discard(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == ROD && !self.toting(ROD) && self.toting(ROD2) {
            obj = ROD2;
        }

        if !self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }

        if gstone(obj) && self.at(CAVITY) && self.prop(CAVITY) != CAVITY_FULL {
            self.rspeak(GEM_FITS, &[]);
            self.set_prop(obj, STATE_IN_CAVITY);
            self.set_prop(CAVITY, CAVITY_FULL);
            if self.here(RUG)
                && ((obj == EMERALD && self.prop(RUG) != RUG_HOVER)
                    || (obj == RUBY && self.prop(RUG) == RUG_HOVER))
            {
                if obj == RUBY {
                    self.rspeak(RUG_SETTLES, &[]);
                } else if self.toting(RUG) {
                    self.rspeak(RUG_WIGGLES, &[]);
                } else {
                    self.rspeak(RUG_RISES, &[]);
                }
                if !self.toting(RUG) || obj == RUBY {
                    let mut k = if self.prop(RUG) == RUG_HOVER {
                        RUG_FLOOR
                    } else {
                        RUG_HOVER
                    };
                    self.set_prop(RUG, k);
                    if k == RUG_HOVER {
                        k = OBJECTS[SAPPH as usize].plac;
                    }
                    self.move_obj(RUG + NOBJ, k);
                }
            }
            self.drop(obj, self.game.loc);
            return Phase::ClearObj;
        }

        if obj == COINS && self.here(VEND) {
            self.destroy(COINS);
            self.drop(BATTERY, self.game.loc);
            self.pspeak(BATTERY, SpeakType::Look, true, FRESH_BATTERIES, &[]);
            return Phase::ClearObj;
        }

        if self.liquid() == obj {
            obj = BOTTLE;
        }
        if obj == BOTTLE && self.liquid() != NO_OBJECT {
            let l = self.liquid();
            self.set_place(l, LOC_NOWHERE);
        }

        if obj == BEAR && self.at(TROLL) {
            self.state_change(TROLL, TROLL_GONE);
            self.move_obj(TROLL, LOC_NOWHERE);
            self.move_obj(TROLL + NOBJ, IS_FREE);
            self.move_obj(TROLL2, OBJECTS[TROLL as usize].plac);
            self.move_obj(TROLL2 + NOBJ, OBJECTS[TROLL as usize].fixd);
            self.juggle(CHASM);
            self.drop(obj, self.game.loc);
            return Phase::ClearObj;
        }

        if obj == VASE && self.game.loc != OBJECTS[PILLOW as usize].plac {
            let st = if self.at(PILLOW) {
                VASE_WHOLE
            } else {
                VASE_DROPPED
            };
            self.state_change(VASE, st);
            if self.prop(VASE) != VASE_WHOLE {
                self.set_fixed(VASE, IS_FIXED);
            }
            self.drop(obj, self.game.loc);
            return Phase::ClearObj;
        }

        if obj == CAGE && self.prop(BIRD) == BIRD_CAGED {
            self.drop(BIRD, self.game.loc);
        }

        if obj == BIRD {
            if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
                self.rspeak(BIRD_BURNT, &[]);
                self.destroy(BIRD);
                return Phase::ClearObj;
            }
            if self.here(SNAKE) {
                self.rspeak(BIRD_ATTACKS, &[]);
                if self.game.closed {
                    return Phase::DwarfWake;
                }
                self.destroy(SNAKE);
                // Set game.prop for use by travel options
                self.set_prop(SNAKE, SNAKE_CHASED);
            } else {
                self.rspeak(OK_MAN, &[]);
            }

            let st = if self.forest(self.game.loc) {
                BIRD_FOREST_UNCAGED
            } else {
                BIRD_UNCAGED
            };
            self.set_prop(BIRD, st);
            self.drop(obj, self.game.loc);
            return Phase::ClearObj;
        }

        self.rspeak(OK_MAN, &[]);
        self.drop(obj, self.game.loc);
        Phase::ClearObj
    }

    /// Drink.  If no object, assume water and look for it here.  If
    /// water is in the bottle, drink that, else must be at a water loc,
    /// so drink stream.
    fn drink(&mut self, verb: i32, obj: i32) -> Phase {
        if obj == INTRANSITIVE
            && self.liqloc(self.game.loc) != WATER
            && (self.liquid() != WATER || !self.here(BOTTLE))
        {
            return Phase::Unknown;
        }

        if obj == BLOOD {
            self.destroy(BLOOD);
            self.state_change(DRAGON, DRAGON_BLOODLESS);
            self.game.blooded = true;
            return Phase::ClearObj;
        }

        if obj != INTRANSITIVE && obj != WATER {
            self.rspeak(RIDICULOUS_ATTEMPT, &[]);
            return Phase::ClearObj;
        }
        if self.liquid() == WATER && self.here(BOTTLE) {
            self.set_place(WATER, LOC_NOWHERE);
            self.state_change(BOTTLE, EMPTY_BOTTLE);
            return Phase::ClearObj;
        }

        self.verb_message(verb);
        Phase::ClearObj
    }

    /// Eat.  Intransitive: assume food if present, else ask what.
    /// Transitive: food ok, some things lose appetite, rest are
    /// ridiculous.
    fn eat(&mut self, verb: i32, obj: i32) -> Phase {
        match obj {
            INTRANSITIVE | FOOD => {
                if obj == INTRANSITIVE && !self.here(FOOD) {
                    return Phase::Unknown;
                }
                self.destroy(FOOD);
                self.rspeak(THANKS_DELICIOUS, &[]);
            }
            BIRD | SNAKE | CLAM | OYSTER | DWARF | DRAGON | TROLL | BEAR | OGRE => {
                self.rspeak(LOST_APPETITE, &[]);
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Extinguish.  Lamp, urn, dragon/volcano (nice try).
    fn extinguish(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == INTRANSITIVE {
            if self.here(LAMP) && self.prop(LAMP) == LAMP_BRIGHT {
                obj = LAMP;
            }
            if self.here(URN) && self.prop(URN) == URN_LIT {
                obj = URN;
            }
            if obj == INTRANSITIVE {
                return Phase::Unknown;
            }
        }

        match obj {
            URN => {
                if self.prop(URN) != URN_EMPTY {
                    self.state_change(URN, URN_DARK);
                } else {
                    self.pspeak(URN, SpeakType::Change, true, URN_DARK, &[]);
                }
            }
            LAMP => {
                self.state_change(LAMP, LAMP_DARK);
                let m = if self.is_dark_here() {
                    PITCH_DARK
                } else {
                    NO_MESSAGE
                };
                self.rspeak(m, &[]);
            }
            DRAGON | VOLCANO => self.rspeak(BEYOND_POWER, &[]),
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Feed.  If bird, no seed.  Snake, dragon, troll: quip.  If dwarf,
    /// make him mad.  Bear, special.
    fn feed(&mut self, verb: i32, obj: i32) -> Phase {
        match obj {
            BIRD => self.rspeak(BIRD_PINING, &[]),
            DRAGON => {
                if self.prop(DRAGON) != DRAGON_BARS {
                    self.rspeak(RIDICULOUS_ATTEMPT, &[]);
                } else {
                    self.rspeak(NOTHING_EDIBLE, &[]);
                }
            }
            SNAKE => {
                if !self.game.closed && self.here(BIRD) {
                    self.destroy(BIRD);
                    self.rspeak(BIRD_DEVOURED, &[]);
                } else {
                    self.rspeak(NOTHING_EDIBLE, &[]);
                }
            }
            TROLL => self.rspeak(TROLL_VICES, &[]),
            DWARF => {
                if self.here(FOOD) {
                    self.game.dflag += 2;
                    self.rspeak(REALLY_MAD, &[]);
                } else {
                    self.verb_message(verb);
                }
            }
            BEAR => {
                if self.prop(BEAR) == BEAR_DEAD {
                    self.rspeak(RIDICULOUS_ATTEMPT, &[]);
                } else if self.prop(BEAR) == UNTAMED_BEAR {
                    if self.here(FOOD) {
                        self.destroy(FOOD);
                        self.set_fixed(AXE, IS_FREE);
                        self.set_prop(AXE, AXE_HERE);
                        self.state_change(BEAR, SITTING_BEAR);
                    } else {
                        self.rspeak(NOTHING_EDIBLE, &[]);
                    }
                } else {
                    self.verb_message(verb);
                }
            }
            OGRE => {
                if self.here(FOOD) {
                    self.rspeak(OGRE_FULL, &[]);
                } else {
                    self.verb_message(verb);
                }
            }
            _ => self.rspeak(AM_GAME, &[]),
        }
        Phase::ClearObj
    }

    /// Fill.  Bottle or urn must be empty, and liquid available.  (Vase
    /// is nasty.)
    fn fill(&mut self, verb: i32, obj: i32) -> Phase {
        if obj == VASE {
            if self.liqloc(self.game.loc) == NO_OBJECT {
                self.rspeak(FILL_INVALID, &[]);
                return Phase::ClearObj;
            }
            if !self.toting(VASE) {
                self.rspeak(ARENT_CARRYING, &[]);
                return Phase::ClearObj;
            }
            self.rspeak(SHATTER_VASE, &[]);
            self.set_prop(VASE, VASE_BROKEN);
            self.set_fixed(VASE, IS_FIXED);
            self.drop(VASE, self.game.loc);
            return Phase::ClearObj;
        }

        if obj == URN {
            if self.prop(URN) != URN_EMPTY {
                self.rspeak(FULL_URN, &[]);
                return Phase::ClearObj;
            }
            if !self.here(BOTTLE) {
                self.rspeak(FILL_INVALID, &[]);
                return Phase::ClearObj;
            }
            let k = self.liquid();
            match k {
                WATER => {
                    self.set_prop(BOTTLE, EMPTY_BOTTLE);
                    self.rspeak(WATER_URN, &[]);
                }
                OIL => {
                    self.set_prop(URN, URN_DARK);
                    self.set_prop(BOTTLE, EMPTY_BOTTLE);
                    self.rspeak(OIL_URN, &[]);
                }
                _ => {
                    self.rspeak(FILL_INVALID, &[]);
                    return Phase::ClearObj;
                }
            }
            self.set_place(k, LOC_NOWHERE);
            return Phase::ClearObj;
        }
        if obj != INTRANSITIVE && obj != BOTTLE {
            self.verb_message(verb);
            return Phase::ClearObj;
        }
        if obj == INTRANSITIVE && !self.here(BOTTLE) {
            return Phase::Unknown;
        }

        if self.here(URN) && self.prop(URN) != URN_EMPTY {
            self.rspeak(URN_NOPOUR, &[]);
            return Phase::ClearObj;
        }
        if self.liquid() != NO_OBJECT {
            self.rspeak(BOTTLE_FULL, &[]);
            return Phase::ClearObj;
        }
        if self.liqloc(self.game.loc) == NO_OBJECT {
            self.rspeak(NO_LIQUID, &[]);
            return Phase::ClearObj;
        }

        let st = if self.liqloc(self.game.loc) == OIL {
            OIL_BOTTLE
        } else {
            WATER_BOTTLE
        };
        self.state_change(BOTTLE, st);
        if self.toting(BOTTLE) {
            let l = self.liquid();
            self.set_place(l, CARRIED);
        }
        Phase::ClearObj
    }

    /// Find.  Might be carrying it, or it might be here.  Else give
    /// caveat.
    fn find(&mut self, verb: i32, obj: i32) -> Phase {
        if self.toting(obj) {
            self.rspeak(ALREADY_CARRYING, &[]);
            return Phase::ClearObj;
        }

        if self.game.closed {
            self.rspeak(NEEDED_NEARBY, &[]);
            return Phase::ClearObj;
        }

        if self.at(obj)
            || (self.liquid() == obj && self.at(BOTTLE))
            || obj == self.liqloc(self.game.loc)
            || (obj == DWARF && self.atdwrf(self.game.loc) > 0)
        {
            self.rspeak(YOU_HAVEIT, &[]);
            return Phase::ClearObj;
        }

        self.verb_message(verb);
        Phase::ClearObj
    }

    /// Fly.  Snide remarks unless hovering rug is here.
    fn fly(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == INTRANSITIVE {
            if !self.here(RUG) {
                self.rspeak(FLAP_ARMS, &[]);
                return Phase::ClearObj;
            }
            if self.prop(RUG) != RUG_HOVER {
                self.rspeak(RUG_NOTHING2, &[]);
                return Phase::ClearObj;
            }
            obj = RUG;
        }

        if obj != RUG {
            self.verb_message(verb);
            return Phase::ClearObj;
        }
        if self.prop(RUG) != RUG_HOVER {
            self.rspeak(RUG_NOTHING1, &[]);
            return Phase::ClearObj;
        }

        if self.game.loc == LOC_CLIFF {
            self.game.oldlc2 = self.game.oldloc;
            self.game.oldloc = self.game.loc;
            self.game.newloc = LOC_LEDGE;
            self.rspeak(RUG_GOES, &[]);
        } else if self.game.loc == LOC_LEDGE {
            self.game.oldlc2 = self.game.oldloc;
            self.game.oldloc = self.game.loc;
            self.game.newloc = LOC_CLIFF;
            self.rspeak(RUG_RETURNS, &[]);
        } else {
            // should never happen
            self.rspeak(NOTHING_HAPPENS, &[]);
        }
        Phase::Terminate
    }

    /// Inventory. If object, treat same as find.  Else report on current
    /// burden.
    fn inven(&mut self) -> Phase {
        let mut empty = true;
        for i in 1..NOBJ {
            if i == BEAR || !self.toting(i) {
                continue;
            }
            if empty {
                self.rspeak(NOW_HOLDING, &[]);
                empty = false;
            }
            self.pspeak(i, SpeakType::Touch, false, -1, &[]);
        }
        if self.toting(BEAR) {
            self.rspeak(TAME_BEAR, &[]);
        }
        if empty {
            self.rspeak(NO_CARRY, &[]);
        }
        Phase::ClearObj
    }

    /// Light.  Applicable only to lamp and urn.
    fn light(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == INTRANSITIVE {
            let mut selects = 0;
            if self.here(LAMP) && self.prop(LAMP) == LAMP_DARK && self.game.limit >= 0 {
                obj = LAMP;
                selects += 1;
            }
            if self.here(URN) && self.prop(URN) == URN_DARK {
                obj = URN;
                selects += 1;
            }
            if selects != 1 {
                return Phase::Unknown;
            }
        }

        match obj {
            URN => {
                let st = if self.prop(URN) == URN_EMPTY {
                    URN_EMPTY
                } else {
                    URN_LIT
                };
                self.state_change(URN, st);
            }
            LAMP => {
                if self.game.limit < 0 {
                    self.rspeak(LAMP_OUT, &[]);
                } else {
                    self.state_change(LAMP, LAMP_BRIGHT);
                    if self.game.wzdark {
                        return Phase::Top;
                    }
                }
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Listen.  Intransitive only.  Print stuff based on object sound
    /// properties.
    fn listen(&mut self) -> Phase {
        let mut soundlatch = false;
        let sound = LOCATIONS[self.game.loc as usize].sound;
        if sound != SILENT {
            self.rspeak(sound, &[]);
            if !LOCATIONS[self.game.loc as usize].loud {
                self.rspeak(NO_MESSAGE, &[]);
            }
            soundlatch = true;
        }
        for i in 1..NOBJ {
            if !self.here(i)
                || OBJECTS[i as usize].sounds[0].is_none()
                || self.object_is_stashed(i)
                || self.object_is_notfound(i)
            {
                continue;
            }
            let mut mi = self.prop(i);
            // (ESR) Some unpleasant magic on object states here. Ideally
            // we'd have liked the bird to be a normal object that we can
            // use state_change() on; can't do it, because there are
            // actually two different series of per-state birdsounds
            // depending on whether player has drunk dragon's blood.
            if i == BIRD {
                mi += 3 * self.game.blooded as i32;
            }
            let zz = self.game.zzword_str();
            self.pspeak(i, SpeakType::Hear, true, mi, &[Arg::S(&zz)]);
            self.rspeak(NO_MESSAGE, &[]);
            if i == BIRD && mi == BIRD_ENDSTATE {
                self.destroy(BIRD);
            }
            soundlatch = true;
        }
        if !soundlatch {
            self.rspeak(ALL_SILENT, &[]);
        }
        Phase::ClearObj
    }

    /// Lock, unlock, no object given.  Assume various things if present.
    fn lock(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == INTRANSITIVE {
            if self.here(CLAM) {
                obj = CLAM;
            }
            if self.here(OYSTER) {
                obj = OYSTER;
            }
            if self.at(DOOR) {
                obj = DOOR;
            }
            if self.at(GRATE) {
                obj = GRATE;
            }
            if self.here(CHAIN) {
                obj = CHAIN;
            }
            if obj == INTRANSITIVE {
                self.rspeak(NOTHING_LOCKED, &[]);
                return Phase::ClearObj;
            }
        }

        //  Lock, unlock object.  Special stuff for opening clam/oyster
        //  and for chain.
        match obj {
            CHAIN => {
                if self.here(KEYS) {
                    return self.chain(verb);
                }
                self.rspeak(NO_KEYS, &[]);
            }
            GRATE => {
                if self.here(KEYS) {
                    if self.game.closng {
                        self.rspeak(EXIT_CLOSED, &[]);
                        if !self.game.panic {
                            self.game.clock2 = PANICTIME;
                        }
                        self.game.panic = true;
                    } else {
                        let st = if verb == LOCK {
                            GRATE_CLOSED
                        } else {
                            GRATE_OPEN
                        };
                        self.state_change(GRATE, st);
                    }
                } else {
                    self.rspeak(NO_KEYS, &[]);
                }
            }
            CLAM => {
                if verb == LOCK {
                    self.rspeak(HUH_MAN, &[]);
                } else if self.toting(CLAM) {
                    self.rspeak(DROP_CLAM, &[]);
                } else if !self.toting(TRIDENT) {
                    self.rspeak(CLAM_OPENER, &[]);
                } else {
                    self.destroy(CLAM);
                    self.drop(OYSTER, self.game.loc);
                    self.drop(PEARL, LOC_CULDESAC);
                    self.rspeak(PEARL_FALLS, &[]);
                }
            }
            OYSTER => {
                if verb == LOCK {
                    self.rspeak(HUH_MAN, &[]);
                } else if self.toting(OYSTER) {
                    self.rspeak(DROP_OYSTER, &[]);
                } else if !self.toting(TRIDENT) {
                    self.rspeak(OYSTER_OPENER, &[]);
                } else {
                    self.rspeak(OYSTER_OPENS, &[]);
                }
            }
            DOOR => {
                let m = if self.prop(DOOR) == DOOR_UNRUSTED {
                    OK_MAN
                } else {
                    RUSTY_DOOR
                };
                self.rspeak(m, &[]);
            }
            CAGE => self.rspeak(NO_LOCK, &[]),
            KEYS => self.rspeak(CANNOT_UNLOCK, &[]),
            _ => self.verb_message(verb),
        }

        Phase::ClearObj
    }

    /// Pour.  If no object, or object is bottle, assume contents of
    /// bottle.  special tests for pouring water or oil on plant or
    /// rusty door.
    fn pour(&mut self, verb: i32, mut obj: i32) -> Phase {
        if obj == BOTTLE || obj == INTRANSITIVE {
            obj = self.liquid();
        }
        if obj == NO_OBJECT {
            return Phase::Unknown;
        }
        if !self.toting(obj) {
            self.verb_message(verb);
            return Phase::ClearObj;
        }

        if obj != OIL && obj != WATER {
            self.rspeak(CANT_POUR, &[]);
            return Phase::ClearObj;
        }
        if self.here(URN) && self.prop(URN) == URN_EMPTY {
            return self.fill(verb, URN);
        }
        self.set_prop(BOTTLE, EMPTY_BOTTLE);
        self.set_place(obj, LOC_NOWHERE);
        if !(self.at(PLANT) || self.at(DOOR)) {
            self.rspeak(GROUND_WET, &[]);
            return Phase::ClearObj;
        }
        if !self.at(DOOR) {
            if obj == WATER {
                // cycle through the three plant states
                let st = (self.prop(PLANT) + 1) % 3;
                self.state_change(PLANT, st);
                self.set_prop(PLANT2, self.prop(PLANT));
                Phase::Move
            } else {
                self.rspeak(SHAKING_LEAVES, &[]);
                Phase::ClearObj
            }
        } else {
            let st = if obj == OIL {
                DOOR_UNRUSTED
            } else {
                DOOR_RUSTED
            };
            self.state_change(DOOR, st);
            Phase::ClearObj
        }
    }

    /// Quit.  Intransitive only.  Verify intent and exit if that's what
    /// he wants.
    fn quit(&mut self) -> Phase {
        if self.yes_or_no(
            Self::arbitrary(REALLY_QUIT),
            Self::arbitrary(OK_MAN),
            Self::arbitrary(OK_MAN),
        ) {
            self.terminate(Termination::QuitGame);
        }
        Phase::ClearObj
    }

    /// Read.  Print stuff based on objtxt.  Oyster (?) is special case.
    fn read(&mut self, mut command: Command) -> Phase {
        if command.obj == INTRANSITIVE {
            let mut matches = 0;
            let mut resolved_obj = NO_OBJECT;
            for i in 1..NOBJ {
                if self.here(i)
                    && OBJECTS[i as usize].texts[0].is_some()
                    && !self.object_is_stashed(i)
                {
                    matches += 1;
                    if matches == 1 {
                        resolved_obj = i;
                    }
                }
            }
            if matches != 1 || self.is_dark_here() {
                return Phase::Unknown;
            }
            command.obj = resolved_obj;
        }

        if self.is_dark_here() {
            self.sspeak(NO_SEE, &[Arg::S(&command.word[0].raw)]);
        } else if command.obj == OYSTER {
            if !self.toting(OYSTER) || !self.game.closed {
                self.rspeak(DONT_UNDERSTAND, &[]);
            } else if !self.game.clshnt {
                self.game.clshnt = self.yes_or_no(
                    Self::arbitrary(CLUE_QUERY),
                    Self::arbitrary(WAYOUT_CLUE),
                    Self::arbitrary(OK_MAN),
                );
            } else {
                // Not really a sound, but oh well.
                self.pspeak(OYSTER, SpeakType::Hear, true, 1, &[]);
            }
        } else if OBJECTS[command.obj as usize].texts[0].is_none()
            || self.object_is_notfound(command.obj)
        {
            self.verb_message(command.verb);
        } else {
            self.pspeak(
                command.obj,
                SpeakType::Study,
                true,
                self.prop(command.obj),
                &[],
            );
        }
        Phase::ClearObj
    }

    /// Z'ZZZ (word gets recomputed at startup; different each game).
    fn reservoir(&mut self) -> Phase {
        if !self.at(RESER) && self.game.loc != LOC_RESBOTTOM {
            self.rspeak(NOTHING_HAPPENS, &[]);
            return Phase::ClearObj;
        }
        let st = if self.prop(RESER) == WATERS_PARTED {
            WATERS_UNPARTED
        } else {
            WATERS_PARTED
        };
        self.state_change(RESER, st);
        if self.at(RESER) {
            Phase::ClearObj
        } else {
            self.game.oldlc2 = self.game.loc;
            self.game.newloc = LOC_NOWHERE;
            self.rspeak(NOT_BRIGHT, &[]);
            Phase::Terminate
        }
    }

    /// Rub.  Yields various snide remarks except for lit urn.
    fn rub(&mut self, verb: i32, obj: i32) -> Phase {
        if obj == URN && self.prop(URN) == URN_LIT {
            self.destroy(URN);
            self.drop(AMBER, self.game.loc);
            self.set_prop(AMBER, AMBER_IN_ROCK);
            self.game.tally -= 1;
            self.drop(CAVITY, self.game.loc);
            self.rspeak(URN_GENIES, &[]);
        } else if obj != LAMP {
            self.rspeak(PECULIAR_NOTHING, &[]);
        } else {
            self.verb_message(verb);
        }
        Phase::ClearObj
    }

    /// Say.  Echo WD2. Magic words override.
    fn say(&mut self, command: &Command) -> Phase {
        let w1 = &command.word[1];
        if w1.wtype == WordType::Motion && (w1.id == XYZZY || w1.id == PLUGH || w1.id == PLOVER) {
            return Phase::Word2;
        }
        if w1.wtype == WordType::Action && w1.id == PART {
            return self.reservoir();
        }

        if w1.wtype == WordType::Action
            && (w1.id == FEE
                || w1.id == FIE
                || w1.id == FOE
                || w1.id == FOO
                || w1.id == FUM
                || w1.id == PART)
        {
            return self.bigwords(w1.id);
        }
        self.sspeak(OKEY_DOKEY, &[Arg::S(&w1.raw)]);
        Phase::ClearObj
    }

    fn throw_support(&mut self, spk: i32) -> Phase {
        self.rspeak(spk, &[]);
        self.drop(AXE, self.game.loc);
        Phase::Move
    }

    ///  Throw.  Same as discard unless axe.  Then same as attack except
    ///  ignore bird, and if dwarf is present then one might be killed.
    ///  (Only way to do so!)  Axe also special for dragon, bear, and
    ///  troll.  Treasures special for troll.
    fn throwit(&mut self, mut command: Command) -> Phase {
        if !self.toting(command.obj) {
            self.verb_message(command.verb);
            return Phase::ClearObj;
        }
        if OBJECTS[command.obj as usize].is_treasure && self.at(TROLL) {
            // Snarf a treasure for the troll.
            self.drop(command.obj, LOC_NOWHERE);
            self.move_obj(TROLL, LOC_NOWHERE);
            self.move_obj(TROLL + NOBJ, IS_FREE);
            self.drop(TROLL2, OBJECTS[TROLL as usize].plac);
            self.drop(TROLL2 + NOBJ, OBJECTS[TROLL as usize].fixd);
            self.juggle(CHASM);
            self.rspeak(TROLL_SATISFIED, &[]);
            return Phase::ClearObj;
        }
        if command.obj == FOOD && self.here(BEAR) {
            // But throwing food is another story.
            command.obj = BEAR;
            return self.feed(command.verb, command.obj);
        }
        if command.obj != AXE {
            return self.discard(command.verb, command.obj);
        }
        if self.atdwrf(self.game.loc) <= 0 {
            if self.at(DRAGON) && self.prop(DRAGON) == DRAGON_BARS {
                return self.throw_support(DRAGON_SCALES);
            }
            if self.at(TROLL) {
                return self.throw_support(TROLL_RETURNS);
            }
            if self.at(OGRE) {
                return self.throw_support(OGRE_DODGE);
            }
            if self.here(BEAR) && self.prop(BEAR) == UNTAMED_BEAR {
                // This'll teach him to throw the axe at the bear!
                self.drop(AXE, self.game.loc);
                self.set_fixed(AXE, IS_FIXED);
                self.juggle(BEAR);
                self.state_change(AXE, AXE_LOST);
                return Phase::ClearObj;
            }
            command.obj = INTRANSITIVE;
            return self.attack(&command);
        }

        if self.randrange(NDWARVES as i32 + 1) < self.game.dflag {
            self.throw_support(DWARF_DODGES)
        } else {
            let i = self.atdwrf(self.game.loc) as usize;
            self.game.dwarves[i].seen = false;
            self.game.dwarves[i].loc = LOC_NOWHERE;
            self.game.dkill += 1;
            let m = if self.game.dkill == 1 {
                DWARF_SMOKE
            } else {
                KILLED_DWARF
            };
            self.throw_support(m)
        }
    }

    /// Wake.  Only use is to disturb the dwarves.
    fn wake(&mut self, verb: i32, obj: i32) -> Phase {
        if obj != DWARF || !self.game.closed {
            self.verb_message(verb);
            Phase::ClearObj
        } else {
            self.rspeak(PROD_DWARF, &[]);
            Phase::DwarfWake
        }
    }

    /// Set seed.
    fn seed(&mut self, verb: i32, arg: &str) -> Phase {
        let seed = strtol(arg) as i32;
        self.speak(ACTIONS[verb as usize].message, &[Arg::I(seed)]);
        self.set_seed(seed);
        self.game.turns -= 1;
        Phase::Top
    }

    /// Burn turns.
    fn waste(&mut self, verb: i32, turns: i32) -> Phase {
        self.game.limit = self.game.limit.wrapping_sub(turns);
        self.speak(ACTIONS[verb as usize].message, &[Arg::I(self.game.limit)]);
        Phase::Top
    }

    /// Wave.  No effect unless waving rod at fissure or at bird.
    fn wave(&mut self, verb: i32, obj: i32) -> Phase {
        if obj != ROD
            || !self.toting(obj)
            || (!self.here(BIRD) && (self.game.closng || !self.at(FISSURE)))
        {
            let msg = if !self.toting(obj) && (obj != ROD || !self.toting(ROD2)) {
                Self::arbitrary(ARENT_CARRYING)
            } else {
                ACTIONS[verb as usize].message
            };
            self.speak(msg, &[]);
            return Phase::ClearObj;
        }

        if self.prop(BIRD) == BIRD_UNCAGED
            && self.game.loc == self.place(STEPS)
            && self.object_is_notfound(JADE)
        {
            self.drop(JADE, self.game.loc);
            self.object_set_found(JADE);
            self.game.tally -= 1;
            self.rspeak(NECKLACE_FLY, &[]);
            return Phase::ClearObj;
        }
        let fly_msg = if self.prop(BIRD) == BIRD_CAGED {
            CAGE_FLY
        } else {
            FREE_FLY
        };
        if self.game.closed {
            self.rspeak(fly_msg, &[]);
            return Phase::DwarfWake;
        }
        if self.game.closng || !self.at(FISSURE) {
            self.rspeak(fly_msg, &[]);
            return Phase::ClearObj;
        }
        if self.here(BIRD) {
            self.rspeak(fly_msg, &[]);
        }

        let st = if self.prop(FISSURE) == BRIDGED {
            UNBRIDGED
        } else {
            BRIDGED
        };
        self.state_change(FISSURE, st);
        Phase::ClearObj
    }

    ///  Analyse a verb.  Remember what it was, go back for object if
    ///  second word unless verb is "say", which snarfs arbitrary second
    ///  word.
    pub(crate) fn action(&mut self, mut command: Command) -> Phase {
        // Previously, actions that result in a message, but don't do
        // anything further were called "specials". Now they're handled
        // here as normal actions. If noaction is true, then we spit out
        // the message and return.
        if ACTIONS[command.verb as usize].noaction {
            self.verb_message(command.verb);
            return Phase::ClearObj;
        }

        if command.part == SpeechPart::Unknown {
            //  Analyse an object word.  See if the thing is here, whether
            //  we've got a verb yet, and so on.  Object must be here
            //  unless verb is "find" or "invent(ory)" (and no new verb
            //  yet to be analysed).  Water and oil are also funny, since
            //  they are never actually dropped at any location, but might
            //  be here inside the bottle or urn or as a feature of the
            //  location.
            if self.here(command.obj) {
                // FALL THROUGH
            } else if command.obj == DWARF && self.atdwrf(self.game.loc) > 0 {
                // FALL THROUGH
            } else if !self.game.closed
                && ((self.liquid() == command.obj && self.here(BOTTLE))
                    || command.obj == self.liqloc(self.game.loc))
            {
                // FALL THROUGH
            } else if command.obj == OIL && self.here(URN) && self.prop(URN) != URN_EMPTY {
                command.obj = URN;
            } else if command.obj == PLANT && self.at(PLANT2) && self.prop(PLANT2) != PLANT_THIRSTY
            {
                command.obj = PLANT2;
            } else if command.obj == KNIFE && self.game.knfloc == self.game.loc {
                self.game.knfloc = -1;
                self.rspeak(KNIVES_VANISH, &[]);
                return Phase::ClearObj;
            } else if command.obj == ROD && self.here(ROD2) {
                command.obj = ROD2;
            } else if (command.verb == FIND || command.verb == INVENTORY)
                && (command.word[1].id == WORD_EMPTY || command.word[1].id == WORD_NOT_FOUND)
            {
                // FALL THROUGH
            } else {
                self.sspeak(NO_SEE, &[Arg::S(&command.word[0].raw)]);
                return Phase::ClearObj;
            }

            if command.verb != 0 {
                command.part = SpeechPart::Transitive;
            }
        }

        match command.part {
            SpeechPart::Intransitive => {
                if !command.word[1].raw.is_empty() && command.verb != SAY {
                    return Phase::Word2;
                }
                if command.verb == SAY {
                    // KEYS is not special, anything not NO_OBJECT or
                    // INTRANSITIVE will do here. We're preventing
                    // interpretation as an intransitive verb when the
                    // word is unknown.
                    command.obj = if !command.word[1].raw.is_empty() {
                        KEYS
                    } else {
                        NO_OBJECT
                    };
                }
                if command.obj == NO_OBJECT || command.obj == INTRANSITIVE {
                    // Analyse an intransitive verb (ie, no object given yet).
                    return match command.verb {
                        CARRY => self.vcarry(command.verb, INTRANSITIVE),
                        DROP => Phase::Unknown,
                        SAY => Phase::Unknown,
                        UNLOCK => self.lock(command.verb, INTRANSITIVE),
                        NOTHING => {
                            self.rspeak(OK_MAN, &[]);
                            Phase::ClearObj
                        }
                        LOCK => self.lock(command.verb, INTRANSITIVE),
                        LIGHT => self.light(command.verb, INTRANSITIVE),
                        EXTINGUISH => self.extinguish(command.verb, INTRANSITIVE),
                        WAVE => Phase::Unknown,
                        TAME => Phase::Unknown,
                        GO => {
                            self.verb_message(command.verb);
                            Phase::ClearObj
                        }
                        ATTACK => {
                            command.obj = INTRANSITIVE;
                            self.attack(&command)
                        }
                        POUR => self.pour(command.verb, INTRANSITIVE),
                        EAT => self.eat(command.verb, INTRANSITIVE),
                        DRINK => self.drink(command.verb, INTRANSITIVE),
                        RUB => Phase::Unknown,
                        THROW => Phase::Unknown,
                        QUIT => self.quit(),
                        FIND => Phase::Unknown,
                        INVENTORY => self.inven(),
                        FEED => Phase::Unknown,
                        FILL => self.fill(command.verb, INTRANSITIVE),
                        BLAST => {
                            self.blast();
                            Phase::ClearObj
                        }
                        SCORE => {
                            self.score(Termination::ScoreGame);
                            Phase::ClearObj
                        }
                        FEE | FIE | FOE | FOO | FUM => self.bigwords(command.word[0].id),
                        BRIEF => self.brief(),
                        READ => {
                            command.obj = INTRANSITIVE;
                            self.read(command)
                        }
                        BREAK => Phase::Unknown,
                        WAKE => Phase::Unknown,
                        SAVE => self.suspend(),
                        RESUME => self.resume(),
                        FLY => self.fly(command.verb, INTRANSITIVE),
                        LISTEN => self.listen(),
                        PART => self.reservoir(),
                        SEED | WASTE => {
                            self.rspeak(NUMERIC_REQUIRED, &[]);
                            Phase::Top
                        }
                        _ => bug(BugType::IntransitiveActionVerbExceedsGotoList),
                    };
                }
                self.transitive(command)
            }
            SpeechPart::Transitive => self.transitive(command),
            SpeechPart::Unknown => {
                // Unknown verb, couldn't deduce object - might need hint
                self.sspeak(WHAT_DO, &[Arg::S(&command.word[0].raw)]);
                Phase::CheckHint
            }
        }
    }

    /// Analyse a transitive verb.
    fn transitive(&mut self, command: Command) -> Phase {
        let (verb, obj) = (command.verb, command.obj);
        match verb {
            CARRY => self.vcarry(verb, obj),
            DROP => self.discard(verb, obj),
            SAY => self.say(&command),
            UNLOCK => self.lock(verb, obj),
            NOTHING => {
                self.rspeak(OK_MAN, &[]);
                Phase::ClearObj
            }
            LOCK => self.lock(verb, obj),
            LIGHT => self.light(verb, obj),
            EXTINGUISH => self.extinguish(verb, obj),
            WAVE => self.wave(verb, obj),
            TAME | GO | QUIT | SCORE | FEE | FIE | FOE | FOO | FUM | BRIEF | SAVE | RESUME
            | LISTEN => {
                self.verb_message(verb);
                Phase::ClearObj
            }
            ATTACK => self.attack(&command),
            POUR => self.pour(verb, obj),
            EAT => self.eat(verb, obj),
            DRINK => self.drink(verb, obj),
            RUB => self.rub(verb, obj),
            THROW => self.throwit(command),
            FIND | INVENTORY => self.find(verb, obj),
            FEED => self.feed(verb, obj),
            FILL => self.fill(verb, obj),
            BLAST => {
                self.blast();
                Phase::ClearObj
            }
            READ => self.read(command),
            BREAK => self.vbreak(verb, obj),
            WAKE => self.wake(verb, obj),
            FLY => self.fly(verb, obj),
            // This case should never happen - here only as placeholder
            PART => self.reservoir(),
            SEED => self.seed(verb, &command.word[1].raw),
            WASTE => self.waste(verb, atol(&command.word[1].raw) as i32),
            _ => bug(BugType::TransitiveActionVerbExceedsGotoList),
        }
    }
}

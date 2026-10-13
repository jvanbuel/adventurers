// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! What each verb does.
//!
//! Handlers that may be used without an object take `Option<Obj>`,
//! where `None` means no object was given and the handler should work
//! out what was meant.  Handlers that can end the game return a
//! [`Flow`].

use std::fs::File;
use std::io::{Read as _, Write as _};

use super::turn::{Command, Phase, SpeechPart};
use super::{Aspect, Console, DwarfAt, Engine, Flow, GameOver, SaveMode};
use crate::dungeon::*;
use crate::objects::{Place, STATE_IN_CAVITY};
use crate::parser::WordKind;
use crate::state::{Bonus, Knife};

/// Most objects the player can carry at once.
const INVLIMIT: i32 = 7;

/// Read a number the way C's `strtol` does: optional leading space and
/// sign, then digits; 0 if there are none; saturating on overflow.  The
/// result is then truncated to 32 bits, as the original stored it.
///
/// The argument is usually a validated number, but command rewriting can
/// leave `seed` or `waste` with any word, or none.
fn numeric_arg(word: &str) -> i32 {
    let s = word.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (negative, rest) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return 0;
    }
    let n = match (digits.parse::<i64>(), negative) {
        (Ok(n), false) => n,
        (Ok(n), true) => -n,
        (Err(_), false) => i64::MAX,
        (Err(_), true) => i64::MIN,
    };
    n as i32
}

impl<C: Console> Engine<C> {
    /// Print the verb's default response.
    fn verb_message(&mut self, verb: Verb) {
        self.speak(verb.info().message, &[]);
    }

    /// Carry out a command whose verb and object are known.
    pub(super) fn action(&mut self, mut command: Command) -> Flow<Phase> {
        // Some verbs only ever print their message.
        if command.verb.info().noaction {
            self.verb_message(command.verb);
            return Ok(Phase::ClearObj);
        }

        if command.part == SpeechPart::Unknown {
            // An object word.  The object must be here unless the verb is
            // "find" or "inventory".  Water and oil are never dropped
            // anywhere, but may be here in the bottle or urn, or as a
            // feature of the location.
            let obj = command.obj;
            let loc = self.game.loc;
            let closed = self.game.closed;
            if self.here(obj)
                || (obj == DWARF && self.dwarf_is_here())
                || (!closed
                    && ((self.liquid() == Some(obj) && self.here(BOTTLE))
                        || Self::liquid_at(loc) == Some(obj)))
            {
                // It's here.
            } else if obj == OIL && self.here(URN) && self.prop(URN) != URN_EMPTY {
                command.obj = URN;
            } else if obj == PLANT && self.at(PLANT2) && self.prop(PLANT2) != PLANT_THIRSTY {
                command.obj = PLANT2;
            } else if obj == KNIFE && self.game.knife == Knife::ThrownAt(loc) {
                self.game.knife = Knife::Explained;
                self.say(KNIVES_VANISH);
                return Ok(Phase::ClearObj);
            } else if obj == ROD && self.here(ROD2) {
                command.obj = ROD2;
            } else if (command.verb == FIND || command.verb == INVENTORY)
                && matches!(
                    command.words[1].kind,
                    WordKind::Empty | WordKind::Numeric | WordKind::Unknown
                )
            {
                // Looking for something that isn't here.
            } else {
                self.say_echo(NO_SEE, &command.words[0].raw);
                return Ok(Phase::ClearObj);
            }

            if command.verb != ACT_NULL {
                command.part = SpeechPart::Transitive;
            }
        }

        match command.part {
            SpeechPart::Intransitive => {
                let has_second_word = !command.words[1].raw.is_empty();
                if has_second_word && command.verb != SAY {
                    return Ok(Phase::Word2);
                }
                if command.verb == SAY {
                    // Anything but NO_OBJECT will do: "say" with a word
                    // is never intransitive.
                    command.obj = if has_second_word { KEYS } else { NO_OBJECT };
                }
                if command.obj == NO_OBJECT {
                    return self.intransitive(&command);
                }
                self.transitive(command)
            }
            SpeechPart::Transitive => self.transitive(command),
            SpeechPart::Unknown => {
                // Unknown verb, couldn't deduce object; might need a hint.
                self.say_echo(WHAT_DO, &command.words[0].raw);
                Ok(Phase::CheckHint)
            }
        }
    }

    /// A verb with no object.
    fn intransitive(&mut self, command: &Command) -> Flow<Phase> {
        let verb = command.verb;
        Ok(match verb {
            CARRY => self.carry(verb, None),
            DROP | SAY | WAVE | TAME | RUB | THROW | FIND | FEED | BREAK | WAKE => Phase::Unknown,
            UNLOCK | LOCK => self.lock(verb, None),
            NOTHING => {
                self.say(OK_MAN);
                Phase::ClearObj
            }
            LIGHT => self.light(verb, None),
            EXTINGUISH => self.extinguish(verb, None),
            GO => {
                self.verb_message(verb);
                Phase::ClearObj
            }
            ATTACK => return self.attack(verb, None),
            POUR => self.pour(verb, None),
            EAT => self.eat(verb, None),
            DRINK => self.drink(verb, None),
            QUIT => return self.quit(),
            INVENTORY => self.inventory(),
            FILL => self.fill(verb, None),
            BLAST => return self.blast(),
            SCORE => {
                self.score(super::score::Ending::Scoring);
                Phase::ClearObj
            }
            FEE | FIE | FOE | FOO | FUM => self.big_words(command.words[0].id()),
            BRIEF => self.brief(),
            READ => return self.read(verb, None, &command.words[0].raw),
            SAVE => return self.suspend(),
            RESUME => return self.resume(),
            FLY => self.fly(verb, None),
            LISTEN => self.listen(),
            PART => self.reservoir(),
            SEED | WASTE => {
                self.say(NUMERIC_REQUIRED);
                Phase::Top
            }
            _ => unreachable!("intransitive verb {verb:?} has no handler"),
        })
    }

    /// A verb with an object.
    fn transitive(&mut self, command: Command) -> Flow<Phase> {
        let (verb, obj) = (command.verb, command.obj);
        Ok(match verb {
            CARRY => self.carry(verb, Some(obj)),
            DROP => self.discard(verb, obj),
            SAY => self.say_word(&command),
            UNLOCK | LOCK => self.lock(verb, Some(obj)),
            NOTHING => {
                self.say(OK_MAN);
                Phase::ClearObj
            }
            LIGHT => self.light(verb, Some(obj)),
            EXTINGUISH => self.extinguish(verb, Some(obj)),
            WAVE => self.wave(verb, obj),
            TAME | GO | QUIT | SCORE | FEE | FIE | FOE | FOO | FUM | BRIEF | SAVE | RESUME
            | LISTEN => {
                self.verb_message(verb);
                Phase::ClearObj
            }
            ATTACK => return self.attack(verb, Some(obj)),
            POUR => self.pour(verb, Some(obj)),
            EAT => self.eat(verb, Some(obj)),
            DRINK => self.drink(verb, Some(obj)),
            RUB => self.rub(verb, obj),
            THROW => return self.throw(verb, obj),
            FIND | INVENTORY => self.find(verb, obj),
            FEED => self.feed(verb, obj),
            FILL => self.fill(verb, Some(obj)),
            BLAST => return self.blast(),
            READ => return self.read(verb, Some(obj), &command.words[0].raw),
            BREAK => self.smash(verb, obj),
            WAKE => self.wake(verb, obj),
            FLY => self.fly(verb, Some(obj)),
            PART => self.reservoir(),
            SEED => self.reseed_command(verb, &command.words[1].raw),
            WASTE => self.waste(verb, &command.words[1].raw),
            _ => unreachable!("transitive verb {verb:?} has no handler"),
        })
    }

    /// Attack.  With no object, pick the target if there's only one
    /// candidate: enemies (dwarf, snake, dragon, troll, ogre, bear)
    /// first, then others (bird, vending machine, clam).  "Throw" with
    /// the axe comes here too, but can't target the bird or machine.
    fn attack(&mut self, verb: Verb, obj: Option<Obj>) -> Flow<Phase> {
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
    fn kill_dragon(&mut self) -> Flow<Phase> {
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
    fn attack_ogre(&mut self) {
        self.say(OGRE_DODGE);
        if self.dwarf_at(self.game.loc) == DwarfAt::None {
            return;
        }
        self.say(KNIFE_THROWN);
        self.game.objects.destroy(OGRE);
        let loc = self.game.loc;
        let mut fled = 0;
        for d in &mut self.game.dwarves[1..super::dwarves::PIRATE] {
            if d.loc == loc {
                fled += 1;
                d.loc = LOC_LONGWEST;
                d.seen = false;
            }
        }
        self.say(if fled > 1 { OGRE_PANIC1 } else { OGRE_PANIC2 });
    }

    /// FEE FIE FOE FOO (and FUM).  Said in order, the last word brings
    /// the golden eggs back to the giant's room.  `id` is the raw number
    /// of the word said.
    fn big_words(&mut self, id: i32) -> Phase {
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

    /// Blast.  Only works with the dynamite in the repository, and ends
    /// the game one way or another.
    fn blast(&mut self) -> Flow<Phase> {
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
        Err(self.terminate(super::score::Ending::Died))
    }

    /// Break.  Works on the mirror in the repository and on the vase.
    fn smash(&mut self, verb: Verb, obj: Obj) -> Phase {
        match obj {
            MIRROR if self.game.closed => {
                self.state_change(MIRROR, MIRROR_BROKEN);
                return Phase::DwarfWake;
            }
            MIRROR => self.say(TOO_FAR),
            VASE if self.prop(VASE) == VASE_WHOLE => {
                if self.toting(VASE) {
                    self.game.objects.drop(VASE, Place::At(self.game.loc));
                }
                self.state_change(VASE, VASE_BROKEN);
                self.game.objects.set_fixed(VASE, Fixed::Immovable);
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Brief.  Only give long descriptions on the first visit.
    fn brief(&mut self) -> Phase {
        self.game.long_desc_interval = 10000;
        self.game.detail = 3;
        self.say(BRIEF_CONFIRM);
        Phase::ClearObj
    }

    /// Take.  With no object, takes the only thing here.  The bird and
    /// cage go together when the bird is caged; liquids go in the
    /// bottle.
    fn carry(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Lock or unlock the bear's chain.
    fn chain(&mut self, verb: Verb) -> Phase {
        let objects = &mut self.game.objects;
        if verb != LOCK {
            if objects.prop(BEAR) == UNTAMED_BEAR {
                self.say(BEAR_BLOCKS);
                return Phase::ClearObj;
            }
            if objects.prop(CHAIN) == CHAIN_HEAP {
                self.say(ALREADY_UNLOCKED);
                return Phase::ClearObj;
            }
            objects.set_prop(CHAIN, CHAIN_HEAP);
            objects.set_fixed(CHAIN, Fixed::Free);
            if objects.prop(BEAR) != BEAR_DEAD {
                objects.set_prop(BEAR, CONTENTED_BEAR);
            }
            // (The bear can't currently be dead here: it only dies on
            // the bridge.)
            let bear_fixed = if objects.prop(BEAR) == BEAR_DEAD {
                Fixed::Immovable
            } else {
                Fixed::Free
            };
            objects.set_fixed(BEAR, bear_fixed);
            self.say(CHAIN_UNLOCKED);
            return Phase::ClearObj;
        }

        if objects.prop(CHAIN) != CHAIN_HEAP {
            self.say(ALREADY_LOCKED);
            return Phase::ClearObj;
        }
        let loc = self.game.loc;
        if loc != CHAIN.info().plac {
            self.say(NO_LOCKSITE);
            return Phase::ClearObj;
        }
        objects.set_prop(CHAIN, CHAIN_FIXED);
        if objects.is_carried(CHAIN) {
            objects.drop(CHAIN, Place::At(loc));
        }
        objects.set_fixed(CHAIN, Fixed::Immovable);
        self.say(CHAIN_LOCKED);
        Phase::ClearObj
    }

    /// Drop.  "Throw" comes here for most objects.  Gems go in the
    /// cavity; coins buy batteries; the bird may attack the snake or be
    /// burnt by the dragon; the vase breaks unless there's a pillow.
    fn discard(&mut self, verb: Verb, obj: Obj) -> Phase {
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
    fn place_gem(&mut self, gem: Obj) {
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

    /// Drink.  With no object, drink water from the bottle or stream.
    fn drink(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Eat.  With no object, eat the food if it's here.
    fn eat(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Extinguish the lamp or the urn.
    fn extinguish(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let obj = match obj {
            Some(obj) => obj,
            None if self.here(URN) && self.prop(URN) == URN_LIT => URN,
            None if self.here(LAMP) && self.prop(LAMP) == LAMP_BRIGHT => LAMP,
            None => return Phase::Unknown,
        };
        match obj {
            URN => {
                if self.prop(URN) == URN_EMPTY {
                    self.describe(URN, Aspect::Change, URN_DARK);
                } else {
                    self.state_change(URN, URN_DARK);
                }
            }
            LAMP => {
                self.state_change(LAMP, LAMP_DARK);
                self.say(if self.is_dark_here() {
                    PITCH_DARK
                } else {
                    NO_MESSAGE
                });
            }
            DRAGON | VOLCANO => self.say(BEYOND_POWER),
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Feed.  The bear is the only creature that can be fed usefully.
    fn feed(&mut self, verb: Verb, obj: Obj) -> Phase {
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

    /// Fill the bottle or the urn from a liquid here.  (The vase is
    /// nasty.)
    fn fill(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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
    fn fill_urn(&mut self) {
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

    /// Find.  It may be carried or here; otherwise, no help.
    fn find(&mut self, verb: Verb, obj: Obj) -> Phase {
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

    /// Fly.  Only works on the hovering rug, between the cliff and the
    /// ledge.
    fn fly(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        match obj {
            None if !self.here(RUG) => {
                self.say(FLAP_ARMS);
                return Phase::ClearObj;
            }
            None if self.prop(RUG) != RUG_HOVER => {
                self.say(RUG_NOTHING2);
                return Phase::ClearObj;
            }
            None | Some(RUG) => {}
            Some(_) => {
                self.verb_message(verb);
                return Phase::ClearObj;
            }
        }
        if self.prop(RUG) != RUG_HOVER {
            self.say(RUG_NOTHING1);
            return Phase::ClearObj;
        }

        let g = &mut self.game;
        let (to, msg) = match g.loc {
            LOC_CLIFF => (LOC_LEDGE, RUG_GOES),
            LOC_LEDGE => (LOC_CLIFF, RUG_RETURNS),
            // Can't happen: the rug only hovers at the cliff or ledge.
            _ => {
                self.say(NOTHING_HAPPENS);
                return Phase::Terminate;
            }
        };
        g.oldloc2 = g.oldloc;
        g.oldloc = g.loc;
        g.newloc = to;
        self.say(msg);
        Phase::Terminate
    }

    /// Inventory.
    fn inventory(&mut self) -> Phase {
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

    /// Light the lamp or the urn.
    fn light(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let obj = match obj {
            Some(obj) => obj,
            None => {
                let lamp =
                    self.here(LAMP) && self.prop(LAMP) == LAMP_DARK && self.game.lamp_life >= 0;
                let urn = self.here(URN) && self.prop(URN) == URN_DARK;
                match (lamp, urn) {
                    (true, false) => LAMP,
                    (false, true) => URN,
                    _ => return Phase::Unknown,
                }
            }
        };
        match obj {
            URN => {
                let state = if self.prop(URN) == URN_EMPTY {
                    URN_EMPTY
                } else {
                    URN_LIT
                };
                self.state_change(URN, state);
            }
            LAMP if self.game.lamp_life < 0 => self.say(LAMP_OUT),
            LAMP => {
                self.state_change(LAMP, LAMP_BRIGHT);
                if self.game.was_dark {
                    return Phase::Top;
                }
            }
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Listen to the location and any noisy objects here.
    fn listen(&mut self) -> Phase {
        let loc = self.game.loc.info();
        let mut heard = false;
        if let Some(sound) = loc.sound {
            self.say(sound);
            if !loc.loud {
                self.say(NO_MESSAGE);
            }
            heard = true;
        }
        for o in Obj::all() {
            let objects = &self.game.objects;
            if !self.here(o)
                || o.info().sounds[0].is_none()
                || objects.is_stashed(o)
                || objects.is_not_found(o)
            {
                continue;
            }
            // The bird has a second set of songs once the player has
            // drunk the dragon's blood.
            let mut state = objects.prop(o);
            if o == BIRD {
                state += 3 * i32::from(self.game.blooded);
            }
            let magic_word = self.game.magic_word();
            self.describe_with(o, Aspect::Sound, state, true, &[magic_word.as_str().into()]);
            self.say(NO_MESSAGE);
            if o == BIRD && state == BIRD_ENDSTATE {
                self.game.objects.destroy(BIRD);
            }
            heard = true;
        }
        if !heard {
            self.say(ALL_SILENT);
        }
        Phase::ClearObj
    }

    /// Lock or unlock.  With no object, pick whatever lockable thing is
    /// here.
    fn lock(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
        let obj = match obj {
            Some(obj) => obj,
            None => {
                let candidates = [
                    (CHAIN, self.here(CHAIN)),
                    (GRATE, self.at(GRATE)),
                    (DOOR, self.at(DOOR)),
                    (OYSTER, self.here(OYSTER)),
                    (CLAM, self.here(CLAM)),
                ];
                match candidates.iter().find(|(_, present)| *present) {
                    Some(&(obj, _)) => obj,
                    None => {
                        self.say(NOTHING_LOCKED);
                        return Phase::ClearObj;
                    }
                }
            }
        };

        match obj {
            CHAIN if self.here(KEYS) => return self.chain(verb),
            GRATE if self.here(KEYS) => {
                if self.game.closing {
                    self.say(EXIT_CLOSED);
                    if !self.game.panicked {
                        self.game.closed_clock = super::turn::PANICTIME;
                    }
                    self.game.panicked = true;
                } else {
                    self.state_change(
                        GRATE,
                        if verb == LOCK {
                            GRATE_CLOSED
                        } else {
                            GRATE_OPEN
                        },
                    );
                }
            }
            CHAIN | GRATE => self.say(NO_KEYS),
            CLAM | OYSTER => self.open_shell(verb, obj),
            DOOR => self.say(if self.prop(DOOR) == DOOR_UNRUSTED {
                OK_MAN
            } else {
                RUSTY_DOOR
            }),
            CAGE => self.say(NO_LOCK),
            KEYS => self.say(CANNOT_UNLOCK),
            _ => self.verb_message(verb),
        }
        Phase::ClearObj
    }

    /// Open the clam or oyster (with the trident, and not while holding
    /// it).  The clam releases the pearl.
    fn open_shell(&mut self, verb: Verb, shell: Obj) {
        let is_clam = shell == CLAM;
        let msg = if verb == LOCK {
            HUH_MAN
        } else if self.toting(shell) {
            if is_clam { DROP_CLAM } else { DROP_OYSTER }
        } else if !self.toting(TRIDENT) {
            if is_clam { CLAM_OPENER } else { OYSTER_OPENER }
        } else if is_clam {
            let loc = self.game.loc;
            let objects = &mut self.game.objects;
            objects.destroy(CLAM);
            objects.drop(OYSTER, Place::At(loc));
            objects.drop(PEARL, Place::At(LOC_CULDESAC));
            PEARL_FALLS
        } else {
            OYSTER_OPENS
        };
        self.say(msg);
    }

    /// Pour.  With no object, or the bottle, pour what's in the bottle.
    /// Water on the plant makes it grow; oil frees the rusty door.
    fn pour(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Quit, if the player confirms.
    fn quit(&mut self) -> Flow<Phase> {
        if self.yes_or_no(REALLY_QUIT, OK_MAN, OK_MAN)? {
            return Err(self.terminate(super::score::Ending::Quit));
        }
        Ok(Phase::ClearObj)
    }

    /// Read.  With no object, read the only readable thing here.  The
    /// oyster gives a clue in the end game.
    fn read(&mut self, verb: Verb, obj: Option<Obj>, word: &str) -> Flow<Phase> {
        let obj = match obj {
            Some(obj) => obj,
            None => {
                let mut readable = Obj::all().filter(|&o| {
                    self.here(o) && o.info().texts[0].is_some() && !self.game.objects.is_stashed(o)
                });
                match (readable.next(), readable.next()) {
                    (Some(only), None) if !self.is_dark_here() => only,
                    _ => return Ok(Phase::Unknown),
                }
            }
        };

        if self.is_dark_here() {
            self.say_echo(NO_SEE, word);
        } else if obj == OYSTER {
            if !self.toting(OYSTER) || !self.game.closed {
                self.say(DONT_UNDERSTAND);
            } else if !self.game.clue_read {
                self.game.clue_read = self.yes_or_no(CLUE_QUERY, WAYOUT_CLUE, OK_MAN)?;
            } else {
                // Not really a sound, but oh well.
                self.describe(OYSTER, Aspect::Sound, 1);
            }
        } else if obj.info().texts[0].is_none() || self.game.objects.is_not_found(obj) {
            self.verb_message(verb);
        } else {
            self.describe(obj, Aspect::Writing, self.prop(obj));
        }
        Ok(Phase::ClearObj)
    }

    /// The magic word parts the reservoir (or drowns the player at its
    /// bottom).
    fn reservoir(&mut self) -> Phase {
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

    /// Rub.  Only the lit urn does anything.
    fn rub(&mut self, verb: Verb, obj: Obj) -> Phase {
        if obj == URN && self.prop(URN) == URN_LIT {
            let loc = Place::At(self.game.loc);
            let objects = &mut self.game.objects;
            objects.destroy(URN);
            objects.drop(AMBER, loc);
            objects.set_prop(AMBER, AMBER_IN_ROCK);
            self.game.treasures_unseen -= 1;
            self.game.objects.drop(CAVITY, loc);
            self.say(URN_GENIES);
        } else if obj != LAMP {
            self.say(PECULIAR_NOTHING);
        } else {
            self.verb_message(verb);
        }
        Phase::ClearObj
    }

    /// Say.  Magic words work as if said alone; anything else is echoed.
    fn say_word(&mut self, command: &Command) -> Phase {
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

    /// Throw.  The axe attacks; a treasure pays the troll; food goes to
    /// the bear; anything else is dropped.
    fn throw(&mut self, verb: Verb, obj: Obj) -> Flow<Phase> {
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
    fn axe_lands(&mut self, msg: Msg) -> Phase {
        self.say(msg);
        self.game.objects.drop(AXE, Place::At(self.game.loc));
        Phase::Move
    }

    /// Wake.  Only the sleeping dwarves, with dire consequences.
    fn wake(&mut self, verb: Verb, obj: Obj) -> Phase {
        if obj != DWARF || !self.game.closed {
            self.verb_message(verb);
            return Phase::ClearObj;
        }
        self.say(PROD_DWARF);
        Phase::DwarfWake
    }

    /// Reseed the random number generator (for testing).
    fn reseed_command(&mut self, verb: Verb, arg: &str) -> Phase {
        let seed = numeric_arg(arg);
        self.speak(verb.info().message, &[seed.into()]);
        self.set_seed(seed);
        self.game.turns -= 1;
        Phase::Top
    }

    /// Burn lamp turns (for testing).
    fn waste(&mut self, verb: Verb, arg: &str) -> Phase {
        self.game.lamp_life = self.game.lamp_life.wrapping_sub(numeric_arg(arg));
        self.speak(verb.info().message, &[self.game.lamp_life.into()]);
        Phase::Top
    }

    /// Wave.  The rod makes a bridge over the fissure, or makes the bird
    /// drop the jade necklace.
    fn wave(&mut self, verb: Verb, obj: Obj) -> Phase {
        let useful = obj == ROD
            && self.toting(obj)
            && (self.here(BIRD) || (!self.game.closing && self.at(FISSURE)));
        if !useful {
            let not_carried = !self.toting(obj) && (obj != ROD || !self.toting(ROD2));
            self.speak(
                if not_carried {
                    ARENT_CARRYING.text()
                } else {
                    verb.info().message
                },
                &[],
            );
            return Phase::ClearObj;
        }

        if self.prop(BIRD) == BIRD_UNCAGED
            && self.game.objects.place(STEPS) == Place::At(self.game.loc)
            && self.game.objects.is_not_found(JADE)
        {
            self.game.objects.drop(JADE, Place::At(self.game.loc));
            self.game.objects.set_found(JADE);
            self.game.treasures_unseen -= 1;
            self.say(NECKLACE_FLY);
            return Phase::ClearObj;
        }

        let fly = if self.prop(BIRD) == BIRD_CAGED {
            CAGE_FLY
        } else {
            FREE_FLY
        };
        if self.game.closed {
            self.say(fly);
            return Phase::DwarfWake;
        }
        if self.game.closing || !self.at(FISSURE) {
            self.say(fly);
            return Phase::ClearObj;
        }
        if self.here(BIRD) {
            self.say(fly);
        }
        let state = if self.prop(FISSURE) == BRIDGED {
            UNBRIDGED
        } else {
            BRIDGED
        };
        self.state_change(FISSURE, state);
        Phase::ClearObj
    }

    /// Save.  Saving costs points, so it can't be used to retry battles
    /// or to start over after learning the magic word.
    fn suspend(&mut self) -> Flow<Phase> {
        if !matches!(self.config.saves, SaveMode::Manual) {
            self.say(SAVERESUME_DISABLED);
            return Ok(Phase::Top);
        }
        self.say(SUSPEND_WARNING);
        if !self.yes_or_no(THIS_ACCEPTABLE, OK_MAN, OK_MAN)? {
            return Ok(Phase::ClearObj);
        }
        self.game.save_penalty += 5;

        let Some((mut file, _)) = self.prompt_for_file(|name| File::create(name)) else {
            return Ok(Phase::Top);
        };
        // There's no good way to report a failed write; the player was
        // told it was saved either way in the original too.
        let _ = file.write_all(&self.save_bytes());
        self.say(RESUME_HELP);
        Err(GameOver::Saved)
    }

    /// Resume a saved game, abandoning this one.
    fn resume(&mut self) -> Flow<Phase> {
        if !matches!(self.config.saves, SaveMode::Manual) {
            self.say(SAVERESUME_DISABLED);
            return Ok(Phase::Top);
        }
        if self.game.loc != LOC_START || self.game.visits[LOC_START.index()] != 1 {
            self.say(RESUME_ABANDON);
            if !self.yes_or_no(THIS_ACCEPTABLE, OK_MAN, OK_MAN)? {
                return Ok(Phase::ClearObj);
            }
        }
        let Some((mut file, _)) = self.prompt_for_file(|name| File::open(name)) else {
            return Ok(Phase::Top);
        };
        let mut bytes = Vec::new();
        // An unreadable file restores as an empty one: "not a save".
        let _ = file.read_to_end(&mut bytes);
        self.restore(&bytes)?;
        Ok(Phase::Top)
    }

    /// Ask for a file name until `open` succeeds with one.  `None` if the
    /// player gives no name or input runs out.
    fn prompt_for_file(
        &mut self,
        open: impl Fn(&str) -> std::io::Result<File>,
    ) -> Option<(File, String)> {
        loop {
            let line = self.console.read_line("\nFile name: ")?;
            let name = line.trim_matches(|c: char| c.is_ascii_whitespace());
            if name.is_empty() {
                return None;
            }
            match open(name) {
                Ok(file) => return Some((file, name.to_string())),
                Err(_) => self
                    .console
                    .print(&format!("Can't open file {name}, try again.\n")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::numeric_arg;

    #[test]
    fn numeric_arguments_follow_strtol() {
        assert_eq!(numeric_arg("42"), 42);
        assert_eq!(numeric_arg("-7"), -7);
        assert_eq!(numeric_arg(""), 0);
        assert_eq!(numeric_arg("lantern"), 0);
        assert_eq!(numeric_arg("12abc"), 12);
        // Saturates to i64::MAX, then truncates to 32 bits.
        assert_eq!(numeric_arg("99999999999999999999"), -1);
        assert_eq!(numeric_arg("4294967296"), 0);
    }
}

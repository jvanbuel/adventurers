// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! What each verb does.
//!
//! This module parses a command into a verb and an object and dispatches
//! to the handlers, which are grouped by what they act on:
//!
//! * `creatures`: attack, throw, feed, wake
//! * `possessions`: take, drop, find, inventory
//! * `food_and_drink`: eat, drink, fill, pour
//! * `devices`: light, extinguish, lock, unlock, break, rub, read, fly,
//!   wave, listen
//! * `magic`: FEE FIE FOE FOO, say, the magic word, blast
//! * `session`: brief, quit, save, resume, seed, waste
//!
//! Handlers that may be used without an object take `Option<Obj>`,
//! where `None` means no object was given and the handler should work
//! out what was meant.  Handlers that can end the game return a
//! [`Flow`].

mod creatures;
mod devices;
mod food_and_drink;
mod magic;
mod possessions;
mod session;

use super::turn::{Command, Phase, SpeechPart};
use super::{Console, Engine, Flow};
use crate::dungeon::*;
use crate::parser::WordKind;
use crate::state::Knife;

impl<C: Console> Engine<C> {
    /// Print the verb's default response.
    pub(super) fn verb_message(&mut self, verb: Verb) {
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
    pub(super) fn intransitive(&mut self, command: &Command) -> Flow<Phase> {
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
    pub(super) fn transitive(&mut self, command: Command) -> Flow<Phase> {
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
}

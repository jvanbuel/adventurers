// SPDX-FileCopyrightText: (C) 1977, 2005 Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Operating things: lamps and urns, locks, the rug, the rod, and
//! reading, rubbing, breaking and listening.

use super::super::turn::Phase;
use super::super::{Aspect, Console, Engine, Flow};
use crate::dungeon::*;
use crate::objects::Place;

impl<C: Console> Engine<C> {
    /// Light the lamp or the urn.
    pub(super) fn light(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Extinguish the lamp or the urn.
    pub(super) fn extinguish(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Lock or unlock.  With no object, pick whatever lockable thing is
    /// here.
    pub(super) fn lock(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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
                        self.game.closed_clock = super::super::turn::PANICTIME;
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
    pub(super) fn open_shell(&mut self, verb: Verb, shell: Obj) {
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

    /// Lock or unlock the bear's chain.
    pub(super) fn chain(&mut self, verb: Verb) -> Phase {
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

    /// Break.  Works on the mirror in the repository and on the vase.
    pub(super) fn smash(&mut self, verb: Verb, obj: Obj) -> Phase {
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

    /// Rub.  Only the lit urn does anything.
    pub(super) fn rub(&mut self, verb: Verb, obj: Obj) -> Phase {
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

    /// Read.  With no object, read the only readable thing here.  The
    /// oyster gives a clue in the end game.
    pub(super) fn read(&mut self, verb: Verb, obj: Option<Obj>, word: &str) -> Flow<Phase> {
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

    /// Fly.  Only works on the hovering rug, between the cliff and the
    /// ledge.
    pub(super) fn fly(&mut self, verb: Verb, obj: Option<Obj>) -> Phase {
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

    /// Wave.  The rod makes a bridge over the fissure, or makes the bird
    /// drop the jade necklace.
    pub(super) fn wave(&mut self, verb: Verb, obj: Obj) -> Phase {
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

    /// Listen to the location and any noisy objects here.
    pub(super) fn listen(&mut self) -> Phase {
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
}

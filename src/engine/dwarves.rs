// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The dwarves and the pirate.
//!
//! The last "dwarf" is the pirate.  He moves like the others but
//! steals treasure instead of throwing knives.

use super::{Console, Engine};
use crate::dungeon::*;
use crate::objects::Place;
use crate::state::Knife;

/// The pirate's index among the dwarves.
pub(super) const PIRATE: usize = NDWARVES;
/// Where a dwarf goes instead if it would start on top of the player.
const DALTLC: Loc = LOC_NUGGET;

impl<C: Console> Engine<C> {
    /// Move the dwarves.  Returns false if the player was killed.
    pub(super) fn dwarves_move(&mut self) -> bool {
        let g = &self.game;
        // Dwarves don't follow the player into a pit or a wall, nor
        // anywhere forbidden to the pirate (beyond the troll bridge, so
        // the pirate can't steal the return toll and dwarves can't meet
        // the bear; also the maze dead ends, where they wait outside).
        if g.loc == LOC_NOWHERE || Self::forced(g.loc) || g.newloc.has(Cond::NoArrr) {
            return true;
        }

        // Activate the dwarves once the player reaches the Hall of Mists.
        if g.dwarf_activity == 0 {
            if g.loc.has(Cond::Deep) {
                self.game.dwarf_activity = 1;
            }
            return true;
        }

        if g.dwarf_activity == 1 {
            self.meet_first_dwarf();
            return true;
        }

        self.dwarves_attack()
    }

    /// On first meeting, 0, 1 or 2 of the five dwarves are killed off,
    /// and any survivor standing on the player is moved aside.
    fn meet_first_dwarf(&mut self) {
        let loc = self.game.loc;
        if !loc.has(Cond::Deep) || (self.pct(95) && (!loc.has(Cond::NoBack) || self.pct(85))) {
            return;
        }
        self.game.dwarf_activity = 2;
        for _ in 0..2 {
            let j = 1 + self.randrange(NDWARVES as i32 - 1) as usize;
            if self.pct(50) {
                self.game.dwarves[j].loc = LOC_NOWHERE;
            }
        }
        for d in &mut self.game.dwarves[1..NDWARVES] {
            if d.loc == loc {
                d.loc = DALTLC;
            }
            d.oldloc = d.loc;
        }
        self.say(DWARF_RAN);
        self.game.objects.drop(AXE, Place::At(loc));
    }

    /// Move each dwarf at random, except that one who has seen the
    /// player follows them.  Dwarves stay deep inside, and don't back up
    /// unless there's no alternative.  A dwarf that doesn't have to move
    /// attacks.  Returns false if the player was killed.
    fn dwarves_attack(&mut self) -> bool {
        self.game.dwarves_here = 0;
        let mut attacks = 0;
        let mut hits = 0;
        for i in 1..=NDWARVES {
            if self.game.dwarves[i].loc == LOC_NOWHERE {
                continue;
            }
            let next = self.pick_dwarf_move(i);
            let loc = self.game.loc;
            let d = &mut self.game.dwarves[i];
            d.oldloc = d.loc;
            d.loc = next;
            d.seen = (d.seen && loc.has(Cond::Deep)) || d.loc == loc || d.oldloc == loc;
            if !d.seen {
                continue;
            }
            d.loc = loc;
            if self.pirate_acts(i) {
                continue;
            }
            // This threatening little dwarf is in the room with the player!
            self.game.dwarves_here += 1;
            if self.game.dwarves[i].oldloc == loc {
                attacks += 1;
                if self.game.knife != Knife::Explained {
                    self.game.knife = Knife::ThrownAt(loc);
                }
                if self.randrange(1000) < 95 * (self.game.dwarf_activity - 2) {
                    hits += 1;
                }
            }
        }

        let here = self.game.dwarves_here;
        if here == 0 {
            return true;
        }
        self.say_with(
            if here == 1 { DWARF_SINGLE } else { DWARF_PACK },
            &[here.into()],
        );
        if attacks == 0 {
            return true;
        }
        if self.game.dwarf_activity == 2 {
            self.game.dwarf_activity = 3;
        }
        if attacks > 1 {
            self.say_with(THROWN_KNIVES, &[attacks.into()]);
            let msg = match hits {
                0 => NONE_HIT,
                1 => ONE_HIT,
                _ => MULTIPLE_HITS,
            };
            self.say_with(msg, &[hits.into()]);
        } else {
            self.say(KNIFE_THROWN);
            self.say(if hits == 0 { MISSES_YOU } else { GETS_YOU });
        }
        if hits == 0 {
            return true;
        }
        self.game.oldloc2 = self.game.loc;
        false
    }

    /// Choose where dwarf `i` wanders to.
    fn pick_dwarf_move(&mut self, i: usize) -> Loc {
        let dwarf = self.game.dwarves[i];
        // Candidate locations, 1-based as in the original.
        let mut tk = [LOC_NOWHERE; 21];
        let mut j = 1;
        for rule in dwarf.loc.info().travel {
            // The original leaves each rule's destination in newloc as
            // it scans; that value can end up in a save file.
            self.game.newloc = Loc::from_raw(rule.dest.raw());
            let Dest::Goto(to) = rule.dest else { continue };
            let excluded = !to.has(Cond::Deep)
                || to == dwarf.oldloc
                || (j > 1 && to == tk[j - 1])
                || j >= tk.len() - 1
                || to == dwarf.loc
                || Self::forced(to)
                || (i == PIRATE && to.has(Cond::NoArrr))
                || rule.nodwarves;
            if !excluded {
                tk[j] = to;
                j += 1;
            }
        }
        tk[j] = dwarf.oldloc;
        if j >= 2 {
            j -= 1;
        }
        tk[1 + self.randrange(j as i32) as usize]
    }

    /// If dwarf `i` is the pirate, he has spotted the player: rob them,
    /// or lurk.  Returns true if it was the pirate.
    fn pirate_acts(&mut self, i: usize) -> bool {
        if i != PIRATE {
            return false;
        }
        // The pirate leaves the player alone once the chest is found.
        let loc = self.game.loc;
        if loc == self.game.chest_loc || !self.game.objects.is_not_found(CHEST) {
            return true;
        }
        // He won't take the pyramid from the plover room or the dark
        // room (too easy!).
        let guarded =
            |o: Obj| o == PYRAMID && (loc == PYRAMID.info().plac || loc == EMERALD.info().plac);
        let treasures = || Obj::all().filter(|o| o.is_treasure() && !guarded(*o));

        let snarfed = treasures().filter(|&o| self.here(o)).count();
        let rob_player = treasures().any(|o| self.toting(o));
        let mut move_chest = rob_player;
        // Make sure the chest turns up before the player finds the last
        // treasure.
        if self.game.treasures_unseen == 1
            && snarfed == 0
            && self.game.objects.place(CHEST) == Place::NOWHERE
            && self.here(LAMP)
            && self.prop(LAMP) == LAMP_BRIGHT
        {
            self.say(PIRATE_SPOTTED);
            move_chest = true;
        }
        // Move the chest before the robbery, so it is listed last at the
        // maze location.
        if move_chest {
            let (chest_loc, message_loc) = (self.game.chest_loc, self.game.message_loc);
            self.game.objects.move_to(CHEST, Place::At(chest_loc));
            self.game.objects.move_to(MESSAG, Place::At(message_loc));
            let pirate = &mut self.game.dwarves[PIRATE];
            pirate.loc = chest_loc;
            pirate.oldloc = chest_loc;
            pirate.seen = false;
        } else {
            // A hint of the pirate's presence even if the chest doesn't move.
            let pirate = self.game.dwarves[PIRATE];
            if pirate.oldloc != pirate.loc && self.pct(20) {
                self.say(PIRATE_RUSTLES);
            }
        }
        if rob_player {
            self.say(PIRATE_POUNCES);
            let chest = Place::At(self.game.chest_loc);
            for o in treasures().collect::<Vec<_>>() {
                let objects = &mut self.game.objects;
                if objects.is_at(o, loc) && objects.fixed(o) == Fixed::Free {
                    objects.carry(o, loc);
                }
                if objects.is_carried(o) {
                    objects.drop(o, chest);
                }
            }
        }
        true
    }
}

// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Initialisation.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::advent::*;
use crate::dungeon::*;

/// A non-negative pseudo-random seed in the range of C's rand().
fn random_seed() -> i32 {
    let mut h = RandomState::new().build_hasher();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    h.write_u128(now.as_nanos());
    h.write_u32(std::process::id());
    (h.finish() & 0x7fff_ffff) as i32
}

impl Advent {
    /// Set up a new game.  Returns the random seed used.
    pub fn initialise(&mut self) -> i32 {
        if self.settings.oldstyle {
            println!("Initialising...");
        }

        let seedval = random_seed();
        self.set_seed(seedval);

        for i in 1..=NDWARVES {
            self.game.dwarves[i].loc = DWARFLOCS[i - 1];
        }

        for i in 1..NOBJECTS {
            self.game.objects[i].place = LOC_NOWHERE;
        }

        for i in 1..NLOCATIONS {
            if !(LOCATIONS[i].big.is_none() || TKEY[i] == 0) {
                let k = TKEY[i] as usize;
                if TRAVEL[k].motion == HERE {
                    self.conditions[i] |= 1 << COND_FORCED;
                }
            }
        }

        //  Set up the game.locs atloc and game.link arrays.
        //  We'll use the DROP subroutine, which prefaces new objects on the
        //  lists.  Since we want things in the other order, we'll run the
        //  loop backwards.  If the object is in two locs, we drop it twice.
        //  Also, since two-placed objects are typically best described
        //  last, we'll drop them first.
        for i in (1..NOBJECTS).rev() {
            if OBJECTS[i].fixd > 0 {
                self.drop((i + NOBJECTS) as i32, OBJECTS[i].fixd);
                self.drop(i as i32, OBJECTS[i].plac);
            }
        }

        for k in (1..NOBJECTS).rev() {
            self.game.objects[k].fixed = OBJECTS[k].fixd;
            if OBJECTS[k].plac != 0 && OBJECTS[k].fixd <= 0 {
                self.drop(k as i32, OBJECTS[k].plac);
            }
        }

        //  Treasure props are initially STATE_NOTFOUND, and are set to
        //  STATE_FOUND the first time they are described.  game.tally
        //  keeps track of how many are not yet found, so we know when to
        //  close the cave.
        for object in 1..NOBJECTS {
            if OBJECTS[object].is_treasure {
                self.game.tally += 1;
                if OBJECTS[object].inventory.is_some() {
                    self.object_set_not_found(object as i32);
                }
            } else {
                self.object_set_found(object as i32);
            }
        }
        self.game.conds = setbit(COND_HBASE);

        seedval
    }
}

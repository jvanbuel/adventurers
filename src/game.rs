// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The main game loop: movement, dwarves, hints, closing, and command
//! dispatch (the port of main.c).

use crate::advent::*;
use crate::dungeon::*;
use crate::misc::bug;

impl Advent {
    ///  Check if this loc is eligible for any hints.  If been here long
    ///  enough, display.  Ignore "HINTS" < 4 (special stuff, see
    ///  database notes).
    fn checkhints(&mut self) {
        if self.conditions[self.game.loc as usize] < self.game.conds {
            return;
        }
        for hint in 0..NHINTS {
            if self.game.hints[hint].used {
                continue;
            }
            if !self.cndbit(self.game.loc, hint as i32 + 1 + COND_HBASE) {
                self.game.hints[hint].lc = -1;
            }
            self.game.hints[hint].lc += 1;
            //  Come here if he's been long enough at required loc(s) for
            //  some unused hint.
            if self.game.hints[hint].lc < HINTS[hint].turns {
                continue;
            }
            let g = &self.game;
            let empty_here = |l: i32| g.locs[l as usize].atloc == NO_OBJECT;
            let show = match hint {
                0 => {
                    // cave
                    if self.prop(GRATE) == GRATE_CLOSED && !self.here(KEYS) {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                1 => {
                    // bird
                    if self.place(BIRD) == g.loc && self.toting(ROD) && g.oldobj == BIRD {
                        true
                    } else {
                        return;
                    }
                }
                2 => {
                    // snake
                    if self.here(SNAKE) && !self.here(BIRD) {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                3 => {
                    // maze
                    if empty_here(g.loc)
                        && empty_here(g.oldloc)
                        && empty_here(g.oldlc2)
                        && g.holdng > 1
                    {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                4 => {
                    // dark
                    if !self.object_is_notfound(EMERALD) && self.object_is_notfound(PYRAMID) {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                5 => true, // witt
                6 => {
                    // urn
                    if g.dflag == 0 {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                7 => {
                    // woods
                    if empty_here(g.loc) && empty_here(g.oldloc) && empty_here(g.oldlc2) {
                        true
                    } else {
                        return;
                    }
                }
                8 => {
                    // ogre
                    let i = self.atdwrf(g.loc);
                    if i < 0 {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                    if self.here(OGRE) && i == 0 {
                        true
                    } else {
                        return;
                    }
                }
                9 => {
                    // jade
                    if g.tally == 1
                        && (self.object_is_stashed(JADE) || self.object_is_notfound(JADE))
                    {
                        true
                    } else {
                        self.game.hints[hint].lc = 0;
                        return;
                    }
                }
                _ => bug(BugType::HintNumberExceedsGotoList),
            };
            debug_assert!(show);

            // Fall through to hint display
            self.game.hints[hint].lc = 0;
            if !self.yes_or_no(
                HINTS[hint].question,
                ARBITRARY_MESSAGES[NO_MESSAGE as usize],
                ARBITRARY_MESSAGES[OK_MAN as usize],
            ) {
                return;
            }
            let penalty = HINTS[hint].penalty;
            self.rspeak(HINT_COST, &[Arg::I(penalty), Arg::I(penalty)]);
            self.game.hints[hint].used = self.yes_or_no(
                ARBITRARY_MESSAGES[WANT_HINT as usize],
                HINTS[hint].hint,
                ARBITRARY_MESSAGES[OK_MAN as usize],
            );
            if self.game.hints[hint].used && self.game.limit > WARNTIME {
                self.game.limit += WARNTIME * penalty;
            }
        }
    }

    fn spotted_by_pirate(&mut self, i: usize) -> bool {
        if i != PIRATE {
            return false;
        }

        //  The pirate's spotted him.  Pirate leaves him alone once we've
        //  found chest.  K counts if a treasure is here.  If not, and
        //  tally=1 for an unseen chest, let the pirate be spotted.  Note
        //  that place[CHEST] = LOC_NOWHERE might mean that he's thrown it
        //  to the troll, but in that case he's seen the chest
        //  OBJECT_IS_FOUND(CHEST) == true.
        if self.game.loc == self.game.chloc || !self.object_is_notfound(CHEST) {
            return true;
        }
        let mut snarfed = 0;
        let mut movechest = false;
        let mut robplayer = false;
        let pyramid_protected = |s: &Self| {
            s.game.loc == OBJECTS[PYRAMID as usize].plac
                || s.game.loc == OBJECTS[EMERALD as usize].plac
        };
        for treasure in 1..NOBJECTS as i32 {
            if !OBJECTS[treasure as usize].is_treasure {
                continue;
            }
            // Pirate won't take pyramid from plover room or dark room
            // (too easy!).
            if treasure == PYRAMID && pyramid_protected(self) {
                continue;
            }
            if self.toting(treasure) || self.here(treasure) {
                snarfed += 1;
            }
            if self.toting(treasure) {
                movechest = true;
                robplayer = true;
            }
        }
        // Force chest placement before player finds last treasure
        if self.game.tally == 1
            && snarfed == 0
            && self.place(CHEST) == LOC_NOWHERE
            && self.here(LAMP)
            && self.prop(LAMP) == LAMP_BRIGHT
        {
            self.rspeak(PIRATE_SPOTTED, &[]);
            movechest = true;
        }
        // Do things in this order (chest move before robbery) so chest is
        // listed last at the maze location.
        if movechest {
            self.move_obj(CHEST, self.game.chloc);
            self.move_obj(MESSAG, self.game.chloc2);
            self.game.dwarves[PIRATE].loc = self.game.chloc;
            self.game.dwarves[PIRATE].oldloc = self.game.chloc;
            self.game.dwarves[PIRATE].seen = false;
        } else {
            // You might get a hint of the pirate's presence even if the
            // chest doesn't move...
            if self.game.dwarves[PIRATE].oldloc != self.game.dwarves[PIRATE].loc && self.pct(20) {
                self.rspeak(PIRATE_RUSTLES, &[]);
            }
        }
        if robplayer {
            self.rspeak(PIRATE_POUNCES, &[]);
            for treasure in 1..NOBJECTS as i32 {
                if !OBJECTS[treasure as usize].is_treasure {
                    continue;
                }
                if !(treasure == PYRAMID && pyramid_protected(self)) {
                    if self.at(treasure) && self.fixed(treasure) == IS_FREE {
                        self.carry(treasure, self.game.loc);
                    }
                    if self.toting(treasure) {
                        self.drop(treasure, self.game.chloc);
                    }
                }
            }
        }

        true
    }

    /// Dwarves move.  Return true if player survives, false if he dies.
    fn dwarfmove(&mut self) -> bool {
        let mut tk = [0i32; 21];

        //  First off, don't let the dwarves follow him into a pit or a
        //  wall.  Activate the whole mess the first time he gets as far
        //  as the Hall of Mists (what INDEEP() tests).  If game.newloc
        //  is forbidden to pirate (in particular, if it's beyond the
        //  troll bridge), bypass dwarf stuff.  That way pirate can't
        //  steal return toll, and dwarves can't meet the bear.  Also
        //  means dwarves won't follow him into dead end in maze, but
        //  c'est la vie.  They'll wait for him outside the dead end.
        if self.game.loc == LOC_NOWHERE
            || self.forced(self.game.loc)
            || self.cndbit(self.game.newloc, COND_NOARRR)
        {
            return true;
        }

        // Dwarf activity level ratchets up
        if self.game.dflag == 0 {
            if self.indeep(self.game.loc) {
                self.game.dflag = 1;
            }
            return true;
        }

        //  When we encounter the first dwarf, we kill 0, 1, or 2 of
        //  the 5 dwarves.  If any of the survivors is at game.loc,
        //  replace him with the alternate.
        if self.game.dflag == 1 {
            if !self.indeep(self.game.loc)
                || (self.pct(95) && (!self.cndbit(self.game.loc, COND_NOBACK) || self.pct(85)))
            {
                return true;
            }
            self.game.dflag = 2;
            for _ in 1..=2 {
                let j = 1 + self.randrange(NDWARVES as i32 - 1) as usize;
                if self.pct(50) {
                    self.game.dwarves[j].loc = 0;
                }
            }

            // Alternate initial loc for dwarf, in case one of them
            // starts out on top of the adventurer.
            for i in 1..NDWARVES {
                if self.game.dwarves[i].loc == self.game.loc {
                    self.game.dwarves[i].loc = DALTLC;
                }
                self.game.dwarves[i].oldloc = self.game.dwarves[i].loc;
            }
            self.rspeak(DWARF_RAN, &[]);
            self.drop(AXE, self.game.loc);
            return true;
        }

        //  Things are in full swing.  Move each dwarf at random, except
        //  if he's seen us he sticks with us.  Dwarves stay deep inside.
        //  If wandering at random, they don't back up unless there's no
        //  alternative.  If they don't have to move, they attack.  And,
        //  of course, dead dwarves don't do much of anything.
        self.game.dtotal = 0;
        let mut attack = 0;
        let mut stick = 0;
        for i in 1..=NDWARVES {
            if self.game.dwarves[i].loc == 0 {
                continue;
            }
            // Fill tk array with all the places this dwarf might go.
            let mut j: usize = 1;
            let mut kk = TKEY[self.game.dwarves[i].loc as usize] as usize;
            if kk != 0 {
                loop {
                    let t = &TRAVEL[kk];
                    self.game.newloc = t.destval;
                    let nl = self.game.newloc;
                    // Have we avoided a dwarf encounter?
                    let skip = t.desttype != DestType::Goto
                        || !self.indeep(nl)
                        || nl == self.game.dwarves[i].oldloc
                        || (j > 1 && nl == tk[j - 1])
                        || j >= tk.len() - 1
                        || nl == self.game.dwarves[i].loc
                        || self.forced(nl)
                        || (i == PIRATE && self.cndbit(nl, COND_NOARRR))
                        || t.nodwarves;
                    if !skip {
                        tk[j] = nl;
                        j += 1;
                    }
                    let stop = TRAVEL[kk].stop;
                    kk += 1;
                    if stop {
                        break;
                    }
                }
            }
            tk[j] = self.game.dwarves[i].oldloc;
            if j >= 2 {
                j -= 1;
            }
            j = 1 + self.randrange(j as i32) as usize;
            self.game.dwarves[i].oldloc = self.game.dwarves[i].loc;
            self.game.dwarves[i].loc = tk[j];
            let d = self.game.dwarves[i];
            self.game.dwarves[i].seen = (d.seen && self.indeep(self.game.loc))
                || (d.loc == self.game.loc || d.oldloc == self.game.loc);
            if !self.game.dwarves[i].seen {
                continue;
            }
            self.game.dwarves[i].loc = self.game.loc;
            if self.spotted_by_pirate(i) {
                continue;
            }
            // This threatening little dwarf is in the room with him!
            self.game.dtotal += 1;
            if self.game.dwarves[i].oldloc == self.game.dwarves[i].loc {
                attack += 1;
                if self.game.knfloc >= LOC_NOWHERE {
                    self.game.knfloc = self.game.loc;
                }
                if self.randrange(1000) < 95 * (self.game.dflag - 2) {
                    stick += 1;
                }
            }
        }

        // Now we know what's happening.  Let's tell the poor sucker
        // about it.
        if self.game.dtotal == 0 {
            return true;
        }
        let msg = if self.game.dtotal == 1 {
            DWARF_SINGLE
        } else {
            DWARF_PACK
        };
        self.rspeak(msg, &[Arg::I(self.game.dtotal)]);
        if attack == 0 {
            return true;
        }
        if self.game.dflag == 2 {
            self.game.dflag = 3;
        }
        if attack > 1 {
            self.rspeak(THROWN_KNIVES, &[Arg::I(attack)]);
            let msg = if stick > 1 {
                MULTIPLE_HITS
            } else if stick == 1 {
                ONE_HIT
            } else {
                NONE_HIT
            };
            self.rspeak(msg, &[Arg::I(stick)]);
        } else {
            self.rspeak(KNIFE_THROWN, &[]);
            self.rspeak(if stick != 0 { GETS_YOU } else { MISSES_YOU }, &[]);
        }
        if stick == 0 {
            return true;
        }
        self.game.oldlc2 = self.game.loc;
        false
    }

    ///  "You're dead, Jim."
    ///
    ///  If the current loc is zero, it means the clown got himself
    ///  killed.  We'll allow this maxdie times.  NDEATHS is
    ///  automatically set based on the number of snide messages
    ///  available.  Each death results in a message (obituaries[n])
    ///  which offers reincarnation; if accepted, this results in
    ///  message obituaries[0], obituaries[2], etc.  The last time, if
    ///  he wants another chance, he gets a snide remark as we exit.
    ///  When reincarnated, all objects being carried get dropped at
    ///  game.oldlc2 (presumably the last place prior to being killed)
    ///  without change of props.  The loop runs backwards to assure
    ///  that the bird is dropped before the cage.  The lamp is a
    ///  special case (it wouldn't do to leave it in the cave). It is
    ///  turned off and left outside the building (only if he was
    ///  carrying it, of course).  He himself is left inside the
    ///  building (and heaven help him if he tries to xyzzy back into
    ///  the cave without the lamp!).  game.oldloc is zapped so he
    ///  can't just "retreat".
    fn croak(&mut self) {
        // Okay, he's dead.  Let's get on with it.
        let obit = &OBITUARIES[self.game.numdie as usize];
        let (query, yes_response) = (obit.query, obit.yes_response);

        self.game.numdie += 1;

        if self.game.closng {
            // He died during closing time.  No resurrection.  Tally up
            // a death and exit.
            self.rspeak(DEATH_CLOSING, &[]);
            self.terminate(Termination::EndGame);
        } else if !self.yes_or_no(query, yes_response, ARBITRARY_MESSAGES[OK_MAN as usize])
            || self.game.numdie == NDEATHS as i32
        {
            // Player is asked if he wants to try again. If not, or if
            // he's already used all of his lives, we end the game.
            self.terminate(Termination::EndGame);
        } else {
            // If player wishes to continue, we empty the liquids in the
            // user's inventory, turn off the lamp, and drop all items
            // where he died.
            self.set_place(WATER, LOC_NOWHERE);
            self.set_place(OIL, LOC_NOWHERE);
            if self.toting(LAMP) {
                self.set_prop(LAMP, LAMP_DARK);
            }
            for j in 1..NOBJECTS as i32 {
                let i = NOBJECTS as i32 - j;
                if self.toting(i) {
                    // Always leave lamp where it's accessible aboveground
                    let to = if i == LAMP {
                        LOC_START
                    } else {
                        self.game.oldlc2
                    };
                    self.drop(i, to);
                }
            }
            self.game.oldloc = LOC_BUILDING;
            self.game.loc = LOC_BUILDING;
            self.game.newloc = LOC_BUILDING;
        }
    }

    /// Describe the location to the user.
    fn describe_location(&mut self) {
        let loc = &LOCATIONS[self.game.loc as usize];
        let mut msg = loc.small;

        if self.game.locs[self.game.loc as usize].abbrev % self.game.abbnum == 0 || msg.is_none() {
            msg = loc.big;
        }

        if !self.forced(self.game.loc) && self.is_dark_here() {
            msg = ARBITRARY_MESSAGES[PITCH_DARK as usize];
        }

        if self.toting(BEAR) {
            self.rspeak(TAME_BEAR, &[]);
        }

        self.speak(msg, &[]);

        if self.game.loc == LOC_Y2 && self.pct(25) && !self.game.closng {
            self.rspeak(SAYS_PLUGH, &[]);
        }
    }

    /// Skip forward past entries equal to `te` (for purposes of skip
    /// after a failed condition).
    fn skip_equal_travel(te: usize) -> usize {
        let traveleq = |a: &TravelOp, b: &TravelOp| {
            a.condtype == b.condtype
                && a.condarg1 == b.condarg1
                && a.condarg2 == b.condarg2
                && a.desttype == b.desttype
                && a.destval == b.destval
        };
        let mut te_tmp = te;
        loop {
            if TRAVEL[te_tmp].stop {
                bug(BugType::ConditionalTravelEntryWithNoAlteration);
            }
            te_tmp += 1;
            if !traveleq(&TRAVEL[te], &TRAVEL[te_tmp]) {
                return te_tmp;
            }
        }
    }

    ///  Given the current location in "game.loc", and a motion verb
    ///  number in "motion", put the new location in "game.newloc".  The
    ///  current loc is saved in "game.oldloc" in case he wants to
    ///  retreat.  The current game.oldloc is saved in game.oldlc2, in
    ///  case he dies.  (if he does, game.newloc will be limbo, and
    ///  game.oldloc will be what killed him, so we need game.oldlc2,
    ///  which is the last place he was safe.)
    fn playermove(&mut self, mut motion: i32) {
        let mut travel_entry = TKEY[self.game.loc as usize] as usize;
        self.game.newloc = self.game.loc;
        if travel_entry == 0 {
            bug(BugType::LocationHasNoTravelEntries);
        }
        if motion == NUL {
            return;
        } else if motion == BACK {
            //  Handle "go back".  Look for verb which goes from game.loc
            //  to game.oldloc, or to game.oldlc2 If game.oldloc has
            //  forced-motion.  te_tmp saves entry -> forced loc ->
            //  previous loc.
            motion = self.game.oldloc;
            if self.forced(motion) {
                motion = self.game.oldlc2;
            }
            self.game.oldlc2 = self.game.oldloc;
            self.game.oldloc = self.game.loc;
            if self.cndbit(self.game.loc, COND_NOBACK) {
                self.rspeak(TWIST_TURN, &[]);
                return;
            }
            if motion == self.game.loc {
                self.rspeak(FORGOT_PATH, &[]);
                return;
            }

            let mut te_tmp = 0;
            loop {
                let desttype = TRAVEL[travel_entry].desttype;
                let scratchloc = TRAVEL[travel_entry].destval;
                if desttype != DestType::Goto || scratchloc != motion {
                    if desttype == DestType::Goto
                        && self.forced(scratchloc)
                        && TRAVEL[TKEY[scratchloc as usize] as usize].destval == motion
                    {
                        te_tmp = travel_entry;
                    }
                    if !TRAVEL[travel_entry].stop {
                        travel_entry += 1; // go to next travel entry for this location
                        continue;
                    }
                    // we've reached the end of travel entries for game.loc
                    travel_entry = te_tmp;
                    if travel_entry == 0 {
                        self.rspeak(NOT_CONNECTED, &[]);
                        return;
                    }
                }

                motion = TRAVEL[travel_entry].motion;
                travel_entry = TKEY[self.game.loc as usize] as usize;
                break; // fall through to ordinary travel
            }
        } else if motion == LOOK {
            //  Look.  Can't give more detail.  Pretend it wasn't dark
            //  (though it may now be dark) so he won't fall into a pit
            //  while staring into the gloom.
            if self.game.detail < 3 {
                self.rspeak(NO_MORE_DETAIL, &[]);
            }
            self.game.detail += 1;
            self.game.wzdark = false;
            self.game.locs[self.game.loc as usize].abbrev = 0;
            return;
        } else if motion == CAVE {
            // Cave.  Different messages depending on whether above ground.
            let msg = if self.outside(self.game.loc) && self.game.loc != LOC_GRATE {
                FOLLOW_STREAM
            } else {
                NEED_DETAIL
            };
            self.rspeak(msg, &[]);
            return;
        } else {
            // none of the specials
            self.game.oldlc2 = self.game.oldloc;
            self.game.oldloc = self.game.loc;
        }

        // Look for a way to fulfil the motion verb passed in -
        // travel_entry indexes the beginning of the motion entries for
        // here (game.loc).
        loop {
            if TRAVEL[travel_entry].motion == HERE || TRAVEL[travel_entry].motion == motion {
                break;
            }
            if TRAVEL[travel_entry].stop {
                // Couldn't find an entry matching the motion word passed
                // in.  Various messages depending on word given.
                let msg = match motion {
                    EAST | WEST | SOUTH | NORTH | NE | NW | SW | SE | UP | DOWN => BAD_DIRECTION,
                    FORWARD | LEFT | RIGHT => UNSURE_FACING,
                    OUTSIDE | INSIDE => NO_INOUT_HERE,
                    XYZZY | PLUGH => NOTHING_HAPPENS,
                    CRAWL => WHICH_WAY,
                    _ => CANT_APPLY,
                };
                self.rspeak(msg, &[]);
                return;
            }
            travel_entry += 1;
        }

        // (ESR) We've found a destination that goes with the motion verb.
        // Next we need to check any conditional(s) on this destination,
        // and possibly on following entries.
        loop {
            // L12 loop
            loop {
                let t = &TRAVEL[travel_entry];
                let (condtype, condarg1, condarg2) = (t.condtype, t.condarg1, t.condarg2);
                if condtype < CondType::Not {
                    // YAML N and [pct N] conditionals
                    if condtype == CondType::Goto || condtype == CondType::Pct {
                        if condarg1 == 0 || self.pct(condarg1) {
                            break;
                        }
                        // else fall through
                    } else if self.toting(condarg1)
                        || (condtype == CondType::With && self.at(condarg1))
                    {
                        // YAML [with OBJ] clause
                        break;
                    }
                    // else fall through to check [not OBJ STATE]
                } else if self.prop(condarg1) != condarg2 {
                    break;
                }

                // We arrive here on conditional failure.  Skip to next
                // non-matching destination.
                travel_entry = Self::skip_equal_travel(travel_entry);
            }

            // Found an eligible rule, now execute it
            let desttype = TRAVEL[travel_entry].desttype;
            self.game.newloc = TRAVEL[travel_entry].destval;
            if desttype == DestType::Goto {
                return;
            }

            if desttype == DestType::Speak {
                // Execute a speak rule
                self.rspeak(self.game.newloc, &[]);
                self.game.newloc = self.game.loc;
                return;
            }

            match self.game.newloc {
                1 => {
                    // Special travel 1.  Plover-alcove passage.  Can carry
                    // only emerald.  Note: travel table must include
                    // "useless" entries going through passage, which can
                    // never be used for actual motion, but can be spotted
                    // by "go back".
                    self.game.newloc = if self.game.loc == LOC_PLOVER {
                        LOC_ALCOVE
                    } else {
                        LOC_PLOVER
                    };
                    if self.game.holdng > 1 || (self.game.holdng == 1 && !self.toting(EMERALD)) {
                        self.game.newloc = self.game.loc;
                        self.rspeak(MUST_DROP, &[]);
                    }
                    return;
                }
                2 => {
                    // Special travel 2.  Plover transport.  Drop the
                    // emerald (only use special travel if toting it), so
                    // he's forced to use the plover-passage to get it
                    // out.  Having dropped it, go back and pretend he
                    // wasn't carrying it after all.
                    self.drop(EMERALD, self.game.loc);
                    travel_entry = Self::skip_equal_travel(travel_entry);
                    continue; // goto L12
                }
                3 => {
                    // Special travel 3.  Troll bridge. Must be done only
                    // as special motion so that dwarves won't wander
                    // across and encounter the bear.  (They won't follow
                    // the player there because that region is forbidden
                    // to the pirate.)  If prop[TROLL]=TROLL_PAIDONCE,
                    // he's crossed since paying, so step out and block
                    // him. (standard travel entries check for
                    // prop[TROLL]=TROLL_UNPAID.)  Special stuff for bear.
                    if self.prop(TROLL) == TROLL_PAIDONCE {
                        self.pspeak(TROLL, SpeakType::Look, true, TROLL_PAIDONCE, &[]);
                        self.set_prop(TROLL, TROLL_UNPAID);
                        self.destroy(TROLL2);
                        self.move_obj(TROLL2 + NOBJECTS as i32, IS_FREE);
                        self.move_obj(TROLL, OBJECTS[TROLL as usize].plac);
                        self.move_obj(TROLL + NOBJECTS as i32, OBJECTS[TROLL as usize].fixd);
                        self.juggle(CHASM);
                        self.game.newloc = self.game.loc;
                        return;
                    } else {
                        self.game.newloc = OBJECTS[TROLL as usize].plac
                            + OBJECTS[TROLL as usize].fixd
                            - self.game.loc;
                        if self.prop(TROLL) == TROLL_UNPAID {
                            self.set_prop(TROLL, TROLL_PAIDONCE);
                        }
                        if !self.toting(BEAR) {
                            return;
                        }
                        self.state_change(CHASM, BRIDGE_WRECKED);
                        self.set_prop(TROLL, TROLL_GONE);
                        self.drop(BEAR, self.game.newloc);
                        self.set_fixed(BEAR, IS_FIXED);
                        self.set_prop(BEAR, BEAR_DEAD);
                        self.game.oldlc2 = self.game.newloc;
                        self.croak();
                        return;
                    }
                }
                _ => bug(BugType::SpecialTravel500GtLGt300ExceedsGotoList),
            }
        }
    }

    /// Check game limit and lamp timers.
    fn lampcheck(&mut self) {
        if self.prop(LAMP) == LAMP_BRIGHT {
            self.game.limit -= 1;
        }

        //  Another way we can force an end to things is by having the
        //  lamp give out.  When it gets close, we come here to warn him.
        //  First following arm checks if the lamp and fresh batteries are
        //  here, in which case we replace the batteries and continue.
        //  Second is for other cases of lamp dying.  Even after it goes
        //  out, he can explore outside for a while if desired.
        if self.game.limit <= WARNTIME {
            if self.here(BATTERY) && self.prop(BATTERY) == FRESH_BATTERIES && self.here(LAMP) {
                self.rspeak(REPLACE_BATTERIES, &[]);
                self.set_prop(BATTERY, DEAD_BATTERIES);
                self.game.limit += BATTERYLIFE;
                self.game.lmwarn = false;
            } else if !self.game.lmwarn && self.here(LAMP) {
                self.game.lmwarn = true;
                if self.prop(BATTERY) == DEAD_BATTERIES {
                    self.rspeak(MISSING_BATTERIES, &[]);
                } else if self.place(BATTERY) == LOC_NOWHERE {
                    self.rspeak(LAMP_DIM, &[]);
                } else {
                    self.rspeak(GET_BATTERIES, &[]);
                }
            }
        }
        if self.game.limit == 0 {
            self.game.limit = -1;
            self.set_prop(LAMP, LAMP_DARK);
            if self.here(LAMP) {
                self.rspeak(LAMP_OUT, &[]);
            }
        }
    }

    ///  Handle the closing of the cave.  The cave closes "clock1" turns
    ///  after the last treasure has been located (including the
    ///  pirate's chest, which may of course never show up).  Note that
    ///  the treasures need not have been taken yet, just located.
    ///  Hence clock1 must be large enough to get out of the cave (it
    ///  only ticks while inside the cave).  When it hits zero, we start
    ///  closing the cave, and then sit back and wait for him to try to
    ///  get out.  If he doesn't within clock2 turns, we close the cave;
    ///  if he does try, we assume he panics, and give him a few
    ///  additional turns to get frantic before we close.  When clock2
    ///  hits zero, we transport him into the final puzzle.
    fn closecheck(&mut self) -> bool {
        // If a turn threshold has been met, apply penalties and tell the
        // player about it.
        for t in TURN_THRESHOLDS.iter() {
            if self.game.turns == t.threshold + 1 {
                self.game.trnluz += t.point_loss;
                self.speak(t.message, &[]);
            }
        }

        // Don't tick game.clock1 unless well into cave (and not at Y2).
        if self.game.tally == 0 && self.indeep(self.game.loc) && self.game.loc != LOC_Y2 {
            self.game.clock1 -= 1;
        }

        //  When the first warning comes, we lock the grate, destroy the
        //  bridge, kill all the dwarves (and the pirate), remove the
        //  troll and bear (unless dead), and set "closng" to true.
        //  Leave the dragon; too much trouble to move it.  from now
        //  until clock2 runs out, he cannot unlock the grate, move to
        //  any location outside the cave, or create the bridge.  Nor can
        //  he be resurrected if he dies.
        if self.game.clock1 == 0 {
            self.set_prop(GRATE, GRATE_CLOSED);
            self.set_prop(FISSURE, UNBRIDGED);
            for i in 1..=NDWARVES {
                self.game.dwarves[i].seen = false;
                self.game.dwarves[i].loc = LOC_NOWHERE;
            }
            self.destroy(TROLL);
            self.move_obj(TROLL + NOBJECTS as i32, IS_FREE);
            self.move_obj(TROLL2, OBJECTS[TROLL as usize].plac);
            self.move_obj(TROLL2 + NOBJECTS as i32, OBJECTS[TROLL as usize].fixd);
            self.juggle(CHASM);
            if self.prop(BEAR) != BEAR_DEAD {
                self.destroy(BEAR);
            }
            self.set_prop(CHAIN, CHAIN_HEAP);
            self.set_fixed(CHAIN, IS_FREE);
            self.set_prop(AXE, AXE_HERE);
            self.set_fixed(AXE, IS_FREE);
            self.rspeak(CAVE_CLOSING, &[]);
            self.game.clock1 = -1;
            self.game.closng = true;
            return self.game.closed;
        } else if self.game.clock1 < 0 {
            self.game.clock2 -= 1;
        }
        if self.game.clock2 == 0 {
            //  Once he's panicked, and clock2 has run out, we come here
            //  to set up the storage room.  The room has two locs,
            //  hardwired as LOC_NE and LOC_SW.  At the ne end, we place
            //  empty bottles, a nursery of plants, a bed of oysters, a
            //  pile of lamps, rods with stars, sleeping dwarves, and him.
            //  At the sw end we place grate over treasures, snake pit,
            //  covey of caged birds, more rods, and pillows.  A mirror
            //  stretches across one wall.  Many of the objects come from
            //  known locations and/or states (e.g. the snake is known to
            //  have been destroyed and needn't be carried away from its
            //  old "place"), making the various objects be handled
            //  differently.  We also drop all other objects he might be
            //  carrying (lest he has some which could cause trouble, such
            //  as the keys).  We describe the flash of light and trundle
            //  back.
            self.put(BOTTLE, LOC_NE, EMPTY_BOTTLE);
            self.put(PLANT, LOC_NE, PLANT_THIRSTY);
            self.put(OYSTER, LOC_NE, STATE_FOUND);
            self.put(LAMP, LOC_NE, LAMP_DARK);
            self.put(ROD, LOC_NE, STATE_FOUND);
            self.put(DWARF, LOC_NE, STATE_FOUND);
            self.game.loc = LOC_NE;
            self.game.oldloc = LOC_NE;
            self.game.newloc = LOC_NE;
            // Leave the grate with normal (non-negative) property.
            // Reuse sign.
            self.move_obj(GRATE, LOC_SW);
            self.move_obj(SIGN, LOC_SW);
            self.set_prop(SIGN, ENDGAME_SIGN);
            self.put(SNAKE, LOC_SW, SNAKE_CHASED);
            self.put(BIRD, LOC_SW, BIRD_CAGED);
            self.put(CAGE, LOC_SW, STATE_FOUND);
            self.put(ROD2, LOC_SW, STATE_FOUND);
            self.put(PILLOW, LOC_SW, STATE_FOUND);

            self.put(MIRROR, LOC_NE, STATE_FOUND);
            self.set_fixed(MIRROR, LOC_SW);

            for i in 1..NOBJECTS as i32 {
                if self.toting(i) {
                    self.destroy(i);
                }
            }

            self.rspeak(CAVE_CLOSED, &[]);
            self.game.closed = true;
            return self.game.closed;
        }

        self.lampcheck();
        false
    }

    ///  Print out descriptions of objects at this location.  If not
    ///  closing and property value is negative, tally off another
    ///  treasure.  Rug is special case; once seen, its game.prop is
    ///  RUG_DRAGON (dragon on it) till dragon is killed.  Similarly for
    ///  chain; game.prop is initially CHAINING_BEAR (locked to bear).
    ///  These hacks are because game.prop=0 is needed to get full score.
    fn listobjects(&mut self) {
        if self.is_dark_here() {
            return;
        }
        self.game.locs[self.game.loc as usize].abbrev += 1;
        let mut i = self.atloc(self.game.loc);
        while i != 0 {
            let mut obj = i;
            if obj >= NOBJECTS as i32 {
                obj -= NOBJECTS as i32;
            }
            let next = self.game.link[i as usize];
            if obj == STEPS && self.toting(NUGGET) {
                i = next;
                continue;
            }
            // (ESR) Warning: it looks like you could get away with
            // running this code only on objects with the treasure
            // property set. Nope.  There is mystery here.
            if self.object_is_stashed(obj) || self.object_is_notfound(obj) {
                if self.game.closed {
                    i = next;
                    continue;
                }
                self.object_set_found(obj);
                if obj == RUG {
                    self.set_prop(RUG, RUG_DRAGON);
                }
                if obj == CHAIN {
                    self.set_prop(CHAIN, CHAINING_BEAR);
                }
                if obj == EGGS {
                    self.game.seenbigwords = true;
                }
                self.game.tally -= 1;
                //  Note: There used to be a test here to see whether the
                //  player had blown it so badly that he could never ever
                //  see the remaining treasures, and if so the lamp was
                //  zapped to 35 turns.  But the tests were too
                //  simple-minded; so the whole thing was flushed.
            }
            let mut kk = self.prop(obj);
            if obj == STEPS {
                kk = if self.game.loc == self.fixed(STEPS) {
                    STEPS_UP
                } else {
                    STEPS_DOWN
                };
            }
            self.pspeak(obj, SpeakType::Look, true, kk, &[]);
            i = self.game.link[i as usize];
        }
    }

    /// Pre-processes a command input to see if we need to tease out a
    /// few specific cases:
    /// - "enter water" or "enter stream": weird specific case that gets
    ///   the user wet, and then kicks us back to get another command
    /// - <object> <verb>: Irregular form of input, but should be
    ///   allowed. We switch back to <verb> <object> form for further
    ///   processing.
    /// - "grate": If in location with grate, we move to that grate. If
    ///   we're in a number of other places, we move to the entrance.
    /// - "water plant", "oil plant", "water door", "oil door": Change to
    ///   "pour water" or "pour oil" based on context
    /// - "cage bird": If bird is present, we change to "carry bird"
    ///
    /// Returns true if pre-processing is complete, and we're ready to
    /// move to the primary command processing, false otherwise.
    fn preprocess_command(&mut self) -> bool {
        let loc = self.game.loc;
        let cmd0 = &self.command.word[0];
        let cmd1 = &self.command.word[1];
        if cmd0.wtype == WordType::Motion
            && cmd0.id == ENTER
            && (cmd1.id == STREAM || cmd1.id == WATER)
        {
            if self.liqloc(loc) == WATER {
                self.rspeak(FEET_WET, &[]);
            } else {
                self.rspeak(WHERE_QUERY, &[]);
            }
            return false;
        }

        if self.command.word[0].wtype == WordType::Object {
            // From OV to VO form
            if self.command.word[1].wtype == WordType::Action {
                self.command.word.swap(0, 1);
            }

            if self.command.word[0].id == GRATE {
                self.command.word[0].wtype = WordType::Motion;
                if loc == LOC_START || loc == LOC_VALLEY || loc == LOC_SLIT {
                    self.command.word[0].id = DEPRESSION;
                }
                if loc == LOC_COBBLE
                    || loc == LOC_DEBRIS
                    || loc == LOC_AWKWARD
                    || loc == LOC_BIRDCHAMBER
                    || loc == LOC_PITTOP
                {
                    self.command.word[0].id = ENTRANCE;
                }
            }
            let (id0, id1) = (self.command.word[0].id, self.command.word[1].id);
            if (id0 == WATER || id0 == OIL) && (id1 == PLANT || id1 == DOOR) && self.at(id1) {
                self.command.word[1] = self.command.word[0].clone();
                self.command.word[0].id = POUR;
                self.command.word[0].wtype = WordType::Action;
                self.command.word[0].raw = "pour".to_string();
            }
            if self.command.word[0].id == CAGE
                && self.command.word[1].id == BIRD
                && self.here(CAGE)
                && self.here(BIRD)
            {
                self.command.word[0].id = CARRY;
                self.command.word[0].wtype = WordType::Action;
            }
        }

        // If no word type is given for the first word, we assume it's a
        // motion.
        if self.command.word[0].wtype == WordType::NoWordType {
            self.command.word[0].wtype = WordType::Motion;
        }

        self.command.state = CommandState::Preprocessed;
        true
    }

    /// Actually execute the move to the new location and dwarf movement.
    fn do_move(&mut self) -> bool {
        // Can't leave cave once it's closing (except by main office).
        if self.outside(self.game.newloc) && self.game.newloc != 0 && self.game.closng {
            self.rspeak(EXIT_CLOSED, &[]);
            self.game.newloc = self.game.loc;
            if !self.game.panic {
                self.game.clock2 = PANICTIME;
            }
            self.game.panic = true;
        }

        //  See if a dwarf has seen him and has come from where he wants
        //  to go.  If so, the dwarf's blocking his way.  If coming from
        //  place forbidden to pirate (dwarves rooted in place) let him
        //  get out (and attacked).
        if self.game.newloc != self.game.loc
            && !self.forced(self.game.loc)
            && !self.cndbit(self.game.loc, COND_NOARRR)
        {
            for i in 1..NDWARVES {
                if self.game.dwarves[i].oldloc == self.game.newloc && self.game.dwarves[i].seen {
                    self.game.newloc = self.game.loc;
                    self.rspeak(DWARF_BLOCK, &[]);
                    break;
                }
            }
        }
        self.game.loc = self.game.newloc;

        if !self.dwarfmove() {
            self.croak();
        }

        if self.game.loc == LOC_NOWHERE {
            self.croak();
        }

        // The easiest way to get killed is to fall into a pit in pitch
        // darkness.
        if !self.forced(self.game.loc)
            && self.is_dark_here()
            && self.game.wzdark
            && self.pct(PIT_KILL_PROB)
        {
            self.rspeak(PIT_FALL, &[]);
            self.game.oldlc2 = self.game.loc;
            self.croak();
            return false;
        }

        true
    }

    /// Get and execute a command.
    fn do_command(&mut self) -> bool {
        self.clear_command();

        // Describe the current location and (maybe) get next command.
        while self.command.state != CommandState::Executed {
            self.describe_location();

            if self.forced(self.game.loc) {
                self.playermove(HERE);
                return true;
            }

            self.listobjects();

            // Command not yet given; keep getting commands from user
            // until valid command is both given and executed.
            self.clear_command();
            while self.command.state <= CommandState::Given {
                if self.game.closed {
                    //  If closing time, check for any stashed objects
                    //  being toted and unstash them.  This way objects
                    //  won't be described until they've been picked up
                    //  and put down separate from their respective piles.
                    if (self.object_is_notfound(OYSTER) || self.object_is_stashed(OYSTER))
                        && self.toting(OYSTER)
                    {
                        self.pspeak(OYSTER, SpeakType::Look, true, 1, &[]);
                    }
                    for i in 1..NOBJECTS as i32 {
                        if self.toting(i)
                            && (self.object_is_notfound(i) || self.object_is_stashed(i))
                        {
                            self.object_stashify(i, self.prop(i));
                        }
                    }
                }

                // Check to see if the room is dark.
                self.game.wzdark = self.is_dark_here();

                // If the knife is not here it permanently disappears.
                // Possibly this should fire if the knife is here but the
                // room is dark?
                if self.game.knfloc > LOC_NOWHERE && self.game.knfloc != self.game.loc {
                    self.game.knfloc = LOC_NOWHERE;
                }

                // Check some for hints, get input from user, increment
                // turn, and pre-process commands. Keep going until
                // pre-processing is done.
                while self.command.state < CommandState::Preprocessed {
                    self.checkhints();

                    // Get command input from user
                    if !self.get_command_input() {
                        return false;
                    }

                    // Every input, check "foobar" flag. If zero, nothing's
                    // going on. If pos, make neg. If neg, he skipped a
                    // word, so make it zero.
                    self.game.foobar = if self.game.foobar > WORD_EMPTY {
                        -self.game.foobar
                    } else {
                        WORD_EMPTY
                    };

                    self.game.turns += 1;
                    self.preprocess_command();
                }

                // check if game is closed, and exit if it is
                if self.closecheck() {
                    return true;
                }

                // loop until all words in command are processed
                while self.command.state == CommandState::Preprocessed {
                    self.command.state = CommandState::Processing;

                    if self.command.word[0].id == WORD_NOT_FOUND {
                        // Gee, I don't understand.
                        let raw = self.command.word[0].raw.clone();
                        self.sspeak(DONT_KNOW, &[Arg::S(&raw)]);
                        self.clear_command();
                        continue;
                    }

                    // Give user hints of shortcuts
                    if self.command.word[0].raw.eq_ignore_ascii_case("west") {
                        self.game.iwest += 1;
                        if self.game.iwest == 10 {
                            self.rspeak(W_IS_WEST, &[]);
                        }
                    }
                    if self.command.word[0].raw.eq_ignore_ascii_case("go")
                        && self.command.word[1].id != WORD_EMPTY
                    {
                        self.game.igo += 1;
                        if self.game.igo == 10 {
                            self.rspeak(GO_UNNEEDED, &[]);
                        }
                    }

                    match self.command.word[0].wtype {
                        WordType::Motion => {
                            self.playermove(self.command.word[0].id);
                            self.command.state = CommandState::Executed;
                            continue;
                        }
                        WordType::Object => {
                            self.command.part = SpeechPart::Unknown;
                            self.command.obj = self.command.word[0].id;
                        }
                        WordType::Action => {
                            self.command.part = if self.command.word[1].wtype == WordType::Numeric {
                                SpeechPart::Transitive
                            } else {
                                SpeechPart::Intransitive
                            };
                            self.command.verb = self.command.word[0].id;
                        }
                        WordType::Numeric => {
                            if !self.settings.oldstyle {
                                let raw = self.command.word[0].raw.clone();
                                self.sspeak(DONT_KNOW, &[Arg::S(&raw)]);
                                self.clear_command();
                                continue;
                            }
                        }
                        WordType::NoWordType => {
                            bug(BugType::VocabularyTypeNOver1000NotBetween0And3)
                        }
                    }

                    match self.action(self.command.clone()) {
                        Phase::Terminate => {
                            self.command.state = CommandState::Executed;
                        }
                        Phase::Move => {
                            self.playermove(NUL);
                            self.command.state = CommandState::Executed;
                        }
                        Phase::Word2 => {
                            // Get second word for analysis.
                            self.command.word[0] = self.command.word[1].clone();
                            self.command.word[1] = CommandWord::empty();
                            self.command.state = CommandState::Preprocessed;
                        }
                        Phase::Unknown => {
                            //  Random intransitive verbs come here.  Clear
                            //  obj just in case (see attack()).
                            let raw = &mut self.command.word[0].raw;
                            if let Some(first) = raw.get(0..1) {
                                let up = first.to_ascii_uppercase();
                                raw.replace_range(0..1, &up);
                            }
                            let raw = self.command.word[0].raw.clone();
                            self.sspeak(DO_WHAT, &[Arg::S(&raw)]);
                            self.command.obj = NO_OBJECT;

                            // object cleared; we need to go back to the
                            // preprocessing step
                            self.command.state = CommandState::Given;
                        }
                        Phase::CheckHint => {
                            self.command.state = CommandState::Given;
                        }
                        Phase::DwarfWake => {
                            // Oh dear, he's disturbed the dwarves.
                            self.rspeak(DWARVES_AWAKEN, &[]);
                            self.terminate(Termination::EndGame);
                        }
                        Phase::ClearObj => {
                            self.clear_command();
                        }
                        Phase::Top => {}
                    }
                } // while command has not been fully processed
            } // while command is not yet given
        } // while command is not executed

        // command completely executed; we return true.
        true
    }

    /// Interpret commands until EOF or interrupt, then show the score
    /// and exit.
    pub fn run(&mut self) -> ! {
        loop {
            // if we're supposed to move, move
            if !self.do_move() {
                continue;
            }

            // get command
            if !self.do_command() {
                break;
            }
        }
        // show score and exit
        self.terminate(Termination::QuitGame);
    }
}

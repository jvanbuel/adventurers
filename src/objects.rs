// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Where every object is, and what state it is in.
//!
//! Each object has a [`Place`] (carried, or at a location), a [`Fixed`]
//! second end, and a numeric state.  Each location keeps an ordered
//! list of what is there; that order decides the order objects are
//! described in, so it is maintained exactly as the original does:
//! dropping something puts it at the front of the list.
//!
//! The lists are intrusive singly linked lists over "nodes": node `o`
//! is object `o`, and node `o + NOBJECTS` is the second end of
//! two-placed object `o`.  That representation is part of the save
//! format and is entirely private to this module.

use crate::dungeon::{BIRD, Fixed, LOC_NOWHERE, Loc, NLOCATIONS, NOBJECTS, Obj};

/// Where an object is.  `At(LOC_NOWHERE)` means out of play.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    Carried,
    At(Loc),
}

impl Place {
    pub(crate) const NOWHERE: Place = Place::At(LOC_NOWHERE);

    pub(crate) fn raw(self) -> i32 {
        match self {
            Place::Carried => -1,
            Place::At(loc) => loc.raw(),
        }
    }

    pub(crate) fn from_raw(n: i32) -> Place {
        if n < 0 {
            Place::Carried
        } else {
            Place::At(Loc::from_raw(n))
        }
    }

    /// The location, if the object is lying somewhere (not carried and
    /// not out of play).
    fn location(self) -> Option<Loc> {
        match self {
            Place::At(loc) if loc != LOC_NOWHERE => Some(loc),
            _ => None,
        }
    }
}

// Object states.  Values >= 0 are object-specific.  Treasures start
// "not found"; a state is "stashed" (value -1 - n) while the object is
// part of the end-game set piece and may not be described or taken.
pub(crate) const STATE_NOTFOUND: i32 = -1;
pub(crate) const STATE_FOUND: i32 = 0;
/// State shared by the gemstones once placed in the cavity.
pub(crate) const STATE_IN_CAVITY: i32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct ObjectState {
    place: i32,
    fixed: i32,
    prop: i32,
}

/// A node in a location's object list (see the module docs).
type Node = i32;
const END: Node = 0;
const N: i32 = NOBJECTS as i32;

/// The positions and states of all objects.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Objects {
    state: Vec<ObjectState>,
    /// Head of each location's object list.
    atloc: Vec<Node>,
    /// Next node after each node.
    link: Vec<Node>,
    /// Number of objects being carried (the bird weighs nothing).
    holding: i32,
}

impl Objects {
    /// Objects in their starting positions and states.
    pub fn new() -> Objects {
        let mut objs = Objects {
            state: vec![ObjectState::default(); NOBJECTS],
            atloc: vec![END; NLOCATIONS],
            link: vec![END; NOBJECTS * 2],
            holding: 0,
        };
        // Lists are built by prefixing, so place objects in reverse to
        // get them in table order.  Two-placed objects are best
        // described last, so they go first.
        for o in Obj::all().rev() {
            if let Fixed::At(far) = o.info().fixd {
                objs.drop_fixed(o, Fixed::At(far));
                objs.drop(o, Place::At(o.info().plac));
            }
        }
        for o in Obj::all().rev() {
            let info = o.info();
            objs.set_fixed(o, info.fixd);
            if info.plac != LOC_NOWHERE && !matches!(info.fixd, Fixed::At(_)) {
                objs.drop(o, Place::At(info.plac));
            }
        }
        // Treasures are "not found" until first seen.
        for o in Obj::all() {
            if !o.is_treasure() || o.info().inventory.is_some() {
                let st = if o.is_treasure() {
                    STATE_NOTFOUND
                } else {
                    STATE_FOUND
                };
                objs.set_prop(o, st);
            }
        }
        objs
    }

    fn st(&self, o: Obj) -> &ObjectState {
        &self.state[o.index()]
    }

    fn st_mut(&mut self, o: Obj) -> &mut ObjectState {
        &mut self.state[o.index()]
    }

    // ---- Queries ----------------------------------------------------------

    pub fn place(&self, o: Obj) -> Place {
        Place::from_raw(self.st(o).place)
    }

    pub fn fixed(&self, o: Obj) -> Fixed {
        Fixed::from_raw(self.st(o).fixed)
    }

    pub fn prop(&self, o: Obj) -> i32 {
        self.st(o).prop
    }

    /// Is the player carrying it?
    pub fn is_carried(&self, o: Obj) -> bool {
        self.place(o) == Place::Carried
    }

    /// Is either end of it at `loc`?
    pub fn is_at(&self, o: Obj, loc: Loc) -> bool {
        self.place(o) == Place::At(loc) || self.fixed(o) == Fixed::At(loc)
    }

    /// Is it at `loc` or being carried?
    pub fn is_here(&self, o: Obj, loc: Loc) -> bool {
        self.is_at(o, loc) || self.is_carried(o)
    }

    /// How many objects the player is carrying.
    pub fn holding(&self) -> i32 {
        self.holding
    }

    /// The objects lying at `loc`, in description order.  A two-placed
    /// object seen from its far end is included.
    pub fn at(&self, loc: Loc) -> impl Iterator<Item = Obj> + '_ {
        let mut node = self.atloc[loc.index()];
        std::iter::from_fn(move || {
            if node == END {
                return None;
            }
            let here = node;
            node = self.link[node as usize];
            Some(Obj::from_raw(if here >= N { here - N } else { here }))
        })
    }

    // ---- States -----------------------------------------------------------

    pub fn set_prop(&mut self, o: Obj, prop: i32) {
        self.st_mut(o).prop = prop;
    }

    pub fn is_not_found(&self, o: Obj) -> bool {
        self.prop(o) == STATE_NOTFOUND
    }

    pub fn is_found(&self, o: Obj) -> bool {
        self.prop(o) == STATE_FOUND
    }

    pub fn set_found(&mut self, o: Obj) {
        self.set_prop(o, STATE_FOUND);
    }

    /// Is it part of the end-game set piece, not to be described?
    pub fn is_stashed(&self, o: Obj) -> bool {
        self.prop(o) < STATE_NOTFOUND
    }

    /// Put the object in the stashed counterpart of state `prop`.
    pub fn stash(&mut self, o: Obj, prop: i32) {
        self.set_prop(o, -1 - prop);
    }

    /// Is it in state `prop`, or the stashed counterpart of it?
    pub fn in_state(&self, o: Obj, prop: i32) -> bool {
        self.prop(o) == prop || self.prop(o) == -1 - prop
    }

    // ---- Movement -----------------------------------------------------------

    /// Set where an object is without touching the location lists.
    /// Only for objects that never appear in them (the liquids).
    pub fn set_place(&mut self, o: Obj, place: Place) {
        self.st_mut(o).place = place.raw();
    }

    /// Set the second end without touching the location lists.
    pub fn set_fixed(&mut self, o: Obj, fixed: Fixed) {
        self.st_mut(o).fixed = fixed.raw();
    }

    /// Start carrying an object that is at `from`, taking it off that
    /// location's list.  Does nothing if it is already carried.
    pub fn carry(&mut self, o: Obj, from: Loc) {
        if self.is_carried(o) {
            return;
        }
        self.st_mut(o).place = Place::Carried.raw();
        // The bird is weightless, so that "take bird" and "take cage"
        // behave the same when the bird is in the cage.
        if o != BIRD {
            self.holding += 1;
        }
        self.unlink(o.raw(), from);
    }

    /// Put an object down at `to`, at the front of its list.  If it was
    /// being carried, the player is no longer carrying it.
    pub fn drop(&mut self, o: Obj, to: Place) {
        if self.is_carried(o) && o != BIRD {
            self.holding -= 1;
        }
        self.st_mut(o).place = to.raw();
        if let Some(loc) = to.location() {
            self.push(o.raw(), loc);
        }
    }

    /// Pick an object up from wherever it is and drop it at `to`.
    pub fn move_to(&mut self, o: Obj, to: Place) {
        if let Some(from) = self.place(o).location() {
            self.carry(o, from);
        }
        self.drop(o, to);
    }

    /// Take an object out of play.
    pub fn destroy(&mut self, o: Obj) {
        self.move_to(o, Place::NOWHERE);
    }

    /// Move an object to `loc` and stash it in state `prop`.
    pub fn put(&mut self, o: Obj, loc: Loc, prop: i32) {
        self.move_to(o, Place::At(loc));
        self.stash(o, prop);
    }

    /// Set an object's second end, adding it to that location's list
    /// without removing it from any other.
    pub fn drop_fixed(&mut self, o: Obj, to: Fixed) {
        self.set_fixed(o, to);
        if let Fixed::At(loc) = to {
            if loc != LOC_NOWHERE {
                self.push(o.raw() + N, loc);
            }
        }
    }

    /// Move an object's second end.
    pub fn move_fixed(&mut self, o: Obj, to: Fixed) {
        if let Fixed::At(from) = self.fixed(o) {
            self.unlink(o.raw() + N, from);
        }
        self.drop_fixed(o, to);
    }

    /// Pick an object up and put it down again where it was, which
    /// brings it to the front of its location's list.
    pub fn juggle(&mut self, o: Obj) {
        let (place, fixed) = (self.place(o), self.fixed(o));
        self.move_to(o, place);
        self.move_fixed(o, fixed);
    }

    fn push(&mut self, node: Node, loc: Loc) {
        self.link[node as usize] = self.atloc[loc.index()];
        self.atloc[loc.index()] = node;
    }

    /// Remove `node` from `loc`'s list.  If it isn't there, the list is
    /// left alone (the C version looped forever).
    fn unlink(&mut self, node: Node, loc: Loc) {
        let head = &mut self.atloc[loc.index()];
        if *head == node {
            *head = self.link[node as usize];
            return;
        }
        let mut prev = *head;
        while self.link[prev as usize] != node {
            if self.link[prev as usize] == END {
                return;
            }
            prev = self.link[prev as usize];
        }
        self.link[prev as usize] = self.link[node as usize];
    }

    // ---- Save-file representation ---------------------------------------------

    /// `(fixed, prop, place)` of every object, as saved.
    pub(crate) fn raw_states(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.state.iter().map(|s| [s.fixed, s.prop, s.place])
    }

    pub(crate) fn raw_atloc(&self) -> &[i32] {
        &self.atloc
    }

    pub(crate) fn raw_link(&self) -> &[i32] {
        &self.link
    }

    /// Rebuild from saved values.  Returns `None` if any value is out of
    /// range.
    pub(crate) fn from_raw(
        states: &[[i32; 3]],
        atloc: Vec<i32>,
        link: Vec<i32>,
        holding: i32,
    ) -> Option<Objects> {
        let nloc = NLOCATIONS as i32;
        let node_ok = |n: &i32| (0..2 * N).contains(n);
        let loc_ok = |n: i32| (-1..nloc).contains(&n);
        let state_ok =
            |p: i32| (-crate::dungeon::MAX_STATE - 1..=crate::dungeon::MAX_STATE).contains(&p);
        let ok = states.len() == NOBJECTS
            && atloc.len() == NLOCATIONS
            && link.len() == NOBJECTS * 2
            && states
                .iter()
                .all(|&[f, p, pl]| loc_ok(f) && loc_ok(pl) && state_ok(p))
            && atloc.iter().all(node_ok)
            && link.iter().all(node_ok);
        ok.then(|| Objects {
            state: states
                .iter()
                .map(|&[fixed, prop, place]| ObjectState { place, fixed, prop })
                .collect(),
            atloc,
            link,
            holding,
        })
    }
}

impl Default for Objects {
    fn default() -> Self {
        Objects::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::*;

    #[test]
    fn starting_positions() {
        let objs = Objects::new();
        assert_eq!(objs.place(LAMP), Place::At(LOC_BUILDING));
        assert!(objs.at(LOC_BUILDING).any(|o| o == LAMP));
        // The grate is visible from both sides.
        assert!(objs.is_at(GRATE, LOC_GRATE));
        assert!(objs.is_at(GRATE, LOC_BELOWGRATE));
        assert!(objs.is_not_found(NUGGET));
        assert_eq!(objs.holding(), 0);
    }

    #[test]
    fn carrying_and_dropping() {
        let mut objs = Objects::new();
        objs.carry(LAMP, LOC_BUILDING);
        assert!(objs.is_carried(LAMP));
        assert!(!objs.at(LOC_BUILDING).any(|o| o == LAMP));
        assert_eq!(objs.holding(), 1);

        objs.drop(LAMP, Place::At(LOC_START));
        assert_eq!(
            objs.at(LOC_START).next(),
            Some(LAMP),
            "dropped objects go first"
        );
        assert_eq!(objs.holding(), 0);
    }

    #[test]
    fn the_bird_is_weightless() {
        let mut objs = Objects::new();
        objs.carry(BIRD, LOC_BIRDCHAMBER);
        assert!(objs.is_carried(BIRD));
        assert_eq!(objs.holding(), 0);
    }

    #[test]
    fn juggling_brings_an_object_to_the_front() {
        let mut objs = Objects::new();
        let last = objs.at(LOC_BUILDING).last().unwrap();
        objs.juggle(last);
        assert_eq!(objs.at(LOC_BUILDING).next(), Some(last));
    }

    #[test]
    fn stashing_round_trips() {
        let mut objs = Objects::new();
        objs.put(BIRD, LOC_SW, BIRD_CAGED);
        assert!(objs.is_stashed(BIRD));
        assert!(objs.in_state(BIRD, BIRD_CAGED));
        assert_eq!(objs.place(BIRD), Place::At(LOC_SW));
    }

    #[test]
    fn carrying_from_the_wrong_place_does_not_hang() {
        let mut objs = Objects::new();
        objs.carry(LAMP, LOC_START);
        assert!(objs.is_carried(LAMP));
        assert!(objs.at(LOC_BUILDING).any(|o| o == KEYS));
    }

    #[test]
    fn raw_round_trip() {
        let mut objs = Objects::new();
        objs.carry(KEYS, LOC_BUILDING);
        let states: Vec<_> = objs.raw_states().collect();
        let back = Objects::from_raw(
            &states,
            objs.raw_atloc().to_vec(),
            objs.raw_link().to_vec(),
            objs.holding(),
        );
        assert_eq!(back, Some(objs));
    }
}

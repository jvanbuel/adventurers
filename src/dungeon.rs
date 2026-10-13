// SPDX-FileCopyrightText: Copyright Eric S. Raymond <esr@thyrsus.com>
// SPDX-License-Identifier: BSD-2-Clause
//! The static dungeon: typed identifiers and the read-only tables
//! compiled from `adventure.yaml` by `build.rs`.
//!
//! Every named thing in the dungeon has a small `Copy` identifier
//! ([`Loc`], [`Obj`], [`Msg`], [`Motion`], [`Verb`]).  The identifiers
//! are the only way into the tables: `LOC_START.info()`,
//! `LAMP.info()`, `CAVE_NEARBY.text()` and so on.

#![allow(dead_code)] // not every generated name is referenced by the engine

/// Defines a dungeon identifier: a `Copy` newtype over the entry's
/// index in its generated table.
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
        pub struct $name(pub(crate) u16);

        impl $name {
            /// Index of this entry in its table.
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// The raw number used by the C implementation and the save
            /// file format.
            pub const fn raw(self) -> i32 {
                self.0 as i32
            }

            /// The identifier with the given raw number.
            pub(crate) const fn from_raw(n: i32) -> Self {
                Self(n as u16)
            }
        }
    };
}

id_type!(
    /// A location (room) in the dungeon.  `LOC_NOWHERE` is limbo.
    Loc
);
id_type!(
    /// An object, including scenery and creatures.  `NO_OBJECT` is a
    /// placeholder entry with no words.
    Obj
);
id_type!(
    /// A message from the `arbitrary_messages` table.
    Msg
);
id_type!(
    /// A movement word, e.g. `NORTH`, `XYZZY` or `BACK`.
    Motion
);
id_type!(
    /// An action word, e.g. `CARRY` or `LIGHT`.
    Verb
);

/// Where the second end of a two-placed object lives.
///
/// Most objects are `Free`: they occupy one place and can be picked up
/// if nothing else stops it.  Immovable scenery is `Immovable`.
/// Two-placed objects (the grate, the troll bridge, ...) are also
/// visible from a second location.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fixed {
    #[default]
    Free,
    Immovable,
    At(Loc),
}

impl Fixed {
    pub(crate) const fn raw(self) -> i32 {
        match self {
            Fixed::Free => 0,
            Fixed::Immovable => -1,
            Fixed::At(loc) => loc.raw(),
        }
    }

    pub(crate) const fn from_raw(n: i32) -> Fixed {
        match n {
            0 => Fixed::Free,
            n if n < 0 => Fixed::Immovable,
            n => Fixed::At(Loc::from_raw(n)),
        }
    }
}

/// Location attribute bits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cond {
    /// Light.
    Lit = 0,
    /// If `Fluid` is set: oil rather than water.
    Oily = 1,
    /// Liquid is available here.
    Fluid = 2,
    /// The pirate doesn't come here unless following the player.
    NoArrr = 3,
    /// Cannot use "back" to move away.
    NoBack = 4,
    /// Above ground, but not in the forest.
    Above = 5,
    /// Deep in the cave, where dwarves are active.
    Deep = 6,
    /// In the forest.
    Forest = 7,
    /// Only one way in or out; the player is moved on automatically.
    Forced = 8,
}

/// Base bit for location hints; hint `n` uses bit `HINT_BASE + n + 1`.
pub const HINT_BASE: u32 = 11;

/// A travel rule's precondition.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TravelCond {
    Always,
    /// Succeeds with the given percent probability.
    Pct(i32),
    /// The player must be carrying the object.
    Carry(Obj),
    /// The player must be carrying or in the same room as the object.
    With(Obj),
    /// The object's state must not be the given value.
    Not(Obj, i32),
}

/// What a travel rule does when it fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    Goto(Loc),
    /// One of the hard-coded special movements (1, 2 or 3).
    Special(u8),
    /// Print a message and stay put.
    Speak(Msg),
}

impl Dest {
    /// The raw number the C code stored for this destination, whatever
    /// its kind.  A few comparisons in the original use it as if it were
    /// a location number; they are preserved for identical behaviour.
    pub fn raw(self) -> i32 {
        match self {
            Dest::Goto(loc) => loc.raw(),
            Dest::Special(n) => n as i32,
            Dest::Speak(msg) => msg.raw(),
        }
    }
}

/// One movement rule.  A location's rules are tried in order.
#[derive(Clone, Copy, Debug)]
pub struct TravelOp {
    pub motion: Motion,
    pub cond: TravelCond,
    pub dest: Dest,
    /// Dwarves may not follow this rule.
    pub nodwarves: bool,
}

impl TravelOp {
    /// Whether two rules are the same apart from their motion word.
    pub fn same_target(&self, other: &TravelOp) -> bool {
        self.cond == other.cond && self.dest == other.dest
    }
}

pub type Text = Option<&'static str>;

pub struct Location {
    pub small: Text,
    pub big: Text,
    pub sound: Option<Msg>,
    pub loud: bool,
    conditions: u32,
    pub travel: &'static [TravelOp],
}

impl Location {
    pub fn has(&self, cond: Cond) -> bool {
        self.has_bit(cond as u32)
    }

    pub fn has_bit(&self, bit: u32) -> bool {
        self.conditions & (1 << bit) != 0
    }

    /// The raw condition word, used to decide whether hints apply.
    pub fn condition_bits(&self) -> u32 {
        self.conditions
    }
}

pub struct Object {
    pub words: &'static [&'static str],
    pub inventory: Text,
    pub plac: Loc,
    pub fixd: Fixed,
    pub is_treasure: bool,
    pub descriptions: &'static [Text],
    pub sounds: &'static [Text],
    pub texts: &'static [Text],
    pub changes: &'static [Text],
}

pub struct MotionInfo {
    pub words: &'static [&'static str],
}

pub struct VerbInfo {
    pub words: &'static [&'static str],
    pub message: Text,
    pub noaction: bool,
}

pub struct Obituary {
    pub query: Text,
    pub yes_response: Text,
}

pub struct TurnThreshold {
    pub threshold: i32,
    pub point_loss: i32,
    pub message: Text,
}

pub struct Class {
    pub threshold: i32,
    pub message: Text,
}

pub struct Hint {
    pub turns: i32,
    pub penalty: i32,
    pub question: Text,
    pub hint: Text,
}

impl Loc {
    pub fn info(self) -> &'static Location {
        &LOCATIONS[self.index()]
    }

    pub fn has(self, cond: Cond) -> bool {
        self.info().has(cond)
    }

    /// Every location, in table order.
    pub fn all() -> impl Iterator<Item = Loc> {
        (0..NLOCATIONS as u16).map(Loc)
    }
}

impl Obj {
    pub fn info(self) -> &'static Object {
        &OBJECTS[self.index()]
    }

    /// Every real object (excluding the `NO_OBJECT` placeholder).
    pub fn all() -> impl DoubleEndedIterator<Item = Obj> {
        (1..NOBJECTS as u16).map(Obj)
    }

    pub fn is_treasure(self) -> bool {
        self.info().is_treasure
    }

    /// True for the four gemstones that fit the cavity.
    pub fn is_gemstone(self) -> bool {
        matches!(self, EMERALD | RUBY | AMBER | SAPPH)
    }
}

impl Msg {
    pub fn text(self) -> Text {
        MESSAGES[self.index()]
    }
}

impl Motion {
    pub fn info(self) -> &'static MotionInfo {
        &MOTIONS[self.index()]
    }

    pub fn all() -> impl Iterator<Item = Motion> {
        (0..NMOTIONS as u16).map(Motion)
    }
}

impl Verb {
    pub fn info(self) -> &'static VerbInfo {
        &ACTIONS[self.index()]
    }

    pub fn all() -> impl Iterator<Item = Verb> {
        (0..NACTIONS as u16).map(Verb)
    }
}

include!(concat!(env!("OUT_DIR"), "/dungeon.rs"));

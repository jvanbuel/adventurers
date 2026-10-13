// SPDX-FileCopyrightText: Copyright Eric S. Raymond <esr@thyrsus.com>
// SPDX-License-Identifier: BSD-2-Clause
//! Dungeon types and the static dungeon database.
//!
//! The data tables themselves are generated from `adventure.yaml` by
//! `build.rs` and included at the bottom of this module.

#![allow(dead_code)]

/// No sound at a location.
pub const SILENT: i32 = -1;

// Symbols for cond bits
pub const COND_LIT: i32 = 0; // Light
pub const COND_OILY: i32 = 1; // If bit 2 is on: on for oil, off for water
pub const COND_FLUID: i32 = 2; // Liquid asset, see bit 1
pub const COND_NOARRR: i32 = 3; // Pirate doesn't go here unless following
pub const COND_NOBACK: i32 = 4; // Cannot use "back" to move away
pub const COND_ABOVE: i32 = 5; // Aboveground, but not in forest
pub const COND_DEEP: i32 = 6; // Deep - e.g where dwarves are active
pub const COND_FOREST: i32 = 7; // In the forest
pub const COND_FORCED: i32 = 8; // Only one way in or out of here
pub const COND_ALLDIFFERENT: i32 = 9; // Room is in maze all different
pub const COND_ALLALIKE: i32 = 10; // Room is in maze all alike

// Bits past 11 indicate areas of interest to "hint" routines
pub const COND_HBASE: i32 = 11; // Base for location hint bits
pub const COND_HCAVE: i32 = 12; // Trying to get into cave
pub const COND_HBIRD: i32 = 13; // Trying to catch bird
pub const COND_HSNAKE: i32 = 14; // Trying to deal with snake
pub const COND_HMAZE: i32 = 15; // Lost in maze
pub const COND_HDARK: i32 = 16; // Pondering dark room
pub const COND_HWITT: i32 = 17; // At Witt's End
pub const COND_HCLIFF: i32 = 18; // Cliff with urn
pub const COND_HWOODS: i32 = 19; // Lost in forest
pub const COND_HOGRE: i32 = 20; // Trying to deal with ogre
pub const COND_HJADE: i32 = 21; // Found all treasures except jade

pub type Msg = Option<&'static str>;

pub struct Object {
    pub words: &'static [&'static str],
    pub inventory: Msg,
    pub plac: i32,
    pub fixd: i32,
    pub is_treasure: bool,
    pub descriptions: &'static [Msg],
    pub sounds: &'static [Msg],
    pub texts: &'static [Msg],
    pub changes: &'static [Msg],
}

pub struct Location {
    pub small: Msg,
    pub big: Msg,
    pub sound: i32,
    pub loud: bool,
}

pub struct Obituary {
    pub query: Msg,
    pub yes_response: Msg,
}

pub struct TurnThreshold {
    pub threshold: i32,
    pub point_loss: i32,
    pub message: Msg,
}

pub struct Class {
    pub threshold: i32,
    pub message: Msg,
}

pub struct Hint {
    pub number: i32,
    pub turns: i32,
    pub penalty: i32,
    pub question: Msg,
    pub hint: Msg,
}

pub struct Motion {
    pub words: &'static [&'static str],
}

pub struct Action {
    pub words: &'static [&'static str],
    pub message: Msg,
    pub noaction: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CondType {
    Goto,
    Pct,
    Carry,
    With,
    Not,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DestType {
    Goto,
    Special,
    Speak,
}

pub struct TravelOp {
    pub motion: i32,
    pub condtype: CondType,
    pub condarg1: i32,
    pub condarg2: i32,
    pub desttype: DestType,
    pub destval: i32,
    pub nodwarves: bool,
    pub stop: bool,
}

include!(concat!(env!("OUT_DIR"), "/dungeon.rs"));

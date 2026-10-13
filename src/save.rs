// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Save files.
//!
//! The format is byte-for-byte the C implementation's in-memory
//! `struct save_t` on a little-endian machine with 4-byte ints, so save
//! files are interchangeable with Open Adventure's.  Field order and the
//! C names are listed in [`encode`].

use std::io::{self, Write};

use thiserror::Error;

use crate::dungeon::{Loc, NDEATHS, NDWARVES, NHINTS, NLOCATIONS, NOBJECTS, Obj};
use crate::objects::{Objects, STATE_NOTFOUND};
use crate::rng::Lcg;
use crate::state::{Bonus, Dwarf, Game, HintState, Knife};

/// Bump on save format change.
pub const SAVE_VERSION: i32 = 32;
/// Identifies the file to file(1) and the like.
const MAGIC: &[u8; 16] = b"open-adventure\n\0";
/// Detects endianness mismatch; unchanged by byte-swapping.
const ENDIAN_MAGIC: i32 = 2317;

const GAME_SIZE: usize = 38 * 4
    + 8
    + NLOCATIONS * 8
    + (NDWARVES + 1) * 12
    + NOBJECTS * 12
    + NHINTS * 8
    + NOBJECTS * 2 * 4;
/// Size in bytes of a save file.
pub const SAVE_SIZE: usize = MAGIC.len() + 4 + 4 + GAME_SIZE;

/// Why a save file could not be restored.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RestoreError {
    /// The file doesn't start with the save-file header.
    #[error("not an Open Adventure save file")]
    NotASave,
    /// The file was written by a version with a different format.
    #[error("save file is format version {found}, expected {SAVE_VERSION}")]
    VersionSkew {
        /// The format version in the file.
        found: i32,
    },
    /// Some value is out of range or inconsistent with the rest.
    #[error("save file contains impossible game state")]
    Tampered,
}

/// Serialize a game in the save-file format, with the given version
/// number in the header.
pub(crate) fn encode(game: &Game, version: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(SAVE_SIZE);
    let int = |out: &mut Vec<u8>, v: i32| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(MAGIC);
    int(&mut out, version);
    int(&mut out, ENDIAN_MAGIC);

    let g = game;
    #[rustfmt::skip]
    let scalars = [
        g.rng.state(),               // lcg_x
        g.long_desc_interval,        // abbnum
        g.bonus as i32,              // bonus
        g.chest_loc.raw(),           // chloc
        g.message_loc.raw(),         // chloc2
        g.closing_clock,             // clock1
        g.closed_clock,              // clock2
        i32::from(g.clue_read),      // clshnt
        i32::from(g.closed),         // closed
        i32::from(g.closing),        // closng
        i32::from(g.lamp_warned),    // lmwarn
        i32::from(g.novice),         // novice
        i32::from(g.panicked),       // panic
        i32::from(g.was_dark),       // wzdark
        i32::from(g.blooded),        // blooded
        g.hint_threshold,            // conds
        g.detail,                    // detail
        g.dwarf_activity,            // dflag
        g.dwarves_killed,            // dkill
        g.dwarves_here,              // dtotal
        g.fee_fie_progress,          // foobar
        g.objects.holding(),         // holdng
        g.go_count,                  // igo
        g.west_count,                // iwest
        g.knife.raw(),               // knfloc
        g.lamp_life,                 // limit
        g.loc.raw(),                 // loc
        g.newloc.raw(),              // newloc
        g.deaths,                    // numdie
        g.oldloc.raw(),              // oldloc
        g.oldloc2.raw(),             // oldlc2
        g.last_object.raw(),         // oldobj
        g.save_penalty,              // saved
        g.treasures_unseen,          // tally
        g.thresh,                    // thresh
        i32::from(g.seen_big_words), // seenbigwords
        g.turn_penalty,              // trnluz
        g.turns,                     // turns
    ];
    for v in scalars {
        int(&mut out, v);
    }
    out.extend_from_slice(&g.magic_word); // zzword
    out.extend_from_slice(&[0, 0]); // struct padding
    for (visits, atloc) in g.visits.iter().zip(g.objects.raw_atloc()) {
        int(&mut out, *visits); // locs[].abbrev
        int(&mut out, *atloc); // locs[].atloc
    }
    for d in &g.dwarves {
        int(&mut out, i32::from(d.seen));
        int(&mut out, d.loc.raw());
        int(&mut out, d.oldloc.raw());
    }
    for state in g.objects.raw_states() {
        for v in state {
            int(&mut out, v);
        }
    }
    for h in &g.hints {
        int(&mut out, i32::from(h.used));
        int(&mut out, h.turns_here);
    }
    for &l in g.objects.raw_link() {
        int(&mut out, l);
    }
    debug_assert_eq!(out.len(), SAVE_SIZE);
    out
}

/// Reads little-endian ints from a buffer.
struct Reader<'a> {
    buf: &'a [u8],
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> &[u8] {
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        head
    }
    fn int(&mut self) -> i32 {
        i32::from_le_bytes(self.bytes(4).try_into().expect("4 bytes"))
    }
    fn ints(&mut self, n: usize) -> Vec<i32> {
        (0..n).map(|_| self.int()).collect()
    }
}

/// Parse a save file into a game state that is safe to play.
///
/// A short file is treated as if padded with zeros, as the C version's
/// `fread` into a zeroed buffer would.
///
/// # Errors
///
/// [`RestoreError::NotASave`] for a bad header,
/// [`RestoreError::VersionSkew`] for another format version, and
/// [`RestoreError::Tampered`] if any value is out of range or the
/// state is inconsistent.
pub(crate) fn decode(bytes: &[u8]) -> Result<Game, RestoreError> {
    let mut buf = vec![0u8; SAVE_SIZE];
    let n = bytes.len().min(SAVE_SIZE);
    buf[..n].copy_from_slice(&bytes[..n]);
    let mut r = Reader { buf: &buf };

    let magic = r.bytes(MAGIC.len());
    let ok_magic = magic == MAGIC;
    let version = r.int();
    if !ok_magic || r.int() != ENDIAN_MAGIC {
        return Err(RestoreError::NotASave);
    }
    if version != SAVE_VERSION {
        return Err(RestoreError::VersionSkew { found: version });
    }
    parse_game(&mut r).ok_or(RestoreError::Tampered)
}

fn parse_game(r: &mut Reader) -> Option<Game> {
    let nloc = NLOCATIONS as i32;
    let loc = |n: i32| (0..nloc).contains(&n).then(|| Loc::from_raw(n));
    let s = r.ints(38);
    let magic_word: [u8; 6] = r.bytes(6).try_into().ok()?;
    r.bytes(2);
    let locs = r.ints(NLOCATIONS * 2);
    let dwarves = r.ints((NDWARVES + 1) * 3);
    let states = r.ints(NOBJECTS * 3);
    let hints = r.ints(NHINTS * 2);
    let link = r.ints(NOBJECTS * 2);

    let atloc = locs.iter().skip(1).step_by(2).copied().collect();
    let states: Vec<[i32; 3]> = states.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    let objects = Objects::from_raw(&states, atloc, link, s[21])?;

    let dwarves = dwarves
        .chunks(3)
        .map(|d| {
            Some(Dwarf {
                seen: d[0] != 0,
                loc: loc(d[1])?,
                oldloc: loc(d[2])?,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let game = Game {
        rng: Lcg::from_state(s[0])?,
        long_desc_interval: s[1],
        bonus: Bonus::from_raw(s[2])?,
        chest_loc: loc(s[3])?,
        message_loc: loc(s[4])?,
        closing_clock: s[5],
        closed_clock: s[6],
        clue_read: s[7] != 0,
        closed: s[8] != 0,
        closing: s[9] != 0,
        lamp_warned: s[10] != 0,
        novice: s[11] != 0,
        panicked: s[12] != 0,
        was_dark: s[13] != 0,
        blooded: s[14] != 0,
        hint_threshold: s[15],
        detail: s[16],
        dwarf_activity: s[17],
        dwarves_killed: s[18],
        dwarves_here: s[19],
        fee_fie_progress: s[20],
        go_count: s[22],
        west_count: s[23],
        knife: (-1..nloc)
            .contains(&s[24])
            .then(|| Knife::from_raw(s[24]))?,
        lamp_life: s[25],
        loc: loc(s[26])?,
        newloc: loc(s[27])?,
        deaths: s[28],
        oldloc: loc(s[29])?,
        oldloc2: loc(s[30])?,
        last_object: (0..NOBJECTS as i32)
            .contains(&s[31])
            .then(|| Obj::from_raw(s[31]))?,
        save_penalty: s[32],
        treasures_unseen: s[33],
        thresh: s[34],
        seen_big_words: s[35] != 0,
        turn_penalty: s[36],
        turns: s[37],
        magic_word,
        objects,
        visits: locs.iter().step_by(2).copied().collect(),
        dwarves,
        hints: hints
            .chunks(2)
            .map(|h| HintState {
                used: h[0] != 0,
                turns_here: h[1],
            })
            .collect(),
    };
    is_consistent(&game).then_some(game)
}

/// Checks on values that are in range but still impossible.
fn is_consistent(g: &Game) -> bool {
    let unseen = Obj::all()
        .filter(|&o| o.is_treasure() && g.objects.prop(o) == STATE_NOTFOUND)
        .count() as i32;
    let dwarf_count = 0..=NDWARVES as i32;
    g.long_desc_interval != 0 // it's used as a divisor
        && dwarf_count.contains(&g.dwarves_here)
        && dwarf_count.contains(&g.dwarves_killed)
        && g.deaths < NDEATHS as i32
        && unseen == g.treasures_unseen
}

/// Values to override in a freshly started game written by
/// [`write_new_game`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewSave {
    /// Seed for the random number generator.
    pub seed: i32,
    /// Number of deaths so far.
    pub deaths: i32,
    /// Remaining lamp life in turns, if not the default.
    pub lamp_life: Option<i32>,
    /// Points already deducted for saving.
    pub save_penalty: i32,
    /// Turns already taken.
    pub turns: i32,
    /// Format version written in the header; 0 means [`SAVE_VERSION`].
    pub version: i32,
}

impl Default for NewSave {
    fn default() -> Self {
        NewSave {
            seed: 0,
            deaths: 0,
            lamp_life: None,
            // A save costs points, so a written game has saved once.
            save_penalty: 1,
            turns: 0,
            version: 0,
        }
    }
}

/// Write a save file for a game that has just started, with some values
/// overridden.  This is how test saves of odd states are produced.
///
/// # Errors
///
/// Any error from writing to `out`.
pub fn write_new_game(mut out: impl Write, opts: &NewSave) -> io::Result<()> {
    let mut game = Game::new();
    game.reseed(opts.seed, |_| {});
    game.deaths = opts.deaths;
    if let Some(life) = opts.lamp_life {
        game.lamp_life = life;
    }
    game.save_penalty = opts.save_penalty;
    game.turns = opts.turns;
    let version = if opts.version == 0 {
        SAVE_VERSION
    } else {
        opts.version
    };
    out.write_all(&encode(&game, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Game {
        let mut g = Game::new();
        g.reseed(1234, |_| {});
        g
    }

    #[test]
    fn round_trip() {
        let g = seeded();
        let bytes = encode(&g, SAVE_VERSION);
        assert_eq!(bytes.len(), SAVE_SIZE);
        assert_eq!(decode(&bytes), Ok(g));
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(decode(b"#!/bin/sh\n"), Err(RestoreError::NotASave));
        assert_eq!(decode(&[]), Err(RestoreError::NotASave));
    }

    #[test]
    fn reports_version_skew() {
        let bytes = encode(&seeded(), 31);
        assert_eq!(decode(&bytes), Err(RestoreError::VersionSkew { found: 31 }));
    }

    #[test]
    fn detects_tampering() {
        let mut g = seeded();
        g.deaths = NDEATHS as i32;
        assert_eq!(
            decode(&encode(&g, SAVE_VERSION)),
            Err(RestoreError::Tampered)
        );

        let mut g = seeded();
        g.treasures_unseen += 1;
        assert_eq!(
            decode(&encode(&g, SAVE_VERSION)),
            Err(RestoreError::Tampered)
        );
    }

    #[test]
    fn truncated_body_is_tampered() {
        // Cut off before the description interval, which then reads as
        // zero; the original rejects that too (it is used as a divisor).
        let bytes = encode(&seeded(), SAVE_VERSION);
        assert_eq!(decode(&bytes[..28]), Err(RestoreError::Tampered));
    }

    #[test]
    fn new_game_overrides_apply() {
        let mut buf = Vec::new();
        let opts = NewSave {
            deaths: -900,
            turns: 7,
            ..NewSave::default()
        };
        write_new_game(&mut buf, &opts).unwrap();
        let g = decode(&buf).unwrap();
        assert_eq!((g.deaths, g.turns, g.save_penalty), (-900, 7, 1));
    }
}

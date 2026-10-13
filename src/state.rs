// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The complete, saveable state of a game in progress.

use crate::dungeon::{
    DWARFLOCS, LOC_DEADEND13, LOC_MAZEEND12, LOC_NOWHERE, LOC_START, Loc, NDWARVES, NHINTS,
    NLOCATIONS, NO_OBJECT, Obj,
};
use crate::objects::Objects;
use crate::rng::Lcg;

/// Base limit of turns before the lamp gives out.
pub(crate) const GAMELIMIT: i32 = 330;
/// Turns from the first closing warning until the cave closes.
pub(crate) const WARNTIME: i32 = 30;
/// Turns from the closing warning until the blinding flash.
pub(crate) const FLASHTIME: i32 = 50;

/// The bonus earned for how the end game went.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Bonus {
    #[default]
    None = 0,
    /// Blew yourself up.
    Splatter = 1,
    /// Blew up the wrong end of the repository.
    Defeat = 2,
    /// Blew up the right end.
    Victory = 3,
}

impl Bonus {
    pub(crate) fn from_raw(n: i32) -> Option<Bonus> {
        [Bonus::None, Bonus::Splatter, Bonus::Defeat, Bonus::Victory]
            .into_iter()
            .find(|b| *b as i32 == n)
    }
}

/// What has become of the knife a dwarf threw.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Knife {
    /// No knife has been thrown, or it has vanished unremarked.
    #[default]
    Absent,
    /// A knife was thrown at the player here.
    ThrownAt(Loc),
    /// The player tried to take it and was told knives vanish.
    Explained,
}

impl Knife {
    pub(crate) fn raw(self) -> i32 {
        match self {
            Knife::Absent => 0,
            Knife::ThrownAt(loc) => loc.raw(),
            Knife::Explained => -1,
        }
    }

    pub(crate) fn from_raw(n: i32) -> Knife {
        match n {
            0 => Knife::Absent,
            n if n < 0 => Knife::Explained,
            n => Knife::ThrownAt(Loc::from_raw(n)),
        }
    }
}

/// A dwarf, or (the last one) the pirate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Dwarf {
    /// The dwarf has seen the player and is following.
    pub seen: bool,
    /// Where it is; `LOC_NOWHERE` if dead or not yet in play.
    pub loc: Loc,
    pub oldloc: Loc,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct HintState {
    pub used: bool,
    /// Turns spent where the hint applies.
    pub turns_here: i32,
}

/// Everything that is saved in a save file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Game {
    pub rng: Lcg,
    /// The reservoir magic word, NUL-padded.
    pub magic_word: [u8; 6],
    pub objects: Objects,
    /// Times each location has been described since its long
    /// description was last given.
    pub visits: Vec<i32>,
    /// Dwarves 1..=NDWARVES; the last is the pirate.  Index 0 is unused.
    pub dwarves: Vec<Dwarf>,
    pub hints: Vec<HintState>,

    /// Give the long description every this many visits.
    pub long_desc_interval: i32,
    pub bonus: Bonus,
    /// Where the pirate's chest is hidden.
    pub chest_loc: Loc,
    /// Where the pirate leaves his message.
    pub message_loc: Loc,
    /// Turns from finding the last treasure until closing.
    pub closing_clock: i32,
    /// Turns from the closing warning until the cave closes.
    pub closed_clock: i32,
    /// The player has read the end-game clue.
    pub clue_read: bool,
    pub closed: bool,
    pub closing: bool,
    pub lamp_warned: bool,
    /// Asked for instructions at startup.
    pub novice: bool,
    /// Tried to leave once the cave began closing.
    pub panicked: bool,
    /// The location being left was dark.
    pub was_dark: bool,
    /// Drank the dragon's blood.
    pub blooded: bool,
    /// Lowest condition word at which hints are considered.
    pub hint_threshold: i32,
    /// Times "look" was used (after three, no more advice).
    pub detail: i32,
    /// 0: no dwarves yet; 1: reached the Hall of Mists; 2: met the
    /// first dwarf; 3: a knife has been thrown; higher: dwarves are mad.
    pub dwarf_activity: i32,
    pub dwarves_killed: i32,
    /// Dwarves (including the pirate) in the player's room.
    pub dwarves_here: i32,
    /// Progress through FEE FIE FOE FOO: the last word said, negated
    /// once another command intervenes.
    pub fee_fie_progress: i32,
    /// Times "go" was used with a direction.
    pub go_count: i32,
    /// Times "west" was spelled out.
    pub west_count: i32,
    pub knife: Knife,
    /// Turns of lamp life remaining.
    pub lamp_life: i32,
    pub loc: Loc,
    pub newloc: Loc,
    pub deaths: i32,
    pub oldloc: Loc,
    /// Where the player was two moves ago: the last safe place.
    pub oldloc2: Loc,
    pub last_object: Obj,
    /// Points deducted for saving.
    pub save_penalty: i32,
    /// Treasures not yet seen.
    pub treasures_unseen: i32,
    /// Unused; kept for the save format.
    pub thresh: i32,
    /// The player has seen the giant's graffiti.
    pub seen_big_words: bool,
    /// Points deducted for taking too many turns.
    pub turn_penalty: i32,
    pub turns: i32,
}

impl Game {
    /// A new game, with no seed applied yet.
    pub fn new() -> Game {
        let mut dwarves = vec![Dwarf::default(); NDWARVES + 1];
        for (d, &loc) in dwarves[1..].iter_mut().zip(DWARFLOCS.iter()) {
            d.loc = loc;
        }
        Game {
            rng: Lcg::default(),
            magic_word: [0; 6],
            objects: Objects::new(),
            visits: vec![0; NLOCATIONS],
            dwarves,
            hints: vec![HintState::default(); NHINTS],
            long_desc_interval: 5,
            bonus: Bonus::None,
            chest_loc: LOC_MAZEEND12,
            message_loc: LOC_DEADEND13,
            closing_clock: WARNTIME,
            closed_clock: FLASHTIME,
            clue_read: false,
            closed: false,
            closing: false,
            lamp_warned: false,
            novice: false,
            panicked: false,
            was_dark: false,
            blooded: false,
            hint_threshold: 1 << crate::dungeon::HINT_BASE,
            detail: 0,
            dwarf_activity: 0,
            dwarves_killed: 0,
            dwarves_here: 0,
            fee_fie_progress: 0,
            go_count: 0,
            west_count: 0,
            knife: Knife::Absent,
            lamp_life: GAMELIMIT,
            loc: LOC_START,
            newloc: LOC_START,
            deaths: 0,
            oldloc: LOC_NOWHERE,
            oldloc2: LOC_NOWHERE,
            last_object: NO_OBJECT,
            save_penalty: 0,
            treasures_unseen: Obj::all().filter(|o| o.is_treasure()).count() as i32,
            thresh: 0,
            seen_big_words: false,
            turn_penalty: 0,
            turns: 0,
        }
    }

    /// Reseed the generator and draw a new magic word, reporting each
    /// raw draw to `on_draw`.
    pub fn reseed(&mut self, seed: i32, mut on_draw: impl FnMut(i32)) {
        self.rng = Lcg::new(seed);
        for i in 0..5 {
            let raw = self.rng.next_raw();
            on_draw(raw);
            self.magic_word[i] = b'A' + Lcg::scale(raw, 26) as u8;
        }
        self.magic_word[1] = b'\'';
        self.magic_word[5] = 0;
    }

    /// The reservoir magic word, e.g. `Z'ZZZ`.
    pub fn magic_word(&self) -> String {
        let end = self.magic_word.iter().position(|&c| c == 0).unwrap_or(6);
        String::from_utf8_lossy(&self.magic_word[..end]).into_owned()
    }
}

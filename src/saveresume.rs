// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Saving and resuming.
//!
//! The save format is byte-for-byte the in-memory `struct save_t` of
//! the C implementation on a little-endian machine with 4-byte ints,
//! so save files are interchangeable with Open Adventure's.

use std::fs::File;
use std::io::{Read, Write};

use crate::advent::*;
use crate::dungeon::*;

/// Size in bytes of a serialized `Game`.
pub const GAME_SIZE: usize = 38 * 4
    + 8
    + NLOCATIONS * 8
    + (NDWARVES + 1) * 12
    + NOBJECTS * 12
    + NHINTS * 8
    + NOBJECTS * 2 * 4;
/// Size in bytes of a serialized `Save`.
pub const SAVE_SIZE: usize = 16 + 4 + 4 + GAME_SIZE;

struct Writer(Vec<u8>);

impl Writer {
    fn i(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn b(&mut self, v: bool) {
        self.i(v as i32);
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn i(&mut self) -> i32 {
        let v = i32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        v
    }
    fn b(&mut self) -> bool {
        self.i() != 0
    }
}

impl Game {
    fn write(&self, w: &mut Writer) {
        for v in [
            self.lcg_x,
            self.abbnum,
            self.bonus,
            self.chloc,
            self.chloc2,
            self.clock1,
            self.clock2,
        ] {
            w.i(v);
        }
        for v in [
            self.clshnt,
            self.closed,
            self.closng,
            self.lmwarn,
            self.novice,
            self.panic,
            self.wzdark,
            self.blooded,
        ] {
            w.b(v);
        }
        for v in [
            self.conds,
            self.detail,
            self.dflag,
            self.dkill,
            self.dtotal,
            self.foobar,
            self.holdng,
            self.igo,
            self.iwest,
            self.knfloc,
            self.limit,
            self.loc,
            self.newloc,
            self.numdie,
            self.oldloc,
            self.oldlc2,
            self.oldobj,
            self.saved,
            self.tally,
            self.thresh,
        ] {
            w.i(v);
        }
        w.b(self.seenbigwords);
        w.i(self.trnluz);
        w.i(self.turns);
        w.0.extend_from_slice(&self.zzword);
        w.0.extend_from_slice(&[0, 0]); // struct padding
        for l in &self.locs {
            w.i(l.abbrev);
            w.i(l.atloc);
        }
        for d in &self.dwarves {
            w.b(d.seen);
            w.i(d.loc);
            w.i(d.oldloc);
        }
        for o in &self.objects {
            w.i(o.fixed);
            w.i(o.prop);
            w.i(o.place);
        }
        for h in &self.hints {
            w.b(h.used);
            w.i(h.lc);
        }
        for &l in &self.link {
            w.i(l);
        }
    }

    fn read(r: &mut Reader) -> Game {
        let mut g = Game::zeroed();
        g.lcg_x = r.i();
        g.abbnum = r.i();
        g.bonus = r.i();
        g.chloc = r.i();
        g.chloc2 = r.i();
        g.clock1 = r.i();
        g.clock2 = r.i();
        g.clshnt = r.b();
        g.closed = r.b();
        g.closng = r.b();
        g.lmwarn = r.b();
        g.novice = r.b();
        g.panic = r.b();
        g.wzdark = r.b();
        g.blooded = r.b();
        g.conds = r.i();
        g.detail = r.i();
        g.dflag = r.i();
        g.dkill = r.i();
        g.dtotal = r.i();
        g.foobar = r.i();
        g.holdng = r.i();
        g.igo = r.i();
        g.iwest = r.i();
        g.knfloc = r.i();
        g.limit = r.i();
        g.loc = r.i();
        g.newloc = r.i();
        g.numdie = r.i();
        g.oldloc = r.i();
        g.oldlc2 = r.i();
        g.oldobj = r.i();
        g.saved = r.i();
        g.tally = r.i();
        g.thresh = r.i();
        g.seenbigwords = r.b();
        g.trnluz = r.i();
        g.turns = r.i();
        g.zzword.copy_from_slice(&r.buf[r.pos..r.pos + TOKLEN + 1]);
        r.pos += TOKLEN + 1 + 2;
        for l in g.locs.iter_mut() {
            l.abbrev = r.i();
            l.atloc = r.i();
        }
        for d in g.dwarves.iter_mut() {
            d.seen = r.b();
            d.loc = r.i();
            d.oldloc = r.i();
        }
        for o in g.objects.iter_mut() {
            o.fixed = r.i();
            o.prop = r.i();
            o.place = r.i();
        }
        for h in g.hints.iter_mut() {
            h.used = r.b();
            h.lc = r.i();
        }
        for l in g.link.iter_mut() {
            *l = r.i();
        }
        g
    }
}

impl Save {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer(Vec::with_capacity(SAVE_SIZE));
        w.0.extend_from_slice(&self.magic);
        w.i(self.version);
        w.i(self.canary);
        self.game.write(&mut w);
        debug_assert_eq!(w.0.len(), SAVE_SIZE);
        w.0
    }

    pub fn from_bytes(buf: &[u8]) -> Save {
        assert!(buf.len() >= SAVE_SIZE);
        let mut magic = [0u8; 16];
        magic.copy_from_slice(&buf[..16]);
        let mut r = Reader { buf, pos: 16 };
        let version = r.i();
        let canary = r.i();
        let game = Game::read(&mut r);
        Save {
            magic,
            version,
            canary,
            game,
        }
    }
}

/// Trim leading and trailing whitespace; might be left there by
/// autocomplete.
fn strip(name: &str) -> &str {
    name.trim_matches(|c: char| c.is_ascii_whitespace())
}

impl Advent {
    /// Save game to file. No input or output from user.
    pub fn savefile(&mut self, fp: &mut File) -> std::io::Result<()> {
        self.save.magic = *ADVENT_MAGIC;
        if self.save.version == 0 {
            self.save.version = SAVE_VERSION;
        }
        if self.save.canary == 0 {
            self.save.canary = ENDIAN_MAGIC;
        }
        self.save.game = self.game.clone();
        fp.write_all(&self.save.to_bytes())
    }

    /// Prompt for a file name until one opens; None if input ran out.
    fn prompt_for_file(&mut self, writing: bool) -> Option<File> {
        loop {
            let line = self.myreadline("\nFile name: ")?;
            let name = strip(&line);
            if name.is_empty() {
                return None;
            }
            let fp = if writing {
                File::create(name)
            } else {
                File::open(name)
            };
            match fp {
                Ok(f) => return Some(f),
                Err(_) => println!("Can't open file {name}, try again."),
            }
        }
    }

    /// Suspend.  Offer to save things in a file, but charging some
    /// points (so can't win by using saved games to retry battles or to
    /// start over after learning zzword).
    pub(crate) fn suspend(&mut self) -> Phase {
        if cfg!(any(feature = "nosave", feature = "autosave")) {
            self.rspeak(SAVERESUME_DISABLED, &[]);
            return Phase::Top;
        }

        self.rspeak(SUSPEND_WARNING, &[]);
        let ok = ARBITRARY_MESSAGES[OK_MAN as usize];
        if !self.yes_or_no(ARBITRARY_MESSAGES[THIS_ACCEPTABLE as usize], ok, ok) {
            return Phase::ClearObj;
        }
        self.game.saved += 5;

        let Some(mut fp) = self.prompt_for_file(true) else {
            return Phase::Top;
        };

        let _ = self.savefile(&mut fp);
        drop(fp);
        self.rspeak(RESUME_HELP, &[]);
        crate::misc::exit(0);
    }

    /// Resume.  Read a suspended game back from a file.
    pub(crate) fn resume(&mut self) -> Phase {
        if cfg!(any(feature = "nosave", feature = "autosave")) {
            self.rspeak(SAVERESUME_DISABLED, &[]);
            return Phase::Top;
        }

        if self.game.loc != LOC_START || self.game.locs[LOC_START as usize].abbrev != 1 {
            self.rspeak(RESUME_ABANDON, &[]);
            let ok = ARBITRARY_MESSAGES[OK_MAN as usize];
            if !self.yes_or_no(ARBITRARY_MESSAGES[THIS_ACCEPTABLE as usize], ok, ok) {
                return Phase::ClearObj;
            }
        }

        let Some(fp) = self.prompt_for_file(false) else {
            return Phase::Top;
        };
        self.restore(fp)
    }

    /// Read and restore game state from file, assuming sane initial
    /// state.
    pub fn restore(&mut self, mut fp: File) -> Phase {
        if cfg!(feature = "nosave") {
            self.rspeak(SAVERESUME_DISABLED, &[]);
            return Phase::Top;
        }

        // Like fread() into the save buffer: a short file leaves the
        // tail of the previous contents in place.
        let mut buf = self.save.to_bytes();
        let mut filled = 0;
        while filled < buf.len() {
            match fp.read(&mut buf[filled..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => filled += n,
            }
        }
        drop(fp);
        self.save = Save::from_bytes(&buf);

        if self.save.magic != *ADVENT_MAGIC || self.save.canary != ENDIAN_MAGIC {
            self.rspeak(BAD_SAVE, &[]);
        } else if self.save.version != SAVE_VERSION {
            let v = self.save.version;
            self.rspeak(
                VERSION_SKEW,
                &[
                    Arg::I(v / 10),
                    Arg::I(v % 10),
                    Arg::I(SAVE_VERSION / 10),
                    Arg::I(SAVE_VERSION % 10),
                ],
            );
        } else if !is_valid(&self.save.game) {
            self.rspeak(SAVE_TAMPERING, &[]);
            crate::misc::exit(0);
        } else {
            self.game = self.save.game.clone();
        }
        Phase::Top
    }

    #[cfg(feature = "autosave")]
    pub fn autosave(&mut self) {
        use std::io::{Seek, SeekFrom};
        if let Some(mut fp) = self.autosave_fp.take() {
            let _ = fp.seek(SeekFrom::Start(0));
            let _ = self.savefile(&mut fp);
            let _ = fp.flush();
            self.autosave_fp = Some(fp);
        }
    }
}

/// Save files can be roughly grouped into three groups: with valid,
/// reachable state, with valid, but unreachable state and with invalid
/// state. We check that state is valid: no states are outside minimal
/// or maximal value.
pub fn is_valid(valgame: &Game) -> bool {
    let nloc = NLOCATIONS as i32;
    let nobj = NOBJECTS as i32;

    // Prevent division by zero
    if valgame.abbnum == 0 {
        return false;
    }

    // Check for RNG overflow. Truncate
    if valgame.lcg_x as i64 >= LCG_M {
        return false;
    }

    // Bounds check for locations
    if valgame.chloc < -1
        || valgame.chloc >= nloc
        || valgame.chloc2 < -1
        || valgame.chloc2 >= nloc
        || valgame.loc < 0
        || valgame.loc >= nloc
        || valgame.newloc < 0
        || valgame.newloc >= nloc
        || valgame.oldloc < 0
        || valgame.oldloc >= nloc
        || valgame.oldlc2 < 0
        || valgame.oldlc2 >= nloc
    {
        return false;
    }

    // Bounds check for location arrays
    for d in &valgame.dwarves {
        if d.loc < -1 || d.loc >= nloc || d.oldloc < -1 || d.oldloc >= nloc {
            return false;
        }
    }

    for o in &valgame.objects {
        if o.place < -1 || o.place >= nloc || o.fixed < -1 || o.fixed >= nloc {
            return false;
        }
    }

    // Bounds check for dwarves
    if valgame.dtotal < 0
        || valgame.dtotal > NDWARVES as i32
        || valgame.dkill < 0
        || valgame.dkill > NDWARVES as i32
    {
        return false;
    }

    // Validate that we didn't die too many times in save
    if valgame.numdie >= NDEATHS as i32 {
        return false;
    }

    // Recalculate tally, throw the towel if in disagreement
    let mut temp_tally = 0;
    for treasure in 1..NOBJECTS {
        if OBJECTS[treasure].is_treasure && valgame.objects[treasure].prop == STATE_NOTFOUND {
            temp_tally += 1;
        }
    }
    if temp_tally != valgame.tally {
        return false;
    }

    // Check that properties of objects aren't beyond expected
    if valgame.objects.iter().any(|o| prop_is_invalid(o.prop)) {
        return false;
    }

    // Check that values in linked lists for objects in locations are
    // inside bounds
    if valgame
        .locs
        .iter()
        .any(|l| l.atloc < NO_OBJECT || l.atloc >= nobj * 2)
    {
        return false;
    }
    if valgame.link.iter().any(|&l| l < NO_OBJECT || l >= nobj * 2) {
        return false;
    }

    true
}

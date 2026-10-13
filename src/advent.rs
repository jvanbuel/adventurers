// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Game types, constants and predicates (the port of advent.h).

use std::fs::File;
use std::io::BufRead;

use crate::dungeon::*;

/// Program version, substituted for `%V` in messages.
pub const VERSION: &str = "1.22";

// LCG PRNG parameters tested against Knuth vol. 2. by the original authors
pub const LCG_A: i64 = 1093;
pub const LCG_C: i64 = 221587;
pub const LCG_M: i64 = 1048576;

pub const LINESIZE: usize = 1024;
pub const TOKLEN: usize = 5; // # outputting characters in a token
pub const PIRATE: usize = NDWARVES;
pub const DALTLC: i32 = LOC_NUGGET; // alternate dwarf location
pub const INVLIMIT: i32 = 7; // inventory limit (# of objects)
pub const INTRANSITIVE: i32 = -1; // illegal object number
pub const GAMELIMIT: i32 = 330; // base limit of turns
pub const NOVICELIMIT: i32 = 1000; // limit of turns for novice
pub const WARNTIME: i32 = 30; // late game starts at game.limit-this
pub const FLASHTIME: i32 = 50; // turns from first warning till blinding flash
pub const PANICTIME: i32 = 15; // time left after closing
pub const BATTERYLIFE: i32 = 2500; // turn limit increment from batteries
pub const WORD_NOT_FOUND: i32 = -1; // "Word not found" flag value for the vocab hash functions.
pub const WORD_EMPTY: i32 = 0; // "Word empty" flag value for the vocab hash functions
pub const PIT_KILL_PROB: i32 = 35; // Percentage probability of dying from fall in pit.
pub const CARRIED: i32 = -1; // Player is toting it

// Special object-state values - integers > 0 are object-specific
pub const STATE_NOTFOUND: i32 = -1; // 'Not found" state of treasures
pub const STATE_FOUND: i32 = 0; // After discovered, before messed with
pub const STATE_IN_CAVITY: i32 = 1; // State value common to all gemstones

// Special fixed object-state values - integers > 0 are location
pub const IS_FIXED: i32 = -1;
pub const IS_FREE: i32 = 0;

pub const PROMPT: &str = "> ";

/// Bump on save format change.
pub const SAVE_VERSION: i32 = 32;
/// Goes at start of file so saves can be identified by file(1) and the like.
pub const ADVENT_MAGIC: &[u8; 16] = b"open-adventure\n\0";
/// Used to detect endianness mismatch.  Can't be unchanged by byte-swapping.
pub const ENDIAN_MAGIC: i32 = 2317;

/// Map a state property value to the negative "stashed" range.
pub const fn prop_stashify(n: i32) -> i32 {
    -1 - n
}

pub const fn prop_is_invalid(val: i32) -> bool {
    val < -MAX_STATE - 1 || val > MAX_STATE
}

/// True if OBJ is a gemstone.
pub fn gstone(obj: i32) -> bool {
    obj == EMERALD || obj == RUBY || obj == AMBER || obj == SAPPH
}

/// Returns true if the specified bit is set in the mask.
pub fn tstbit(mask: i32, bit: i32) -> bool {
    (mask & (1 << bit)) != 0
}

/// Returns 2**bit for use in constructing bit-masks.
pub fn setbit(bit: i32) -> i32 {
    1 << bit
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BugType {
    SpecialTravel500GtLGt300ExceedsGotoList,
    VocabularyTypeNOver1000NotBetween0And3,
    IntransitiveActionVerbExceedsGotoList,
    TransitiveActionVerbExceedsGotoList,
    ConditionalTravelEntryWithNoAlteration,
    LocationHasNoTravelEntries,
    HintNumberExceedsGotoList,
    SpeechpartNotTransitiveOrIntransitiveOrUnknown,
    ActionReturnedPhaseCodeBeyondEndOfSwitch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpeakType {
    Touch,
    Look,
    Hear,
    Study,
    Change,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Termination {
    EndGame,
    QuitGame,
    ScoreGame,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpeechPart {
    Unknown,
    Intransitive,
    Transitive,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WordType {
    NoWordType,
    Motion,
    Object,
    Action,
    Numeric,
}

// Kinds of finishing bonus (score_t)
pub const BONUS_NONE: i32 = 0;
pub const BONUS_SPLATTER: i32 = 1;
pub const BONUS_DEFEAT: i32 = 2;
pub const BONUS_VICTORY: i32 = 3;

/// Phase codes for action returns.
/// These were at one time FORTRAN line numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Terminate,
    Move,
    Top,
    ClearObj,
    CheckHint,
    Word2,
    Unknown,
    DwarfWake,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct LocState {
    pub abbrev: i32, // has location been seen?
    pub atloc: i32,  // head of object linked list per location
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct DwarfState {
    pub seen: bool,  // true if dwarf has seen him
    pub loc: i32,    // location of dwarves, initially hard-wired in
    pub oldloc: i32, // prior loc of each dwarf, initially garbage
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ObjState {
    pub fixed: i32, // fixed location of object (if not IS_FREE)
    pub prop: i32,  // object state
    pub place: i32, // location of object
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct HintState {
    pub used: bool, // hints[i].used = true iff hint i has been used.
    pub lc: i32,    // hints[i].lc = show int at LOC with cond bit i
}

/// The complete, saveable state of a game in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub lcg_x: i32,
    pub abbnum: i32,   // How often to print int descriptions
    pub bonus: i32,    // What kind of finishing bonus we are getting
    pub chloc: i32,    // pirate chest location
    pub chloc2: i32,   // pirate chest alternate location
    pub clock1: i32,   // # turns from finding last treasure to close
    pub clock2: i32,   // # turns from warning till blinding flash
    pub clshnt: bool,  // has player read the clue in the endgame?
    pub closed: bool,  // whether we're all the way closed
    pub closng: bool,  // whether it's closing time yet
    pub lmwarn: bool,  // has player been warned about lamp going dim?
    pub novice: bool,  // asked for instructions at start-up?
    pub panic: bool,   // has player found out he's trapped?
    pub wzdark: bool,  // whether the loc he's leaving was dark
    pub blooded: bool, // has player drunk of dragon's blood?
    pub conds: i32,    // min value for cond[loc] if loc has any hints
    pub detail: i32,   // level of detail in descriptions

    ///  dflag controls the level of activation of dwarves:
    ///   0  No dwarf stuff yet (wait until reaches Hall Of Mists)
    ///   1  Reached Hall Of Mists, but hasn't met first dwarf
    ///   2  Met 1t dwarf, others start moving, no knives thrown yet
    ///   3  A knife has been thrown (first set always misses)
    ///   3+ Dwarves are mad (increases their accuracy)
    pub dflag: i32,

    pub dkill: i32,               // dwarves killed
    pub dtotal: i32,              // total dwarves (including pirate) in loc
    pub foobar: i32,              // progress in saying "FEE FIE FOE FOO".
    pub holdng: i32,              // number of objects being carried
    pub igo: i32,                 // # uses of "go" instead of a direction
    pub iwest: i32,               // # times he's said "west" instead of "w"
    pub knfloc: i32,              // knife location; LOC_NOWERE if none, -1 after caveat
    pub limit: i32,               // lifetime of lamp
    pub loc: i32,                 // where player is now
    pub newloc: i32,              // where player is going
    pub numdie: i32,              // number of times killed so far
    pub oldloc: i32,              // where player was
    pub oldlc2: i32,              // where player was two moves ago
    pub oldobj: i32,              // last object player handled
    pub saved: i32,               // point penalty for saves
    pub tally: i32,               // count of treasures gained
    pub thresh: i32,              // current threshold for endgame scoring tier
    pub seenbigwords: bool,       // have we red the graffiti in the Giant's Room?
    pub trnluz: i32,              // # points lost so far due to turns used
    pub turns: i32,               // counts commands given (ignores yes/no)
    pub zzword: [u8; TOKLEN + 1], // randomly generated magic word from bird
    pub locs: Vec<LocState>,
    pub dwarves: Vec<DwarfState>,
    pub objects: Vec<ObjState>,
    pub hints: Vec<HintState>,
    pub link: Vec<i32>, // object-list links
}

impl Game {
    /// All-zero state, as for a C static that has not been initialized.
    pub fn zeroed() -> Game {
        Game {
            lcg_x: 0,
            abbnum: 0,
            bonus: BONUS_NONE,
            chloc: 0,
            chloc2: 0,
            clock1: 0,
            clock2: 0,
            clshnt: false,
            closed: false,
            closng: false,
            lmwarn: false,
            novice: false,
            panic: false,
            wzdark: false,
            blooded: false,
            conds: 0,
            detail: 0,
            dflag: 0,
            dkill: 0,
            dtotal: 0,
            foobar: 0,
            holdng: 0,
            igo: 0,
            iwest: 0,
            knfloc: 0,
            limit: 0,
            loc: 0,
            newloc: 0,
            numdie: 0,
            oldloc: 0,
            oldlc2: 0,
            oldobj: 0,
            saved: 0,
            tally: 0,
            thresh: 0,
            seenbigwords: false,
            trnluz: 0,
            turns: 0,
            zzword: [0; TOKLEN + 1],
            locs: vec![LocState::default(); NLOCATIONS],
            dwarves: vec![DwarfState::default(); NDWARVES + 1],
            objects: vec![ObjState::default(); NOBJECTS],
            hints: vec![HintState::default(); NHINTS],
            link: vec![0; NOBJECTS * 2],
        }
    }

    /// The statically initialized starting state, before initialise().
    pub fn new() -> Game {
        // Last dwarf is special (the pirate).  He always starts at his
        // chest's eventual location inside the maze. This loc is saved
        // in chloc for ref. The dead end in the other maze has its
        // loc stored in chloc2.
        Game {
            chloc: LOC_MAZEEND12,
            chloc2: LOC_DEADEND13,
            abbnum: 5,
            clock1: WARNTIME,
            clock2: FLASHTIME,
            newloc: LOC_START,
            loc: LOC_START,
            limit: GAMELIMIT,
            foobar: WORD_EMPTY,
            ..Game::zeroed()
        }
    }

    /// The magic word as a string (up to its NUL terminator).
    pub fn zzword_str(&self) -> String {
        let end = self
            .zzword
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(self.zzword.len());
        String::from_utf8_lossy(&self.zzword[..end]).into_owned()
    }
}

impl Default for Game {
    fn default() -> Self {
        Game::new()
    }
}

/// Game application settings - settings, but not state of the game, per se.
/// This data is not saved in a saved game.
#[derive(Default)]
pub struct Settings {
    pub logfp: Option<File>,
    pub oldstyle: bool,
    pub prompt: bool,
    pub argv: Vec<String>,
    pub optind: usize,
    pub debug: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandWord {
    pub raw: String,
    pub id: i32,
    pub wtype: WordType,
}

impl CommandWord {
    /// Represent an empty command word.
    pub fn empty() -> CommandWord {
        CommandWord {
            raw: String::new(),
            id: WORD_EMPTY,
            wtype: WordType::NoWordType,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum CommandState {
    Empty,
    Raw,
    Tokenized,
    Given,
    Preprocessed,
    Processing,
    Executed,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub part: SpeechPart,
    pub word: [CommandWord; 2],
    pub verb: i32,
    pub obj: i32,
    pub state: CommandState,
}

impl Command {
    pub fn new() -> Command {
        Command {
            part: SpeechPart::Unknown,
            word: [CommandWord::empty(), CommandWord::empty()],
            verb: 0,
            obj: 0,
            state: CommandState::Empty,
        }
    }
}

impl Default for Command {
    fn default() -> Self {
        Command::new()
    }
}

/// The on-disk save record.
#[derive(Clone, Debug)]
pub struct Save {
    pub magic: [u8; 16],
    pub version: i32,
    pub canary: i32,
    pub game: Game,
}

impl Save {
    pub fn zeroed() -> Save {
        Save {
            magic: [0; 16],
            version: 0,
            canary: 0,
            game: Game::zeroed(),
        }
    }
}

/// Where script input is currently coming from.
pub(crate) enum Script {
    Stdin,
    File(Box<dyn BufRead>),
}

/// Argument for a message with format specifiers.
#[derive(Clone, Copy, Debug)]
pub enum Arg<'a> {
    I(i32),
    S(&'a str),
}

/// The game engine: the game state plus everything needed to run it.
pub struct Advent {
    pub game: Game,
    pub settings: Settings,
    pub save: Save,
    /// Location condition bits (initialised from the dungeon database,
    /// then augmented with COND_FORCED at startup).
    pub conditions: Vec<i32>,
    pub(crate) command: Command,
    pub(crate) mxscor: i32,
    pub(crate) script: Option<Script>,
    #[cfg(feature = "autosave")]
    pub autosave_fp: Option<File>,
}

impl Default for Advent {
    fn default() -> Self {
        Advent::new()
    }
}

impl Advent {
    pub fn new() -> Advent {
        Advent {
            game: Game::new(),
            settings: Settings {
                prompt: true,
                ..Settings::default()
            },
            save: Save::zeroed(),
            conditions: CONDITIONS.to_vec(),
            command: Command::new(),
            mxscor: 0,
            script: None,
            #[cfg(feature = "autosave")]
            autosave_fp: None,
        }
    }

    // ---- Object state accessors ----------------------------------------

    pub fn prop(&self, obj: i32) -> i32 {
        self.game.objects[obj as usize].prop
    }
    pub fn set_prop(&mut self, obj: i32, val: i32) {
        self.game.objects[obj as usize].prop = val;
    }
    pub fn place(&self, obj: i32) -> i32 {
        self.game.objects[obj as usize].place
    }
    pub fn set_place(&mut self, obj: i32, val: i32) {
        self.game.objects[obj as usize].place = val;
    }
    pub fn fixed(&self, obj: i32) -> i32 {
        self.game.objects[obj as usize].fixed
    }
    pub fn set_fixed(&mut self, obj: i32, val: i32) {
        self.game.objects[obj as usize].fixed = val;
    }
    pub fn atloc(&self, loc: i32) -> i32 {
        self.game.locs[loc as usize].atloc
    }

    pub fn object_is_notfound(&self, obj: i32) -> bool {
        self.prop(obj) == STATE_NOTFOUND
    }
    pub fn object_is_found(&self, obj: i32) -> bool {
        self.prop(obj) == STATE_FOUND
    }
    pub fn object_set_found(&mut self, obj: i32) {
        self.set_prop(obj, STATE_FOUND);
    }
    pub fn object_set_not_found(&mut self, obj: i32) {
        self.set_prop(obj, STATE_NOTFOUND);
    }
    pub fn object_stashify(&mut self, obj: i32, pval: i32) {
        self.set_prop(obj, prop_stashify(pval));
    }
    pub fn object_is_stashed(&self, obj: i32) -> bool {
        self.prop(obj) < STATE_NOTFOUND
    }
    pub fn object_state_equals(&self, obj: i32, pval: i32) -> bool {
        self.prop(obj) == pval || self.prop(obj) == prop_stashify(pval)
    }

    // ---- Predicates (the macros of advent.h) ----------------------------

    /// True if the OBJ is being carried.
    pub fn toting(&self, obj: i32) -> bool {
        self.place(obj) == CARRIED
    }
    /// True if on either side of two-placed object.
    pub fn at(&self, obj: i32) -> bool {
        self.place(obj) == self.game.loc || self.fixed(obj) == self.game.loc
    }
    /// True if the OBJ is at "LOC" (or is being carried).
    pub fn here(&self, obj: i32) -> bool {
        self.at(obj) || self.toting(obj)
    }
    /// True if COND(L) has bit n set (bit 0 is units bit).
    pub fn cndbit(&self, loc: i32, n: i32) -> bool {
        tstbit(self.conditions[loc as usize], n)
    }
    /// Object number of liquid in bottle.
    pub fn liquid(&self) -> i32 {
        match self.prop(BOTTLE) {
            WATER_BOTTLE => WATER,
            OIL_BOTTLE => OIL,
            _ => NO_OBJECT,
        }
    }
    /// Object number of liquid (if any) at LOC.
    pub fn liqloc(&self, loc: i32) -> i32 {
        if self.cndbit(loc, COND_FLUID) {
            if self.cndbit(loc, COND_OILY) {
                OIL
            } else {
                WATER
            }
        } else {
            NO_OBJECT
        }
    }
    /// True if LOC moves without asking for input (COND=2).
    pub fn forced(&self, loc: i32) -> bool {
        self.cndbit(loc, COND_FORCED)
    }
    /// True if location "LOC" is dark.
    pub fn is_dark_here(&self) -> bool {
        !self.cndbit(self.game.loc, COND_LIT) && (self.prop(LAMP) == LAMP_DARK || !self.here(LAMP))
    }
    /// True N% of the time (N integer from 0 to 100).
    pub fn pct(&mut self, n: i32) -> bool {
        self.randrange(100) < n
    }
    /// True if LOC is part of the forest.
    pub fn forest(&self, loc: i32) -> bool {
        self.cndbit(loc, COND_FOREST)
    }
    /// True if location not in the cave.
    pub fn outside(&self, loc: i32) -> bool {
        self.cndbit(loc, COND_ABOVE) || self.forest(loc)
    }
    /// True if location is in the cave or the building at the beginning
    /// of the game.
    pub fn inside(&self, loc: i32) -> bool {
        !self.outside(loc) || loc == LOC_BUILDING
    }
    /// True if location is in the Hall of Mists or deeper.
    pub fn indeep(&self, loc: i32) -> bool {
        self.cndbit(loc, COND_DEEP)
    }

    /// Get rid of an item by putting it in LOC_NOWHERE.
    pub fn destroy(&mut self, obj: i32) {
        self.move_obj(obj, LOC_NOWHERE);
    }
}

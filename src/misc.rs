// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! I/O and support routines.

use std::fs::File;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};

use crate::advent::*;
use crate::dungeon::*;

/// C's isspace() set, which is what sscanf("%s") splits on.
fn is_c_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// Split like repeated sscanf("%s") calls would.
pub(crate) fn c_tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_c_space).filter(|t| !t.is_empty())
}

/// strncasecmp(a, b, n) == 0
fn strncaseeq(a: &str, b: &str, n: usize) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    for i in 0..n {
        let ca = a.get(i).copied().unwrap_or(0).to_ascii_lowercase();
        let cb = b.get(i).copied().unwrap_or(0).to_ascii_lowercase();
        if ca != cb {
            return false;
        }
        if ca == 0 {
            return true;
        }
    }
    true
}

/// Read one line from standard input, as readline() would: the prompt
/// is shown only when interactive, and the newline is stripped.
fn read_stdin_line(prompt: &str) -> Option<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        print!("{prompt}");
    }
    let _ = io::stdout().flush();
    let mut buf = Vec::new();
    match stdin.lock().read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            if buf.last() == Some(&b'\n') {
                buf.pop();
            }
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
    }
}

/// Read one raw line (newline included) from a script source.
fn read_script_line(reader: &mut dyn BufRead) -> Option<String> {
    let mut buf = Vec::new();
    match reader.read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(String::from_utf8_lossy(&buf).into_owned()),
    }
}

/// Flush everything and leave.
pub fn exit(code: i32) -> ! {
    let _ = io::stdout().flush();
    std::process::exit(code);
}

impl Advent {
    /// This function isn't required for gameplay, readline() straight
    /// up would suffice for that.  It's where we interpret command-line
    /// logfiles for testing purposes.
    pub fn myreadline(&mut self, prompt: &str) -> Option<String> {
        // Normal case - no script arguments
        if self.settings.argv.is_empty() {
            let ln = read_stdin_line(prompt);
            if ln.is_none() {
                print!("{prompt}");
            }
            return ln;
        }

        loop {
            if self.script.is_none() {
                if self.settings.optind >= self.settings.argv.len() {
                    return None;
                }
                let next = self.settings.argv[self.settings.optind].clone();
                self.settings.optind += 1;
                if next == "-" {
                    self.script = Some(Script::Stdin);
                } else {
                    match File::open(&next) {
                        Ok(f) => self.script = Some(Script::File(Box::new(BufReader::new(f)))),
                        Err(_) => {
                            eprintln!("Can't open script {next}");
                            continue;
                        }
                    }
                }
            }

            let ln = match self.script.as_mut() {
                Some(Script::Stdin) => {
                    if io::stdin().is_terminal() && !self.settings.oldstyle {
                        return read_stdin_line(prompt);
                    }
                    read_script_line(&mut io::stdin().lock())
                }
                Some(Script::File(r)) => read_script_line(r.as_mut()),
                None => unreachable!(),
            };
            match ln {
                Some(ln) => {
                    print!("{prompt}{ln}");
                    return Some(ln);
                }
                None => self.script = None,
            }
        }
    }

    // ---- I/O routines (speak, pspeak, rspeak, sspeak, get_input, yes) ----

    /// Engine for various speak functions.
    fn vspeak(&self, msg: Msg, blank: bool, args: &[Arg]) {
        // Do nothing if we got a null pointer or an empty string.
        let Some(msg) = msg else { return };
        if msg.is_empty() {
            return;
        }

        if blank {
            println!();
        }

        let bytes = msg.as_bytes();
        let mut rendered: Vec<u8> = Vec::with_capacity(bytes.len() + 16);
        let mut args = args.iter();
        // Handle format specifiers (including the custom %S).
        let mut pluralize = false;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'%' {
                // Ugh.  Least obtrusive way to deal with artifacts "on
                // the floor" being dropped outside of both cave and
                // building.
                if bytes[i..].starts_with(b"floor")
                    && matches!(bytes.get(i + 5), None | Some(b' ') | Some(b'.'))
                    && !self.inside(self.game.loc)
                {
                    rendered.extend_from_slice(b"ground");
                    i += 5;
                    continue;
                }
                rendered.push(bytes[i]);
            } else {
                i += 1;
                match bytes.get(i) {
                    // Integer specifier.
                    Some(b'd') => {
                        let arg = match args.next() {
                            Some(Arg::I(n)) => *n,
                            _ => 0,
                        };
                        rendered.extend_from_slice(arg.to_string().as_bytes());
                        pluralize = arg != 1;
                    }
                    // Unmodified string specifier.
                    Some(b's') => {
                        if let Some(Arg::S(s)) = args.next() {
                            rendered.extend_from_slice(s.as_bytes());
                        }
                    }
                    // Singular/plural specifier: look at the *previous*
                    // numeric parameter.
                    Some(b'S') => {
                        if pluralize {
                            rendered.push(b's');
                        }
                    }
                    // Version specifier
                    Some(b'V') => rendered.extend_from_slice(VERSION.as_bytes()),
                    _ => {}
                }
            }
            i += 1;
        }

        // Print the message.
        println!("{}", String::from_utf8_lossy(&rendered));
    }

    /// Speak a specified string.
    pub fn speak(&self, msg: Msg, args: &[Arg]) {
        self.vspeak(msg, true, args);
    }

    /// Speak a message from the arbitrary-messages list, printf-style.
    pub fn sspeak(&self, msg: i32, args: &[Arg]) {
        let fmt = ARBITRARY_MESSAGES[msg as usize].unwrap_or("");
        let mut out = String::new();
        let mut args = args.iter();
        let mut chars = fmt.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some('s') => {
                    if let Some(Arg::S(s)) = args.next() {
                        out.push_str(s);
                    }
                }
                Some('d') => {
                    if let Some(Arg::I(n)) = args.next() {
                        out.push_str(&n.to_string());
                    }
                }
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        }
        print!("\n{out}\n");
    }

    /// Find the skip+1st message from msg and print it.  Modes are:
    /// touch = for inventory, what you can touch
    /// look = the full description for the state the object is in
    /// hear = the sound for the state the object is in
    /// study = text on the object
    /// change = the message for a change to the given state
    pub fn pspeak(&self, msg: i32, mode: SpeakType, blank: bool, skip: i32, args: &[Arg]) {
        let obj = &OBJECTS[msg as usize];
        let pick = |list: &'static [Msg]| -> Msg {
            if skip < 0 {
                None
            } else {
                list.get(skip as usize).copied().flatten()
            }
        };
        let text = match mode {
            SpeakType::Touch => obj.inventory,
            SpeakType::Look => pick(obj.descriptions),
            SpeakType::Hear => pick(obj.sounds),
            SpeakType::Study => pick(obj.texts),
            SpeakType::Change => pick(obj.changes),
        };
        self.vspeak(text, blank, args);
    }

    /// Print the i-th "random" message (section 6 of database).
    pub fn rspeak(&self, i: i32, args: &[Arg]) {
        self.vspeak(ARBITRARY_MESSAGES[i as usize], true, args);
    }

    fn get_input(&mut self) -> Option<String> {
        // Set up the prompt
        let input_prompt = if self.settings.prompt { PROMPT } else { "" };

        // Print a blank line
        println!();

        let mut input;
        loop {
            input = self.myreadline(input_prompt)?;
            if input.starts_with('#') {
                // Ignore comments.
                continue;
            }
            // We have a 'normal' line; leave the loop.
            break;
        }

        // Strip trailing newlines from the input
        if let Some(pos) = input.find('\n') {
            input.truncate(pos);
        }

        if !io::stdin().is_terminal() {
            println!("{input_prompt}{input}");
        }

        if let Some(fp) = self.settings.logfp.as_mut() {
            let _ = writeln!(fp, "{input}");
            let _ = fp.flush();
        }

        Some(input)
    }

    /// Read a reply and classify it; None means it was neither yes nor no.
    fn classify_reply(reply: &str) -> Option<bool> {
        let firstword = c_tokens(reply).next().unwrap_or("").to_ascii_lowercase();
        if firstword.starts_with('y') {
            Some(true)
        } else if firstword.starts_with('n') {
            Some(false)
        } else {
            None
        }
    }

    pub fn silent_yes_or_no(&mut self) -> bool {
        loop {
            let Some(reply) = self.get_input() else {
                // Should be unreachable. Reply should never be NULL
                exit(0);
            };
            if reply.is_empty() {
                self.rspeak(PLEASE_ANSWER, &[]);
                continue;
            }
            match Self::classify_reply(&reply) {
                Some(outcome) => return outcome,
                None => self.rspeak(PLEASE_ANSWER, &[]),
            }
        }
    }

    /// Print message X, wait for yes/no answer.  If yes, print Y and
    /// return true; if no, print Z and return false.
    pub fn yes_or_no(&mut self, question: Msg, yes_response: Msg, no_response: Msg) -> bool {
        loop {
            self.speak(question, &[]);

            let Some(reply) = self.get_input() else {
                // Should be unreachable. Reply should never be NULL
                exit(0);
            };

            if reply.is_empty() {
                self.rspeak(PLEASE_ANSWER, &[]);
                continue;
            }

            match Self::classify_reply(&reply) {
                Some(true) => {
                    self.speak(yes_response, &[]);
                    return true;
                }
                Some(false) => {
                    self.speak(no_response, &[]);
                    return false;
                }
                None => self.rspeak(PLEASE_ANSWER, &[]),
            }
        }
    }

    // ---- Vocabulary lookup ----------------------------------------------

    fn single_letter_ok(&self, word: &str) -> bool {
        word.len() > 1
            || !IGNORE.as_bytes().contains(&word.as_bytes()[0])
            || !self.settings.oldstyle
    }

    /// Return the first motion number that has 'word' as one of its words.
    fn get_motion_vocab_id(&self, word: &str) -> i32 {
        for (i, m) in MOTIONS.iter().enumerate() {
            for w in m.words {
                if strncaseeq(word, w, TOKLEN) && self.single_letter_ok(word) {
                    return i as i32;
                }
            }
        }
        WORD_NOT_FOUND
    }

    /// Return the first object number that has 'word' as one of its words.
    fn get_object_vocab_id(&self, word: &str) -> i32 {
        for (i, o) in OBJECTS.iter().enumerate() {
            for w in o.words {
                if strncaseeq(word, w, TOKLEN) {
                    return i as i32;
                }
            }
        }
        WORD_NOT_FOUND
    }

    /// Return the first action number that has 'word' as one of its words.
    fn get_action_vocab_id(&self, word: &str) -> i32 {
        for (i, a) in ACTIONS.iter().enumerate() {
            for w in a.words {
                if strncaseeq(word, w, TOKLEN) && self.single_letter_ok(word) {
                    return i as i32;
                }
            }
        }
        WORD_NOT_FOUND
    }

    fn get_vocab_metadata(&self, word: &str) -> (i32, WordType) {
        // Check for an empty string
        if word.is_empty() {
            return (WORD_EMPTY, WordType::NoWordType);
        }

        let ref_num = self.get_motion_vocab_id(word);
        if ref_num != WORD_NOT_FOUND {
            return (ref_num, WordType::Motion);
        }

        let ref_num = self.get_object_vocab_id(word);
        if ref_num != WORD_NOT_FOUND {
            return (ref_num, WordType::Object);
        }

        let ref_num = self.get_action_vocab_id(word);
        if ref_num != WORD_NOT_FOUND && ref_num != PART {
            return (ref_num, WordType::Action);
        }

        // Check for the reservoir magic word.
        if word.eq_ignore_ascii_case(&self.game.zzword_str()) {
            return (PART, WordType::Action);
        }

        // Check words that are actually numbers.
        if is_valid_int(word) {
            return (WORD_EMPTY, WordType::Numeric);
        }

        (WORD_NOT_FOUND, WordType::NoWordType)
    }

    fn tokenize(&mut self, raw: &str) {
        // Be careful about modifying this. We do not want to nuke the
        // the speech part or ID from the previous turn.
        let mut toks = c_tokens(raw);
        let mut w0 = toks.next().unwrap_or("").to_string();
        let mut w1 = toks.next().unwrap_or("").to_string();

        // (ESR) In oldstyle mode, simulate the uppercasing and truncating
        // effect on raw tokens of packing them into sixbit characters, 5
        // to a 32-bit word.  This is something the FORTRAN version did
        // because archaic FORTRAN had no string types.
        const TRUNCLEN: usize = TOKLEN + TOKLEN;
        if self.settings.oldstyle {
            for w in [&mut w0, &mut w1] {
                if w.len() > TRUNCLEN {
                    let mut cut = TRUNCLEN;
                    while !w.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    w.truncate(cut);
                }
                w.make_ascii_uppercase();
            }
        }

        // populate command with parsed vocabulary metadata
        let (id0, t0) = self.get_vocab_metadata(&w0);
        let (id1, t1) = self.get_vocab_metadata(&w1);
        let cmd = &mut self.command;
        cmd.word[0] = CommandWord {
            raw: w0,
            id: id0,
            wtype: t0,
        };
        cmd.word[1] = CommandWord {
            raw: w1,
            id: id1,
            wtype: t1,
        };
        cmd.state = CommandState::Tokenized;
    }

    /// Get user input on stdin, parse and map to command.
    pub(crate) fn get_command_input(&mut self) -> bool {
        let mut input;
        loop {
            match self.get_input() {
                None => return false,
                Some(s) => input = s,
            }
            if word_count(&input) > 2 {
                self.rspeak(TWO_WORDS, &[]);
                continue;
            }
            if !input.is_empty() {
                break;
            }
        }

        if input.len() > LINESIZE - 1 {
            let mut cut = LINESIZE - 1;
            while !input.is_char_boundary(cut) {
                cut -= 1;
            }
            input.truncate(cut);
        }

        self.tokenize(&input);
        self.command.state = CommandState::Given;
        true
    }

    /// Resets the state of the command to empty.
    pub(crate) fn clear_command(&mut self) {
        self.command.verb = ACT_NULL;
        self.command.part = SpeechPart::Unknown;
        self.game.oldobj = self.command.obj;
        self.command.obj = NO_OBJECT;
        self.command.state = CommandState::Empty;
    }

    // ---- Data structure routines ---------------------------------------

    /// Juggle an object by picking it up and putting it down again, the
    /// purpose being to get the object to the front of the chain of
    /// things at its loc.
    pub fn juggle(&mut self, object: i32) {
        let i = self.place(object);
        let j = self.fixed(object);
        self.move_obj(object, i);
        self.move_obj(object + NOBJECTS as i32, j);
    }

    /// Place any object anywhere by picking it up and dropping it.  May
    /// already be toting, in which case the carry is a no-op.  Mustn't
    /// pick up objects which are not at any loc, since carry wants to
    /// remove objects from game atloc chains.
    pub fn move_obj(&mut self, object: i32, where_: i32) {
        let from = if object >= NOBJECTS as i32 {
            self.fixed(object - NOBJECTS as i32)
        } else {
            self.place(object)
        };
        if from != LOC_NOWHERE && from != CARRIED {
            self.carry(object, from);
        }
        self.drop(object, where_);
    }

    /// The same as move(), except the object is stashed and can no
    /// longer be picked up.
    pub fn put(&mut self, object: i32, where_: i32, pval: i32) {
        self.move_obj(object, where_);
        self.object_stashify(object, pval);
    }

    /// Start toting an object, removing it from the list of things at
    /// its former location.  Incr holdng unless it was already being
    /// toted.  If object>NOBJECTS (moving "fixed" second loc), don't
    /// change game.place or game.holdng.
    pub fn carry(&mut self, object: i32, where_: i32) {
        if object < NOBJECTS as i32 {
            if self.place(object) == CARRIED {
                return;
            }
            self.set_place(object, CARRIED);

            // Without this conditional your inventory is overcounted
            // when you pick up the bird while it's caged. This fixes
            // a cosmetic bug in the original.
            if object != BIRD {
                self.game.holdng += 1;
            }
        }
        let w = where_ as usize;
        if self.game.locs[w].atloc == object {
            self.game.locs[w].atloc = self.game.link[object as usize];
            return;
        }
        let mut temp = self.game.locs[w].atloc;
        while self.game.link[temp as usize] != object {
            temp = self.game.link[temp as usize];
        }
        self.game.link[temp as usize] = self.game.link[object as usize];
    }

    /// Place an object at a given loc, prefixing it onto the game atloc
    /// list.  Decr game.holdng if the object was being toted. No state
    /// change on the object.
    pub fn drop(&mut self, object: i32, where_: i32) {
        if object >= NOBJECTS as i32 {
            self.set_fixed(object - NOBJECTS as i32, where_);
        } else {
            // The bird has to be weightless.  This ugly hack (and the
            // corresponding code in the carry function) brought to you
            // by the fact that when the bird is caged, we need to be
            // able to either 'take bird' or 'take cage' and have the
            // right thing happen.
            if self.place(object) == CARRIED && object != BIRD {
                self.game.holdng -= 1;
            }
            self.set_place(object, where_);
        }
        if where_ == LOC_NOWHERE || where_ == CARRIED {
            return;
        }
        self.game.link[object as usize] = self.game.locs[where_ as usize].atloc;
        self.game.locs[where_ as usize].atloc = object;
    }

    /// Return the index of first dwarf at the given location, zero if no
    /// dwarf is there (or if dwarves not active yet), -1 if all dwarves
    /// are dead.  Ignore the pirate (6th dwarf).
    pub fn atdwrf(&self, where_: i32) -> i32 {
        if self.game.dflag < 2 {
            return 0;
        }
        let mut at = -1;
        for i in 1..NDWARVES {
            if self.game.dwarves[i].loc == where_ {
                return i as i32;
            }
            if self.game.dwarves[i].loc != 0 {
                at = 0;
            }
        }
        at
    }

    // ---- Utility routines (set_seed, get_next_lcg_value, randrange) ----

    /// Set the LCG1 seed.
    pub fn set_seed(&mut self, seedval: i32) {
        let mut x = (seedval as i64 % LCG_M) as i32;
        if x < 0 {
            x += LCG_M as i32;
        }
        self.game.lcg_x = x;
        // once seed is set, we need to generate the Z`ZZZ word
        for i in 0..5 {
            self.game.zzword[i] = b'A' + self.randrange(26) as u8;
        }
        self.game.zzword[1] = b'\''; // force second char to apostrophe
        self.game.zzword[5] = 0;
    }

    /// Return the LCG's current value, and then iterate it.
    fn get_next_lcg_value(&mut self) -> i32 {
        let old_x = self.game.lcg_x;
        self.game.lcg_x = ((LCG_A * old_x as i64 + LCG_C) % LCG_M) as i32;
        if self.settings.debug != 0 {
            println!("# random {old_x}");
        }
        old_x
    }

    /// Return a random integer from [0, range).
    pub fn randrange(&mut self, range: i32) -> i32 {
        (range as i64 * self.get_next_lcg_value() as i64 / LCG_M) as i32
    }

    /// Object must have a change-message list for this to be useful;
    /// only some do.
    pub fn state_change(&mut self, obj: i32, state: i32) {
        self.set_prop(obj, state);
        self.pspeak(obj, SpeakType::Change, true, state, &[]);
    }
}

/// Count words delimited by blanks and tabs.
fn word_count(s: &str) -> usize {
    s.split([' ', '\t']).filter(|w| !w.is_empty()).count()
}

/// Returns true if the string passed in is represents a valid integer,
/// that could then be parsed by atoi().
fn is_valid_int(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
}

/// C's atol(): optional whitespace and sign, then leading digits.
pub fn atol(s: &str) -> i64 {
    let s = s.trim_start_matches(is_c_space);
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut n: i64 = 0;
    for c in digits.bytes().take_while(|c| c.is_ascii_digit()) {
        n = n.wrapping_mul(10).wrapping_add((c - b'0') as i64);
    }
    if neg {
        -n
    } else {
        n
    }
}

/// C's strtol(s, NULL, 10): like atol() but saturating on overflow.
pub fn strtol(s: &str) -> i64 {
    let s = s.trim_start_matches(is_c_space);
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut n: i64 = 0;
    for c in digits.bytes().take_while(|c| c.is_ascii_digit()) {
        let d = (c - b'0') as i64;
        n = match n.checked_mul(10).and_then(|v| {
            if neg {
                v.checked_sub(d)
            } else {
                v.checked_add(d)
            }
        }) {
            Some(v) => v,
            None => return if neg { i64::MIN } else { i64::MAX },
        };
    }
    n
}

/// Report a "can't happen" condition and exit.
pub fn bug(num: BugType) -> ! {
    let _ = io::stdout().flush();
    eprintln!("Fatal error {}, {:?}.", num as i32, num);
    std::process::exit(1);
}

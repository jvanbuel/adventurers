// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//
//  Adventure (rev 2: 20 treasures)
//  History: Original idea & 5-treasure version (adventures) by Willie Crowther
//           15-treasure version (adventure) by Don Woods, April-June 1977
//           20-treasure version (rev 2) by Don Woods, August 1978
//           Errata fixed: 78/12/25
//           Revived 2017 as Open Adventure.
//           Ported to Rust as adventurers.

use std::fs::File;
use std::io::Write;

use adventurers::advent::*;
use adventurers::dungeon::*;
use adventurers::misc::exit;
use adventurers::Advent;

#[cfg(feature = "autosave")]
const OPTS: &str = "dl:oa:";
#[cfg(feature = "autosave")]
const USAGE: &str = "[-l logfilename] [-o] [-a filename] [script...]";
#[cfg(all(not(feature = "autosave"), not(feature = "nosave")))]
const OPTS: &str = "dl:or:";
#[cfg(all(not(feature = "autosave"), not(feature = "nosave")))]
const USAGE: &str = "[-l logfilename] [-o] [-r restorefilename] [script...]";
#[cfg(all(not(feature = "autosave"), feature = "nosave"))]
const OPTS: &str = "dl:o";
#[cfg(all(not(feature = "autosave"), feature = "nosave"))]
const USAGE: &str = "[-l logfilename] [-o] [script...]";

/// Minimal GNU-style getopt: options may be clustered and may appear
/// anywhere before "--"; everything else is an operand.
fn getopt(
    args: &[String],
    opts: &str,
    progname: &str,
) -> (Vec<(char, Option<String>)>, Vec<String>) {
    let mut found = Vec::new();
    let mut operands = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" {
            operands.extend(args[i..].iter().cloned());
            break;
        }
        if !a.starts_with('-') || a == "-" {
            operands.push(a.clone());
            continue;
        }
        let chars: Vec<char> = a[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            j += 1;
            let spec = opts.find(c).filter(|_| c != ':');
            match spec {
                None => {
                    eprintln!("{progname}: invalid option -- '{c}'");
                    found.push(('?', None));
                }
                Some(pos) if opts[pos + 1..].starts_with(':') => {
                    let rest: String = chars[j..].iter().collect();
                    if !rest.is_empty() {
                        found.push((c, Some(rest)));
                    } else if i < args.len() {
                        found.push((c, Some(args[i].clone())));
                        i += 1;
                    } else {
                        eprintln!("{progname}: option requires an argument -- '{c}'");
                        found.push(('?', None));
                    }
                    break;
                }
                Some(_) => found.push((c, None)),
            }
        }
    }
    (found, operands)
}

fn usage(progname: &str) -> ! {
    eprintln!("Usage: {progname} {USAGE}");
    eprintln!("        -l create a log file of your game named as specified'");
    eprintln!("        -o 'oldstyle' (no prompt, no command editing, displays 'Initialising...')");
    #[cfg(feature = "autosave")]
    eprintln!("        -a automatic save/restore from specified saved game file");
    #[cfg(all(not(feature = "autosave"), not(feature = "nosave")))]
    eprintln!("        -r restore from specified saved game file");
    exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let progname = args
        .first()
        .cloned()
        .unwrap_or_else(|| "advent".to_string());
    let mut adv = Advent::new();
    #[allow(unused_mut)]
    let mut rfp: Option<File> = None;
    #[cfg(feature = "autosave")]
    let mut autosave_filename: Option<String> = None;

    let (opts, operands) = getopt(&args[1..], OPTS, &progname);
    for (ch, optarg) in opts {
        match ch {
            'd' => adv.settings.debug += 1,
            'l' => {
                let name = optarg.unwrap();
                match File::create(&name) {
                    Ok(f) => adv.settings.logfp = Some(f),
                    Err(_) => eprintln!("advent: can't open logfile {name} for write"),
                }
            }
            'o' => {
                adv.settings.oldstyle = true;
                adv.settings.prompt = false;
            }
            #[cfg(feature = "autosave")]
            'a' => {
                let name = optarg.unwrap();
                rfp = File::open(&name).ok();
                autosave_filename = Some(name);
            }
            #[cfg(all(not(feature = "autosave"), not(feature = "nosave")))]
            'r' => {
                let name = optarg.unwrap();
                match File::open(&name) {
                    Ok(f) => rfp = Some(f),
                    Err(_) => eprintln!("advent: can't open save file {name} for read"),
                }
            }
            _ => usage(&progname),
        }
    }

    // copy invocation line part after switches
    adv.settings.argv = operands;
    adv.settings.optind = 0;

    // Initialize game variables
    let seedval = adv.initialise();

    match rfp {
        None => {
            adv.game.novice = adv.yes_or_no(
                ARBITRARY_MESSAGES[WELCOME_YOU as usize],
                ARBITRARY_MESSAGES[CAVE_NEARBY as usize],
                ARBITRARY_MESSAGES[NO_MESSAGE as usize],
            );
            if adv.game.novice {
                adv.game.limit = NOVICELIMIT;
            }
        }
        Some(f) => {
            adv.restore(f);
            #[cfg(feature = "autosave")]
            adv.score(Termination::ScoreGame);
        }
    }

    #[cfg(feature = "autosave")]
    if let Some(name) = autosave_filename {
        match File::create(&name) {
            Ok(f) => adv.autosave_fp = Some(f),
            Err(e) => {
                eprintln!("{name}: {e}");
                exit(1);
            }
        }
        adv.autosave();
    }

    if let Some(fp) = adv.settings.logfp.as_mut() {
        let _ = writeln!(fp, "seed {seedval}");
    }

    // interpret commands until EOF or interrupt
    adv.run();
}

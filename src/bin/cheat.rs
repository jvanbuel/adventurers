// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//
// 'cheat' is a tool for generating save game files to test states that
// ought not happen. It leverages chunks of advent, mostly initialise()
// and savefile(), so we know we're always outputting save files that
// advent can import.

use std::fs::File;

use adventurers::misc::{atol, exit};
use adventurers::Advent;

const USAGE: &str = "Usage: {} [-d numdie] [-s numsaves] [-v version] -o savefilename \n\
        -d number of deaths. Signed integer.\n\
        -l lifetime of lamp in turns. Signed integer.\n\
        -s number of saves. Signed integer.\n\
        -t number of turns. Signed integer.\n\
        -v version number of save format.\n\
        -o required. File name of save game to write.";

fn usage(progname: &str) {
    eprintln!("{}", USAGE.replacen("{}", progname, 1));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let progname = args.first().cloned().unwrap_or_else(|| "cheat".to_string());
    let mut adv = Advent::new();
    let mut savefilename: Option<String> = None;

    // Initialize game variables
    adv.initialise();

    // we're generating a saved game, so saved once by default, unless
    // overridden with command-line options below.
    adv.game.saved = 1;

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" || !a.starts_with('-') || a.len() < 2 {
            break;
        }
        let opt = a.as_bytes()[1] as char;
        if !"dlstvo".contains(opt) {
            eprintln!("{progname}: invalid option -- '{opt}'");
            usage(&progname);
            exit(1);
        }
        let optarg = if a.len() > 2 {
            a[2..].to_string()
        } else if i < args.len() {
            i += 1;
            args[i - 1].clone()
        } else {
            eprintln!("{progname}: option requires an argument -- '{opt}'");
            usage(&progname);
            exit(1);
        };
        let n = atol(&optarg) as i32;
        match opt {
            'd' => {
                adv.game.numdie = n;
                println!("cheat: game.numdie = {n}");
            }
            'l' => {
                adv.game.limit = n;
                println!("cheat: game.limit = {n}");
            }
            's' => {
                adv.game.saved = n;
                println!("cheat: game.saved = {n}");
            }
            't' => {
                adv.game.turns = n;
                println!("cheat: game.turns = {n}");
            }
            'v' => {
                adv.save.version = n;
                println!("cheat: version = {n}");
            }
            'o' => savefilename = Some(optarg),
            _ => unreachable!(),
        }
    }

    // Save filename required; the point of cheat is to generate save file
    let Some(savefilename) = savefilename else {
        usage(&progname);
        eprintln!("ERROR: filename required");
        exit(1);
    };

    let mut fp = match File::create(&savefilename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("Can't open file {savefilename}. Exiting.");
            exit(1);
        }
    };

    if adv.savefile(&mut fp).is_err() {
        eprintln!("Can't write file {savefilename}. Exiting.");
        exit(1);
    }

    println!("cheat: {savefilename} created.");
}

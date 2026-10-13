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

//! The `advent` command: play Colossal Cave Adventure.

mod terminal;

use std::fs::{self, File};
use std::path::PathBuf;
use std::process::ExitCode;

use adventurers::{Config, Engine, SaveMode, Start, random_seed};
use clap::{ArgAction, Parser};

use terminal::Terminal;

/// Colossal Cave Adventure, the 430-point version.
#[derive(Parser, Debug)]
#[command(name = "advent", version)]
struct Cli {
    /// Create a log file of your game named as specified
    #[arg(short = 'l', value_name = "LOGFILE")]
    log: Option<PathBuf>,

    /// 'Oldstyle' (no prompt, no command editing, displays 'Initialising...')
    #[arg(short = 'o')]
    oldstyle: bool,

    /// Restore from specified saved game file
    #[arg(short = 'r', value_name = "SAVEFILE", hide = cfg!(any(feature = "nosave", feature = "autosave")))]
    restore: Option<PathBuf>,

    /// Automatic save/restore from specified saved game file
    #[arg(short = 'a', value_name = "SAVEFILE", hide = cfg!(not(feature = "autosave")))]
    autosave: Option<PathBuf>,

    /// Trace random number generation
    #[arg(short = 'd', action = ArgAction::Count, hide = true)]
    debug: u8,

    /// Script files of commands to play ('-' for standard input)
    scripts: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if (cli.restore.is_some() && cfg!(any(feature = "nosave", feature = "autosave")))
        || (cli.autosave.is_some() && cfg!(not(feature = "autosave")))
    {
        eprintln!("advent: that option is not available in this build");
        return ExitCode::FAILURE;
    }

    let log = cli.log.as_ref().and_then(|path| match File::create(path) {
        Ok(f) => Some(f),
        Err(_) => {
            eprintln!("advent: can't open logfile {} for write", path.display());
            None
        }
    });

    // A save to restore from, if one was named and can be read.
    let mut start = Start::New;
    if let Some(path) = &cli.restore {
        match fs::read(path) {
            Ok(bytes) => start = Start::Restore(bytes),
            Err(_) => eprintln!("advent: can't open save file {} for read", path.display()),
        }
    }
    let mut saves = SaveMode::default();
    if cfg!(feature = "nosave") {
        saves = SaveMode::Disabled;
    }
    if let Some(path) = &cli.autosave {
        if let Ok(bytes) = fs::read(path) {
            start = Start::Restore(bytes);
        }
        match File::create(path) {
            Ok(f) => saves = SaveMode::Auto(f),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    } else if cfg!(feature = "autosave") {
        saves = SaveMode::Disabled;
    }

    let config = Config {
        oldstyle: cli.oldstyle,
        debug: cli.debug > 0,
        saves,
    };
    let console = Terminal::new(cli.scripts, log, cli.oldstyle);
    let mut engine = Engine::new(console, config, random_seed());

    let _over = match engine.start(start) {
        Err(over) => over,
        Ok(()) => {
            let seed = engine.seed();
            engine.console_mut().log_seed(seed);
            engine.run()
        }
    };
    engine.console_mut().flush();
    ExitCode::SUCCESS
}

// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause

//! `cheat` writes save files of states that ought not to happen, for
//! testing `advent`'s handling of them.

use std::fs::File;
use std::path::PathBuf;
use std::process::ExitCode;

use adventurers::random_seed;
use adventurers::save::{NewSave, write_new_game};
use clap::Parser;

/// Generate an Open Adventure save file with unusual values.
#[derive(Parser, Debug)]
#[command(name = "cheat", allow_negative_numbers = true)]
struct Cli {
    /// Number of deaths
    #[arg(short = 'd', value_name = "NUMDIE")]
    deaths: Option<i32>,
    /// Lifetime of lamp in turns
    #[arg(short = 'l', value_name = "TURNS")]
    lamp: Option<i32>,
    /// Number of saves
    #[arg(short = 's', value_name = "NUMSAVES")]
    saves: Option<i32>,
    /// Number of turns
    #[arg(short = 't', value_name = "TURNS")]
    turns: Option<i32>,
    /// Version number of save format
    #[arg(short = 'v', value_name = "VERSION")]
    version: Option<i32>,
    /// File name of save game to write
    #[arg(short = 'o', value_name = "SAVEFILE", required = true)]
    output: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut opts = NewSave {
        seed: random_seed(),
        ..NewSave::default()
    };
    if let Some(n) = cli.deaths {
        opts.deaths = n;
        println!("cheat: game.numdie = {n}");
    }
    if let Some(n) = cli.lamp {
        opts.lamp_life = Some(n);
        println!("cheat: game.limit = {n}");
    }
    if let Some(n) = cli.saves {
        opts.save_penalty = n;
        println!("cheat: game.saved = {n}");
    }
    if let Some(n) = cli.turns {
        opts.turns = n;
        println!("cheat: game.turns = {n}");
    }
    if let Some(n) = cli.version {
        opts.version = n;
        println!("cheat: version = {n}");
    }

    let path = &cli.output;
    let written = File::create(path).and_then(|f| write_new_game(f, &opts));
    if let Err(e) = written {
        eprintln!("cheat: can't write {}: {e}", path.display());
        return ExitCode::FAILURE;
    }
    println!("cheat: {} created.", path.display());
    ExitCode::SUCCESS
}

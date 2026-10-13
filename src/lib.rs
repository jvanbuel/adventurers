// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Colossal Cave Adventure: a Rust port of Open Adventure.
//!
//! The game is the 430-point version 2.5 by Will Crowther and Don Woods,
//! as maintained by Eric S. Raymond in Open Adventure.  It behaves
//! identically to the C version: same parser, same random numbers, same
//! messages, and interchangeable save files.
//!
//! The library's interface is small.  An [`Engine`] runs the game,
//! talking to the player through a [`Console`]:
//!
//! ```
//! use adventurers::{Config, Engine, GameOver, ScriptedConsole, Start};
//!
//! let console = ScriptedConsole::new(["no", "in", "take lamp", "quit", "yes"]);
//! let mut engine = Engine::new(console, Config::default(), 42);
//! let over = engine.play(Start::New);
//!
//! assert!(matches!(over, GameOver::Finished { .. }));
//! assert!(engine.console().transcript().contains("> take lamp\n\nOK\n"));
//! ```
//!
//! [`save::write_new_game`] writes save files of unusual states, for
//! testing.

mod console;
mod dungeon;
mod engine;
mod objects;
mod parser;
mod rng;
pub mod save;
mod state;
mod text;

pub use console::{Console, ScriptedConsole};
pub use engine::{Config, Engine, GameOver, SaveMode, Start};

/// A seed for a new game, from the clock and the process id.
pub fn random_seed() -> i32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    h.write_u128(now.as_nanos());
    h.write_u32(std::process::id());
    (h.finish() & 0x7fff_ffff) as i32
}

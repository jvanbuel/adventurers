// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! Open Adventure, ported to Rust.
//!
//! This is a faithful port of Eric S. Raymond's Open Adventure, the
//! forward port of Crowther & Woods' Colossal Cave Adventure 2.5 (the
//! 430-point version).

// The code deliberately mirrors the structure of the C original (index
// loops over parallel tables, fall-through if/else chains) so the two
// can be compared line by line.
#![allow(clippy::needless_range_loop, clippy::if_same_then_else)]

pub mod actions;
pub mod advent;
pub mod dungeon;
pub mod game;
pub mod init;
pub mod misc;
pub mod saveresume;
pub mod score;

pub use advent::Advent;

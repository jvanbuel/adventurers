// SPDX-FileCopyrightText: (C) 1977, 2005 by Will Crowther and Don Woods
// SPDX-License-Identifier: BSD-2-Clause
//! The game's linear congruential random number generator.
//!
//! The parameters were tested against Knuth vol. 2 by the original
//! authors.  The generator's state is part of a saved game, and the
//! regression tests depend on its exact sequence.

const LCG_A: i64 = 1093;
const LCG_C: i64 = 221_587;
pub(crate) const LCG_M: i64 = 1_048_576;

/// A seeded LCG.  Each draw yields the current state, then advances it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Lcg {
    x: i32,
}

impl Lcg {
    /// A generator seeded with any `i32`; the seed is reduced into range.
    pub fn new(seed: i32) -> Lcg {
        Lcg {
            x: (i64::from(seed).rem_euclid(LCG_M)) as i32,
        }
    }

    /// Return the current state and advance the generator.
    pub fn next_raw(&mut self) -> i32 {
        let old = self.x;
        self.x = ((LCG_A * i64::from(old) + LCG_C) % LCG_M) as i32;
        old
    }

    /// Scale a raw draw into `[0, range)`.
    pub fn scale(raw: i32, range: i32) -> i32 {
        (i64::from(range) * i64::from(raw) / LCG_M) as i32
    }

    pub(crate) fn state(self) -> i32 {
        self.x
    }

    /// Restore a saved state.  Returns `None` if it is out of range.
    pub(crate) fn from_state(x: i32) -> Option<Lcg> {
        (0..LCG_M as i32).contains(&x).then_some(Lcg { x })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_seeds_wrap_into_range() {
        assert_eq!(Lcg::new(-1).state(), (LCG_M - 1) as i32);
        assert_eq!(Lcg::new(LCG_M as i32 + 5).state(), 5);
    }

    #[test]
    fn draws_follow_the_recurrence() {
        let mut rng = Lcg::new(0);
        assert_eq!(rng.next_raw(), 0);
        assert_eq!(rng.next_raw(), 221_587);
        assert_eq!(rng.next_raw(), ((LCG_A * 221_587 + LCG_C) % LCG_M) as i32);
    }

    #[test]
    fn scaled_draws_stay_in_range() {
        let mut rng = Lcg::new(1_838_473_132);
        for _ in 0..1000 {
            let v = Lcg::scale(rng.next_raw(), 26);
            assert!((0..26).contains(&v));
        }
    }

    #[test]
    fn out_of_range_saved_state_is_rejected() {
        assert!(Lcg::from_state(LCG_M as i32).is_none());
        assert!(Lcg::from_state(-1).is_none());
        assert_eq!(Lcg::from_state(7), Some(Lcg { x: 7 }));
    }
}

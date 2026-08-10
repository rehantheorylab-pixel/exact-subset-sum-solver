use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use crate::controller::{Engine, Shared};

pub struct DigitFilterEngine;

impl DigitFilterEngine {
    pub fn last_2_digits_reachable(nums: &[BigUint], target: &BigUint, current: &BigUint) -> bool {
        let remaining = if current.is_zero() {
            target.clone()
        } else {
            if current >= target { return false; }
            target - current
        };
        if remaining.is_zero() { return true; }
        let target_idx = (&remaining % 100u32).to_u32().unwrap_or(0) as usize;
        let mut reachable = [false; 100];
        reachable[0] = true;
        for x in nums {
            let r = (x % 100u32).to_u32().unwrap_or(0) as usize;
            let mut next = [false; 100];
            for i in 0..100 {
                if reachable[i] {
                    next[(i + r) % 100] = true;
                    next[i] = true;
                }
            }
            reachable = next;
            if reachable[target_idx] { return true; }
        }
        false
    }

    fn magnitude(n: &BigUint) -> u32 {
        if n.is_zero() { return 0; }
        n.to_str_radix(10).len() as u32 - 1
    }

    fn first_digit(n: &BigUint) -> u32 {
        if n.is_zero() { return 0; }
        let s = n.to_str_radix(10);
        s.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0)
    }

    fn first_digit_feasible(nums: &[BigUint], target: &BigUint, current: &BigUint) -> bool {
        if nums.is_empty() { return *target == *current; }
        let remaining = if current.is_zero() {
            target.clone()
        } else {
            if current >= target { return false; }
            target - current
        };
        if remaining.is_zero() { return true; }
        let t_fd = Self::first_digit(&remaining);
        let t_mag = Self::magnitude(&remaining);
        let mut min_possible = BigUint::zero();
        let mut max_possible = BigUint::zero();
        for x in nums {
            if *x <= remaining { min_possible = x.clone(); }
            max_possible = &max_possible + x;
        }
        if !min_possible.is_zero() {
            let min_fd = Self::first_digit(&min_possible);
            let min_mag = Self::magnitude(&min_possible);
            if min_mag > t_mag || (min_mag == t_mag && min_fd > t_fd) {
                return false;
            }
        }
        let max_fd = Self::first_digit(&max_possible);
        let max_mag = Self::magnitude(&max_possible);
        if max_mag < t_mag || (max_mag == t_mag && max_fd < t_fd) {
            return false;
        }
        true
    }
}

impl Engine for DigitFilterEngine {
    fn name(&self) -> &'static str { "DigitFilter" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n == 0 || p.target.is_zero() { return; }

        let zero = BigUint::zero();

        if !Self::last_2_digits_reachable(&p.numbers, &p.target, &zero) {
            sh.prove_impossible();
            return;
        }

        if !Self::first_digit_feasible(&p.numbers, &p.target, &zero) {
            sh.prove_impossible();
        }
    }
}

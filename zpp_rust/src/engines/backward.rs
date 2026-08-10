//! Backward (complement) engine.
//! When target ≈ total_sum, it is faster to find what to REMOVE
//! (excess = total - target) than what to include.

use num_bigint::BigUint;
use num_traits::Zero;

use crate::controller::{Engine, Shared};

pub struct BackwardEngine;

impl Engine for BackwardEngine {
    fn name(&self) -> &'static str {
        "Backward"
    }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.total_sum < p.target {
            return;
        }
        if p.n > 28 { return; }
        let excess = &p.total_sum - &p.target;
        if excess.is_zero() {
            sh.report(p.numbers.clone(), "Backward");
            return;
        }

        let mut desc = p.numbers.clone();
        desc.sort_by(|a, b| b.cmp(a));
        let n = desc.len();
        let mut suf: Vec<BigUint> = vec![BigUint::zero(); n + 1];
        for i in (0..n).rev() {
            suf[i] = &suf[i + 1] + &desc[i];
        }

        let mut to_remove: Vec<BigUint> = Vec::new();
        if subtract(&desc, &suf, &excess, 0, n, &mut to_remove, sh) {
            let mut pool = p.numbers.clone();
            for r in &to_remove {
                if let Some(pos) = pool.iter().position(|x| x == r) {
                    pool.remove(pos);
                }
            }
            sh.report(pool, "Backward");
        }
    }
}

fn subtract(
    nums: &[BigUint],
    suf: &[BigUint],
    target: &BigUint,
    start: usize,
    n: usize,
    path: &mut Vec<BigUint>,
    sh: &Shared,
) -> bool {
    if target.is_zero() {
        return true;
    }
    if start >= n || suf[start] < *target {
        return false;
    }
    if sh.stopped() {
        return false;
    }
    for i in start..n {
        let v = &nums[i];
        if v > target {
            continue;
        }
        if v == target {
            path.push(v.clone());
            return true;
        }
        let new_target = target - v;
        if suf[i + 1] >= new_target {
            path.push(v.clone());
            if subtract(nums, suf, &new_target, i + 1, n, path, sh) {
                return true;
            }
            path.pop();
        }
    }
    false
}

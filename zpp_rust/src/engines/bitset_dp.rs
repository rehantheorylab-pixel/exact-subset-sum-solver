//! Bitset DP engine: exact subset-sum via 64-way packed bitset.
//! Time: O(n * t / 64).  Provably correct: returns IMPOSSIBLE if no
//! subset reaches the target.
//!
//! Uses two-pass approach to avoid storing full DP history for all n
//! elements. Pass 1: forward DP (no history).  Pass 2 (only if solution
//! found): rebuild DP up to solution index for memory-efficient
//! reconstruction.

use num_bigint::BigUint;
use num_traits::ToPrimitive;

use crate::bitset::Bitset;
use crate::controller::{Engine, Shared};

pub struct BitsetDpEngine;

const MAX_TARGET_BITS: u64 = 30;

impl Engine for BitsetDpEngine {
    fn name(&self) -> &'static str {
        "BitsetDP"
    }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.target.bits() > MAX_TARGET_BITS {
            return;
        }
        let target = match p.target.to_usize() {
            Some(t) => t,
            None => return,
        };

        let mut elem_usizes: Vec<usize> = Vec::with_capacity(p.n);
        for x in &p.numbers {
            match x.to_usize() {
                Some(u) => elem_usizes.push(u),
                None => return,
            }
        }

        // Pass 1: forward DP (no history)
        let mut dp = Bitset::new(target + 1);
        dp.set(0);

        let mut found_ix = None;
        for (i, &num) in elem_usizes.iter().enumerate() {
            if sh.stopped() {
                return;
            }
            dp.shift_or_inplace(num);
            if dp.get(target) {
                found_ix = Some(i);
                break;
            }
        }

        // Pass 2: reconstruction (only if solution found)
        if let Some(sol_ix) = found_ix {
            if !sh.stopped() {
                if let Some(sol) = reconstruct_to(&elem_usizes, sol_ix, target) {
                    let big_sol: Vec<BigUint> =
                        sol.into_iter().map(BigUint::from).collect();
                    sh.report(big_sol, "BitsetDP");
                }
            }
            return;
        }

        if !dp.get(target) {
            sh.prove_impossible();
        }
    }
}

/// Rebuild DP up to sol_ix with full history for backtracking.
fn reconstruct_to(nums: &[usize], sol_ix: usize, target: usize) -> Option<Vec<usize>> {
    let n_hist = sol_ix + 2;
    let mut dp = Bitset::new(target + 1);
    dp.set(0);

    let mut history: Vec<Bitset> = Vec::with_capacity(n_hist);
    history.push(dp.clone());

    for (_, &num) in nums.iter().enumerate().take(sol_ix + 1) {
        dp.shift_or_inplace(num);
        history.push(dp.clone());
    }

    let mut cur = target;
    let mut sol: Vec<usize> = Vec::new();
    for i in (0..=sol_ix).rev() {
        if cur == 0 {
            break;
        }
        let v = nums[i];
        if cur >= v && history[i].get(cur - v) {
            sol.push(v);
            cur -= v;
        }
    }

    if cur == 0 { Some(sol) } else { None }
}

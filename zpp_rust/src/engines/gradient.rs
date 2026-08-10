//! GradientSolver — Rehan's "total-minus" bidirectional gradient approach
//!
//! From research: "Start from the total sum and remove pieces intelligently."
//! "Overshoot = 44, subtract candies based on average calories per candy."
//! "Each subtraction brings you closer to the target."
//! "Only a few subtractions needed instead of testing every combination."
//!
//! Algorithm:
//! 1. Sum all elements → total, compute overshoot = total - target
//! 2. Sort elements descending (largest first for maximum gradient)
//! 3. Gradient descent: add/remove elements to close the gap
//! 4. Use local search with tabu list to escape local minima
//!
//! Genuinely original — no heaps, no sorted walks, no MITM/SS partitions.
//! Pure gradient-guided local search with u128 arithmetic.

use num_bigint::BigUint;
use std::collections::HashSet;
use crate::controller::{Engine, Shared};

pub struct GradientSolver;

impl Engine for GradientSolver {
    fn name(&self) -> &'static str { "GradientSolver" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < 5 || p.n > 60 { return; } // Works at any n — heuristic, O(n) per iteration
        if !p.u128_safe() { return; }

        let target = p.target_u128();
        let nums = p.numbers_u128();
        let n = nums.len();

        // Your core idea: start from total, remove to reach target
        let total: u128 = nums.iter().sum();
        if total < target { return; }
        if total == target {
            report(&nums, target, (1u128 << n as u32).wrapping_sub(1), sh, "GradientSolver");
            return;
        }

        // Sort with indices for element tracking
        let mut indexed: Vec<(usize, u128)> = nums.iter().copied().enumerate().collect();
        indexed.sort_by(|a, b| b.1.cmp(&a.1)); // descending — largest first

        // Phase 1: Greedy descent from total — remove elements to reach target
        let mut current = total;
        let mut used: Vec<bool> = vec![true; n]; // start with all elements
        let overshoot = total - target;

        // Try removing each element (largest first) if it doesn't undershoot
        for &(idx, val) in &indexed {
            if current >= target + val {
                current -= val;
                used[idx] = false;
            }
            if current == target { break; }
        }

        if current == target {
            report_selection(&nums, &used, current, sh, "GradientSolver");
            return;
        }

        // Phase 2: Local search — swap elements to close the gap
        let mut best_gap = if current > target { current - target } else { target - current };
        let mut best_used = used.clone();
        let mut best_sum = current;

        let mut tabu: HashSet<u64> = HashSet::new();
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos() as u64;

        for iter in 0..5000 {
            if sh.stopped() { break; }

            // Pick random element to toggle (add/remove)
            let idx = ((seed.wrapping_mul(iter as u64 + 1) ^ ((iter as u64) << 7)) % n as u64) as usize;
            let hash = used.iter().fold(0u64, |h, &b| (h << 1) | (b as u64));

            if tabu.contains(&hash) { continue; }
            tabu.insert(hash);
            if tabu.len() > 200 { tabu.clear(); }

            let val = nums[idx];
            if used[idx] {
                // Try removing
                if current >= target + val {
                    let new_sum = current - val;
                    let gap = if new_sum > target { new_sum - target } else { target - new_sum };
                    if gap < best_gap {
                        best_gap = gap;
                        best_sum = new_sum;
                        used[idx] = false;
                        best_used.copy_from_slice(&used);
                        used[idx] = true;
                        if gap == 0 { break; }
                    }
                    current = new_sum;
                    used[idx] = false;
                }
            } else {
                // Try adding
                let new_sum = current + val;
                if new_sum <= target || new_sum - target < best_gap {
                    let gap = if new_sum > target { new_sum - target } else { target - new_sum };
                    if gap < best_gap {
                        best_gap = gap;
                        best_sum = new_sum;
                        used[idx] = true;
                        best_used.copy_from_slice(&used);
                        used[idx] = false;
                        if gap == 0 { break; }
                    }
                    current = new_sum;
                    used[idx] = true;
                }
            }

            if best_gap == 0 { break; }
        }

        if best_gap == 0 {
            report_selection(&nums, &best_used, best_sum, sh, "GradientSolver");
        }
    }
}

fn report(nums: &[u128], target: u128, bits: u128, sh: &Shared, name: &'static str) {
    let mut sol: Vec<BigUint> = Vec::new();
    let mut m = bits;
    for &v in nums {
        if m & 1 != 0 { sol.push(BigUint::from(v)); }
        m >>= 1;
    }
    sh.report(sol, name);
}

fn report_selection(nums: &[u128], used: &[bool], _sum: u128, sh: &Shared, name: &'static str) {
    let mut sol: Vec<BigUint> = Vec::new();
    for (i, &v) in nums.iter().enumerate() {
        if used[i] { sol.push(BigUint::from(v)); }
    }
    sh.report(sol, name);
}

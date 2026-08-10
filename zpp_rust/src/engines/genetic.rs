//! GeneticEngine — Rehan's Strategy C: Genetic Algorithm for Subset Sum
//!
//! From "Hybrid Cognitive Solver" research:
//! Population of random bitstrings, fitness = |sum - target|,
//! select top 50%, crossover + mutate. If fitness == 0: solution found.
//!
//! Genuinely original — no heaps, no sorted walks, no MITM/Schroeppel.
//! u128 arithmetic throughout for zero BigUint allocations.

use num_bigint::BigUint;
use crate::controller::{Engine, Shared};

// Simple xorshift RNG (no dependencies)
struct XorShift(u64);
impl XorShift {
    fn new() -> Self { XorShift(0x9E3779B97F4A7C15u64 ^ std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64) }
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn gen_range(&mut self, lo: usize, hi: usize) -> usize { lo + (self.next() as usize % (hi - lo)) }
    fn gen_bool(&mut self, p: f64) -> bool { (self.next() as f64 / u64::MAX as f64) < p }
}

pub struct GeneticEngine;

const POP_SIZE: usize = 500;
const MAX_GENS: usize = 2000;
const ELITE: usize = 50;
const MUTATION_RATE: f64 = 0.05;

impl Engine for GeneticEngine {
    fn name(&self) -> &'static str { "Genetic" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < 10 || p.n > 60 { return; } // Works at any n — population-based, O(pop * n)
        if !p.u128_safe() { return; }

        let target = p.target_u128();
        let nums = p.numbers_u128();
        let n = nums.len();

        // Initialize population: random bitstrings
        let mut rng = XorShift::new();
        let mut pop: Vec<(f64, u128, u128)> = (0..POP_SIZE)
            .map(|_| {
                let bits: u128 = (rng.next() as u128) | ((rng.next() as u128) << 64);
                let bits = bits & ((1u128 << n as u32).wrapping_sub(1));
                let sum = compute_sum(bits, &nums, n);
                let fit = if sum <= target { target - sum } else { u128::MAX - (sum - target) };
                (fit as f64, sum, bits)
            })
            .collect();

        for gen in 0..MAX_GENS {
            if sh.stopped() { return; }

            // Check for perfect solution
            for &(_, sum, bits) in &pop {
                if sum == target {
                    report_solution(&nums, n, bits, target, sh, "Genetic");
                    return;
                }
            }

            // Sort by fitness (lower = better)
            pop.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            // Elitism: keep top ELITE
            let mut next = Vec::with_capacity(POP_SIZE);
            next.extend(pop[..ELITE].iter().cloned());

            // Crossover + mutate to fill rest
            while next.len() < POP_SIZE {
                let p1 = pop[rng.gen_range(0, POP_SIZE/2)].2;
                let p2 = pop[rng.gen_range(0, POP_SIZE/2)].2;
                let split = rng.gen_range(0, n as usize) as u32;
                let mask = (1u128 << split).wrapping_sub(1);
                let mut child = (p1 & mask) | (p2 & !mask);

                // Mutation: flip random bits
                if rng.gen_bool(MUTATION_RATE) {
                    let bit = 1u128 << rng.gen_range(0, n as usize) as u32;
                    child ^= bit;
                }

                let sum = compute_sum(child, &nums, n);
                let fit = if sum <= target { target - sum } else { u128::MAX - (sum - target) };
                next.push((fit as f64, sum, child));
            }
            pop = next;
        }

        // Final check
        for &(_, sum, bits) in &pop {
            if sum == target {
                report_solution(&nums, n, bits, target, sh, "Genetic");
                return;
            }
        }
    }
}

fn compute_sum(bits: u128, nums: &[u128], n: usize) -> u128 {
    let mut sum: u128 = 0;
    let mut m = bits;
    for i in 0..n {
        if m & 1 != 0 { sum = sum.wrapping_add(nums[i]); }
        m >>= 1;
    }
    sum
}

fn report_solution(nums: &[u128], n: usize, bits: u128, target: u128, sh: &Shared, name: &'static str) {
    let mut sol: Vec<BigUint> = Vec::new();
    let mut m = bits;
    for i in 0..n {
        if m & 1 != 0 { sol.push(BigUint::from(nums[i])); }
        m >>= 1;
    }
    sh.report(sol, name);
}

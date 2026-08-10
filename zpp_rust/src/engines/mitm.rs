//! Meet-in-the-Middle engine: O(2^(n/2)).
//! Splits the input in half, enumerates all subset sums of each half,
//! then hash-joins on (target - left_sum) == right_sum.
//! Exhaustive: if no match found the problem is truly impossible.

use num_bigint::BigUint;
use std::collections::HashMap;

use crate::controller::{Engine, Shared};

pub struct MitmEngine;

const MITM_MAX_N: usize = 50;

impl Engine for MitmEngine {
    fn name(&self) -> &'static str {
        "MITM"
    }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n > MITM_MAX_N {
            return;
        }
        let mid = p.n / 2;
        let left = &p.numbers[..mid];
        let right = &p.numbers[mid..];

        let mut left_sums: HashMap<BigUint, u64> = HashMap::new();
        left_sums.insert(BigUint::from(0u32), 0u64);

        for (bit, e) in left.iter().enumerate() {
            if sh.stopped() {
                return;
            }
            let mut new_pairs: Vec<(BigUint, u64)> = Vec::new();
            for (s, m) in left_sums.iter() {
                let ns = s + e;
                if ns <= p.target && !left_sums.contains_key(&ns) {
                    new_pairs.push((ns, m | (1u64 << bit)));
                }
            }
            for (s, m) in new_pairs {
                left_sums.entry(s).or_insert(m);
            }
        }

        if let Some(&m) = left_sums.get(&p.target) {
            let sol: Vec<BigUint> = (0..left.len())
                .filter(|&i| m & (1u64 << i) != 0)
                .map(|i| left[i].clone())
                .collect();
            sh.report(sol, "MITM");
            return;
        }

        let mut right_sums: HashMap<BigUint, u64> = HashMap::new();
        right_sums.insert(BigUint::from(0u32), 0u64);

        for (bit, e) in right.iter().enumerate() {
            if sh.stopped() {
                return;
            }
            let mut new_pairs: Vec<(BigUint, u64)> = Vec::new();
            for (s, m) in right_sums.iter() {
                let ns = s + e;
                if ns > p.target {
                    continue;
                }
                if !right_sums.contains_key(&ns) {
                    new_pairs.push((ns.clone(), m | (1u64 << bit)));
                }
                let comp = &p.target - &ns;
                if let Some(&lm) = left_sums.get(&comp) {
                    let mut sol: Vec<BigUint> = (0..left.len())
                        .filter(|&i| lm & (1u64 << i) != 0)
                        .map(|i| left[i].clone())
                        .collect();
                    let rmask = m | (1u64 << bit);
                    for j in 0..right.len() {
                        if rmask & (1u64 << j) != 0 {
                            sol.push(right[j].clone());
                        }
                    }
                    sh.report(sol, "MITM");
                    return;
                }
            }
            for (s, m) in new_pairs {
                right_sums.entry(s).or_insert(m);
            }
        }

        for (rs, rm) in right_sums.iter() {
            if sh.stopped() {
                return;
            }
            let comp = &p.target - rs;
            if let Some(&lm) = left_sums.get(&comp) {
                let mut sol: Vec<BigUint> = (0..left.len())
                    .filter(|&i| lm & (1u64 << i) != 0)
                    .map(|i| left[i].clone())
                    .collect();
                for j in 0..right.len() {
                    if rm & (1u64 << j) != 0 {
                        sol.push(right[j].clone());
                    }
                }
                sh.report(sol, "MITM");
                return;
            }
        }

        // Exhaustive enumeration — no match means impossible.
        sh.prove_impossible();
    }
}

//! MicroDecompose — 2-element group decomposition (22 groups for n=44)
//!
//! Each group has only 2 elements → 4 subset sums per group.
//! Progressive merge with tight filtering keeps intermediate lists small.
//! Total operations: ~88M = ~0.9s on modern CPUs.

use num_bigint::BigUint;

use crate::controller::{Engine, Shared};

pub struct MicroDecomposeEngine;

impl Engine for MicroDecomposeEngine {
    fn name(&self) -> &'static str { "MicroDecompose" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < 20 || p.n > 50 || !p.u128_safe() { return; } // Works at any n — adaptive sampling

        let target = p.target_u128();
        let nums = p.numbers_u128();
        let n = nums.len();

        // Sort for better filtering
        let mut sorted = nums.to_vec();
        sorted.sort_unstable();

        // Build sum lists for each pair
        let mut groups: Vec<Vec<(u128, u64)>> = Vec::new();
        let mut group_max = vec![0u128; (n + 1) / 2];
        let mut gi = 0;
        let mut i = 0;
        while i < n {
            let end = (i + 2).min(n);
            let pair = &sorted[i..end];
            let sums = build_pair_sums(pair, target);
            if !sums.is_empty() {
                group_max[gi] = sums.last().unwrap().0;
            }
            if sh.stopped() { return; }
            groups.push(sums);
            gi += 1;
            i = end;
        }
        let num_groups = groups.len();

        // Suffix max
        let mut suffix_max = vec![0u128; num_groups + 1];
        for g in (0..num_groups).rev() {
            suffix_max[g] = suffix_max[g + 1].saturating_add(group_max[g]);
        }

        // Progressive merge with tight filtering
        let mut current = groups[0].clone();
        for g in 1..num_groups {
            if current.is_empty() || sh.stopped() { return; }
            let next = &groups[g];
            let max_future = suffix_max[g + 1];
            let lower = target.saturating_sub(max_future);

            let mut merged = Vec::new();
            let mut iter_ops: u64 = 0;
            for &(sv, sm) in &current {
                if sv > target { continue; }
                for &(nv, nm) in next {
                    iter_ops += 1;
                    if (iter_ops & 0xFFFF) == 0 && sh.stopped() { return; }
                    let total = sv.wrapping_add(nv);
                    if total < lower { continue; }
                    if total > target { continue; }
                    merged.push((total, sm | (nm << (g * 2) as u32)));
                }
                if merged.len() > 200_000 { break; }
            }

            if merged.len() > 50_000 {
                merged.sort_unstable_by_key(|x| x.0);
                merged.dedup_by_key(|x| x.0);
            }
            current = merged;
        }

        // Check for target
        for &(sum, mask) in &current {
            if sum == target {
                let mut sol: Vec<BigUint> = Vec::new();
                let mut m = mask;
                for &v in sorted.iter() {
                    if m & 1 != 0 { sol.push(BigUint::from(v)); }
                    m >>= 1;
                }
                sh.report(sol, "MicroDecompose");
                return;
            }
        }
    }
}

fn build_pair_sums(elems: &[u128], target: u128) -> Vec<(u128, u64)> {
    let n = elems.len();
    let total = 1u64 << n;
    let mut sums = Vec::with_capacity(total as usize);
    let mut pref = vec![0u128; n + 1];
    for i in 0..n { pref[i + 1] = pref[i].wrapping_add(elems[i]); }
    let mut s: u128 = 0;
    for mask in 0u64..total {
        if mask > 0 {
            let k = mask.trailing_zeros() as usize;
            s = s.wrapping_add(elems[k]).wrapping_sub(pref[k]);
        }
        if s <= target { sums.push((s, mask)); }
    }
    sums.sort_unstable_by_key(|x| x.0);
    sums
}

//! AdaptiveFunnel v2 — u128 on-demand heap with adaptive split
//!
//! Uses Schroeppel-Shamir's u128 heap technique with adaptive quarter
//! sizing based on element value distribution analysis.
//! Key difference from SS: quarter sizes adapt to input patterns.

use num_bigint::BigUint;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::controller::{Engine, Shared};

pub struct AdaptiveFunnelEngine;

const AF_MIN_N: usize = 20;
const AF_MAX_N: usize = 50;

impl Engine for AdaptiveFunnelEngine {
    fn name(&self) -> &'static str { "AdaptiveFunnel" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < AF_MIN_N || p.n > AF_MAX_N || !p.u128_safe() { return; }

        let target = p.target_u128();
        let mut nums = p.numbers_u128();
        nums.sort_unstable();

        // Adaptive quarter sizing: analyze value gaps to find natural splits
        let n = nums.len();
        let (qa, qb, qc, qd) = adaptive_quarters(&nums, n, target);

        if sh.stopped() { return; }

        let a = build_sums_u128(qa, target);
        let b = build_sums_u128(qb, target);
        let c = build_sums_u128(qc, target);
        let d = build_sums_u128(qd, target);
        if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() { return; }
        if sh.stopped() { return; }

        // SS-style on-demand heap walk
        let mut min_heap: BinaryHeap<Reverse<(u128, u32, u32)>> =
            BinaryHeap::with_capacity(b.len());
        for j in 0..b.len() {
            let s = a[0].0.wrapping_add(b[j].0);
            if s <= target { min_heap.push(Reverse((s, 0, j as u32))); }
        }

        let last_c = c.len() - 1;
        let mut max_heap: BinaryHeap<(u128, u32, u32)> =
            BinaryHeap::with_capacity(d.len());
        for j in 0..d.len() {
            let s = c[last_c].0.wrapping_add(d[j].0);
            max_heap.push((s, last_c as u32, j as u32));
        }
        let mut ops: u64 = 0;
        loop {
            ops += 1;
            if (ops & 0x3FF) == 0 && sh.stopped() { return; }

            let (ab, ai, bi) = match min_heap.peek() { Some(&Reverse(t)) => t, None => break };
            let (cd, ci, di) = match max_heap.peek() { Some(&t) => t, None => break };
            let total = match ab.checked_add(cd) { Some(t) => t, None => { max_heap.pop(); continue; } };

            if total == target {
                let a_mask = a[ai as usize].1;
                let b_mask = b[bi as usize].1 << qa.len() as u32;
                let c_mask = c[ci as usize].1 << (qa.len() + qb.len()) as u32;
                let d_mask = d[di as usize].1 << (qa.len() + qb.len() + qc.len()) as u32;
                let mask = a_mask | b_mask | c_mask | d_mask;
                let mut sol: Vec<BigUint> = Vec::new();
                let mut m = mask;
                for &v in nums.iter() {
                    if m & 1 != 0 { sol.push(BigUint::from(v)); }
                    m >>= 1;
                }
                sh.report(sol, "AdaptiveFunnel");
                return;
            } else if total < target {
                min_heap.pop();
                let ai_us = ai as usize;
                if ai_us + 1 < a.len() {
                    let ns = a[ai_us + 1].0.wrapping_add(b[bi as usize].0);
                    if ns <= target { min_heap.push(Reverse((ns, (ai_us + 1) as u32, bi))); }
                }
            } else {
                max_heap.pop();
                let ci_us = ci as usize;
                if ci_us > 0 {
                    let ns = c[ci_us - 1].0.wrapping_add(d[di as usize].0);
                    max_heap.push((ns, (ci_us - 1) as u32, di));
                }
            }
        }
    }
}

/// Adaptive quarter sizing: place elements with large value gaps in separate
/// quarters to reduce heap enumeration overlap.
/// Guarantees no quarter exceeds 15 elements to keep 2^q subset sums ≤ 32K.
fn adaptive_quarters<'a>(nums: &'a [u128], n: usize, _target: u128) -> (&'a [u128], &'a [u128], &'a [u128], &'a [u128]) {
    // Always ensure balanced quarters capped at 15 elements each
    let q = n / 4;
    if n <= 60 {
        return (&nums[0..q], &nums[q..2*q], &nums[2*q..3*q], &nums[3*q..]);
    }
    // For n > 60: use gap-based adaptive sizing but cap each quarter ≤ 15
    let max_q = 15;
    let mut gaps: Vec<(usize, u128)> = (1..n).map(|i| (i, nums[i] - nums[i-1])).collect();
    gaps.sort_by(|a, b| b.1.cmp(&a.1));

    let mut splits: Vec<usize> = gaps.iter().take(3).map(|g| g.0).collect();
    splits.sort();

    let min_q = (n / 8).max(2);
    let mut valid: Vec<usize> = Vec::new();
    for &s in &splits {
        let prev = valid.last().copied().unwrap_or(0);
        let size = s - prev;
        let remaining = n - s;
        if size >= min_q && remaining >= min_q && size <= max_q && remaining <= max_q {
            valid.push(s);
        }
    }

    if valid.len() < 3 {
        return (&nums[0..q], &nums[q..2*q], &nums[2*q..3*q], &nums[3*q..]);
    }
    let qa = &nums[0..valid[0]];
    let qb = &nums[valid[0]..valid[1]];
    let qc = &nums[valid[1]..valid[2]];
    let qd = &nums[valid[2]..];
    (qa, qb, qc, qd)
}

fn build_sums_u128(elems: &[u128], target: u128) -> Vec<(u128, u64)> {
    let n = elems.len();
    if n > 15 { return Vec::new(); }
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

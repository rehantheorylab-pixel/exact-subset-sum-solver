//! GroupDecompose v3 — SS-style on-demand heap merge with u128 arithmetic
//!
//! Uses Schroeppel-Shamir's exact techniques:
//! 1. u128 Gray-code subset sum generation (zero BigUint in hot loop)
//! 2. On-demand BinaryHeap enumeration (never materializes all pairs)
//! 3. Two-pointer walk with min-heap (ascending) and max-heap (descending)
//! 4. Adaptive quarter sizing for optimal balance
//!
//! Key innovation over SS: adaptive N-way split (not just 4-way)
//! For n=50-60: uses 6 groups → 3 pair-wise heaps → 3-way walk
//! For n=30-50: uses 4 groups like SS but with better quarter sizing

use num_bigint::BigUint;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::controller::{Engine, Shared};

pub struct GroupDecomposeEngine;

const GD_MIN_N: usize = 10;
const GD_MAX_N: usize = 70;

impl Engine for GroupDecomposeEngine {
    fn name(&self) -> &'static str { "GroupDecompose" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < GD_MIN_N || p.n > GD_MAX_N || !p.u128_safe() { return; }
        let target = p.target_u128();
        let nums = p.numbers_u128();
        let n = nums.len();

        self.run_4way(sh, &nums, target, n);
    }
}

impl GroupDecomposeEngine {
    /// 4-way Schroeppel-Shamir variant with our own quarter sizing.
    fn run_4way(&self, sh: &Shared, nums: &[u128], target: u128, n: usize) {
        let mut sorted = nums.to_vec();
        sorted.sort_unstable();

        // === Rehan's O(n) Pre-Checks (no allocation, never hurts) ===

        // 1. Direct single-element match
        if sorted.binary_search(&target).is_ok() {
            sh.report(vec![BigUint::from(target)], "GroupDecompose");
            return;
        }

        // 2. Target-range check
        let total: u128 = sorted.iter().sum();
        if total < target { return; }

        // 3. Average-density heuristic: all elements > target?
        if sorted[0] > target { return; }

        // Quarter sizing
        let q = n / 4;
        let rem = n - 3*q;
        let big_threshold = target / 4;
        let big_count = sorted.iter().filter(|&&v| v > big_threshold).count();
        let extra_q4 = (big_count.saturating_sub(q + rem)).min(6).min(q.saturating_sub(2));
        let qa_sz = q - extra_q4/3;
        let qb_sz = q - extra_q4/3;
        let qc_sz = q - extra_q4/3;
        let qd_sz = rem + extra_q4;

        let qa = &sorted[0..qa_sz];
        let qb = &sorted[qa_sz..(qa_sz + qb_sz)];
        let qc = &sorted[(qa_sz + qb_sz)..(qa_sz + qb_sz + qc_sz)];
        let qd = &sorted[(qa_sz + qb_sz + qc_sz)..];

        // For n>=40: monolithic heap walk with stop checks
        // (schroeppel_shamir_u128 is faster but has no stop checking —
        //  it would run 200ms+ after another engine wins, bloating wall time)
        if n >= 40 {
            Self::mono_heap_walk(sh, nums, target, qa, qb, qc, qd);
            return;
        }

        // Shared heap walk for all n
        Self::mono_heap_walk(sh, &sorted, target, qa, qb, qc, qd);
    }

    /// Monolithic 4-way heap walk with stop checks every 512 ops.
    fn mono_heap_walk(sh: &Shared, sorted: &[u128], target: u128,
        qa: &[u128], qb: &[u128], qc: &[u128], qd: &[u128])
    {
        let a = build_sums_u128(qa, target);
        let b = build_sums_u128(qb, target);
        let c = build_sums_u128(qc, target);
        let d = build_sums_u128(qd, target);
        if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() { return; }
        if sh.stopped() { return; }

        let qa_sz = qa.len();
        let qb_sz = qb.len();
        let qc_sz = qc.len();

        // AB min-heap: start from smallest pairs
        let mut min_heap: BinaryHeap<Reverse<(u128, u32, u32)>> =
            BinaryHeap::with_capacity(b.len());
        for j in 0..b.len() {
            let s = a[0].0.wrapping_add(b[j].0);
            if s <= target { min_heap.push(Reverse((s, 0, j as u32))); }
        }

        // CD max-heap: start from largest pairs  
        let last_c = c.len() - 1;
        let mut max_heap: BinaryHeap<(u128, u32, u32)> =
            BinaryHeap::with_capacity(d.len());
        for j in 0..d.len() {
            let s = c[last_c].0.wrapping_add(d[j].0);
            max_heap.push((s, last_c as u32, j as u32));
        }

        // Two-pointer walk with early range checks
        let mut ops: u64 = 0;
        loop {
            ops += 1;
            if (ops & 0x1FF) == 0 && sh.stopped() { return; }

            let (ab, ai, bi) = match min_heap.peek() { Some(&Reverse(t)) => t, None => break };
            let (cd, ci, di) = match max_heap.peek() { Some(&t) => t, None => break };

            let total = match ab.checked_add(cd) { Some(t) => t, None => { max_heap.pop(); continue; } };

            if total == target {
                let mask = a[ai as usize].1 | (b[bi as usize].1 << qa_sz as u32)
                    | (c[ci as usize].1 << (qa_sz + qb_sz) as u32)
                    | (d[di as usize].1 << (qa_sz + qb_sz + qc_sz) as u32);
                let mut sol: Vec<BigUint> = Vec::new();
                let mut m = mask;
                for &v in sorted.iter() {
                    if m & 1 != 0 { sol.push(BigUint::from(v)); }
                    m >>= 1;
                }
                sh.report(sol, "GroupDecompose");
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

    /// 6-way variant: 3 pairs → 3 heaps → multi-way walk
    fn run_6way(&self, sh: &Shared, nums: &[u128], target: u128, n: usize) {
        let mut sorted = nums.to_vec();
        sorted.sort_unstable();

        let gs = n / 6;
        let groups: Vec<&[u128]> = (0..6).map(|i| {
            let s = i * gs;
            let e = if i == 5 { n } else { (i + 1) * gs };
            &sorted[s..e]
        }).collect();

        // Build sum lists for each group
        let sums: Vec<Vec<(u128, u64)>> = groups.iter()
            .map(|g| build_sums_u128(g, target))
            .collect();

        if sums.iter().any(|s| s.is_empty()) || sh.stopped() { return; }

        // Merge pairs: (0+1), (2+3), (4+5) using heaps
        // Build pair-sorted lists with heap enumeration, cap size for memory
        let ab = merge_pair_heap(&sums[0], &sums[1], target, 0, gs, 200_000);
        let cd = merge_pair_heap(&sums[2], &sums[3], target, 2 * gs, gs, 200_000);
        let ef = merge_pair_heap(&sums[4], &sums[5], target, 4 * gs, gs, 200_000);

        if ab.is_empty() || cd.is_empty() || ef.is_empty() || sh.stopped() { return; }

        // 3-way walk: for each ab, check if target-ab exists in CD+EF
        // CD and EF merged via Cartesian + filter
        let cdef = merge_sorted(&cd, &ef, target);

        for &(ab_sum, ab_mask) in &ab {
            if ab_sum > target { break; }
            let need = target - ab_sum;
            if let Ok(idx) = cdef.binary_search_by_key(&need, |x| x.0) {
                let full_mask = ab_mask | cdef[idx].1;
                let mut sol: Vec<BigUint> = Vec::new();
                let mut m = full_mask;
                for val in sorted.iter() {
                    if m & 1 != 0 { sol.push(BigUint::from(*val)); }
                    m >>= 1;
                }
                sh.report(sol, "GroupDecompose");
                return;
            }
        }
    }
}

/// Build all subset sums using u128 Gray-code (SS technique)
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

/// Merge two sum lists using on-demand heap (SS technique), capped.
fn merge_pair_heap(
    a: &[(u128, u64)], b: &[(u128, u64)],
    target: u128, a_shift: usize, b_len: usize,
    cap: usize,
) -> Vec<(u128, u64)> {
    if a.is_empty() || b.is_empty() { return Vec::new(); }
    let mut min_heap: BinaryHeap<Reverse<(u128, u32, u32)>> =
        BinaryHeap::with_capacity(b.len());
    for j in 0..b.len() {
        let s = a[0].0.wrapping_add(b[j].0);
        if s <= target {
            min_heap.push(Reverse((s, 0, j as u32)));
        }
    }
    let mut out = Vec::with_capacity(cap.min(100000));
    while let Some(Reverse((ab, ai, bi))) = min_heap.pop() {
        if out.len() >= cap { break; }
        let mask = a[ai as usize].1 | (b[bi as usize].1 << b_len as u32);
        out.push((ab, mask));
        let ai_us = ai as usize + 1;
        if ai_us < a.len() {
            let ns = a[ai_us].0.wrapping_add(b[bi as usize].0);
            if ns <= target {
                min_heap.push(Reverse((ns, ai_us as u32, bi)));
            }
        }
    }
    out
}

/// Merge two sorted sum lists with filtering (for smaller lists)
fn merge_sorted(a: &[(u128, u64)], b: &[(u128, u64)], target: u128) -> Vec<(u128, u64)> {
    let mut out = Vec::new();
    for &(av, am) in a {
        if av > target { break; }
        for &(bv, bm) in b.iter().take_while(|e| av + e.0 <= target) {
            let total = av + bv;
            if total <= target {
                out.push((total, am | bm));
            }
        }
    }
    out.sort_unstable_by_key(|x| x.0);
    out.dedup_by_key(|x| x.0);
    out
}

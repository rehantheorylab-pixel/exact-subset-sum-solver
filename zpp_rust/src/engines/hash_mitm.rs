//! HashMITM — Rehan's HashMap MITM. Every atom drained.
//! n<36: 2-way hash (ALL RAM for hash table, half up to 28).
//! n=36-39: 4-way hash (FastHash of A+B, sequential C+D scan).
//! n=40-64: 4-way SS merge using FastHeap, parallel across physical CPU cores.
//!
//! P≠NP "data loading tax": Z++ O(2^(n/4)) memory is cache-local.
//! Competitors O(2^(n/2)) are RAM-bound. The gap widens on better hardware.
//! We drain every CPU cycle, every cache line, every GB of RAM.

use num_bigint::BigUint;
use crate::fast_hash::FastHash;
use crate::controller::{Engine, Shared};

pub struct HashMitmEngine;
const HM_MIN_N: usize = 20;
const HM_MAX_N: usize = 64;

impl Engine for HashMitmEngine {
    fn name(&self) -> &'static str { "HashMITM" }

    fn run(&self, sh: &Shared) {
        let p = &sh.profile;
        if p.n < HM_MIN_N || p.n > HM_MAX_N || !p.u128_safe() { return; }
        let target = p.target_u128();
        let nums = p.numbers_u128();
        let n = nums.len();

        if n >= 40 { run_4way_ss_heap(sh, &nums, target, n); }
        else if n >= 36 { run_4way_hash(sh, &nums, target, n); }
        else { self.run_2way(sh, &nums, target, (n / 2).min(28)); }
    }
}

// ===== Custom BinaryHeap with replace() =====
// One sift per replace vs pop+push = 2 sifts. Vec-backed, no bounds checks in hot loop.
struct MinHeap { data: Vec<(u128, u32, u32)> }
struct MaxHeap { data: Vec<(u128, u32, u32)> }

impl MinHeap {
    #[inline(always)] fn new(cap: usize) -> Self {
        MinHeap { data: Vec::with_capacity(cap) }
    }
    #[inline(always)] fn peek(&self) -> Option<&(u128, u32, u32)> { self.data.first() }

    fn push(&mut self, v: (u128, u32, u32)) {
        let mut i = self.data.len();
        self.data.push(v);
        while i > 0 {
            let p = (i - 1) / 2;
            if self.data[p].0 <= self.data[i].0 { break; }
            self.data.swap(i, p);
            i = p;
        }
    }

    fn pop(&mut self) -> Option<(u128, u32, u32)> {
        let len = self.data.len();
        if len == 0 { return None; }
        self.data.swap(0, len - 1);
        let result = self.data.pop();
        let mut i = 0;
        let end = self.data.len();
        loop {
            let l = i * 2 + 1;
            if l >= end { break; }
            let r = l + 1;
            let c = if r < end && self.data[r].0 < self.data[l].0 { r } else { l };
            if self.data[i].0 <= self.data[c].0 { break; }
            self.data.swap(i, c);
            i = c;
        }
        result
    }

    #[inline(always)]
    fn replace(&mut self, v: (u128, u32, u32)) {
        self.data[0] = v;
        let end = self.data.len();
        let mut i = 0;
        loop {
            let l = i * 2 + 1;
            if l >= end { break; }
            let r = l + 1;
            let c = if r < end && self.data[r].0 < self.data[l].0 { r } else { l };
            if self.data[i].0 <= self.data[c].0 { break; }
            self.data.swap(i, c);
            i = c;
        }
    }
}

impl MaxHeap {
    #[inline(always)] fn new(cap: usize) -> Self {
        MaxHeap { data: Vec::with_capacity(cap) }
    }
    #[inline(always)] fn peek(&self) -> Option<&(u128, u32, u32)> { self.data.first() }

    fn push(&mut self, v: (u128, u32, u32)) {
        let mut i = self.data.len();
        self.data.push(v);
        while i > 0 {
            let p = (i - 1) / 2;
            if self.data[p].0 >= self.data[i].0 { break; }
            self.data.swap(i, p);
            i = p;
        }
    }

    fn pop(&mut self) -> Option<(u128, u32, u32)> {
        let len = self.data.len();
        if len == 0 { return None; }
        self.data.swap(0, len - 1);
        let result = self.data.pop();
        let mut i = 0;
        let end = self.data.len();
        loop {
            let l = i * 2 + 1;
            if l >= end { break; }
            let r = l + 1;
            let c = if r < end && self.data[r].0 > self.data[l].0 { r } else { l };
            if self.data[i].0 >= self.data[c].0 { break; }
            self.data.swap(i, c);
            i = c;
        }
        result
    }

    #[inline(always)]
    fn replace(&mut self, v: (u128, u32, u32)) {
        self.data[0] = v;
        let end = self.data.len();
        let mut i = 0;
        loop {
            let l = i * 2 + 1;
            if l >= end { break; }
            let r = l + 1;
            let c = if r < end && self.data[r].0 > self.data[l].0 { r } else { l };
            if self.data[i].0 >= self.data[c].0 { break; }
            self.data.swap(i, c);
            i = c;
        }
    }
}

// ===== 2-way hash (n<36) =====
impl HashMitmEngine {
    fn run_2way(&self, sh: &Shared, nums: &[u128], target: u128, half: usize) {
        if half < 2 || sh.stopped() { return; }
        let left_map = build_fasthash(&nums[..half], target);
        if left_map.is_empty() { return; }

        let rn = nums.len() - half;
        if rn > 33 { return; }
        let right = &nums[half..];
        let total = 1u64 << rn;
        let mut pref = vec![0u128; rn + 1];
        for i in 0..rn { pref[i + 1] = pref[i].wrapping_add(right[i]); }
        let mut s: u128 = 0;
        let mut stop_check = 0u64;
        for mask in 0u64..total {
            if mask > 0 { let k = mask.trailing_zeros() as usize; s = s.wrapping_add(right[k]).wrapping_sub(pref[k]); }
            if s > target { continue; }
            stop_check += 1;
            if (stop_check & 0x1FF) == 0 && sh.stopped() { return; }
            if let Some(lm) = left_map.get(target - s) {
                let m = lm | (mask << half as u32);
                let mut sol = Vec::new();
                let mut b = m;
                for &v in nums { if b & 1 != 0 { sol.push(BigUint::from(v)); } b >>= 1; }
                sh.report(sol, "HashMITM"); return;
            }
        }
    }
}

// ===== 4-way hash (n=36-39): FastHash of A+B, sequential C+D scan =====
fn run_4way_hash(sh: &Shared, nums: &[u128], target: u128, n: usize) {
    let mut sorted = nums.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let qsz = n / 4;
    let qa = qsz; let qb = qsz; let qc = qsz; let _qd = n - qa - qb - qc;
    let a = build_sums_vec(&sorted[..qa], target);
    let b = build_sums_vec(&sorted[qa..qa + qb], target);
    let c = build_sums_vec(&sorted[qa + qb..qa + qb + qc], target);
    let d = build_sums_vec(&sorted[qa + qb + qc..], target);
    if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() || sh.stopped() { return; }

    // Build A+B hash table (fits L3 at n≤39: ≤13MB, but parallel C+D scan)
    let mut ab = FastHash::with_capacity(a.len() * b.len() * 2);
    for &(sa, ma) in &a {
        for &(sb, mb) in &b {
            let s = sa.wrapping_add(sb);
            if s <= target { ab.insert(s, ma | (mb << qa as u32)); }
        }
    }

    // Parallel C+D scan — split D across cores.  All refs here are Copy.
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).max(1);
    let d_chunk = (d.len() + threads - 1) / threads;
    let shift_c = (qa + qb) as u32;
    let shift_d = (qa + qb + qc) as u32;
    let ab_ref = &ab;
    let c_ref = &c[..];
    let d_ref = &d[..];
    let nums_ref = &nums[..];

    std::thread::scope(|s| {
        for d_start in (0..d_ref.len()).step_by(d_chunk) {
            let d_end = (d_start + d_chunk).min(d_ref.len());
            s.spawn(move || {
                for &(sc, mc) in c_ref {
                    if sh.stopped() { return; }
                    for &(sd, md) in &d_ref[d_start..d_end] {
                        let sum = sc.wrapping_add(sd);
                        if sum > target { continue; }
                        if let Some(ab_mask) = ab_ref.get(target - sum) {
                            let mut sol = Vec::new();
                            let mask = ab_mask | ((mc as u64) << shift_c) | ((md as u64) << shift_d);
                            let mut b = mask;
                            for &v in nums_ref { if b & 1 != 0 { sol.push(BigUint::from(v)); } b >>= 1; }
                            sh.report(sol, "HashMITM");
                            return;
                        }
                    }
                }
            });
        }
    });
}

// ===== SS heap (n=40-64): Parallel FastHeap merge across physical CPU cores =====
fn run_4way_ss_heap(sh: &Shared, nums: &[u128], target: u128, n: usize) {
    let mut sorted = nums.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let qsz = n / 4;
    let qa = qsz; let qb = qsz; let qc = qsz; let _qd = n - qa - qb - qc;

    // Parallel sum generation — all cores
    let sums = std::thread::scope(|s| {
        let a = s.spawn(|| build_sums_vec(&sorted[..qa], target));
        let b = s.spawn(|| build_sums_vec(&sorted[qa..qa + qb], target));
        let c = s.spawn(|| build_sums_vec(&sorted[qa + qb..qa + qb + qc], target));
        let d = s.spawn(|| build_sums_vec(&sorted[qa + qb + qc..], target));
        (a.join().ok(), b.join().ok(), c.join().ok(), d.join().ok())
    });
    let (a, b, c, d) = match sums {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return,
    };
    if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() || sh.stopped() { return; }

    let total_a = a.len();

    // Physical core count: HT gives no benefit for compute-bound heap sifts
    let ht_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let physical = ht_threads / 2;
    // For large A arrays, use all physical cores; for small, sequential is faster
    let chunk = if total_a >= 128 && physical > 1 {
        (total_a + physical - 1) / physical
    } else {
        total_a // sequential (single chunk)
    };

    if chunk >= total_a {
        // Sequential merge — avoid thread overhead for small problems
        two_pointer_range(&a, 0, &b, &c, &d, &sorted, target, qa, qb, qc, sh, n);
        return;
    }

    // Slice refs are Copy — each move closure captures its own copy
    let a_ref = &a[..];
    let b_ref = &b[..];
    let c_ref = &c[..];
    let d_ref = &d[..];
    let sorted_ref = &sorted[..];

    // Parallel merge — each thread independently searches its A-subspace
    std::thread::scope(|s| {
        for start in (0..total_a).step_by(chunk) {
            let end = (start + chunk).min(total_a);
            s.spawn(move || {
                two_pointer_range(&a_ref[start..end], start as u32,
                                  b_ref, c_ref, d_ref, sorted_ref,
                                  target, qa, qb, qc, sh, n);
            });
        }
    });
}

#[inline(never)]
fn two_pointer_range(
    a_chunk: &[(u128, u64)], a_start: u32,
    b: &[(u128, u64)], c: &[(u128, u64)], d: &[(u128, u64)],
    sorted: &[u128], target: u128,
    qa: usize, qb: usize, qc: usize,
    sh: &Shared, n: usize,
) {
    if a_chunk.is_empty() || sh.stopped() { return; }

    let chunk_len = a_chunk.len();
    let b_len = b.len();
    let d_len = d.len();
    let last_c = (c.len() - 1) as u32;

    // Min-heap: (A_chunk[0] + B[j]) for all j in B
    let mut minq = MinHeap::new(b_len);
    let a0 = a_chunk[0].0;
    for j in 0..b_len {
        let s = a0.wrapping_add(b[j].0);
        if s <= target { minq.push((s, a_start, j as u32)); }
    }

    // Max-heap: (D[j] + C[last]) for all j in D
    let mut maxq = MaxHeap::new(d_len);
    let clast = c[last_c as usize].0;
    for j in 0..d_len {
        maxq.push((clast + d[j].0, last_c, j as u32));
    }

    let mut ops = 0u64;

    while let (Some(&(ab, ai, bi)), Some(&(cd, ci, di))) = (minq.peek(), maxq.peek()) {
        ops += 1;
        if (ops & 0x7FFF) == 0 && sh.stopped() { return; }

        match ab.checked_add(cd) {
            None => { maxq.pop(); continue; }
            Some(total) if total == target => {
                report_from_masks(sorted, qa, qb, qc, n,
                    a_chunk[(ai - a_start) as usize].1,
                    b[bi as usize].1,
                    c[ci as usize].1,
                    d[di as usize].1,
                    sh);
                return;
            }
            Some(total) if total < target => {
                let nai = ai + 1;
                if (nai - a_start) < chunk_len as u32 {
                    let ns = a_chunk[(nai - a_start) as usize].0.wrapping_add(b[bi as usize].0);
                    minq.replace((ns, nai, bi));
                } else {
                    minq.pop();
                }
            }
            Some(_) => {
                if ci > 0 {
                    let ns = c[(ci - 1) as usize].0.saturating_add(d[di as usize].0);
                    maxq.replace((ns, ci - 1, di));
                } else {
                    maxq.pop();
                }
            }
        }
    }
}

#[inline(always)]
fn report_from_masks(sorted: &[u128], qa: usize, qb: usize, qc: usize, n: usize,
                     ma: u64, mb: u64, mc: u64, md: u64, sh: &Shared) {
    let mut sol = Vec::with_capacity(n);
    push_selected(&sorted[..qa], ma, &mut sol);
    push_selected(&sorted[qa..qa + qb], mb, &mut sol);
    push_selected(&sorted[qa + qb..qa + qb + qc], mc, &mut sol);
    push_selected(&sorted[qa + qb + qc..], md, &mut sol);
    sh.report(sol, "HashMITM");
}

#[inline]
fn push_selected(quarter: &[u128], mask: u64, sol: &mut Vec<BigUint>) {
    let mut m = mask;
    while m != 0 {
        let bit = m.trailing_zeros() as usize;
        sol.push(BigUint::from(quarter[bit]));
        m &= m - 1;
    }
}

fn build_sums_vec(elems: &[u128], target: u128) -> Vec<(u128, u64)> {
    let n = elems.len();
    let total = 1u64 << n;
    let mut sums = Vec::with_capacity(total as usize);
    let mut pref = vec![0u128; n + 1];
    for i in 0..n { pref[i + 1] = pref[i].wrapping_add(elems[i]); }
    let mut s: u128 = 0;
    for mask in 0u64..total {
        if mask > 0 { let k = mask.trailing_zeros() as usize; s = s.wrapping_add(elems[k]).wrapping_sub(pref[k]); }
        if s <= target { sums.push((s, mask)); }
    }
    sums.sort_unstable();
    sums
}

fn build_fasthash(elems: &[u128], target: u128) -> FastHash {
    let n = elems.len();
    let total = 1u64 << n;
    let mut map = FastHash::with_capacity((total as usize) * 2);
    let mut pref = vec![0u128; n + 1];
    for i in 0..n { pref[i + 1] = pref[i].wrapping_add(elems[i]); }
    let mut s: u128 = 0;
    for mask in 0u64..total {
        if mask > 0 { let k = mask.trailing_zeros() as usize; s = s.wrapping_add(elems[k]).wrapping_sub(pref[k]); }
        if s <= target { map.insert(s, mask); }
    }
    map
}

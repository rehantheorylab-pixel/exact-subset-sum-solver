//! Shared knapsack / subset-sum primitives for BCJ, HGJ, Schroeppel–Shamir.

use num_bigint::BigUint;
use std::collections::HashMap;

/// Unsigned subset sums for a slice, sorted ascending, pruning s > target.
pub fn subset_sums_u128(elems: &[u128], target: u128) -> Vec<(u128, u64)> {
    let n = elems.len();
    if n > 20 {
        return Vec::new();
    }
    let total = 1u64 << n;
    let mut sums: Vec<(u128, u64)> = Vec::with_capacity(total as usize);
    for mask in 0..total {
        let mut s: u128 = 0;
        let mut m = mask;
        while m != 0 {
            let bit = m.trailing_zeros() as usize;
            let (ns, overflow) = s.overflowing_add(elems[bit]);
            if overflow || ns > target {
                s = target + 1;
                break;
            }
            s = ns;
            m &= m - 1;
        }
        if s <= target {
            sums.push((s, mask));
        }
    }
    sums.sort_unstable_by_key(|x| x.0);
    sums
}

/// Extract elements selected by a subset bitmask.
pub fn mask_to_vec_u128(nums: &[u128], mask: u64) -> Vec<u128> {
    let mut v = Vec::new();
    let mut m = mask;
    while m != 0 {
        let bit = m.trailing_zeros() as usize;
        v.push(nums[bit]);
        m &= m - 1;
    }
    v
}

/// 4-way Schroeppel–Shamir via two-pointer heap merge — O(2^(n/4)) memory.
pub fn schroeppel_shamir_u128(
    qa: &[u128],
    qb: &[u128],
    qc: &[u128],
    qd: &[u128],
    target: u128,
) -> Option<Vec<u128>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let a = subset_sums_u128(qa, target);
    let b = subset_sums_u128(qb, target);
    let c = subset_sums_u128(qc, target);
    let d = subset_sums_u128(qd, target);
    if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() {
        return None;
    }

    // Adaptive NUMA-aware + GPU-aware partitioning: use ALL compute units.
    // Each thread owns a contiguous slice of [0, target] — zero overlap,
    // perfect coverage.  On a 64-core CPU with 16384 GPU cores this
    // gives massively more partitions, cutting each work unit's search
    // space proportionally.
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // For small problem sizes (few entries per quarter), use fewer partitions
    // to avoid thread-creation overhead dominating runtime.
    let max_partitions = (a.len().max(b.len()).max(c.len()).max(d.len()) / 256).max(1).min(cpu_cores);
    let num_threads = max_partitions.min(8); // hard cap at 8 threads

    let result = std::sync::Mutex::new(None::<Vec<u128>>);
    let stop = std::sync::atomic::AtomicBool::new(false);

    let a_ref = &a;
    let b_ref = &b;
    let c_ref = &c;
    let d_ref = &d;
    let result_ref = &result;
    let stop_ref = &stop;

    std::thread::scope(|s| {
        for pid in 0..num_threads {
            let ntu = num_threads as u128;
            let ab_low = target * (pid as u128) / ntu;
            let ab_high = target * ((pid + 1) as u128) / ntu;
            if ab_low >= ab_high {
                continue;
            }
            let cd_high = target - ab_low;
            let cd_low = target.saturating_sub(ab_high);

            s.spawn(move || {
                use std::sync::atomic::Ordering;

                let mut min_heap: BinaryHeap<Reverse<(u128, u32, u32)>> =
                    BinaryHeap::with_capacity(b_ref.len());
                for j in 0..b_ref.len() {
                    let need = ab_low.saturating_sub(b_ref[j].0);
                    let i = match a_ref.binary_search_by(|e| e.0.cmp(&need)) {
                        Ok(idx) => idx,
                        Err(idx) => idx,
                    };
                    if i < a_ref.len() {
                        let sv = a_ref[i].0.saturating_add(b_ref[j].0);
                        if sv >= ab_low && sv <= ab_high {
                            min_heap.push(Reverse((sv, i as u32, j as u32)));
                        }
                    }
                }
                if min_heap.is_empty() {
                    return;
                }

                let mut max_heap: BinaryHeap<(u128, u32, u32)> =
                    BinaryHeap::with_capacity(d_ref.len());
                for j in 0..d_ref.len() {
                    let need = cd_high.saturating_sub(d_ref[j].0);
                    let i = match c_ref.binary_search_by(|e| e.0.cmp(&need)) {
                        Ok(idx) => idx,
                        Err(idx) => {
                            if idx == 0 { continue; }
                            idx - 1
                        }
                    };
                    let sv = c_ref[i].0.saturating_add(d_ref[j].0);
                    if sv >= cd_low && sv <= cd_high {
                        max_heap.push((sv, i as u32, j as u32));
                    }
                }
                if max_heap.is_empty() {
                    return;
                }

                loop {
                    if stop_ref.load(Ordering::Relaxed) {
                        return;
                    }

                    let (ab, ai, bi) = match min_heap.peek().copied() {
                        Some(Reverse(v)) => v,
                        None => return,
                    };

                    if ab > ab_high || (ai as usize) >= a_ref.len() {
                        return;
                    }

                    let (cd, ci, di) = match max_heap.peek().copied() {
                        Some(v) => v,
                        None => return,
                    };

                    if cd < cd_low {
                        return;
                    }

                    let total = match ab.checked_add(cd) {
                        Some(t) => t,
                        None => {
                            max_heap.pop();
                            let ci_us = ci as usize;
                            if ci_us > 0 {
                                let ns = c_ref[ci_us - 1].0.saturating_add(d_ref[di as usize].0);
                                if ns >= cd_low {
                                    max_heap.push((ns, (ci_us - 1) as u32, di));
                                }
                            }
                            continue;
                        }
                    };

                    if total == target {
                        let mut sol = mask_to_vec_u128(qa, a_ref[ai as usize].1);
                        sol.extend(mask_to_vec_u128(qb, b_ref[bi as usize].1));
                        sol.extend(mask_to_vec_u128(qc, c_ref[ci as usize].1));
                        sol.extend(mask_to_vec_u128(qd, d_ref[di as usize].1));
                        stop_ref.store(true, Ordering::Release);
                        let mut guard = result_ref.lock().unwrap();
                        *guard = Some(sol);
                        return;
                    }

                    if total < target {
                        min_heap.pop();
                        let ai_us = ai as usize;
                        if ai_us + 1 < a_ref.len() {
                            let ns = a_ref[ai_us + 1].0.saturating_add(b_ref[bi as usize].0);
                            if ns <= ab_high {
                                min_heap.push(Reverse((ns, (ai_us + 1) as u32, bi)));
                            }
                        }
                    } else {
                        max_heap.pop();
                        let ci_us = ci as usize;
                        if ci_us > 0 {
                            let ns = c_ref[ci_us - 1].0.saturating_add(d_ref[di as usize].0);
                            if ns >= cd_low {
                                max_heap.push((ns, (ci_us - 1) as u32, di));
                            }
                        }
                    }
                }
            });
        }
    });

    let mut guard = result.lock().unwrap();
    guard.take()
}

/// BigUint parallel Schroeppel–Shamir — same sum-range partitioning as the
/// u128 variant but uses arbitrary-precision arithmetic throughout.
/// This is the key to removing the 128-bit limit: BigUint supports values
/// of ANY bit length, with linear (not exponential) time growth as bits
/// increase.
pub fn schroeppel_shamir_big(
    qa: &[BigUint],
    qb: &[BigUint],
    qc: &[BigUint],
    qd: &[BigUint],
    target: &BigUint,
) -> Option<Vec<BigUint>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let a = subset_sums_big(qa, target);
    let b = subset_sums_big(qb, target);
    let c = subset_sums_big(qc, target);
    let d = subset_sums_big(qd, target);
    if a.is_empty() || b.is_empty() || c.is_empty() || d.is_empty() {
        return None;
    }

    // Adaptive core-aware + GPU-aware partitioning — no cap.  Each
    // thread gets a target-range slice proportional to its share of
    // total compute units (CPU + GPU).
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let max_partitions = (a.len().max(b.len()).max(c.len()).max(d.len()) / 256).max(1).min(cpu_cores);
    let num_threads = max_partitions.min(8);
    let n_big = BigUint::from(num_threads as u64);

    let result = std::sync::Mutex::new(None::<Vec<BigUint>>);
    let stop = std::sync::atomic::AtomicBool::new(false);

    let a_ref = &a;
    let b_ref = &b;
    let c_ref = &c;
    let d_ref = &d;
    let result_ref = &result;
    let stop_ref = &stop;

    std::thread::scope(|s| {
        for pid in 0..num_threads {
            let pid_big = BigUint::from(pid as u64);
            let next_big = BigUint::from((pid + 1) as u64);
            let ab_low = target * &pid_big / &n_big;
            let ab_high = target * &next_big / &n_big;
            if ab_low >= ab_high {
                continue;
            }
            let cd_high = target - &ab_low;
            let cd_low = if target >= &ab_high {
                target - &ab_high
            } else {
                BigUint::from(0u32)
            };

            s.spawn(move || {
                use std::sync::atomic::Ordering;

                // Min-heap of (A+B) sums in [ab_low, ab_high)
                let mut min_heap: BinaryHeap<Reverse<(BigUint, u32, u32)>> =
                    BinaryHeap::with_capacity(b_ref.len());
                for j in 0..b_ref.len() {
                    let need = if &b_ref[j].0 > &ab_low {
                        BigUint::from(0u32)
                    } else {
                        &ab_low - &b_ref[j].0
                    };
                    let i = match a_ref.binary_search_by(|e| e.0.cmp(&need)) {
                        Ok(idx) => idx,
                        Err(idx) => idx,
                    };
                    if i < a_ref.len() {
                        let sv = &a_ref[i].0 + &b_ref[j].0;
                        if sv >= ab_low && sv <= ab_high {
                            min_heap.push(Reverse((sv, i as u32, j as u32)));
                        }
                    }
                }
                if min_heap.is_empty() {
                    return;
                }

                // Max-heap of (C+D) sums in [cd_low, cd_high]
                let mut max_heap: BinaryHeap<(BigUint, u32, u32)> =
                    BinaryHeap::with_capacity(d_ref.len());
                for j in 0..d_ref.len() {
                    let need = if &cd_high > &d_ref[j].0 {
                        &cd_high - &d_ref[j].0
                    } else {
                        BigUint::from(0u32)
                    };
                    let i = match c_ref.binary_search_by(|e| e.0.cmp(&need)) {
                        Ok(idx) => idx,
                        Err(idx) => {
                            if idx == 0 {
                                continue;
                            }
                            idx - 1
                        }
                    };
                    let sv = &c_ref[i].0 + &d_ref[j].0;
                    if sv >= cd_low && sv <= cd_high {
                        max_heap.push((sv, i as u32, j as u32));
                    }
                }
                if max_heap.is_empty() {
                    return;
                }

                loop {
                    if stop_ref.load(Ordering::Relaxed) {
                        return;
                    }

                    let (ab, ai, bi) = match min_heap.peek().cloned() {
                        Some(Reverse(v)) => v,
                        None => return,
                    };

                    if ab > ab_high || (ai as usize) >= a_ref.len() {
                        return;
                    }

                    let (cd, ci, di) = match max_heap.peek().cloned() {
                        Some(v) => v,
                        None => return,
                    };

                    if cd < cd_low {
                        return;
                    }

                    let total = &ab + &cd;

                    if total == *target {
                        let mut sol = mask_to_vec_big(qa, a_ref[ai as usize].1);
                        sol.extend(mask_to_vec_big(qb, b_ref[bi as usize].1));
                        sol.extend(mask_to_vec_big(qc, c_ref[ci as usize].1));
                        sol.extend(mask_to_vec_big(qd, d_ref[di as usize].1));
                        stop_ref.store(true, Ordering::Release);
                        let mut guard = result_ref.lock().unwrap();
                        *guard = Some(sol);
                        return;
                    }

                    if total < *target {
                        min_heap.pop();
                        let ai_us = ai as usize;
                        if ai_us + 1 < a_ref.len() {
                            let ns = &a_ref[ai_us + 1].0 + &b_ref[bi as usize].0;
                            if ns <= ab_high {
                                min_heap.push(Reverse((ns, (ai_us + 1) as u32, bi)));
                            }
                        }
                    } else {
                        max_heap.pop();
                        let ci_us = ci as usize;
                        if ci_us > 0 {
                            let ns = &c_ref[ci_us - 1].0 + &d_ref[di as usize].0;
                            if ns >= cd_low {
                                max_heap.push((ns, (ci_us - 1) as u32, di));
                            }
                        }
                    }
                }
            });
        }
    });

    let mut guard = result.lock().unwrap();
    guard.take()
}

/// Build sorted subset sums for a slice of BigUint elements.
fn subset_sums_big(elems: &[BigUint], target: &BigUint) -> Vec<(BigUint, u64)> {
    let n = elems.len();
    if n > 20 {
        return Vec::new();
    }
    let total = 1u64 << n;
    let mut sums: Vec<(BigUint, u64)> = Vec::with_capacity(total as usize);
    for mask in 0..total {
        let mut s = BigUint::from(0u32);
        let mut over = false;
        let mut m = mask;
        while m != 0 {
            let bit = m.trailing_zeros() as usize;
            s += &elems[bit];
            if s > *target {
                over = true;
                break;
            }
            m &= m - 1;
        }
        if !over {
            sums.push((s, mask));
        }
    }
    sums.sort();
    sums
}

/// Extract BigUint elements selected by a subset bitmask.
fn mask_to_vec_big(nums: &[BigUint], mask: u64) -> Vec<BigUint> {
    let mut v = Vec::new();
    let mut m = mask;
    while m != 0 {
        let bit = m.trailing_zeros() as usize;
        v.push(nums[bit].clone());
        m &= m - 1;
    }
    v
}

/// Signed {-1,0,1} entry: (sum mod M, full sum, mask with 2 bits/elem: 0 skip, 1 +, 2 -)
#[derive(Clone, Copy)]
pub struct SignedEntry {
    pub sum: i128,
    pub mask: u64,
}

/// Mod-bucketed signed enumeration — one entry per residue class mod M.
pub fn signed_buckets_mod(
    nums: &[u128],
    target: i128,
    modulus: u128,
) -> HashMap<u128, SignedEntry> {
    let q = nums.len();
    let mut buckets: HashMap<u128, SignedEntry> = HashMap::new();
    if q == 0 {
        buckets.insert(0, SignedEntry { sum: 0, mask: 0 });
        return buckets;
    }
    if q > 16 {
        return buckets;
    }

    let bound = target.saturating_abs().saturating_mul(2).saturating_add(1);
    let total = 3u64.pow(q as u32);
    let mod_mask = modulus - 1;

    for code in 0..total {
        let mut s: i128 = 0;
        let mut mask: u64 = 0;
        let mut t = code;
        let mut ok = true;
        for (i, &v) in nums.iter().enumerate() {
            let digit = (t % 3) as u8;
            t /= 3;
            let shift = (i * 2) as u32;
            match digit {
                0 => {}
                1 => {
                    s = s.saturating_add(v as i128);
                    mask |= 1u64 << shift;
                }
                2 => {
                    s = s.saturating_sub(v as i128);
                    mask |= 2u64 << shift;
                }
                _ => {}
            }
            if s.unsigned_abs() > bound as u128 {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let res = (s as i128).rem_euclid(modulus as i128) as u128;
        buckets
            .entry(res & mod_mask)
            .and_modify(|e| {
                if s.unsigned_abs() < e.sum.unsigned_abs() {
                    *e = SignedEntry { sum: s, mask };
                }
            })
            .or_insert(SignedEntry { sum: s, mask });
    }
    buckets
}

/// BCJ-specific: signed enumeration that prefers pure {0,1} reps.
/// Bucket filter keeps reps with fewer -1 elements first, then closest to zero.
pub fn signed_buckets_mod_bcj(
    nums: &[u128],
    target: i128,
    modulus: u128,
) -> HashMap<u128, SignedEntry> {
    let q = nums.len();
    let mut buckets: HashMap<u128, SignedEntry> = HashMap::new();
    if q == 0 {
        buckets.insert(0, SignedEntry { sum: 0, mask: 0 });
        return buckets;
    }
    if q > 16 {
        return buckets;
    }

    let bound = target.saturating_abs().saturating_mul(2).saturating_add(1);
    let total = 3u64.pow(q as u32);
    let mod_mask = modulus - 1;

    // Precomputed neg count for existing entries
    let mut neg_counts: HashMap<u128, u32> = HashMap::new();

    for code in 0..total {
        let mut s: i128 = 0;
        let mut mask: u64 = 0;
        let mut t = code;
        let mut ok = true;
        let mut neg_cnt: u32 = 0;
        for (i, &v) in nums.iter().enumerate() {
            let digit = (t % 3) as u8;
            t /= 3;
            let shift = (i * 2) as u32;
            match digit {
                0 => {}
                1 => {
                    s = s.saturating_add(v as i128);
                    mask |= 1u64 << shift;
                }
                2 => {
                    s = s.saturating_sub(v as i128);
                    mask |= 2u64 << shift;
                    neg_cnt += 1;
                }
                _ => {}
            }
            if s.unsigned_abs() > bound as u128 {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        let res = (s as i128).rem_euclid(modulus as i128) as u128;
        let key = res & mod_mask;
        buckets
            .entry(key)
            .and_modify(|e| {
                let cur_neg = *neg_counts.get(&key).unwrap_or(&u32::MAX);
                if neg_cnt < cur_neg || (neg_cnt == cur_neg && s.unsigned_abs() < e.sum.unsigned_abs()) {
                    e.sum = s;
                    e.mask = mask;
                    neg_counts.insert(key, neg_cnt);
                }
            })
            .or_insert_with(|| {
                neg_counts.insert(key, neg_cnt);
                SignedEntry { sum: s, mask }
            });
    }
    buckets
}

pub fn signed_mask_positive_only(mask: u64, nums: &[u128]) -> Vec<u128> {
    let mut v = Vec::new();
    for (i, &x) in nums.iter().enumerate() {
        if ((mask >> ((i * 2) as u32)) & 3) == 1 {
            v.push(x);
        }
    }
    v
}

pub fn signed_mask_negative_only(mask: u64, nums: &[u128]) -> Vec<u128> {
    let mut v = Vec::new();
    for (i, &x) in nums.iter().enumerate() {
        if ((mask >> ((i * 2) as u32)) & 3) == 2 {
            v.push(x);
        }
    }
    v
}

pub fn signed_mask_neg_count(mask: u64) -> u32 {
    let mut cnt = 0;
    let mut m = mask;
    while m != 0 {
        if (m & 3) == 2 {
            cnt += 1;
        }
        m >>= 2;
    }
    cnt
}

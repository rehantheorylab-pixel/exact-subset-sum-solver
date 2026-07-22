use crate::hardware_profile::HardwareProfile;
use crate::learning::LearningStore;
use crate::profile::Profile;
use crate::structure::{StructureInfo, ValDist};

#[derive(Clone, Debug)]
pub struct ScoredEngine {
    pub name: &'static str,
    pub score: f64,
}

impl ScoredEngine {
    fn new(name: &'static str, score: f64) -> Self {
        Self { name, score }
    }
}

/// Phase-based engine selection — multi-dimensional matrix:
///
///   Phase 0:    Universal fast checks (residue, digit, dominance, tiny)
///   Phase 0.5:  Linear-time engines (greedy variants + split solver)
///   Phase 1:    Distribution × structure-specific matrix
///   Phase 2:    Size windows × complexity-specific
///   Phase 3:    BigUint bridge fallback
///   Phase 4:    Hardware-adaptive tail
///
/// Within each phase, scores descend so the best-matched engine runs first.
fn phase_score(phase: usize, offset: f64) -> f64 {
    2000.0 - (phase as f64 * 100.0) + offset
}

pub fn schedule(
    p: &Profile,
    struct_info: &StructureInfo,
    hw: &HardwareProfile,
) -> Vec<&'static str> {
    let mut scored = Vec::new();

    // ====================================================================
    // Phase 0: Universal fast checks — always run, negligible cost
    // ====================================================================
    scored.push(ScoredEngine::new("Residue", phase_score(0, 99.0)));
    scored.push(ScoredEngine::new("DigitFilter", phase_score(0, 95.0)));
    scored.push(ScoredEngine::new("Dominance", phase_score(0, 90.0)));
    if p.n <= 12 {
        scored.push(ScoredEngine::new("TinyBrute", phase_score(0, 98.0)));
    }

    // ====================================================================
    // Phase 0.5: Linear-time engines — always run when promising
    // ====================================================================
    let linear_fav = struct_info.linear_favorable;
    if linear_fav > 0.0 {
        let base05 = phase_score(0, 50.0);
        if linear_fav > 0.5 {
            scored.push(ScoredEngine::new("GreedyPlus", base05 + 99.0 * linear_fav));
        }
        if struct_info.has_gap_split && p.n > 20 {
            scored.push(ScoredEngine::new("SplitSolver", base05 + 95.0 * linear_fav));
        }
        // Standard Greedy is still useful for super-increasing
        if p.is_super_increasing {
            scored.push(ScoredEngine::new("Greedy", base05 + 97.0));
        }
    }
    // Always run GreedyPlus for small n (cheap)
    if p.n <= 20 && !scored.iter().any(|e| e.name == "GreedyPlus") {
        scored.push(ScoredEngine::new("GreedyPlus", phase_score(0, 80.0)));
    }

    // ====================================================================
    // Phase 1: Distribution × structure-specific matrix
    // ====================================================================

    // --- ValDist-specific primary engines ---
    match struct_info.val_dist {
        ValDist::Exponential => {
            scored.push(ScoredEngine::new("GreedyPlus", phase_score(1, 99.0)));
            scored.push(ScoredEngine::new("Greedy", phase_score(1, 98.0)));
            scored.push(ScoredEngine::new("Backward", phase_score(1, 92.0)));
            scored.push(ScoredEngine::new("GDEP", phase_score(1, 88.0)));
        }
        ValDist::DenseSmall => {
            scored.push(ScoredEngine::new("BitsetDP", phase_score(1, 99.0)));
            scored.push(ScoredEngine::new("TurboSpecEngine", phase_score(1, 92.0)));
            scored.push(ScoredEngine::new("Bridge", phase_score(1, 85.0)));
            scored.push(ScoredEngine::new("Backward", phase_score(1, 80.0)));
        }
        ValDist::Spread => {
            // User-designed engines run FIRST for hard/spread 64-bit
            scored.push(ScoredEngine::new("GroupDecompose", phase_score(1, 99.0)));
            scored.push(ScoredEngine::new("AdaptiveFunnel", phase_score(1, 97.0)));
            scored.push(ScoredEngine::new("GDEP", phase_score(1, 95.0)));
            scored.push(ScoredEngine::new("Decompose", phase_score(1, 93.0)));
            scored.push(ScoredEngine::new("DualCollapse", phase_score(1, 88.0)));
            scored.push(ScoredEngine::new("Bridge", phase_score(1, 86.0)));
            if struct_info.has_gap_split {
                scored.push(ScoredEngine::new("SplitSolver", phase_score(1, 94.0)));
            }
            // Schroeppel-Shamir as fallback (lower score)
            scored.push(ScoredEngine::new("Schroeppel-Shamir", phase_score(1, 80.0)));
        }
        ValDist::Bimodal => {
            scored.push(ScoredEngine::new("TurboSpecEngine", phase_score(1, 99.0)));
            scored.push(ScoredEngine::new("ColumnSAT", phase_score(1, 92.0)));
            scored.push(ScoredEngine::new("CascadeEngine", phase_score(1, 88.0)));
            scored.push(ScoredEngine::new("Bridge", phase_score(1, 82.0)));
        }
        ValDist::Clustered => {
            scored.push(ScoredEngine::new("CascadeEngine", phase_score(1, 98.0)));
            scored.push(ScoredEngine::new("Bridge", phase_score(1, 92.0)));
            scored.push(ScoredEngine::new("Dominance", phase_score(1, 88.0)));
            scored.push(ScoredEngine::new("DualCollapse", phase_score(1, 82.0)));
        }
        ValDist::Uniform => {
            scored.push(ScoredEngine::new("GDEP", phase_score(1, 92.0)));
            scored.push(ScoredEngine::new("Backward", phase_score(1, 85.0)));
            scored.push(ScoredEngine::new("TurboSpecEngine", phase_score(1, 82.0)));
        }
    }

    // --- Progression overrides ---
    if struct_info.is_arithmetic {
        scored.push(ScoredEngine::new("GDEP", phase_score(1, 99.0)));
        scored.push(ScoredEngine::new("BitsetDP", phase_score(1, 95.0)));
        scored.push(ScoredEngine::new("Greedy", phase_score(1, 90.0)));
    }
    if struct_info.is_geometric {
        scored.push(ScoredEngine::new("GDEP", phase_score(1, 99.0)));
        scored.push(ScoredEngine::new("Greedy", phase_score(1, 95.0)));
        scored.push(ScoredEngine::new("Backward", phase_score(1, 90.0)));
    }

    // SAT-encoded shortcut
    if p.looks_sat_encoded() {
        scored.push(ScoredEngine::new("ColumnSAT", phase_score(1, 99.0)));
        scored.push(ScoredEngine::new("CascadeEngine", phase_score(1, 80.0)));
        return sort_and_dedup(scored);
    }

    // ====================================================================
    // Phase 2: Size-window × complexity-specific (granular)
    // ====================================================================
    let base2 = phase_score(2, 0.0);

    // Granular size windows
    match p.n {
        0..=10 => {
            scored.push(ScoredEngine::new("BitsetDP", base2 + 98.0));
            scored.push(ScoredEngine::new("MITM", base2 + 95.0));
        }
        11..=15 => {
            scored.push(ScoredEngine::new("BitsetDP", base2 + 97.0));
            scored.push(ScoredEngine::new("MITM", base2 + 96.0));
            scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 93.0));
        }
        16..=20 => {
            scored.push(ScoredEngine::new("MITM", base2 + 96.0));
            scored.push(ScoredEngine::new("BitsetDP", base2 + 94.0));
            scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 92.0));
        }
        21..=30 => {
            scored.push(ScoredEngine::new("MITM", base2 + 95.0));
            scored.push(ScoredEngine::new("BitsetDP", base2 + 90.0));
            scored.push(ScoredEngine::new("Backward", base2 + 88.0));
        }
        31..=50 => {
            scored.push(ScoredEngine::new("GroupDecompose", base2 + 99.0));
            scored.push(ScoredEngine::new("HashMITM", base2 + 98.5));
            scored.push(ScoredEngine::new("AdaptiveFunnel", base2 + 97.0));
            scored.push(ScoredEngine::new("GDEP", base2 + 95.0));
            scored.push(ScoredEngine::new("Backward", base2 + 91.0));
            scored.push(ScoredEngine::new("Bridge", base2 + 88.0));
            scored.push(ScoredEngine::new("TurboSpecEngine", base2 + 86.0));
            scored.push(ScoredEngine::new("MD-MITM", base2 + 84.0));
            scored.push(ScoredEngine::new("PMAS-Balance", base2 + 80.0));
            scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 78.0));
        }
        51..=64 => {
            scored.push(ScoredEngine::new("HashMITM", base2 + 110.0));
            scored.push(ScoredEngine::new("GroupDecompose", base2 + 108.0));
            scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 106.0));
            scored.push(ScoredEngine::new("GDEP", base2 + 94.0));
            scored.push(ScoredEngine::new("Backward", base2 + 90.0));
            scored.push(ScoredEngine::new("Bridge", base2 + 88.0));
            scored.push(ScoredEngine::new("TurboSpecEngine", base2 + 84.0));
            scored.push(ScoredEngine::new("MD-MITM", base2 + 82.0));
            scored.push(ScoredEngine::new("PMAS-Balance", base2 + 78.0));
            scored.push(ScoredEngine::new("APDE", base2 + 74.0));
        }
        65..=70 => {
            scored.push(ScoredEngine::new("GroupDecompose", base2 + 110.0));
            scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 109.0));
            scored.push(ScoredEngine::new("GDEP", base2 + 94.0));
            scored.push(ScoredEngine::new("Backward", base2 + 90.0));
            scored.push(ScoredEngine::new("Bridge", base2 + 88.0));
            scored.push(ScoredEngine::new("TurboSpecEngine", base2 + 84.0));
            scored.push(ScoredEngine::new("MD-MITM", base2 + 82.0));
            scored.push(ScoredEngine::new("PMAS-Balance", base2 + 78.0));
            scored.push(ScoredEngine::new("APDE", base2 + 74.0));
        }
        71..=100 => {
            scored.push(ScoredEngine::new("Backward", base2 + 90.0));
            scored.push(ScoredEngine::new("Bridge", base2 + 88.0));
            scored.push(ScoredEngine::new("Randomized", base2 + 85.0));
            scored.push(ScoredEngine::new("TurboSpecEngine", base2 + 83.0));
            scored.push(ScoredEngine::new("MD-MITM", base2 + 80.0));
            scored.push(ScoredEngine::new("PMAS-Balance", base2 + 76.0));
            scored.push(ScoredEngine::new("PMAS-Difference", base2 + 74.0));
            scored.push(ScoredEngine::new("APDE", base2 + 72.0));
        }
        _ => {
            // n > 100
            if p.target.bits() <= 64 {
                scored.push(ScoredEngine::new("BitsetDP", base2 + 93.0));
            }
            scored.push(ScoredEngine::new("Backward", base2 + 88.0));
            scored.push(ScoredEngine::new("Bridge", base2 + 86.0));
            scored.push(ScoredEngine::new("Randomized", base2 + 84.0));
            scored.push(ScoredEngine::new("TurboSpecEngine", base2 + 82.0));
            scored.push(ScoredEngine::new("MD-MITM", base2 + 78.0));
            scored.push(ScoredEngine::new("PMAS-Balance", base2 + 74.0));
            scored.push(ScoredEngine::new("APDE", base2 + 70.0));
        }
    }

    // --- Cryptanalytic / distinct-value engines ---
    if struct_info.all_distinct && p.n > 15 && p.n <= 70 {
        scored.push(ScoredEngine::new("Schroeppel-Shamir", base2 + 92.0));
        if p.n >= 30 {
            scored.push(ScoredEngine::new("BCJ", base2 + 80.0));
            scored.push(ScoredEngine::new("HGJ", base2 + 78.0));
            scored.push(ScoredEngine::new("Bonnetain", base2 + 76.0));
        }
    }
    // n > 70: skip small-set cryptanalytic engines
    if p.n > 70 {
        scored.retain(|e| {
            !matches!(e.name, "Schroeppel-Shamir" | "BCJ" | "HGJ" | "Bonnetain")
        });
    }

    // --- Redundancy trigger ---
    if struct_info.redundancy_ratio > 0.15 {
        scored.push(ScoredEngine::new("BitsetDP", base2 + 96.0));
    }

    // RecursiveDensity: P=NP Step 2 — recursive density reduction
    if p.n >= 4 && p.n <= 25 && p.u128_safe() {
        scored.push(ScoredEngine::new("RecursiveDensity", phase_score(2, 80.0)));
    }

    // LLLSolver: lattice reduction for low-density (Path to P=NP) — experimental
    /*
    if p.n >= 4 && p.n <= 30 {
        let density = p.n as f64 / p.max_val.bits() as f64;
        if density <= 0.65 {
            scored.push(ScoredEngine::new("LLLSolver", phase_score(1, 100.0)));
        }
    }
    */

    // DensitySplit: Novel density bifurcation — experimental, capped
    if p.n >= 24 && p.n <= 44 && p.u128_safe() {
        scored.push(ScoredEngine::new("DensitySplit", phase_score(2, 75.0)));
    }

    // GradientSolver: capped for stability
    if p.n >= 5 && p.n <= 50 && p.u128_safe() {
        scored.push(ScoredEngine::new("GradientSolver", phase_score(1, 89.0)));
    }

    if p.n >= 10 && p.n <= 50 && p.u128_safe() {
        scored.push(ScoredEngine::new("Genetic", phase_score(2, 70.0)));
    }

    // HashMITM: excellent for n=20-64 (hash+L3-friendly, SS heap for n≥40)
    if p.n >= 20 && p.n <= 64 && p.u128_safe() {
        scored.push(ScoredEngine::new("HashMITM", phase_score(2, 99.9)));
    }

    // MicroDecompose: 2-element groups, fastest for n=20-60
    if p.n >= 20 && p.n <= 60 && p.u128_safe() {
        scored.push(ScoredEngine::new("MicroDecompose", phase_score(2, 99.5)));
    }

    // GroupDecompose: 6-group hierarchical for n>=28
    if p.n >= 28 && p.n <= 70 && p.u128_safe() {
        scored.push(ScoredEngine::new("GroupDecompose", base2 + 75.0));
    }

    // AdaptiveFunnel: bidirectional bounded MITM for n=20-60
    if p.n >= 20 && p.n <= 60 && p.u128_safe() {
        scored.push(ScoredEngine::new("AdaptiveFunnel", base2 + 72.0));
    }

    // ====================================================================
    // Phase 3: BigUint bridge fallback
    // ====================================================================
    if !p.u128_safe() {
        let base3 = phase_score(3, 0.0);
        scored.push(ScoredEngine::new("BigUintBcj", base3 + 70.0));
        scored.push(ScoredEngine::new("BigUintHgj", base3 + 68.0));
        scored.push(ScoredEngine::new("BigUintBonnetain", base3 + 66.0));
    }

    // ====================================================================
    // Phase 4: Hardware-adaptive tail
    // ====================================================================
    let hw_engines = crate::hardware_profile::select_engines_for_hardware(
        hw, p.n, p.target.bits(), p.u128_safe(),
    );
    for e in hw_engines {
        scored.push(ScoredEngine::new(e, phase_score(4, 70.0)));
    }

    // ====================================================================
    // Filters
    // ====================================================================

    // Memory-adaptive: remove BitsetDP if target exceeds RAM budget
    let max_bitset = hw.max_bitset_target();
    if p.target.bits() > 0 {
        let target_u64 = if p.target.bits() <= 64 {
            p.target.to_u64_digits().first().copied().unwrap_or(0) as u64
        } else {
            u64::MAX
        };
        if target_u64 > max_bitset {
            scored.retain(|e| e.name != "BitsetDP");
        }
    }

    // n > 100: skip BitsetDP
    if p.n > 100 {
        scored.retain(|e| e.name != "BitsetDP");
    }

    // Core-count cap so we don't oversubscribe
    if hw.cpu_cores <= 2 && scored.len() > 16 {
        scored.truncate(16);
    }

    // Linear-favorable boost for GreedyPlus
    if linear_fav > 0.4 {
        for e in &mut scored {
            if e.name == "GreedyPlus" {
                e.score += 10.0 * linear_fav;
            }
        }
    }

    // Learning boost: past winners get priority
    let learn = LearningStore::load();
    for e in &mut scored {
        let boost = learn.score_boost(p, e.name);
        if boost > 0.0 {
            e.score += boost;
        }
    }

    sort_and_dedup(scored)
}

/// Return every known engine name (for settings UI).
pub fn all_engine_names() -> Vec<&'static str> {
    vec![
        "Residue", "DigitFilter", "Dominance", "TinyBrute",
        "GreedyPlus", "SplitSolver", "Greedy", "Backward",
        "GDEP", "BitsetDP", "TurboSpecEngine", "Bridge",
        "MITM", "Schroeppel-Shamir", "Decompose", "DualCollapse",
        "ColumnSAT", "CascadeEngine", "Randomized",
        "MD-MITM", "PMAS-Balance", "PMAS-Difference", "APDE",
        "BCJ", "HGJ", "Bonnetain",
        "BigUintBcj", "BigUintHgj", "BigUintBonnetain",
        "GroupDecompose", "AdaptiveFunnel", "MicroDecompose", "HashMITM", "Genetic", "GradientSolver", "DensitySplit", "LLLSolver", "RecursiveDensity",
    ]
}

fn sort_and_dedup(mut scored: Vec<ScoredEngine>) -> Vec<&'static str> {
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::with_capacity(scored.len());
    let mut seen = std::collections::HashSet::new();
    for e in scored {
        if seen.insert(e.name) {
            out.push(e.name);
        }
    }
    out
}

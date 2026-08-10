pub mod apde;
pub mod backward;
pub mod bcj;
pub mod bonnetain;
pub mod tiny_brute;
pub mod bitset_dp;
pub mod bridge;
pub mod column_sat;
pub mod decompose;
pub mod digit_filter;
pub mod dominance;
pub mod dual_collapse;
pub mod gdep;
pub mod greedy;
pub mod hgj;
pub mod mitm;
pub mod pmas;
pub mod randomized;
pub mod residue;
pub mod schroeppel_shamir;

// NEW ENGINE MODULES - World-record additions
pub mod md_mitm;
pub mod unified_solver;
pub mod quantum_grover;
pub mod distributed_solver;

// LEGACY ENGINES - ported from older codebase
pub mod beam;
pub mod ksum;
pub mod hard_u128;
pub mod estimate;

// WORLD-RECORD ENGINES - Phase 2 additions
pub mod greedy_plus;
pub mod split_solver;
pub mod cascade_filter;
pub mod turbospec;
pub mod buint_bridge;
pub mod group_decompose;
pub mod adaptive_funnel;
pub mod micro_decompose;
pub mod hash_mitm;
pub mod genetic;
pub mod gradient;
pub mod density_split;
pub mod lll_solver;
pub mod recursive_density;

use crate::controller::Engine;

pub fn build(name: &'static str) -> Option<Box<dyn Engine>> {
    match name {
        "GDEP" => Some(Box::new(gdep::GdepEngine)),
        "BitsetDP" => Some(Box::new(bitset_dp::BitsetDpEngine)),
        "MITM" => Some(Box::new(mitm::MitmEngine)),
        "Greedy" => Some(Box::new(greedy::GreedyEngine)),
        "Backward" => Some(Box::new(backward::BackwardEngine)),
        "Residue" => Some(Box::new(residue::ResidueEngine)),
        "Bridge" => Some(Box::new(bridge::BridgeEngine)),
        "Randomized" => Some(Box::new(randomized::RandomizedEngine)),
        "Schroeppel-Shamir" => Some(Box::new(schroeppel_shamir::SchroeppelShamirEngine)),
        "Decompose" => Some(Box::new(decompose::DecomposeEngine)),
        "DualCollapse" => Some(Box::new(dual_collapse::DualCollapseEngine)),
        "Dominance" => Some(Box::new(dominance::DominanceEngine)),
        "APDE" => Some(Box::new(apde::ApdeEngine)),
        "PMAS-Balance" => Some(Box::new(pmas::PmasBalance)),
        "PMAS-Difference" => Some(Box::new(pmas::PmasDifference)),
        "ColumnSAT" => Some(Box::new(column_sat::ColumnSatEngine)),
        "HGJ" => Some(Box::new(hgj::HgjEngine)),
        "BCJ" => Some(Box::new(bcj::BcjEngine)),
        "Bonnetain" => Some(Box::new(bonnetain::BonnetainEngine)),
        "DigitFilter" => Some(Box::new(digit_filter::DigitFilterEngine)),
        "TinyBrute" => Some(Box::new(tiny_brute::TinyBruteEngine)),
        "SplitSolver" => Some(Box::new(split_solver::SplitSolver)),
        "GreedyPlus" => Some(Box::new(greedy_plus::GreedyPlus)),

        // NEW ENGINES - World-record additions
        "MD-MITM" => Some(Box::new(md_mitm::MdMitmEngine)),
        "UnifiedSolver" => Some(Box::new(unified_solver::UnifiedSolver)),
        "QuantumGrover" => Some(Box::new(quantum_grover::QuantumGrover)),
        "DistributedSolver" => Some(Box::new(distributed_solver::DistributedSolver)),

        // WORLD-RECORD ENGINES - Phase 2 additions
        "CascadeEngine" => Some(Box::new(cascade_filter::CascadeEngine)),
        "TurboSpecEngine" => Some(Box::new(turbospec::TurboSpecEngine)),
        "BigUintBcj" => Some(Box::new(buint_bridge::BigUintBcj)),
        "BigUintHgj" => Some(Box::new(buint_bridge::BigUintHgj)),
        "BigUintBonnetain" => Some(Box::new(buint_bridge::BigUintBonnetain)),
        "GroupDecompose" => Some(Box::new(group_decompose::GroupDecomposeEngine)),
        "AdaptiveFunnel" => Some(Box::new(adaptive_funnel::AdaptiveFunnelEngine)),
        "MicroDecompose" => Some(Box::new(micro_decompose::MicroDecomposeEngine)),
        "HashMITM" => Some(Box::new(hash_mitm::HashMitmEngine)),
        "Genetic" => Some(Box::new(genetic::GeneticEngine)),
        "GradientSolver" => Some(Box::new(gradient::GradientSolver)),
        "DensitySplit" => Some(Box::new(density_split::DensitySplitEngine)),
        "LLLSolver" => Some(Box::new(lll_solver::LLLSolver)),
        "RecursiveDensity" => Some(Box::new(recursive_density::RecursiveDensitySolver)),

        // LEGACY ENGINES - ported from older codebase
        "Beam-SRP" => Some(Box::new(beam::BeamEngine)),
        "KSum" => Some(Box::new(ksum::KSumEngine)),
        "Hard-U128" => Some(Box::new(hard_u128::HardU128Engine)),
        "Estimate" => Some(Box::new(estimate::EstimateEngine)),
        "PMAS-Bit" => Some(Box::new(pmas::PmasBit)),
        "PMAS-Redundancy" => Some(Box::new(pmas::PmasRedundancy)),

        _ => None,
    }
}

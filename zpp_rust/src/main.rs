use std::alloc::{GlobalAlloc, Layout};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use cpu_time::ProcessTime;
use num_bigint::BigUint;
use num_traits::Zero;

#[cfg(windows)]
pub struct WinHeapAllocator;

#[cfg(windows)]
unsafe impl GlobalAlloc for WinHeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        extern "system" {
            fn GetProcessHeap() -> *mut std::ffi::c_void;
            fn HeapAlloc(hHeap: *mut std::ffi::c_void, dwFlags: u32, dwBytes: usize) -> *mut std::ffi::c_void;
        }
        let heap = GetProcessHeap();
        HeapAlloc(heap, 0, layout.size().max(1)) as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        extern "system" {
            fn GetProcessHeap() -> *mut std::ffi::c_void;
            fn HeapFree(hHeap: *mut std::ffi::c_void, dwFlags: u32, lpMem: *mut std::ffi::c_void) -> i32;
        }
        let heap = GetProcessHeap();
        HeapFree(heap, 0, ptr as *mut std::ffi::c_void);
    }
}

#[cfg(windows)]
#[global_allocator]
static ALLOCATOR: WinHeapAllocator = WinHeapAllocator;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    if arg == "gui" {
        let port = std::env::args()
            .nth(2)
            .and_then(|a| a.parse::<u16>().ok())
            .unwrap_or(8080);
        zpp::gui::serve(port);
        return;
    }

    println!();
    println!("  Z++ Ultimate Engine — Rust Edition (v1.1)");
    println!("  Select Run Mode:");
    println!("    [1] Demo Mode (built-in instance)");
    println!("    [2] Headless Mode (comma-separated elements + goal)");
    println!("    [3] Load from file (e.g. z_test_elements.txt)");
    println!("    [4] GUI Mode (web interface at http://127.0.0.1:8080)");
    println!();
    print!("  Enter choice (1, 2, 3, or 4): ");
    let _ = io::stdout().flush();

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();

    match line.trim() {
        "2" => run_headless(),
        "3" => run_file(),
        "4" => zpp::gui::serve(8080),
        _ => run_demo(),
    }
}

fn run_demo() {
    let nums = vec![
        1u64, 3, 7, 21, 50, 200, 400, 499, 1000, 1500, 2000, 5000, 10000, 25000,
    ];
    let target = 5570u64;
    let nums_big: Vec<BigUint> = nums.iter().map(|n| BigUint::from(*n)).collect();
    println!();
    println!("{}", "=".repeat(56));
    println!("   Z++ DEMO MODE");
    println!("{}", "=".repeat(56));
    println!("   Elements : {}", nums.len());
    println!("   Target   : {}", target);
    println!("{}", "=".repeat(56));
    println!();
    solve_and_report(nums_big, BigUint::from(target));
}

fn run_file() {
    println!();
    println!("{}", "=".repeat(56));
    println!("   Z++ FILE LOAD MODE");
    println!("{}", "=".repeat(56));
    println!();
    println!("  Enter path to .txt file");
    println!("  (comma-separated elements, then line: goal: NUMBER)");
    print!("  Path: ");
    let _ = io::stdout().flush();

    let stdin = io::stdin();
    let mut path_line = String::new();
    stdin.lock().read_line(&mut path_line).ok();
    let path = path_line.trim().trim_matches('"');

    let default = "z_test_elements.txt".to_string();
    let path = if path.is_empty() { default } else { path.to_string() };

    if !Path::new(&path).exists() {
        println!("  File not found: {}", path);
        return;
    }

    println!("  Reading {} ...", path);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            println!("  Read error: {}", e);
            return;
        }
    };

    let (nums, target) = match parse_file_content(&content) {
        Some(p) => p,
        None => {
            println!("  Could not parse file. Expected:");
            println!("    elem1, elem2, ...");
            println!("    goal: 12345");
            return;
        }
    };

    println!("  Loaded {} elements", nums.len());
    let td = target.to_str_radix(10).len();
    if td <= 40 {
        println!("  Target: {}", target);
    } else {
        println!("  Target: {}-digit number", td);
    }
    println!();
    solve_and_report(nums, target);
}

fn parse_file_content(content: &str) -> Option<(Vec<BigUint>, BigUint)> {
    let goal_marker = "\ngoal:";
    let (elem_part, goal_part) = if let Some(idx) = content.find(goal_marker) {
        (&content[..idx], &content[idx + goal_marker.len()..])
    } else if let Some(idx) = content.rfind("goal:") {
        let before = &content[..idx];
        let after = &content[idx + 5..];
        (before, after)
    } else {
        return None;
    };

    let nums: Vec<BigUint> = elem_part
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| BigUint::parse_bytes(s.as_bytes(), 10))
        .collect();

    let goal_digits: String = goal_part.chars().filter(|c| c.is_ascii_digit()).collect();
    let target = BigUint::parse_bytes(goal_digits.as_bytes(), 10)?;

    if nums.is_empty() {
        return None;
    }
    Some((nums, target))
}

fn run_headless() {
    println!();
    println!("{}", "=".repeat(56));
    println!("   Z++ HEADLESS MODE");
    println!("{}", "=".repeat(56));
    println!();
    println!("  Enter elements (comma-separated):");
    print!("  ");
    let _ = io::stdout().flush();

    let stdin = io::stdin();
    let mut elem_line = String::new();
    stdin.lock().read_line(&mut elem_line).ok();
    let nums: Vec<BigUint> = elem_line
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .filter_map(|s| BigUint::parse_bytes(s.as_bytes(), 10))
        .collect();

    print!("\n  Enter target goal: ");
    let _ = io::stdout().flush();
    let mut tgt_line = String::new();
    stdin.lock().read_line(&mut tgt_line).ok();
    let target = BigUint::parse_bytes(tgt_line.trim().as_bytes(), 10)
        .unwrap_or_else(BigUint::zero);

    if nums.is_empty() {
        println!("  (no elements provided)");
        return;
    }

    println!();
    println!("{}", "=".repeat(56));
    println!("   RUNNING Z++ ENGINE...");
    println!("{}", "=".repeat(56));
    println!("   Elements : {}", nums.len());
    let td = target.to_str_radix(10).len();
    if td <= 40 {
        println!("   Target   : {}", target);
    } else {
        println!("   Target   : {}-digit number", td);
    }
    println!("{}", "=".repeat(56));
    println!();

    solve_and_report(nums, target);
}

fn solve_and_report(numbers: Vec<BigUint>, target: BigUint) {
    let cpu_start = ProcessTime::now();
    let wall_start = Instant::now();
    let outcome = zpp::solve(numbers.clone(), target.clone(), Duration::from_secs(600));
    let wall = wall_start.elapsed();
    let cpu = cpu_start.elapsed();

    let exact = match outcome.solution.as_ref() {
        Some(sol) => sol.iter().sum::<BigUint>() == target,
        None => false,
    };
    let td = target.to_str_radix(10).len();

    println!();
    println!("{}", "=".repeat(56));
    println!("   Z++ PERFORMANCE REPORT");
    println!("{}", "=".repeat(56));
    println!("   Match Found     : {}", exact);
    if outcome.proved_impossible {
        println!("   PROVED IMPOSSIBLE");
    }
    println!("   Engine Winner   : {}", outcome.winner);
    println!("   Input size      : {} elements", numbers.len());
    if let Some(sol) = outcome.solution.as_ref() {
        println!("   Solution Size   : {} elements", sol.len());
        if td <= 40 {
            let s_str: Vec<String> = sol.iter().map(|x| x.to_string()).collect();
            println!("   Solution        : [{}]", s_str.join(", "));
            let total: BigUint = sol.iter().sum();
            println!("   Sum             : {}", total);
        }
    }
    println!();
    println!("   --- WALL-CLOCK TIME ---");
    println!("      {}", zpp::timing::fmt_duration(wall));
    println!();
    println!("   --- CPU TIME (all threads) ---");
    println!("      {}", zpp::timing::fmt_duration(cpu));
    println!();
    let par = zpp::timing::parallelism_ratio(cpu, wall);
    println!("   Parallelism ratio : {:.3}x", par);
    println!("{}", "=".repeat(56));
    println!();
}

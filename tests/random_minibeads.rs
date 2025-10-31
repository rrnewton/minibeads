//! Cargo test wrapper for random property-based testing
//!
//! This test invokes the test_minibeads binary with the random-actions subcommand.
//! For manual testing with custom parameters, use the binary directly:
//!
//!   cargo build --release --bin test_minibeads
//!   ./target/release/test_minibeads random-actions --seed 42 --verbose
//!   ./target/release/test_minibeads random-actions --seed 42 --impl upstream
//!   ./target/release/test_minibeads random-actions --seed-from-entropy --iters 10

use std::path::PathBuf;
use std::process::Command;

/// Build minibeads binaries in release mode
fn build_minibeads() -> PathBuf {
    // Build the bd binary
    let bd_build = Command::new("cargo")
        .args(["build", "--release", "--bin", "bd"])
        .status()
        .expect("Failed to build bd binary");
    assert!(bd_build.success(), "Failed to build bd binary");

    // Build the test_minibeads binary
    let test_build = Command::new("cargo")
        .args(["build", "--release", "--bin", "test_minibeads"])
        .status()
        .expect("Failed to build test_minibeads binary");
    assert!(
        test_build.success(),
        "Failed to build test_minibeads binary"
    );

    // Return path to test_minibeads binary
    std::env::current_dir()
        .expect("Failed to get current directory")
        .join("target/release/test_minibeads")
}

/// Build upstream bd binary
fn build_upstream() -> bool {
    let upstream_build = Command::new("make")
        .arg("upstream")
        .status()
        .expect("Failed to build upstream bd binary");

    if !upstream_build.success() {
        println!("⚠️  Skipping upstream test: make upstream failed");
        println!("   This is expected if beads/ submodule is not initialized");
        return false;
    }

    true
}

/// Run test_minibeads with specified arguments
fn run_test(binary_path: &PathBuf, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} random-actions {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("random-actions")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "{} failed with exit code: {:?}",
        test_name,
        output.status.code()
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_minibeads() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &["--seed", "42", "--impl", "minibeads"],
        "Random test against minibeads",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_stress_minibeads_parallel() {
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "minibeads",
            "--seconds",
            "3",
            "--parallel=3",
        ],
        "Parallel stress test against minibeads",
    );
}

#[test]
#[ignore] // TODO: Fix upstream prefix handling - upstream uses "bd-" prefix instead of respecting --prefix
fn test_random_actions_upstream() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &["--seed", "42", "--impl", "upstream"],
        "Random test against upstream bd",
    );
}

#[test]
#[ignore] // Requires upstream bd to be built
fn test_stress_upstream_parallel() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        &binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "upstream",
            "--seconds",
            "3",
            "--parallel=3",
            "--test-import",
            "true",
        ],
        "Parallel stress test against upstream bd with import",
    );
}

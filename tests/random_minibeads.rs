//! Cargo test wrapper for random property-based testing
//!
//! This test invokes the test_minibeads binary with the random-actions subcommand.
//! For manual testing with custom parameters, use the binary directly:
//!
//!   cargo build --bin test_minibeads
//!   ./target/debug/test_minibeads random-actions --seed 42 --verbose
//!   ./target/debug/test_minibeads random-actions --seed 42 --impl upstream
//!   ./target/debug/test_minibeads random-actions --seed-from-entropy --iters 10

use std::process::Command;

#[test]
fn test_random_actions_minibeads() {
    // Build the bd binary first
    let bd_build = Command::new("cargo")
        .args(["build", "--bin", "bd"])
        .status()
        .expect("Failed to build bd binary");

    assert!(bd_build.success(), "Failed to build bd binary");

    // Build the test_minibeads binary
    let test_build = Command::new("cargo")
        .args(["build", "--bin", "test_minibeads"])
        .status()
        .expect("Failed to build test_minibeads binary");

    assert!(
        test_build.success(),
        "Failed to build test_minibeads binary"
    );

    // Get absolute path to test_minibeads binary
    let binary_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("target/debug/test_minibeads");

    println!(
        "\nRunning: {} random-actions --seed 42 --impl minibeads",
        binary_path.display()
    );

    // Run the random test against minibeads
    let output = Command::new(&binary_path)
        .args(["random-actions", "--seed", "42", "--impl", "minibeads"])
        .output()
        .expect("Failed to execute test_minibeads");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "Random test against minibeads failed with exit code: {:?}",
        output.status.code()
    );
}

#[test]
#[ignore] // TODO: Fix upstream prefix handling - upstream uses "bd-" prefix instead of respecting --prefix
fn test_random_actions_upstream() {
    // Build upstream bd binary first
    let upstream_build = Command::new("make")
        .arg("upstream")
        .status()
        .expect("Failed to build upstream bd binary");

    if !upstream_build.success() {
        println!("⚠️  Skipping upstream test: make upstream failed");
        println!("   This is expected if beads/ submodule is not initialized");
        return;
    }

    // Build the test_minibeads binary
    let test_build = Command::new("cargo")
        .args(["build", "--bin", "test_minibeads"])
        .status()
        .expect("Failed to build test_minibeads binary");

    assert!(
        test_build.success(),
        "Failed to build test_minibeads binary"
    );

    // Get absolute path to test_minibeads binary
    let binary_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join("target/debug/test_minibeads");

    println!(
        "\nRunning: {} random-actions --seed 42 --impl upstream",
        binary_path.display()
    );

    // Run the random test against upstream bd
    let output = Command::new(&binary_path)
        .args(["random-actions", "--seed", "42", "--impl", "upstream"])
        .output()
        .expect("Failed to execute test_minibeads");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "Random test against upstream bd failed with exit code: {:?}",
        output.status.code()
    );
}

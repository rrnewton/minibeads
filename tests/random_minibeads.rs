//! Cargo test wrapper for random property-based testing
//!
//! This test invokes the test_minibeads binary with the random-mb subcommand.
//! For manual testing with custom parameters, use the binary directly:
//!
//!   cargo build --bin test_minibeads
//!   ./target/debug/test_minibeads random-mb --seed 42 --verbose
//!   ./target/debug/test_minibeads random-mb --seed-from-entropy --iters 10

use std::process::Command;

#[test]
fn test_random_minibeads() {
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

    println!("\nRunning: {} random-mb --seed 42", binary_path.display());

    // Run the random test with default seed
    let output = Command::new(&binary_path)
        .args(["random-mb", "--seed", "42"])
        .output()
        .expect("Failed to execute test_minibeads");

    // Print output for debugging
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }

    assert!(
        output.status.success(),
        "Random test failed with exit code: {:?}",
        output.status.code()
    );
}

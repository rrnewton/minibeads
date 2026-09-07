//! Cargo test wrapper for random property-based testing
//!
//! This test invokes the test_minibeads binary with the random-actions subcommand.
//! For manual testing with custom parameters, use the binary directly:
//!
//!   cargo build --locked --bin mb --bin test_minibeads --features test-tools
//!   ./target/debug/test_minibeads random-actions --seed 42 --verbose
//!   ./target/debug/test_minibeads random-actions --seed 42 --impl upstream
//!   ./target/debug/test_minibeads random-actions --seed-from-entropy --iters 10

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(serde::Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum CargoMessage {
    CompilerArtifact {
        target: CargoTarget,
        executable: Option<PathBuf>,
    },
    #[serde(other)]
    Other,
}

#[derive(serde::Deserialize)]
struct CargoTarget {
    name: CargoBinary,
}

#[derive(serde::Deserialize)]
enum CargoBinary {
    #[serde(rename = "test_minibeads")]
    Harness,
    #[serde(other)]
    Other,
}

fn harness_build_command() -> Command {
    let mut command = Command::new(env!("CARGO"));
    command.current_dir(env!("CARGO_MANIFEST_DIR")).args([
        "build",
        "--locked",
        "--profile",
        "dev",
        "--bin",
        "mb",
        "--bin",
        "test_minibeads",
        "--features",
        "test-tools",
        "--message-format=json",
    ]);
    command
}

fn harness_executable(messages: &[u8]) -> anyhow::Result<PathBuf> {
    for line in messages
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        if let CargoMessage::CompilerArtifact {
            target: CargoTarget {
                name: CargoBinary::Harness,
            },
            executable: Some(path),
        } = serde_json::from_slice(line)?
        {
            return Ok(path);
        }
    }
    anyhow::bail!("Cargo did not report the test_minibeads executable");
}

fn initialize_harness(
    build: &OnceLock<anyhow::Result<PathBuf>>,
    builder: impl FnOnce() -> anyhow::Result<PathBuf>,
) -> Result<&Path, &anyhow::Error> {
    build.get_or_init(builder).as_deref()
}

fn build_minibeads() -> &'static Path {
    static BUILD: OnceLock<anyhow::Result<PathBuf>> = OnceLock::new();
    initialize_harness(&BUILD, || {
        let output = harness_build_command().output()?;
        anyhow::ensure!(
            output.status.success(),
            "Failed to build minibeads binaries: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        harness_executable(&output.stdout)
    })
    .expect("Failed to initialize minibeads test harness")
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

/// Run test_minibeads random-actions with specified arguments
fn run_test(binary_path: &Path, args: &[&str], test_name: &str) {
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

/// Run test_minibeads sync-test with specified arguments
fn run_sync_test(binary_path: &Path, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} sync-test {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("sync-test")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads sync-test");

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
fn test_random_actions_minibeads_numeric() {
    let binary_path = build_minibeads();
    run_test(
        binary_path,
        &["--seed", "42", "--impl", "minibeads", "--ids", "numeric"],
        "Random test against minibeads with numeric IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_minibeads_hash() {
    let binary_path = build_minibeads();
    run_test(
        binary_path,
        &["--seed", "42", "--impl", "minibeads", "--ids", "hash"],
        "Random test against minibeads with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_stress_minibeads_parallel_numeric() {
    let binary_path = build_minibeads();
    run_test(
        binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "minibeads",
            "--seconds",
            "3",
            "--parallel=3",
            "--ids",
            "numeric",
        ],
        "Parallel stress test against minibeads with numeric IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_stress_minibeads_parallel_hash() {
    let binary_path = build_minibeads();
    run_test(
        binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "minibeads",
            "--seconds",
            "3",
            "--parallel=3",
            "--ids",
            "hash",
        ],
        "Parallel stress test against minibeads with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_random_actions_upstream() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        binary_path,
        &[
            "--seed",
            "42",
            "--impl",
            "upstream",
            "--ids",
            "hash",
            "--test-import=false",
        ],
        "Random test against upstream bd with hash IDs",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
#[ignore] // Requires upstream bd to be built
fn test_stress_upstream_parallel() {
    if !build_upstream() {
        return;
    }
    let binary_path = build_minibeads();
    run_test(
        binary_path,
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
            "--ids",
            "hash",
        ],
        "Parallel stress test against upstream bd with hash IDs and import",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_sync_stress() {
    // Check if upstream is available
    if !build_upstream() {
        return;
    }

    let binary_path = build_minibeads();
    run_sync_test(
        binary_path,
        &[
            "--seed",
            "12345",
            "--cycles",
            "10",
            "--actions-per-phase",
            "15",
        ],
        "Bidirectional sync stress test (10 cycles, 15 actions/phase, ~3s)",
    );
}

#[test]
#[cfg(not(tarpaulin))] // Skip under coverage - this test invokes external binaries
fn test_migration_stress() {
    let binary_path = build_minibeads();
    run_migration_test(
        binary_path,
        &["--seed", "54321", "--actions", "50"],
        "Hash ID migration stress test (50 actions, then migrate)",
    );
}

/// Run migration test: generate numeric state, migrate to hash, verify
fn run_migration_test(binary_path: &Path, args: &[&str], test_name: &str) {
    println!(
        "\nRunning: {} migration-test {}",
        binary_path.display(),
        args.join(" ")
    );

    let output = Command::new(binary_path)
        .arg("migration-test")
        .args(args)
        .output()
        .expect("Failed to execute test_minibeads migration-test");

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

mod harness_initialization {
    use super::*;
    use std::ffi::OsStr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    #[test]
    fn concurrent_callers_build_once() {
        let build = OnceLock::new();
        let calls = AtomicUsize::new(0);
        let start = Barrier::new(16);

        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    start.wait();
                    let binary = initialize_harness(&build, || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(PathBuf::from("custom-target/debug/test_minibeads"))
                    })
                    .unwrap();
                    assert_eq!(binary, Path::new("custom-target/debug/test_minibeads"));
                });
            }
        });

        initialize_harness(&build, || panic!("Completed build must be reused")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_build_is_not_retried() {
        let build = OnceLock::new();
        assert!(initialize_harness(&build, || anyhow::bail!("build failed")).is_err());
        let error =
            initialize_harness(&build, || panic!("Failed build must be cached")).unwrap_err();
        assert_eq!(error.to_string(), "build failed");
    }

    #[test]
    fn both_binaries_share_one_debug_build() {
        let command = harness_build_command();
        assert_eq!(
            command.get_current_dir(),
            Some(Path::new(env!("CARGO_MANIFEST_DIR")))
        );
        assert!(command.get_args().eq([
            "build",
            "--locked",
            "--profile",
            "dev",
            "--bin",
            "mb",
            "--bin",
            "test_minibeads",
            "--features",
            "test-tools",
            "--message-format=json",
        ]
        .map(OsStr::new)));
    }

    #[test]
    fn cargo_artifact_preserves_target_directory_and_executable_suffix() {
        for executable in [
            "custom target/debug/test_minibeads",
            "custom target/debug/test_minibeads.exe",
        ] {
            let messages = format!(
                "{{\"reason\":\"compiler-message\"}}\n\
                 {{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"mb\"}},\"executable\":\"custom target/debug/mb\"}}\n\
                 {{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"test_minibeads\"}},\"executable\":\"{executable}\"}}\n"
            );
            assert_eq!(
                harness_executable(messages.as_bytes()).unwrap(),
                Path::new(executable)
            );
        }
    }

    #[test]
    fn missing_harness_artifact_is_rejected() {
        assert!(harness_executable(b"{\"reason\":\"build-finished\",\"success\":true}\n").is_err());
    }
}

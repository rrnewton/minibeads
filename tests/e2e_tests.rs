use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Discovers and runs all .sh test scripts in the tests/ directory
fn discover_shell_tests() -> Vec<PathBuf> {
    let tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut shell_tests = Vec::new();

    if let Ok(entries) = fs::read_dir(&tests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("sh") {
                shell_tests.push(path);
            }
        }
    }

    shell_tests.sort();
    shell_tests
}

/// Runs a single shell test script
fn run_shell_test(test_path: &PathBuf) -> Result<(), String> {
    let output = Command::new("bash")
        .arg(test_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to execute test: {}", e))?;

    if output.status.success() {
        // Print stdout for successful tests (shows summary)
        if !output.stdout.is_empty() {
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        Ok(())
    } else {
        // Print both stdout and stderr for failed tests
        let mut error_msg = String::new();
        if !output.stdout.is_empty() {
            error_msg.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            error_msg.push_str("\nSTDERR:\n");
            error_msg.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        Err(error_msg)
    }
}

#[test]
fn run_all_shell_tests() {
    let shell_tests = discover_shell_tests();

    if shell_tests.is_empty() {
        println!("No shell tests found in tests/ directory");
        return;
    }

    println!("\nDiscovered {} shell test(s):", shell_tests.len());
    for test_path in &shell_tests {
        println!("  - {}", test_path.display());
    }
    println!();

    let mut failed_tests = Vec::new();

    for test_path in &shell_tests {
        let test_name = test_path.file_name().unwrap().to_string_lossy();
        println!("Running: {}", test_name);

        match run_shell_test(test_path) {
            Ok(()) => {
                println!("✓ {} passed\n", test_name);
            }
            Err(e) => {
                eprintln!("✗ {} failed:", test_name);
                eprintln!("{}\n", e);
                failed_tests.push(test_name.to_string());
            }
        }
    }

    if !failed_tests.is_empty() {
        panic!(
            "\n{} shell test(s) failed:\n  - {}\n",
            failed_tests.len(),
            failed_tests.join("\n  - ")
        );
    }

    println!("All {} shell test(s) passed!", shell_tests.len());
}

#[test]
fn test_basic_operations_exists() {
    let test_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("basic_operations.sh");
    assert!(test_path.exists(), "basic_operations.sh test should exist");

    #[cfg(unix)]
    {
        assert!(
            test_path.metadata().unwrap().permissions().mode() & 0o111 != 0,
            "basic_operations.sh should be executable"
        );
    }
}

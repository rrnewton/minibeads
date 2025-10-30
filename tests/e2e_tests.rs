use dir_test::Fixture;
use std::process::Command;

/// Test harness for running shell scripts
/// This function is called by dir_test for each discovered .sh file
#[cfg(unix)]
fn run_shell_script<P: AsRef<std::path::Path>>(path: P) {
    let test_path = path.as_ref();

    let output = Command::new("bash")
        .arg(test_path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to execute test {}: {}", test_path.display(), e));

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "Test {} failed:\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
            test_path.display(),
            stdout,
            stderr
        );
    }

    // Print stdout for successful tests (shows summary)
    if !output.stdout.is_empty() {
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

// Discover and create individual test functions for each .sh file in tests/
#[cfg(unix)]
#[dir_test::dir_test(
    dir: "$CARGO_MANIFEST_DIR/tests",
    glob: "*.sh",
)]
fn shell_tests(fixture: Fixture<&str>) {
    run_shell_script(fixture.path());
}

// On non-Unix platforms, just have a placeholder test
#[cfg(not(unix))]
#[test]
fn shell_tests_not_supported_on_this_platform() {
    // Shell tests are only supported on Unix platforms
}

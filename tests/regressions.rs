use std::process::Command;

#[test]
fn cli_help_and_version_start_successfully() {
    for argument in ["--help", "--version", "github"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mb"));
        command.arg(argument);
        if argument == "github" {
            command.arg("--help");
        }
        let output = command.output().expect("run the actual CLI binary");
        assert!(
            output.status.success(),
            "{argument}: {:?}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.stdout.is_empty(), "{argument} produced no output");
    }
}

fn main() {
    // Set build date
    let now = chrono::Utc::now();
    println!(
        "cargo:rustc-env=BUILD_DATE={}",
        now.format("%Y-%m-%d %H:%M:%S UTC")
    );

    // Generate build-time information (git hash, etc.)
    built::write_built_file().expect("Failed to acquire build-time information");
}

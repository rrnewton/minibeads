fn main() {
    // Clap derives a large command tree. The default Windows stack is too
    // small for unoptimized CLI construction, including --help and --version.
    let target = std::env::var("TARGET").expect("Cargo must set TARGET");
    if target.ends_with("windows-msvc") {
        println!("cargo:rustc-link-arg-bin=mb=/STACK:16777216");
    } else if target.ends_with("windows-gnu") || target.ends_with("windows-gnullvm") {
        println!("cargo:rustc-link-arg-bin=mb=-Wl,--stack,16777216");
    }

    // Set build date
    let now = chrono::Utc::now();
    println!(
        "cargo:rustc-env=BUILD_DATE={}",
        now.format("%Y-%m-%d %H:%M:%S UTC")
    );

    // Generate build-time information (git hash, etc.)
    built::write_built_file().expect("Failed to acquire build-time information");
}

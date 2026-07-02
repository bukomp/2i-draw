use std::process::Command;
fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/config");
    let commit = Command::new("git").args(["rev-parse", "HEAD"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=IDRAW_COMMIT={commit}");

    // A cargo-cache checkout's `origin` points at cargo's internal git db —
    // useless for updating. Fall back to Cargo.toml's `repository` field then.
    let origin = Command::new("git").args(["remote", "get-url", "origin"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty() && !s.contains("/git/db/") && !s.contains("\\git\\db\\"))
        .or_else(|| std::env::var("CARGO_PKG_REPOSITORY").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=IDRAW_ORIGIN={origin}");
}

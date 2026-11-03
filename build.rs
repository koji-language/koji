//! Builds the runtime of compiled programs (`runtime/`) as a static library, which
//! `kojic build` links into every program. Its path is given to `kojic` as `KOJI_RUNTIME_LIB`.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=runtime/src");
    println!("cargo:rerun-if-changed=runtime/Cargo.toml");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    // A separate target directory: the outer build holds the lock of the main one.
    let target_dir = out_dir.join("runtime");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args(["rustc", "--release", "--lib", "--crate-type", "staticlib", "--manifest-path"])
        .arg(PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("runtime/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("cannot run cargo to build the runtime");
    assert!(status.success(), "building the runtime failed");

    let library = target_dir.join("release").join("libkoji_runtime.a");
    println!("cargo:rustc-env=KOJI_RUNTIME_LIB={}", library.display());
}

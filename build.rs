//! Cross-compilation helper.
//!
//! Rust's `x86_64-pc-windows-gnu` target unconditionally links `-l:libpthread.a`.
//! Some MinGW-w64 toolchains (for example the `mcf` threading model shipped by
//! Nix) do not provide that archive because threading support lives elsewhere.
//! When the archive is genuinely missing we drop an empty stub in `OUT_DIR` and
//! add it to the link search path; when the toolchain provides a real
//! `libpthread.a` nothing is changed.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.ends_with("pc-windows-gnu") {
        return;
    }
    println!("cargo::rerun-if-env-changed=CC_x86_64_pc_windows_gnu");
    println!("cargo::rerun-if-env-changed=CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER");

    if toolchain_has_libpthread(&target) {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let stub_dir = out_dir.join("mingw-stub");
    if let Err(err) = std::fs::create_dir_all(&stub_dir) {
        println!("cargo::warning=could not create libpthread stub directory: {err}");
        return;
    }
    if let Err(err) = std::fs::write(stub_dir.join("libpthread.a"), b"!<arch>\n") {
        println!("cargo::warning=could not write libpthread stub: {err}");
        return;
    }

    println!("cargo::rustc-link-search=native={}", stub_dir.display());
}

fn toolchain_has_libpthread(target: &str) -> bool {
    let candidates = compiler_candidates(target);
    for compiler in candidates {
        let Ok(output) = Command::new(&compiler)
            .arg("-print-file-name=libpthread.a")
            .output()
        else {
            continue;
        };
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // GCC echoes back the bare file name when it cannot find the library.
        if path != "libpthread.a" && PathBuf::from(&path).exists() {
            return true;
        }
    }
    // No usable compiler found; assume the stub is required.
    false
}

fn compiler_candidates(target: &str) -> Vec<String> {
    let env_key = format!("CC_{}", target.replace('-', "_"));
    let linker_key = format!(
        "CARGO_TARGET_{}_LINKER",
        target.replace('-', "_").to_ascii_uppercase()
    );
    let mut candidates = Vec::new();
    if let Ok(value) = std::env::var(&linker_key) {
        candidates.push(value);
    }
    if let Ok(value) = std::env::var(&env_key) {
        candidates.push(value);
    }
    candidates.push("x86_64-w64-mingw32-gcc".to_string());
    candidates
}

//! Builds CoBRA and its dependencies with CMake, compiles the C++ side of the
//! cxx bridge, and links everything statically into the cobra-mcp binary.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

    let cobra_src = manifest_dir.join("vendor/CoBRA");
    if !cobra_src.join("CMakeLists.txt").is_file() {
        panic!("vendor/CoBRA is missing; run `git submodule update --init`");
    }

    // CoBRA's superbuild fetches Abseil and Highway and installs them into
    // <build dir>/install. It has no install target of its own.
    let deps_prefix = cmake::Config::new(cobra_src.join("dependencies"))
        .profile("Release")
        .out_dir(out_dir.join("deps"))
        .build_target("all")
        .build()
        .join("build/install");

    let mut cobra = cmake::Config::new(&cobra_src);
    cobra
        .profile("Release")
        .out_dir(out_dir.join("cobra"))
        .define("CMAKE_PREFIX_PATH", &deps_prefix)
        .define("CMAKE_DISABLE_FIND_PACKAGE_Z3", "ON");
    if target_env == "msvc" {
        // CoBRA passes /wd4146 before /sdl, which turns C4146 back into an
        // error. cl appends _CL_ after the command line, so this one sticks.
        cobra.env("_CL_", "/wd4146");
    }
    let cobra_prefix = cobra.build();

    // The expression parser lives in CoBRA's CLI rather than its library.
    cxx_build::bridge("src/cobra.rs")
        .file("cpp/bridge.cc")
        .file(cobra_src.join("tools/cobra-cli/ExprParser.cpp"))
        .include(cobra_prefix.join("include"))
        .include(cobra_src.join("tools/cobra-cli"))
        .std(if target_env == "msvc" {
            "c++latest"
        } else {
            "c++23"
        })
        // Linked below, after the CoBRA archives that also depend on it.
        .cpp_link_stdlib(None)
        .compile("cobra-mcp-bridge");

    let mut archives = static_libs(&cobra_prefix.join("lib"));
    for lib_dir in ["lib", "lib64"] {
        archives.extend(static_libs(&deps_prefix.join(lib_dir)));
    }
    if target_os == "linux" {
        // GNU ld resolves archives in command-line order, and Abseil's many
        // archives depend on each other. Within a single archive it rescans
        // until nothing new resolves, so merging sidesteps ordering entirely.
        archives = vec![merge_archives(
            &archives,
            &out_dir.join("libcobra-native.a"),
        )];
    }
    for archive in &archives {
        println!(
            "cargo:rustc-link-search=native={}",
            archive.parent().unwrap().display()
        );
        println!("cargo:rustc-link-lib=static={}", link_name(archive));
    }

    println!("cargo:rerun-if-env-changed=CXXSTDLIB");
    let stdlib = env::var("CXXSTDLIB")
        .ok()
        .or_else(|| match target_os.as_str() {
            "macos" | "ios" | "freebsd" | "openbsd" => Some("c++".into()),
            "windows" => None,
            _ => Some("stdc++".into()),
        });
    if let Some(stdlib) = stdlib.filter(|s| !s.is_empty()) {
        println!("cargo:rustc-link-lib={stdlib}");
    }

    if target_os == "macos" {
        // rustc links with -nodefaultlibs, which drops the compiler-rt
        // builtins clang would add; x86_64 needs its __cpu_model for
        // CoBRA's __builtin_cpu_supports.
        let output = cc::Build::new()
            .get_compiler()
            .to_command()
            .arg("-print-libgcc-file-name")
            .output()
            .expect("failed to run the C compiler");
        assert!(output.status.success(), "locating compiler-rt failed");
        let builtins = String::from_utf8(output.stdout).unwrap();
        println!("cargo:rustc-link-arg={}", builtins.trim());
    }

    println!("cargo:rerun-if-changed=cpp");
    println!("cargo:rerun-if-changed=vendor/CoBRA");
}

fn static_libs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut libs: Vec<PathBuf> = entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| matches!(path.extension().and_then(OsStr::to_str), Some("a" | "lib")))
        .collect();
    libs.sort();
    libs
}

fn link_name(archive: &Path) -> String {
    let stem = archive.file_stem().unwrap().to_str().unwrap();
    if archive.extension() == Some(OsStr::new("a")) {
        stem.strip_prefix("lib").unwrap_or(stem).to_owned()
    } else {
        stem.to_owned()
    }
}

fn merge_archives(archives: &[PathBuf], merged: &Path) -> PathBuf {
    let _ = fs::remove_file(merged);
    let mut script = format!("CREATE {}\n", merged.display());
    for archive in archives {
        script += &format!("ADDLIB {}\n", archive.display());
    }
    script += "SAVE\nEND\n";

    let mut ar = cc::Build::new()
        .get_archiver()
        .arg("-M")
        .stdin(Stdio::piped())
        .spawn()
        .expect("failed to run the archiver");
    ar.stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let status = ar.wait().unwrap();
    assert!(status.success(), "merging static archives failed: {status}");
    merged.to_owned()
}

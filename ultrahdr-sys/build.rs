use std::env;
use std::path::{Path, PathBuf};

fn locate_src_dir(manifest_dir: &Path) -> PathBuf {
    if let Ok(env) = env::var("ULTRAHDR_SRC_DIR") {
        return PathBuf::from(env);
    }

    let workspace_root = manifest_dir
        .parent()
        .expect("ultrahdr-sys has no parent dir");

    let submodule_path = workspace_root.join("libultrahdr");
    if submodule_path.join("CMakeLists.txt").is_file() {
        return submodule_path;
    }

    // Fallback: sibling checkout (old layout).
    let sibling = workspace_root
        .parent()
        .expect("workspace has no parent dir")
        .join("libultrahdr");
    sibling
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src_dir = locate_src_dir(&manifest_dir);
    if !src_dir.join("CMakeLists.txt").is_file() {
        panic!(
            "Could not find libultrahdr sources; set ULTRAHDR_SRC_DIR (current: {})",
            src_dir.display()
        );
    }

    let mut cfg = cmake::Config::new(&src_dir);
    cfg.profile("Release")
        .define("UHDR_BUILD_EXAMPLES", "OFF")
        .define("UHDR_BUILD_TESTS", "OFF")
        .define("UHDR_BUILD_BENCHMARK", "OFF")
        .define("UHDR_BUILD_FUZZERS", "OFF")
        .define("UHDR_BUILD_JAVA", "OFF")
        .define("UHDR_ENABLE_INSTALL", "OFF")
        .define(
            "UHDR_BUILD_DEPS",
            if cfg!(feature = "vendored") {
                "ON"
            } else {
                "OFF"
            },
        )
        .define(
            "BUILD_SHARED_LIBS",
            if cfg!(feature = "shared") {
                "ON"
            } else {
                "OFF"
            },
        );

    if cfg!(feature = "gles") {
        cfg.define("UHDR_ENABLE_GLES", "ON");
    }
    // Control ISO 21496-1 metadata emission via feature flag (default ON).
    cfg.define(
        "UHDR_WRITE_ISO",
        if cfg!(feature = "iso21496") {
            "ON"
        } else {
            "OFF"
        },
    );

    // Build only the main library target; install target is disabled upstream.
    let cmake_target = if env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "msvc"
        && !cfg!(feature = "shared")
    {
        "uhdr-static"
    } else {
        "uhdr"
    };
    cfg.build_target(cmake_target);

    let dst = cfg.build();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    // Link search paths (CMake binary dir holds libs when install is disabled).
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", dst.display());
    println!("cargo:rustc-link-search=native={}/build", dst.display());
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if cfg!(feature = "vendored") {
        println!(
            "cargo:rustc-link-search=native={}/build/turbojpeg/src/turbojpeg-build",
            dst.display()
        );
        if target_env == "msvc" {
            println!(
                "cargo:rustc-link-search=native={}/build/turbojpeg/src/turbojpeg-build/Release",
                dst.display()
            );
            println!(
                "cargo:rustc-link-search=native={}/build/turbojpeg/src/turbojpeg-build/Debug",
                dst.display()
            );
        }
        let jpeg_name = if target_env == "msvc" {
            "jpeg-static"
        } else {
            "jpeg"
        };
        println!("cargo:rustc-link-lib=static={}", jpeg_name);
    } else {
        println!("cargo:rustc-link-lib=jpeg");
    }

    let shared = cfg!(feature = "shared");
    let link_name = if target_env == "msvc" && !shared {
        "uhdr-static"
    } else {
        "uhdr"
    };
    let link_kind = if shared { "dylib" } else { "static" };
    println!("cargo:rustc-link-lib={}={}", link_kind, link_name);
    if target_env != "msvc" {
        let cxx_stdlib = if target_os == "macos" {
            "c++"
        } else {
            "stdc++"
        };
        println!("cargo:rustc-link-lib={}", cxx_stdlib);
    }

    // Re-run if the public header changes.
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("ultrahdr_api.h").display()
    );

    let bindings = bindgen::Builder::default()
        .header(src_dir.join("ultrahdr_api.h").to_string_lossy())
        .clang_arg(format!("-I{}", src_dir.display()))
        .allowlist_function("uhdr_.*")
        .allowlist_type("uhdr_.*")
        .allowlist_var("UHDR_.*")
        .rustified_enum("uhdr_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .generate()
        .expect("bindgen failed");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings");
}

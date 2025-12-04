use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn wasi_toolchain() -> Option<(PathBuf, PathBuf)> {
    let target = env::var("TARGET").ok()?;
    if !target.contains("wasm32-wasi") {
        return None;
    }

    let prefix = env::var("WASI_SDK_PREFIX")
        .or_else(|_| env::var("WASI_SDK_PATH"))
        .unwrap_or_else(|_| "/opt/wasi-sdk".to_string());
    let env_toolchain = env::var_os("WASI_SDK_TOOLCHAIN_FILE").map(PathBuf::from);
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let toolchain = env_toolchain.unwrap_or_else(|| {
        let name = if target.contains("wasip1") || target_env == "p1" {
            "wasi-sdk-p1.cmake"
        } else {
            "wasi-sdk.cmake"
        };
        PathBuf::from(&prefix).join("share/cmake").join(name)
    });
    Some((toolchain, PathBuf::from(prefix)))
}

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

fn apply_patch_once(src_dir: &Path, patch_path: &Path) {
    if !patch_path.is_file() {
        return;
    }

    let patch_str = patch_path
        .to_str()
        .expect("patch path contains non-UTF8 characters");

    // If patch applies in reverse, assume already applied.
    let reverse_ok = Command::new("git")
        .current_dir(src_dir)
        .args(["apply", "--reverse", "--check", patch_str])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if reverse_ok {
        return;
    }

    let status = Command::new("git")
        .current_dir(src_dir)
        .args(["apply", "--whitespace=nowarn", patch_str])
        .status()
        .expect("failed to execute git apply");
    if !status.success() {
        panic!(
            "failed to apply patch {} in {}",
            patch_path.display(),
            src_dir.display()
        );
    }
}

fn apply_local_patches(manifest_dir: &Path, src_dir: &Path) {
    if env::var("ULTRAHDR_SKIP_PATCHES").is_ok() {
        return;
    }
    let workspace_root = manifest_dir
        .parent()
        .expect("ultrahdr-sys has no parent dir");
    apply_patch_once(
        src_dir,
        &workspace_root.join("patches/libultrahdr-no-threads.patch"),
    );
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

    let patch_path = manifest_dir
        .parent()
        .expect("ultrahdr-sys has no parent dir")
        .join("patches/libultrahdr-no-threads.patch");
    apply_local_patches(&manifest_dir, &src_dir);
    println!("cargo:rerun-if-env-changed=ULTRAHDR_SRC_DIR");
    println!("cargo:rerun-if-env-changed=ULTRAHDR_SKIP_PATCHES");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PREFIX");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=WASI_SDK_TOOLCHAIN_FILE");

    let target = env::var("TARGET").expect("TARGET");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let is_wasm = target_family == "wasm";
    let wasi = wasi_toolchain();

    let mut cfg = cmake::Config::new(&src_dir);
    cfg.profile("Release");
    if let Some((toolchain, prefix)) = &wasi {
        if !toolchain.is_file() {
            panic!(
                "WASI CMake toolchain file not found: {}",
                toolchain.display()
            );
        }
        cfg.define(
            "CMAKE_TOOLCHAIN_FILE",
            toolchain
                .to_str()
                .expect("toolchain path contains non-UTF8 characters"),
        );
        cfg.define(
            "WASI_SDK_PREFIX",
            prefix
                .to_str()
                .expect("WASI_SDK_PREFIX contains non-UTF8 characters"),
        );
        cfg.define("CMAKE_TRY_COMPILE_TARGET_TYPE", "STATIC_LIBRARY");
        cfg.define("CMAKE_SYSTEM_NAME", "WASI");
        cfg.define("CMAKE_SYSTEM_PROCESSOR", "wasm32");
    }
    if is_wasm && cfg!(feature = "shared") {
        panic!("shared linking is not supported for wasm32 targets");
    }
    if is_wasm && cfg!(feature = "gles") {
        panic!("gles feature is not supported for wasm32 targets");
    }

    let build_shared = cfg!(feature = "shared") && !is_wasm;
    let disable_threads = cfg!(feature = "no-threads") || is_wasm;

    cfg.define("UHDR_BUILD_EXAMPLES", "OFF");
    cfg.define("UHDR_BUILD_TESTS", "OFF");
    cfg.define("UHDR_BUILD_BENCHMARK", "OFF");
    cfg.define("UHDR_BUILD_FUZZERS", "OFF");
    cfg.define("UHDR_BUILD_JAVA", "OFF");
    cfg.define("UHDR_ENABLE_INSTALL", "OFF");
    cfg.define(
        "UHDR_BUILD_DEPS",
        if cfg!(feature = "vendored") {
            "ON"
        } else {
            "OFF"
        },
    );
    cfg.define("BUILD_SHARED_LIBS", if build_shared { "ON" } else { "OFF" });

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
    if disable_threads {
        cfg.define("UHDR_DISABLE_THREADS", "ON");
    }

    // Build only the main library target; install target is disabled upstream.
    let cmake_target = if target_env == "msvc" && !build_shared {
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
    if target_env == "msvc" {
        println!(
            "cargo:rustc-link-search=native={}/build/Release",
            dst.display()
        );
        println!(
            "cargo:rustc-link-search=native={}/build/Debug",
            dst.display()
        );
    }

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

    let link_name = if target_env == "msvc" && !build_shared {
        "uhdr-static"
    } else {
        "uhdr"
    };
    let link_kind = if build_shared { "dylib" } else { "static" };
    println!("cargo:rustc-link-lib={}={}", link_kind, link_name);
    if target_env != "msvc" {
        if is_wasm {
            if let Some((_, prefix)) = &wasi {
                println!(
                    "cargo:rustc-link-search=native={}/share/wasi-sysroot/lib/wasm32-wasip1",
                    prefix.display()
                );
            }
            println!("cargo:rustc-link-lib=static=c++");
            println!("cargo:rustc-link-lib=static=c++abi");
            println!("cargo:rustc-link-lib=static=setjmp");
        } else {
            let cxx_stdlib = if target_os == "macos" {
                "c++"
            } else {
                "stdc++"
            };
            println!("cargo:rustc-link-lib={}", cxx_stdlib);
        }
    }

    // Re-run if the public header changes.
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("ultrahdr_api.h").display()
    );
    println!("cargo:rerun-if-changed={}", patch_path.display());

    let bindgen_target = if is_wasm {
        "i686-unknown-linux-gnu"
    } else {
        target.as_str()
    };

    let mut bindings = bindgen::Builder::default()
        .header(src_dir.join("ultrahdr_api.h").to_string_lossy())
        .clang_arg(format!("-I{}", src_dir.display()))
        .rustified_enum("uhdr_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .clang_arg(format!("--target={}", bindgen_target));
    if !is_wasm {
        bindings = bindings
            .allowlist_function("uhdr_.*")
            .allowlist_type("uhdr_.*")
            .allowlist_var("UHDR_.*");
    }
    if !is_wasm {
        if let Some((_, prefix)) = &wasi {
            bindings =
                bindings.clang_arg(format!("--sysroot={}/share/wasi-sysroot", prefix.display()));
        }
    }
    let bindings = bindings.generate().expect("bindgen failed");

    let bindings_path = out_dir.join("bindings.rs");
    bindings
        .write_to_file(&bindings_path)
        .expect("failed to write bindings");
    if is_wasm {
        if let Ok(content) = fs::read_to_string(&bindings_path) {
            let fn_count = content.matches("pub fn ").count();
            if fn_count == 0 {
                println!("cargo:warning=bindgen generated 0 functions for wasm target");
            }
        }
    }
}

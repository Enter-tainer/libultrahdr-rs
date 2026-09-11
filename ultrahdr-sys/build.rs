use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let file_name = entry.file_name();
        if file_name.to_str() == Some(".git") {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(file_name);
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)?;
        } else if file_type.is_symlink() {
            let target = fs::read_link(&src_path)?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, &dst_path)?;
            }
            #[cfg(windows)]
            {
                if target.is_dir() {
                    std::os::windows::fs::symlink_dir(&target, &dst_path)?;
                } else {
                    std::os::windows::fs::symlink_file(&target, &dst_path)?;
                }
            }
        }
    }
    Ok(())
}

/// Link flags for the codec libraries the vendored libheif was built against.
///
/// libheif links its codecs (libaom, x265, dav1d, ...) PRIVATE, so `aom_*`/`x265_*` symbols stay
/// undefined in the `libheif.a` archive. Anything that pulls that archive in — the Rust
/// executable and a shared `libuhdr` alike — therefore has to add the codecs itself, otherwise the
/// static link fails and the shared object fails to load at runtime. Only libraries that are
/// actually present are emitted, so this is a no-op on systems whose libheif has no such codecs.
fn libheif_codec_link_flags() -> Vec<String> {
    // (pkg-config package name, library name)
    const CODECS: [(&str, &str); 7] = [
        ("x265", "x265"),
        ("aom", "aom"),
        ("dav1d", "dav1d"),
        ("libde265", "de265"),
        ("rav1e", "rav1e"),
        ("SvtAv1Enc", "SvtAv1Enc"),
        ("vvenc", "vvenc"),
    ];

    let mut flags: Vec<String> = Vec::new();
    for (package, lib_name) in CODECS {
        let candidate = match Command::new("pkg-config")
            .args(["--libs", package])
            .output()
        {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            // No pkg-config (or the package is unknown): fall back to a plain existence probe.
            _ if library_present(lib_name) => vec![format!("-l{lib_name}")],
            _ => continue,
        };
        for flag in candidate {
            if !flags.contains(&flag) {
                flags.push(flag);
            }
        }
    }
    flags
}

/// Whether `lib<name>.so`/`.a`/`.dylib` exists in one of the usual library directories.
fn library_present(name: &str) -> bool {
    const DIRS: [&str; 6] = [
        "/usr/lib",
        "/usr/lib64",
        "/usr/local/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/opt/homebrew/lib",
    ];
    DIRS.iter().any(|dir| {
        let dir = Path::new(dir);
        ["so", "a", "dylib"]
            .iter()
            .any(|extension| dir.join(format!("lib{name}.{extension}")).exists())
    })
}

/// Run a command from the build script, failing loudly with the full command line.
fn run_command(cmd: &mut Command) {
    let status = cmd
        .status()
        .unwrap_or_else(|error| panic!("failed to spawn {cmd:?}: {error}"));
    assert!(status.success(), "command failed: {cmd:?}");
}

/// libaom release cross-compiled for wasm, pinned so the codec build is reproducible.
const WASM_AOM_VERSION: &str = "v3.15.0";

/// Whether this compilation cross-compiles and links the wasm AV1 codec.
fn wasm_avif_enabled(target: &str) -> bool {
    is_wasm_target(target) && cfg!(feature = "heif") && cfg!(feature = "wasm-avif")
}

/// Cross-compile libaom for wasm32-wasip1 and return a prefix that libheif's `find_package(AOM)`
/// can consume.
///
/// libheif needs a codec library to do more than parse containers, and a host codec cannot be
/// linked into a wasm module. libaom is the portable choice: it has a `generic` target (no x86
/// SIMD), builds single-threaded, and its setjmp-based error handling maps onto the
/// `env.setjmp`/`env.longjmp` imports that wasi-libc's `libsetjmp.a` already produces (the browser
/// demo stubs those). HEVC is deliberately out of scope: x265/libde265 have no comparable WASI
/// port, so wasm gets AVIF only.
///
/// The result is cached in `OUT_DIR`, so later builds reuse the archive instead of rebuilding it.
/// Set `ULTRAHDR_AOM_SRC` to an existing libaom checkout to skip the clone entirely.
fn build_wasm_libaom(out_dir: &Path, wasi_prefix: &Path) -> PathBuf {
    let prefix = out_dir.join("aom-wasm-prefix");
    let archive = prefix.join("lib/libaom.a");
    let pkg_config = prefix.join("lib/pkgconfig/aom.pc");
    if archive.is_file() && pkg_config.is_file() {
        return prefix;
    }

    let src = match env::var_os("ULTRAHDR_AOM_SRC") {
        Some(dir) => PathBuf::from(dir),
        None => {
            let dir = out_dir.join("aom-src");
            if !dir.join("CMakeLists.txt").is_file() {
                let _ = fs::remove_dir_all(&dir);
                run_command(
                    Command::new("git")
                        .args([
                            "clone",
                            "--depth",
                            "1",
                            "--branch",
                            WASM_AOM_VERSION,
                            "https://aomedia.googlesource.com/aom",
                        ])
                        .arg(&dir),
                );
            }
            dir
        }
    };
    assert!(
        src.join("CMakeLists.txt").is_file(),
        "libaom sources not found at {}; set ULTRAHDR_AOM_SRC",
        src.display()
    );

    let build = out_dir.join("aom-build");
    let jobs = std::thread::available_parallelism()
        .map_or(4, |jobs| jobs.get())
        .to_string();
    let toolchain = wasi_prefix.join("share/cmake/wasi-sdk-p1.cmake");
    run_command(
        Command::new("cmake")
            .arg("-S")
            .arg(&src)
            .arg("-B")
            .arg(&build)
            .arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()))
            .arg(format!("-DWASI_SDK_PREFIX={}", wasi_prefix.display()))
            .arg("-DCMAKE_BUILD_TYPE=Release")
            // Portable C only: libaom's x86/AArch64 SIMD paths have no wasm equivalent.
            .arg("-DAOM_TARGET_CPU=generic")
            .arg("-DCONFIG_MULTITHREAD=0")
            .arg("-DCONFIG_RUNTIME_CPU_DETECT=0")
            .arg("-DCONFIG_AV1_ENCODER=1")
            .arg("-DCONFIG_AV1_DECODER=1")
            .arg("-DENABLE_TESTS=0")
            .arg("-DENABLE_TOOLS=0")
            .arg("-DENABLE_EXAMPLES=0")
            .arg("-DENABLE_DOCS=0")
            .arg("-DBUILD_SHARED_LIBS=0")
            // wasi-sdk's <setjmp.h> refuses to compile unless the SJLJ emulation is enabled, and
            // libaom uses setjmp/longjmp for its error recovery.
            .arg("-DCMAKE_C_FLAGS=-mllvm -wasm-enable-sjlj")
            .arg("-DCMAKE_CXX_FLAGS=-mllvm -wasm-enable-sjlj"),
    );
    run_command(
        Command::new("cmake")
            .arg("--build")
            .arg(&build)
            .arg("-j")
            .arg(jobs)
            .args(["--target", "aom"]),
    );

    // Lay out a prefix that looks like an installed libaom. libheif's FindAOM.cmake combines
    // pkg-config with find_path/find_library, so a matching aom.pc is what makes
    // CMAKE_PREFIX_PATH/PKG_CONFIG_LIBDIR prefer this copy over any host one.
    fs::create_dir_all(&prefix).expect("failed to create the libaom prefix");
    copy_dir_recursive(&src.join("aom"), &prefix.join("include/aom"))
        .expect("failed to copy the libaom headers");
    fs::create_dir_all(prefix.join("lib/pkgconfig")).expect("failed to create the libaom .pc dir");
    fs::copy(build.join("libaom.a"), &archive).expect("failed to copy libaom.a");
    fs::write(
        &pkg_config,
        format!(
            "prefix={prefix}\n\
             exec_prefix=${{prefix}}\n\
             libdir=${{exec_prefix}}/lib\n\
             includedir=${{prefix}}/include\n\
             \n\
             Name: aom\n\
             Description: AV1 codec library (wasm32-wasip1)\n\
             Version: {WASM_AOM_VERSION}\n\
             Libs: -L${{libdir}} -laom\n\
             Libs.private: -lm\n\
             Cflags: -I${{includedir}}\n",
            prefix = prefix.display()
        ),
    )
    .expect("failed to write aom.pc");
    prefix
}

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

    let submodule_path = manifest_dir.join("libultrahdr");
    if submodule_path.join("CMakeLists.txt").is_file() {
        return submodule_path;
    }

    let workspace_root = manifest_dir
        .parent()
        .expect("ultrahdr-sys has no parent dir");

    let submodule_path = workspace_root.join("libultrahdr");
    if submodule_path.join("CMakeLists.txt").is_file() {
        return submodule_path;
    }

    // Fallback: sibling checkout (old layout).
    workspace_root
        .parent()
        .expect("workspace has no parent dir")
        .join("libultrahdr")
}

fn apply_patch_once(src_dir: &Path, patch_path: &Path) {
    if !patch_path.is_file() {
        return;
    }

    let patch_str = patch_path
        .to_str()
        .expect("patch path contains non-UTF8 characters");

    // If patch applies in reverse, assume already applied.
    // NOTE: `git apply` doesn't work on crates.io source tarballs (no `.git`), so we use `patch`.
    // `patch -R` may auto-detect and ignore `-R`, so we add `--force` to make the exit status reliable.
    let reverse_ok = Command::new("patch")
        .current_dir(src_dir)
        .args(["-p1", "--dry-run", "--batch", "--silent", "--force", "-R"])
        .args(["-i", patch_str])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if reverse_ok {
        return;
    }

    let dry_run = Command::new("patch")
        .current_dir(src_dir)
        .args(["-p1", "--dry-run", "--batch", "--silent", "--forward"])
        .args(["-i", patch_str])
        .status()
        .expect("failed to execute patch (is `patch` installed?)");
    if !dry_run.success() {
        panic!(
            "failed to apply patch {} in {} (dry-run); set ULTRAHDR_SKIP_PATCHES=1 to bypass",
            patch_path.display(),
            src_dir.display()
        );
    }

    let status = Command::new("patch")
        .current_dir(src_dir)
        .args(["-p1", "--batch", "--silent", "--forward"])
        .args(["-i", patch_str])
        .status()
        .expect("failed to execute patch (is `patch` installed?)");
    if !status.success() {
        panic!(
            "failed to apply patch {} in {}",
            patch_path.display(),
            src_dir.display()
        );
    }
}

/// Codec libraries libheif would otherwise look for on the build host.
const LIBHEIF_HOST_CODECS: [&str; 18] = [
    "LIBDE265",
    "X265",
    "KVAZAAR",
    "UVG266",
    "VVDEC",
    "VVENC",
    "OpenH264_DECODER",
    "DAV1D",
    "AOM_DECODER",
    "AOM_ENCODER",
    "SvtEnc",
    "RAV1E",
    "JPEG_DECODER",
    "JPEG_ENCODER",
    "OpenJPEG_ENCODER",
    "OpenJPEG_DECODER",
    "FFMPEG_DECODER",
    "OPENJPH_ENCODER",
];

/// Patch hunk that stops libheif from probing the **build host** for codec libraries when the
/// target is WASI.
///
/// Beyond being the wrong architecture, the host include directories (`/usr/include` on a typical
/// Linux host) leak into the cross compile and break it (`gnu/stubs-32.h` not found). `keep_aom`
/// is set by the `wasm-avif` feature, which cross-compiles libaom into a prefix libheif can find,
/// so the AOM entries stay enabled in that case. Native builds never read this hunk.
fn libheif_wasi_codec_guard(keep_aom: bool) -> String {
    let mut added = vec![
        "# UltraHDR: when cross-compiling for WASI, never probe the build host for codec"
            .to_owned(),
        "# libraries: their include directories (e.g. /usr/include) and archives break the"
            .to_owned(),
        "# cross build, and a host library cannot be linked into a wasm module anyway.".to_owned(),
        String::new(),
        "if(CMAKE_SYSTEM_NAME STREQUAL \"WASI\")".to_owned(),
    ];
    for codec in LIBHEIF_HOST_CODECS {
        if keep_aom && codec.starts_with("AOM") {
            continue;
        }
        added.push(format!("  set(WITH_{codec} OFF CACHE BOOL \"\" FORCE)"));
    }
    added.push("endif()".to_owned());
    added.push(String::new());

    let context = [
        "     unset(msg)",
        " endmacro()",
        " ",
        " # libde265",
        " ",
        " plugin_option(LIBDE265 \"libde265 HEVC decoder\" ON OFF)",
    ];
    let mut hunk = String::from(
        "diff --git a/CMakeLists.txt b/CMakeLists.txt\n\
         --- a/CMakeLists.txt\n\
         +++ b/CMakeLists.txt\n",
    );
    hunk.push_str(&format!(
        "@@ -114,{} +114,{} @@\n",
        context.len(),
        context.len() + added.len()
    ));
    hunk.push_str(&context[..3].join("\n"));
    hunk.push('\n');
    for line in &added {
        hunk.push('+');
        hunk.push_str(line);
        hunk.push('\n');
    }
    hunk.push_str(&context[3..].join("\n"));
    hunk.push('\n');
    hunk
}

fn apply_local_patches(manifest_dir: &Path, src_dir: &Path) {
    if env::var("ULTRAHDR_SKIP_PATCHES").is_ok() {
        return;
    }
    apply_patch_once(
        src_dir,
        &manifest_dir.join("patches/libultrahdr-no-threads.patch"),
    );
    // libheif uses POSIX mkstemp(), which wasi-libc does not implement, so it
    // fails to build for wasm32-wasip1. libheif is not present in the staged
    // source here (the ExternalProject clones it *during* the CMake build), so
    // instead of patching it directly we append the fix onto the existing
    // cmake/patches/libheif_pr1503.patch, which the ExternalProject's
    // PATCH_COMMAND git-applies to the cloned tree.
    const MKSTEMP_FIX: &str = r#"
diff --git a/libheif/box.cc b/libheif/box.cc
index 3c8bdc86..dceb4c82 100644
--- a/libheif/box.cc
+++ b/libheif/box.cc
@@ -1506,7 +1506,12 @@ void Box_iloc::set_use_tmp_file(bool flag)
 {
   m_use_tmpfile = flag;
   if (flag) {
-#if !defined(_WIN32)
+#if defined(__wasi__)
+    // WASI has no mkstemp()/temp-file support in libc, so keep the item data in
+    // memory instead of spilling it to a file.
+    m_use_tmpfile = false;
+    m_tmpfile_fd = -1;
+#elif !defined(_WIN32)
     strcpy(m_tmp_filename, "/tmp/libheif-XXXXXX");
     m_tmpfile_fd = mkstemp(m_tmp_filename);
 #else
diff --git a/libheif/pixelimage.cc b/libheif/pixelimage.cc
index 04e81fe2..92be5bb9 100644
--- a/libheif/pixelimage.cc
+++ b/libheif/pixelimage.cc
@@ -275,23 +275,8 @@ Error HeifPixelImage::ImagePlane::alloc(uint32_t width, uint32_t height, heif_ch
             sstr.str()};
   }
 
-  try {
-    allocated_mem = new uint8_t[static_cast<size_t>(m_mem_height) * stride + alignment - 1];
-    uint8_t* mem_8 = allocated_mem;
-
-    // shift beginning of image data to aligned memory position
-
-    auto mem_start_addr = (uint64_t) mem_8;
-    auto mem_start_offset = (mem_start_addr & (alignment - 1U));
-    if (mem_start_offset != 0) {
-      mem_8 += alignment - mem_start_offset;
-    }
-
-    mem = mem_8;
-
-    return Error::Ok;
-  }
-  catch (const std::bad_alloc& excpt) {
+  allocated_mem = new (std::nothrow) uint8_t[static_cast<size_t>(m_mem_height) * stride + alignment - 1];
+  if (allocated_mem == nullptr) {
     std::stringstream sstr;
     sstr << "Allocating " << static_cast<size_t>(m_mem_height) * stride + alignment - 1 << " bytes failed";
 
@@ -299,6 +284,19 @@ Error HeifPixelImage::ImagePlane::alloc(uint32_t width, uint32_t height, heif_ch
             heif_suberror_Unspecified,
             sstr.str()};
   }
+  uint8_t* mem_8 = allocated_mem;
+
+  // shift beginning of image data to aligned memory position
+
+  auto mem_start_addr = (uint64_t) mem_8;
+  auto mem_start_offset = (mem_start_addr & (alignment - 1U));
+  if (mem_start_offset != 0) {
+    mem_8 += alignment - mem_start_offset;
+  }
+
+  mem = mem_8;
+
+  return Error::Ok;
 }
 
 
"#;
    let heif_patch = src_dir.join("cmake/patches/libheif_pr1503.patch");
    if heif_patch.is_file()
        && let Ok(mut f) = fs::OpenOptions::new().append(true).open(&heif_patch)
    {
        use std::io::Write;
        let _ = f.write_all(MKSTEMP_FIX.as_bytes());
        let guard =
            libheif_wasi_codec_guard(wasm_avif_enabled(&env::var("TARGET").unwrap_or_default()));
        let _ = f.write_all(guard.as_bytes());
    }
}

fn prepare_src_dir(manifest_dir: &Path, src_dir: &Path, out_dir: &Path) -> PathBuf {
    let work_src = out_dir.join("libultrahdr-src");
    let _ = fs::remove_dir_all(out_dir.join("build"));
    let _ = fs::remove_dir_all(&work_src);
    copy_dir_recursive(src_dir, &work_src).expect("failed to copy libultrahdr sources");
    // Normalize the line endings of the files `patches/` touches to LF. The
    // upstream v2.0+ tree carries a few CRLF lines; embed them byte-for-byte in
    // the patch and Strawberry's old `patch.exe` (used on Windows CI) chokes,
    // and `git apply` on any platform is line-ending strict. Normalizing keeps
    // the patch portable across git configs (core.autocrlf) and patch tools.
    for rel in ["CMakeLists.txt", "lib/src/jpegr.cpp"] {
        let p = work_src.join(rel);
        if let Ok(contents) = fs::read(&p) {
            let text = String::from_utf8_lossy(&contents);
            let normalized = text.replace("\r\n", "\n");
            if normalized.as_bytes() != contents.as_slice() {
                let _ = fs::write(&p, normalized.as_bytes());
            }
        }
    }
    apply_local_patches(manifest_dir, &work_src);
    work_src
}

fn is_wasm_target(target: &str) -> bool {
    target.starts_with("wasm32-")
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source_dir = locate_src_dir(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    // docs.rs builds in a sandboxed environment without network access.
    // Skip the full CMake build and only generate bindings for documentation.
    if env::var("DOCS_RS").is_ok() {
        println!("cargo:warning=Building for docs.rs: skipping CMake build");
        let bindings = bindgen::Builder::default()
            .header(source_dir.join("ultrahdr_api.h").to_string_lossy())
            .clang_arg(format!("-I{}", source_dir.display()))
            .rustified_enum("uhdr_.*")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .layout_tests(false)
            .allowlist_function("uhdr_.*")
            // `is_uhdr_image` does not carry the `uhdr_` prefix, allowlist it explicitly.
            .allowlist_function("is_uhdr_image")
            .allowlist_type("uhdr_.*")
            .allowlist_var("UHDR_.*")
            .generate()
            .expect("bindgen failed");
        bindings
            .write_to_file(out_dir.join("bindings.rs"))
            .expect("failed to write bindings");
        return;
    }

    if !source_dir.join("CMakeLists.txt").is_file() {
        panic!(
            "Could not find libultrahdr sources; set ULTRAHDR_SRC_DIR (current: {})",
            source_dir.display()
        );
    }

    let patch_path = manifest_dir.join("patches/libultrahdr-no-threads.patch");
    let libheif_patch_path = source_dir.join("cmake/patches/libheif_pr1503.patch");
    println!("cargo:rerun-if-env-changed=ULTRAHDR_SRC_DIR");
    println!("cargo:rerun-if-env-changed=ULTRAHDR_SKIP_PATCHES");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PREFIX");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=WASI_SDK_TOOLCHAIN_FILE");
    println!(
        "cargo:rerun-if-changed={}",
        source_dir.join("ultrahdr_api.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        source_dir.join("CMakeLists.txt").display()
    );
    println!("cargo:rerun-if-changed={}", patch_path.display());
    println!("cargo:rerun-if-changed={}", libheif_patch_path.display());

    let src_dir = prepare_src_dir(&manifest_dir, &source_dir, &out_dir);

    // CMake's FetchContent (libheif, libsmpte2094-50) runs a *nested* `cargo
    // build` on upstream's bundled Rust crates. When the outer build is
    // `cargo clippy`, those nested builds inherit clippy's driver and fail on
    // upstream lints enabled by `-D warnings`. Rebuild them with plain rustc by
    // stripping clippy-only env vars for the C/CMake process.
    for var in [
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "CLIPPY_ARGS",
    ] {
        // SAFETY: removing vars from the process env is safe here; a build
        // script does not spawn threads that would race on the env (and the
        // cmake crate already spawns the child process separately).
        unsafe { env::remove_var(var) };
    }

    let target = env::var("TARGET").expect("TARGET");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_wasm = is_wasm_target(&target);
    let wasi = wasi_toolchain();

    // `wasm-avif` cross-compiles libaom for the target and points the nested libheif configure at
    // it, because the WASI guard added above disables every host codec.
    println!("cargo:rerun-if-env-changed=ULTRAHDR_AOM_SRC");
    let wasm_aom = if cfg!(feature = "wasm-avif") && !cfg!(feature = "heif") {
        println!("cargo:warning=the `wasm-avif` feature only has an effect together with `heif`");
        None
    } else if wasm_avif_enabled(&target) {
        let (_, wasi_prefix) = wasi
            .as_ref()
            .expect("wasm-avif needs a wasi-sdk toolchain; set WASI_SDK_PREFIX");
        let prefix = build_wasm_libaom(&out_dir, wasi_prefix);
        let pc_dir = prefix.join("lib/pkgconfig");
        // SAFETY: removing/adding process env vars is safe here; the build script does not spawn
        // threads that race on them, and these only affect the CMake children spawned below.
        unsafe {
            // Make libheif find this libaom (CMAKE_PREFIX_PATH + its aom.pc) while hiding every
            // host .pc file, whose include directories would leak into the cross build.
            env::set_var("CMAKE_PREFIX_PATH", &prefix);
            env::set_var("PKG_CONFIG_LIBDIR", &pc_dir);
            env::set_var("PKG_CONFIG_PATH", &pc_dir);
        }
        Some(prefix)
    } else {
        None
    };

    let mut cfg = cmake::Config::new(&src_dir);
    cfg.profile("Release");
    // Shrink the wasm by size-optimizing + LTO the C++ side (libjpeg-turbo and
    // libultrahdr). The wasm link already uses `--gc-sections` to drop unreached
    // C++, but -Oz keeps the reachable code compact and -flto lets rust-lld run
    // cross-function optimisations over the archived bitcode at the final link.
    // We must also override the Release-profile flags: the default `-O3` would
    // otherwise come last and silently win over `-Oz` (last `-O` flag wins).
    if is_wasm {
        cfg.cflag("-Oz");
        cfg.cxxflag("-Oz");
        cfg.cflag("-flto");
        cfg.cxxflag("-flto");
        // wasi-sdk >= 22 defaults C++ to wasm exception handling, which emits the `exnref` and
        // `try_table` instructions that browsers cannot run yet (and which the WASI shim used by
        // the web demo does not model). libultrahdr never throws, so build the C++ side without
        // exceptions and link the matching `noeh` libc++ instead; see the link section below.
        cfg.cxxflag("-fno-exceptions");
        cfg.define("CMAKE_C_FLAGS_RELEASE", "-Oz -flto -DNDEBUG");
        cfg.define(
            "CMAKE_CXX_FLAGS_RELEASE",
            "-Oz -flto -fno-exceptions -DNDEBUG",
        );
    }
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
    // Control HEIF/AVIF container support via libheif. NOTE: upstream v2.0+
    // defaults UHDR_ENABLE_HEIF to ON, so we must explicitly disable it unless
    // the `heif` feature is requested; otherwise the default vendored build
    // would fetch and build libheif as a dependency.
    let heif = cfg!(feature = "heif");
    cfg.define("UHDR_ENABLE_HEIF", if heif { "ON" } else { "OFF" });

    // The vendored libheif is linked PRIVATE into libuhdr, so its codec libraries have to be
    // added to every final link: the Rust executable and, with `shared`, libuhdr itself (which
    // would otherwise be left with undefined `aom_*`/`x265_*` symbols and fail to load).
    let heif_codec_flags = if heif && cfg!(feature = "vendored") && !is_wasm && target_env != "msvc"
    {
        libheif_codec_link_flags()
    } else {
        Vec::new()
    };
    if build_shared && !heif_codec_flags.is_empty() {
        cfg.define("CMAKE_SHARED_LINKER_FLAGS", heif_codec_flags.join(" "));
    }
    // Control SMPTE ST 2094-50 (AGTM) dynamic metadata support. This is only
    // built when `vendored` is enabled upstream (FetchContent clones
    // webmproject/libsmpte2094-50 v0.1.4); with UHDR_BUILD_DEPS=OFF it is
    // skipped with a warning and disabled.
    cfg.define(
        "UHDR_ENABLE_SMPTE2094_50",
        if cfg!(feature = "smpte2094-50") {
            "ON"
        } else {
            "OFF"
        },
    );
    // Control ISO 21496-1 metadata emission via feature flag (default ON).
    cfg.define(
        "UHDR_WRITE_ISO",
        if cfg!(feature = "iso21496") {
            "ON"
        } else {
            "OFF"
        },
    );
    // Control XMP gain map metadata emission via feature flag (default ON).
    cfg.define(
        "UHDR_WRITE_XMP",
        if cfg!(feature = "xmp") { "ON" } else { "OFF" },
    );
    if disable_threads {
        cfg.define("UHDR_DISABLE_THREADS", "ON");
    }
    if cfg!(feature = "jpeg-max-dimension") {
        // Use libjpeg-turbo's hardcoded JPEG_MAX_DIMENSION (65500).
        cfg.define("UHDR_MAX_DIMENSION", "65500");
    }

    // Build only the main library target; install target is disabled upstream.
    let cmake_target = if target_env == "msvc" && !build_shared {
        "uhdr-static"
    } else {
        "uhdr"
    };
    cfg.build_target(cmake_target);

    let dst = cfg.build();
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

    // When HEIF/AVIF support is enabled, the upstream CMake builds libheif as a
    // static ExternalProject (vendored) or links a system libheif (non-vendored).
    // `uhdr`/`core` link it PRIVATE, so the final Rust executable must pull it in.
    if cfg!(feature = "heif") {
        if cfg!(feature = "vendored") {
            // Bundled libheif static archive (non-multi build) lives at
            // <dst>/build/libheif/src/libheif-build/libheif/libheif.a
            println!(
                "cargo:rustc-link-search=native={}/build/libheif/src/libheif-build/libheif",
                dst.display()
            );
            if target_env == "msvc" {
                println!(
                    "cargo:rustc-link-search=native={}/build/libheif/src/libheif-build/libheif/Release",
                    dst.display()
                );
                println!(
                    "cargo:rustc-link-search=native={}/build/libheif/src/libheif-build/libheif/Debug",
                    dst.display()
                );
            }
            println!("cargo:rustc-link-lib=static=heif");
        } else {
            println!("cargo:rustc-link-lib=heif");
        }
        if is_wasm {
            // Without `wasm-avif` the artifact keeps libheif container support only, i.e. no
            // HEIF/AVIF codecs at runtime.
            if let Some(prefix) = &wasm_aom {
                println!(
                    "cargo:rustc-link-search=native={}",
                    prefix.join("lib").display()
                );
                println!("cargo:rustc-link-lib=static=aom");
            }
        } else {
            for flag in &heif_codec_flags {
                if let Some(dir) = flag.strip_prefix("-L") {
                    println!("cargo:rustc-link-search=native={dir}");
                } else if let Some(lib) = flag.strip_prefix("-l") {
                    println!("cargo:rustc-link-lib={lib}");
                }
            }
        }
    }

    // SMPTE ST 2094-50 (AGTM) is provided by a FetchContent static library that
    // `uhdr`/`core` link PRIVATE, so expose it to the final link too. The
    // FetchContent build only runs when `vendored` (UHDR_BUILD_DEPS=ON) is set;
    // without it upstream warns and disables SMPTE, so don't emit a bogus -l.
    if cfg!(feature = "smpte2094-50") && cfg!(feature = "vendored") {
        // FetchContent defaults to <dst>/build/_deps/libsmpte2094_50-build.
        println!(
            "cargo:rustc-link-search=native={}/build/_deps/libsmpte2094_50-build",
            dst.display()
        );
        println!("cargo:rustc-link-lib=static=smpte2094_50_utils");
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
                let sysroot_lib = prefix.join("share/wasi-sysroot/lib/wasm32-wasip1");
                println!("cargo:rustc-link-search=native={}", sysroot_lib.display());
                // wasi-sdk >= 22 ships libc++ in `eh` (wasm exception handling) and `noeh`
                // (exceptions disabled) variants instead of directly in the sysroot library
                // directory. The C++ driver would pick one automatically, but the Rust link
                // invokes wasm-ld itself, so add it explicitly. `noeh` matches the
                // `-fno-exceptions` build above; older sysroots keep libc++ in the base directory,
                // which is already on the search path.
                for variant in ["noeh", "eh"] {
                    let candidate = sysroot_lib.join(variant);
                    if candidate.join("libc++.a").is_file() {
                        println!("cargo:rustc-link-search=native={}", candidate.display());
                        break;
                    }
                }
                println!("cargo:rustc-link-lib=static=c++");
                println!("cargo:rustc-link-lib=static=c++abi");
                println!("cargo:rustc-link-lib=static=setjmp");
            }
        } else {
            let cxx_stdlib = if target_os == "macos" {
                "c++"
            } else {
                "stdc++"
            };
            println!("cargo:rustc-link-lib={}", cxx_stdlib);
        }
    }

    let bindgen_target = if is_wasm {
        "i686-unknown-linux-gnu"
    } else {
        target.as_str()
    };

    let mut bindings = bindgen::Builder::default()
        .header(source_dir.join("ultrahdr_api.h").to_string_lossy())
        .clang_arg(format!("-I{}", source_dir.display()))
        .rustified_enum("uhdr_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .clang_arg(format!("--target={}", bindgen_target));
    if !is_wasm {
        bindings = bindings
            .allowlist_function("uhdr_.*")
            // `is_uhdr_image` does not carry the `uhdr_` prefix, allowlist it explicitly.
            .allowlist_function("is_uhdr_image")
            .allowlist_type("uhdr_.*")
            .allowlist_var("UHDR_.*");
    }
    if !is_wasm && let Some((_, prefix)) = &wasi {
        bindings = bindings.clang_arg(format!("--sysroot={}/share/wasi-sysroot", prefix.display()));
    }
    let bindings = bindings.generate().expect("bindgen failed");

    let bindings_path = out_dir.join("bindings.rs");
    bindings
        .write_to_file(&bindings_path)
        .expect("failed to write bindings");
    if is_wasm && let Ok(content) = fs::read_to_string(&bindings_path) {
        let fn_count = content.matches("pub fn ").count();
        if fn_count == 0 {
            println!("cargo:warning=bindgen generated 0 functions for wasm target");
        }
    }
}

//! Bundle the native engine's advertised runtime and CPU modules. Shared
//! libraries keep CPU-specific instructions behind runtime feature selection.
use std::{env, fs, path::Path};

fn is_runtime(name: &str) -> bool {
    (name.starts_with("ggml")
        || name.starts_with("libggml")
        || name.starts_with("transcribe")
        || name.starts_with("libtranscribe"))
        && (name.ends_with(".dll")
            || name.ends_with(".so")
            || name.contains(".so.")
            || name.ends_with(".dylib"))
}

fn copy_runtime(from: &Path, to: &Path) -> std::io::Result<usize> {
    fs::create_dir_all(to)?;
    let mut count = 0;
    for entry in fs::read_dir(from)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() && is_runtime(name) {
            // Copy the SONAME as well as the unversioned link. fs::copy follows
            // symlinks so a relocated installation has no build-tree targets.
            fs::copy(&path, to.join(name))?;
            count += 1;
        }
    }
    Ok(count)
}

#[allow(dead_code)] // The app's unit tests share this build-script module.
pub fn stage(package_name: &str) {
    let manifest = env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory");
    let destination = Path::new(&manifest).join("transcribe-libs");
    if destination.exists() {
        fs::remove_dir_all(&destination).expect("clear generated native runtime staging");
    }
    for key in [
        "DEP_TRANSCRIBE_CPP_RUNTIME_DIR",
        "DEP_TRANSCRIBE_CPP_MODULE_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        let source = env::var_os(key).unwrap_or_else(|| {
            panic!("{key} is missing; build the speech engine with dynamic-backends")
        });
        assert!(
            copy_runtime(Path::new(&source), &destination).expect("stage native libraries") > 0,
            "no native runtime libraries in {}",
            Path::new(&source).display()
        );
    }
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64") {
        assert!(
            fs::read_dir(&destination).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("ggml-cpu-x64")),
            "the baseline x64 CPU backend is required"
        );
    }
    // Tests and development binaries must use exactly the same libraries as
    // the installer. OUT_DIR is <profile>/build/<package>/out.
    let out = env::var_os("OUT_DIR").expect("Cargo output directory");
    let profile = Path::new(&out)
        .ancestors()
        .nth(3)
        .expect("Cargo profile directory");
    copy_runtime(&destination, profile).expect("stage native libraries beside executable");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN:$ORIGIN/..:$ORIGIN/../lib/{package_name}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_names_include_versioned_sonames_and_exclude_linker_inputs() {
        for name in [
            "transcribe.dll",
            "ggml-cpu-x64.dll",
            "libtranscribe.so",
            "libtranscribe.so.0",
            "libggml-cpu-haswell.so",
        ] {
            assert!(is_runtime(name), "{name}");
        }
        for name in [
            "transcribe.lib",
            "libtranscribe.a",
            "transcribe-link.json",
            "other.dll",
            "ggml-headers",
        ] {
            assert!(!is_runtime(name), "{name}");
        }
    }

    #[test]
    fn copies_runtime_bytes_and_versioned_names_without_headers() {
        let root = env::temp_dir().join(format!("bard-native-runtime-{}", std::process::id()));
        fs::create_dir_all(root.join("input")).unwrap();
        fs::write(root.join("input/libtranscribe.so.0"), b"synthetic library").unwrap();
        fs::write(root.join("input/libtranscribe.a"), b"linker input").unwrap();
        assert_eq!(
            copy_runtime(&root.join("input"), &root.join("output")).unwrap(),
            1
        );
        assert_eq!(
            fs::read(root.join("output/libtranscribe.so.0")).unwrap(),
            b"synthetic library"
        );
        assert!(!root.join("output/libtranscribe.a").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

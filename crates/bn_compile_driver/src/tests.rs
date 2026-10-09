//! Unit tests of the compilation driver: build-only option extension
//! (`--opt`, `--target`), clang default, wasm32 compiler selection (Homebrew
//! LLVM on macOS, never Apple clang) and native link arguments.
use crate::options::{BuildOptions, Optimization, Target};
use crate::toolchain::{clang_has_wasm32, configured_clang, configured_wasm_clang};

fn build_options(arguments: &[&str]) -> Result<BuildOptions, String> {
    let mut build = BuildOptions::default();
    bn_cli::options::parse_options(arguments.iter().map(ToString::to_string), &mut build)?;
    Ok(build)
}

#[test]
fn optimization_option_has_explicit_levels_and_default() {
    assert_eq!(
        build_options(&["file.bn"]).expect("default").optimization,
        Optimization::Level(2)
    );
    assert_eq!(
        build_options(&["--opt", "none", "file.bn"])
            .expect("none")
            .optimization,
        Optimization::None
    );
    assert_eq!(
        build_options(&["--opt", "s", "file.bn"])
            .expect("size")
            .optimization,
        Optimization::Size
    );
    assert_eq!(
        build_options(&["--target", "wasm32", "file.bn"])
            .expect("target")
            .target,
        Target::Wasm32
    );
    assert!(build_options(&["--opt", "4", "file.bn"]).is_err());
}

#[test]
fn compiler_configuration_selects_clang() {
    assert_eq!(configured_clang().expect("read configuration"), "clang");
}

/// First line of `<clang> --version` (empty when the command cannot run).
fn clang_version_banner(clang: &str) -> String {
    std::process::Command::new(clang)
        .arg("--version")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

/// The wasm32 compiler the driver selects must really target wasm32 and,
/// on macOS, must be Homebrew's LLVM clang: Apple clang has no wasm32
/// backend, so falling back to it would only fail later inside `bn build`.
#[test]
fn wasm32_compiler_is_selected_from_homebrew_llvm_not_apple_clang() {
    if std::env::var_os("BN_WASM_CLANG").is_some() {
        return; // explicit override: the operator owns the choice
    }
    let clang = configured_wasm_clang().expect("read toolchain configuration");
    let banner = clang_version_banner(&clang);
    assert!(
        !banner.contains("Apple clang"),
        "wasm32 compiler resolved to Apple clang ({clang}: {banner})"
    );
    assert!(
        clang_has_wasm32(&clang),
        "selected wasm32 compiler has no wasm32 target ({clang}: {banner})"
    );
    #[cfg(target_os = "macos")]
    assert!(
        banner.contains("Homebrew clang"),
        "on macOS the wasm32 compiler must be Homebrew LLVM (brew install llvm); got {clang}: {banner}"
    );
}

#[test]
fn native_runtime_linking_adds_platform_system_libraries() {
    #[cfg(target_os = "linux")]
    assert_eq!(crate::artifact::native_runtime_link_args(), ["-lm"]);

    #[cfg(windows)]
    assert!(
        crate::artifact::native_runtime_link_args().contains(&"-lntdll"),
        "bn_rt.lib needs the Windows system libraries rustc reports"
    );

    #[cfg(not(any(target_os = "linux", windows)))]
    assert!(crate::artifact::native_runtime_link_args().is_empty());
}

#[test]
fn native_programs_use_the_dynamic_crt_and_legacy_stdio_only_on_windows() {
    #[cfg(windows)]
    assert!(crate::artifact::native_program_link_args().contains(&"-llegacy_stdio_definitions"));

    #[cfg(not(windows))]
    assert!(crate::artifact::native_program_link_args().is_empty());
}

#[test]
fn build_dir_is_private_beside_the_source_and_removed_on_drop() {
    let source_dir = std::env::temp_dir().join(format!("bn-build-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&source_dir);
    std::fs::create_dir_all(&source_dir).unwrap();
    let source = source_dir.join("prog.bn");
    let path;
    {
        let build_dir = crate::artifact::BuildDir::beside(&source).unwrap();
        path = build_dir.join("module.ll");
        let directory = path.parent().unwrap().to_path_buf();
        assert_eq!(directory.parent(), Some(source_dir.as_path()));
        assert!(
            directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".prog.bnbuild-")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&directory).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        crate::artifact::write_new(&path, "ir").unwrap();
        // An existing file is never overwritten.
        assert!(crate::artifact::write_new(&path, "again").is_err());
        assert!(directory.is_dir());
    }
    assert!(!path.exists(), "BuildDir must remove its contents on drop");
    assert_eq!(std::fs::read_dir(&source_dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&source_dir).unwrap();
}

/// `write_new` never follows a planted symlink.
#[cfg(unix)]
#[test]
fn write_new_refuses_a_planted_symlink() {
    let directory = std::env::temp_dir().join(format!("bn-write-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let victim = directory.join("victim");
    std::fs::write(&victim, "keep").unwrap();
    let link = directory.join("module.ll");
    std::os::unix::fs::symlink(&victim, &link).unwrap();
    assert!(crate::artifact::write_new(&link, "ir").is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    std::fs::remove_dir_all(&directory).unwrap();
}

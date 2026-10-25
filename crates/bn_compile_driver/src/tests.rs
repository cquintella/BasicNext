//! Unit tests of the compilation driver: build-only option extension
//! (`--opt`, `--target`), clang default and native link arguments.
use crate::options::{BuildOptions, Optimization, Target};
use crate::toolchain::configured_clang;

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
        build_options(&["--target", "native", "file.bn"])
            .expect("target")
            .target,
        Target::Native
    );
    assert!(build_options(&["--target", "wasm32", "file.bn"]).is_err());
    assert!(build_options(&["--opt", "4", "file.bn"]).is_err());
}

#[test]
fn compiler_configuration_selects_clang() {
    assert_eq!(configured_clang().expect("read configuration"), "clang");
}

#[test]
fn native_runtime_linking_adds_platform_system_libraries() {
    assert_eq!(
        crate::artifact::native_runtime_link_args_for_os("linux"),
        ["-lm"]
    );
    assert!(
        crate::artifact::native_runtime_link_args_for_os("windows").contains(&"-lntdll"),
        "bn_rt.lib needs the Windows system libraries rustc reports"
    );
    assert!(crate::artifact::native_runtime_link_args_for_os("macos").is_empty());
    assert_eq!(
        crate::artifact::native_runtime_link_args(),
        crate::artifact::native_runtime_link_args_for_os(std::env::consts::OS)
    );
}

#[test]
fn native_programs_use_the_dynamic_crt_and_legacy_stdio_only_on_windows() {
    assert!(
        crate::artifact::native_program_link_args_for_os("windows")
            .contains(&"-llegacy_stdio_definitions")
    );
    assert!(crate::artifact::native_program_link_args_for_os("macos").is_empty());
    assert!(crate::artifact::native_program_link_args_for_os("linux").is_empty());
    assert_eq!(
        crate::artifact::dead_code_link_args_for_os("macos"),
        ["-Wl,-dead_strip"]
    );
    assert_eq!(
        crate::artifact::dead_code_link_args_for_os("linux"),
        ["-Wl,--gc-sections"]
    );
    assert!(crate::artifact::dead_code_link_args_for_os("windows").is_empty());
    assert_eq!(
        crate::artifact::native_program_link_args(),
        crate::artifact::native_program_link_args_for_os(std::env::consts::OS)
    );
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

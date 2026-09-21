// Unit tests of the `bn` driver: build-only option extension (`--opt`,
// `--target`), clang/wasm toolchain selection and native link arguments.
use super::cli_toolchain::{clang_has_wasm32, configured_clang};
use super::{BuildOptions, Optimization, Target};

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

#[test]
fn apple_clang_is_not_a_wasm32_compiler() {
    if configured_clang().ok().as_deref() == Some("clang") {
        assert!(
            !clang_has_wasm32("clang")
                || std::process::Command::new("clang")
                    .arg("--version")
                    .output()
                    .is_ok_and(
                        |output| !String::from_utf8_lossy(&output.stdout).contains("Apple clang")
                    ),
            "PATH clang is Apple clang and must not be used for wasm32"
        );
    }
}

#[test]
fn native_runtime_linking_adds_libm_only_on_linux() {
    #[cfg(target_os = "linux")]
    assert_eq!(super::native_runtime_link_args(), ["-lm"]);

    #[cfg(not(target_os = "linux"))]
    assert!(super::native_runtime_link_args().is_empty());
}

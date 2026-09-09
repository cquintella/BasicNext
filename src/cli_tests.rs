use super::cli_toolchain::{clang_has_wasm32, configured_clang};
use super::{Color, colorize};

#[test]
fn success_color_can_be_disabled() {
    assert_eq!(colorize("ok", Color::Never), "ok");
    assert!(colorize("ok", Color::Always).contains("ok"));
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

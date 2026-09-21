//! One HOST environment composition for every interpreter entrypoint (run,
//! eval, DAP): shipped providers, filesystem policy from the common options,
//! then the single environment narrowing parser (`bn_rt::Policy` through
//! `HostEnv::narrowed_by_env`).

use std::env;

use bn_cli::options::Options;
use bn_interp::HostEnv;

/// The providers this build ships. A `HostEnv` starts with **empty**
/// registries (the language alone); the binaries and tests add the shipped
/// `BN*` libraries and HOST capabilities through this trait, so the core
/// never names a provider (bucket 0.5.1d, D-F2-07).
pub trait HostEnvDefaults {
    #[must_use]
    fn with_default_providers(self) -> Self;
}

impl HostEnvDefaults for HostEnv {
    fn with_default_providers(self) -> Self {
        self.with_libraries(crate::libraries::default_libraries())
            .with_hosts(crate::hosts::default_hosts())
    }
}

/// Builds the environment a program runs in: `arguments` become `HOST.Args`,
/// `--sandbox`/`--no-filesystem` shape filesystem access, and the process
/// environment narrows the policy exactly as `bn_rt_policy_init` does for a
/// compiled artifact.
///
/// # Errors
///
/// Returns the configuration message for invalid sandbox roots or a
/// malformed policy environment (both `CONFIG_INVALID`, exit 2).
pub fn host_env(options: &Options, arguments: Vec<String>) -> Result<HostEnv, String> {
    let host = if options.sandbox {
        HostEnv::system(arguments)
            .with_default_providers()
            .with_filesystem_roots(options.read_roots.clone(), options.write_roots.clone())?
    } else if options.filesystem {
        HostEnv::system(arguments).with_default_providers()
    } else {
        HostEnv::system(arguments)
            .with_default_providers()
            .without_filesystem()
    };
    // One environment read, one parser (`bn_rt::Policy::narrow_from_env`).
    host.narrowed_by_env(|name| env::var(name).ok())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::host_env;
    use bn_cli::options::parse_options;

    fn options(arguments: &[&str]) -> bn_cli::options::Options {
        parse_options(arguments.iter().map(ToString::to_string), &mut ()).expect("options")
    }

    #[test]
    fn shipped_hosts_are_registered_by_capability_name() {
        let hosts = crate::hosts::default_hosts().instantiate();
        for name in ["Net", "FileSystem", "Exec", "Clock", "Random", "Console"] {
            assert!(hosts.contains_key(name), "missing HOST.{name}");
        }
    }

    #[test]
    fn sandbox_root_that_cannot_be_opened_is_a_configuration_error() {
        let missing = std::env::temp_dir().join(format!("bn-no-such-root-{}", std::process::id()));
        let sandboxed = options(&[
            "--sandbox",
            "--read-root",
            missing.to_str().expect("UTF-8 path"),
            "file.bn",
        ]);
        let Err(error) = host_env(&sandboxed, vec!["file.bn".into()]) else {
            panic!("missing sandbox root must be rejected");
        };
        assert!(error.contains("root"), "{error}");
        assert!(host_env(&options(&["--no-filesystem", "file.bn"]), Vec::new()).is_ok());
    }
}

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The trust roots of the HTTPS client (bnweb.md, "HTTPS client trust
//! roots"): the operating system's store, always, plus at most one extra PEM
//! file — `BN_CA_FILE`, otherwise `bn-cacert.pem` then `bn-ca.pem` in the
//! program's directory. `SSL_CERT_FILE` and `SSL_CERT_DIR` are not read: the
//! platform loaders below do not consult the environment.
//!
//! The roots are built once per process, at the first HTTPS request that
//! builds them successfully; a failure is not kept, so the next request tries
//! again.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;

/// The environment variable that names the extra PEM file.
pub const CA_FILE_VARIABLE: &str = "BN_CA_FILE";
/// The extra PEM files looked for in the program's directory, in order.
pub const PROGRAM_CA_FILES: [&str; 2] = ["bn-cacert.pem", "bn-ca.pem"];

/// Where "the program's directory" is.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ProgramDirectory {
    /// Nobody said: a `bnc` executable, whose directory is that of
    /// `current_exe`.
    #[default]
    Executable,
    /// The directory of the entry `.bn` file (`bni`).
    Known(PathBuf),
    /// The program has no file (`eval`, a REPL session): no default file.
    None,
}

static PROGRAM_DIRECTORY: Mutex<ProgramDirectory> = Mutex::new(ProgramDirectory::Executable);
static ROOTS: Mutex<Option<Arc<RootCertStore>>> = Mutex::new(None);

/// Tells the client where the program lives. `bni` calls it before it runs a
/// program; a compiled executable never does.
pub fn set_program_directory(directory: ProgramDirectory) {
    *PROGRAM_DIRECTORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = directory;
}

fn program_directory() -> Option<PathBuf> {
    match PROGRAM_DIRECTORY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    {
        ProgramDirectory::Known(directory) => Some(directory),
        ProgramDirectory::None => None,
        ProgramDirectory::Executable => std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf)),
    }
}

/// The client's roots, built at the first successful call.
///
/// # Errors
///
/// The extra file named by `BN_CA_FILE` cannot be read, the chosen extra file
/// holds no valid certificate, or no usable root remains.
pub(crate) fn client_roots() -> Result<Arc<RootCertStore>, String> {
    let mut cached = ROOTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(roots) = cached.as_ref() {
        return Ok(Arc::clone(roots));
    }
    let extra = choose_extra(
        std::env::var_os(CA_FILE_VARIABLE).map(PathBuf::from),
        program_directory().as_deref(),
    );
    let roots = Arc::new(build_roots(platform::roots(), extra.as_deref())?);
    *cached = Some(Arc::clone(&roots));
    Ok(roots)
}

/// The one extra file, in the order of the rule: the variable when it is
/// set, otherwise the first default file that exists in `directory`.
fn choose_extra(variable: Option<PathBuf>, directory: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = variable {
        return Some(path);
    }
    let directory = directory?;
    PROGRAM_CA_FILES
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

/// The platform roots plus the extra file. An extra file is never skipped:
/// unreadable, or without a valid certificate, it is an error.
/// A platform store that could not be read counts as empty; its error is
/// reported only when no root remains.
fn build_roots(
    platform: Result<Vec<CertificateDer<'static>>, String>,
    extra: Option<&Path>,
) -> Result<RootCertStore, String> {
    let mut roots = RootCertStore::empty();
    let (platform, unreadable) = match platform {
        Ok(certificates) => (certificates, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    // A platform store may hold an entry rustls cannot parse; it is skipped.
    roots.add_parsable_certificates(platform);
    if let Some(path) = extra {
        let certificates = read_pem_certificates(path)?;
        let (added, _skipped) = roots.add_parsable_certificates(certificates);
        if added == 0 {
            return Err(bn_catalog::fill(
                "web",
                "tls.ca_file_empty.cause",
                &[("path", &path.display())],
            ));
        }
    }
    if roots.is_empty() {
        return Err(match unreadable {
            Some(error) => {
                bn_catalog::fill("web", "tls.platform_unreadable.cause", &[("error", &error)])
            }
            None => bn_catalog::text("web", "tls.no_roots.cause").into(),
        });
    }
    Ok(roots)
}

fn read_pem_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let unreadable = |error: &dyn std::fmt::Display| {
        bn_catalog::fill(
            "web",
            "tls.ca_file_unreadable.cause",
            &[("path", &path.display()), ("error", &error)],
        )
    };
    CertificateDer::pem_file_iter(path)
        .map_err(|error| unreadable(&error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| unreadable(&error))
}

/// The operating system's store, read without the environment.
#[cfg(target_os = "macos")]
mod platform {
    use rustls::pki_types::CertificateDer;
    use security_framework::trust_settings::{Domain, TrustSettings, TrustSettingsForCertificate};
    use std::collections::BTreeMap;

    /// The certificates the keychain trust settings make roots for TLS. User
    /// settings override admin settings, which override system settings, so
    /// the first domain that names a certificate decides.
    pub(super) fn roots() -> Result<Vec<CertificateDer<'static>>, String> {
        let mut decided: BTreeMap<Vec<u8>, TrustSettingsForCertificate> = BTreeMap::new();
        for domain in [Domain::User, Domain::Admin, Domain::System] {
            let settings = TrustSettings::new(domain);
            let certificates = match settings.iter() {
                Ok(certificates) => certificates,
                // The system domain holds the roots: its failure is reported.
                Err(error) if matches!(domain, Domain::System) => return Err(error.to_string()),
                // User and Admin domains usually have no trust settings.
                Err(_) => continue,
            };
            for certificate in certificates {
                // No specific setting means "trust as a root" (Apple's rule).
                let Ok(trust) = settings.tls_trust_settings_for_certificate(&certificate) else {
                    continue;
                };
                decided
                    .entry(certificate.to_der())
                    .or_insert_with(|| trust.unwrap_or(TrustSettingsForCertificate::TrustRoot));
            }
        }
        Ok(decided
            .into_iter()
            .filter(|(_, trust)| {
                matches!(
                    trust,
                    TrustSettingsForCertificate::TrustRoot
                        | TrustSettingsForCertificate::TrustAsRoot
                )
            })
            .map(|(der, _)| CertificateDer::from(der))
            .collect())
    }
}

#[cfg(windows)]
mod platform {
    use rustls::pki_types::CertificateDer;
    use schannel::cert_context::ValidUses;
    use schannel::cert_store::CertStore;

    /// The TLS server-authentication purpose.
    const SERVER_AUTH_OID: &str = "1.3.6.1.5.5.7.3.1";

    /// The current user's `ROOT` store, which includes the machine's roots:
    /// the certificates valid now and usable for server authentication.
    pub(super) fn roots() -> Result<Vec<CertificateDer<'static>>, String> {
        let store = CertStore::open_current_user("ROOT").map_err(|error| error.to_string())?;
        Ok(store
            .certs()
            .filter(|certificate| {
                let usable = certificate.valid_uses().is_ok_and(|uses| match uses {
                    ValidUses::All => true,
                    ValidUses::Oids(oids) => oids.iter().any(|oid| oid == SERVER_AUTH_OID),
                });
                usable && certificate.is_time_valid().unwrap_or(false)
            })
            .map(|certificate| CertificateDer::from(certificate.to_der().to_vec()))
            .collect())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use rustls::pki_types::CertificateDer;
    use std::path::Path;

    /// The system CA bundle files, the first that exists is read (the list of
    /// `openssl-probe`, which is private there; Go's `root_linux.go` has the
    /// same). `openssl_probe::probe` is not used: it reads `SSL_CERT_FILE`.
    #[cfg(target_os = "linux")]
    const BUNDLE_FILES: &[&str] = &[
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
        "/etc/pki/tls/cacert.pem",
        "/etc/ssl/cert.pem",
    ];
    #[cfg(not(target_os = "linux"))]
    const BUNDLE_FILES: &[&str] = &["/etc/ssl/cert.pem", "/usr/local/etc/ssl/cert.pem"];

    pub(super) fn roots() -> Result<Vec<CertificateDer<'static>>, String> {
        let file = BUNDLE_FILES
            .iter()
            .map(Path::new)
            .find(|path| path.is_file());
        let mut certificates = rustls_native_certs::load_certs_from_paths(file, None).certs;
        // `candidate_cert_dirs` lists system directories only, never the
        // environment.
        for directory in openssl_probe::candidate_cert_dirs() {
            certificates
                .extend(rustls_native_certs::load_certs_from_paths(None, Some(directory)).certs);
        }
        Ok(certificates)
    }
}

#[cfg(test)]
mod tests {
    use super::{PROGRAM_CA_FILES, build_roots, choose_extra};
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/tls")
            .join(name)
    }

    fn test_ca() -> Vec<rustls::pki_types::CertificateDer<'static>> {
        super::read_pem_certificates(&fixture("test-ca.pem")).expect("the test CA")
    }

    #[test]
    fn the_variable_comes_first_then_the_default_files_in_order() {
        let directory = tempfile_directory("order");
        assert_eq!(choose_extra(None, Some(&directory)), None);
        std::fs::write(directory.join(PROGRAM_CA_FILES[1]), "x").expect("write");
        assert_eq!(
            choose_extra(None, Some(&directory)),
            Some(directory.join(PROGRAM_CA_FILES[1]))
        );
        std::fs::write(directory.join(PROGRAM_CA_FILES[0]), "x").expect("write");
        assert_eq!(
            choose_extra(None, Some(&directory)),
            Some(directory.join(PROGRAM_CA_FILES[0])),
            "bn-cacert.pem is looked for before bn-ca.pem"
        );
        let named = PathBuf::from("/elsewhere/ca.pem");
        assert_eq!(
            choose_extra(Some(named.clone()), Some(&directory)),
            Some(named),
            "BN_CA_FILE wins over the default files"
        );
        assert_eq!(
            choose_extra(None, None),
            None,
            "no program file, no default"
        );
        std::fs::remove_dir_all(&directory).expect("clean up");
    }

    #[test]
    fn an_extra_file_is_added_never_skipped() {
        let platform = test_ca();
        // Added to the platform roots, not in place of them.
        let both = build_roots(Ok(platform.clone()), Some(&fixture("test-ca.pem"))).expect("roots");
        assert_eq!(both.len(), 2, "the platform root and the extra one");
        // A file that cannot be read fails, though the platform has roots.
        let missing = build_roots(Ok(platform.clone()), Some(Path::new("/no/such/bn-ca.pem")));
        assert!(missing.is_err_and(|message| message.contains("cannot be read")));
        // A file without a certificate fails.
        let directory = tempfile_directory("empty");
        let empty = directory.join(PROGRAM_CA_FILES[0]);
        std::fs::write(&empty, "no certificate here").expect("write");
        assert!(
            build_roots(Ok(platform), Some(&empty))
                .is_err_and(|message| message.contains("no valid certificate"))
        );
        std::fs::remove_dir_all(&directory).expect("clean up");
    }

    /// A TLS handshake with a local server whose certificate the test CA
    /// signed, from a client over `roots`. `Ok` when the client accepts the
    /// server's chain.
    fn handshake(roots: rustls::RootCertStore) -> Result<(), String> {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{PrivateKeyDer, ServerName};
        use std::io::{Read, Write};
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let chain = super::read_pem_certificates(&fixture("server.pem")).expect("chain");
        let key = PrivateKeyDer::from_pem_file(fixture("server-key.pem")).expect("key");
        let server = rustls::ServerConfig::builder_with_provider(std::sync::Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("server configuration");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let serving = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let connection = rustls::ServerConnection::new(std::sync::Arc::new(server))
                .expect("server connection");
            let mut tls = rustls::StreamOwned::new(connection, stream);
            let mut byte = [0_u8; 1];
            // The handshake runs inside the first read; a refused client
            // ends it with an alert.
            if tls.read_exact(&mut byte).is_ok() {
                let _ = tls.write_all(b"!");
            }
        });
        let client = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from("127.0.0.1").expect("name");
        let connection = rustls::ClientConnection::new(std::sync::Arc::new(client), name)
            .expect("client connection");
        let stream = std::net::TcpStream::connect(address).expect("connect");
        let mut tls = rustls::StreamOwned::new(connection, stream);
        let outcome = tls
            .write_all(b"?")
            .and_then(|()| tls.read_exact(&mut [0_u8; 1]))
            .map_err(|error| error.to_string());
        drop(tls);
        serving.join().expect("server thread");
        outcome
    }

    #[test]
    fn the_extra_root_lets_a_private_ca_server_through_and_only_it() {
        let platform = super::platform::roots().expect("the platform store");
        let without = build_roots(Ok(platform.clone()), None).expect("platform roots");
        let refused = handshake(without).expect_err("the test CA is no platform root");
        assert!(refused.contains("UnknownIssuer"), "{refused}");
        let with = build_roots(Ok(platform), Some(&fixture("test-ca.pem"))).expect("roots");
        handshake(with).expect("the extra root verifies the server");
    }

    #[test]
    fn no_usable_root_fails_closed() {
        assert!(build_roots(Ok(Vec::new()), None).is_err());
        assert!(build_roots(Ok(Vec::new()), Some(&fixture("test-ca.pem"))).is_ok());
        // An unreadable store is reported, not described as empty; an extra
        // file still gives a root.
        let unreadable = build_roots(Err("the keychain is locked".into()), None);
        assert!(unreadable.is_err_and(|message| message.contains("the keychain is locked")));
        assert!(build_roots(Err("locked".into()), Some(&fixture("test-ca.pem"))).is_ok());
    }

    #[test]
    fn the_platform_store_is_read_without_the_environment() {
        // The loaders take no environment input; on a host with a trust
        // store they find roots (the CI images have one).
        assert!(
            !super::platform::roots()
                .expect("the platform store")
                .is_empty(),
            "no platform root found"
        );
    }

    fn tempfile_directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("bn_web_tls_roots_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        directory
    }
}

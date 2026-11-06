use std::sync::OnceLock;

/// Builds a client configuration from the platform trust store: the system
/// keychain on macOS, the certificate store on Windows, and on other systems
/// the usual bundle files (`SSL_CERT_FILE` and `SSL_CERT_DIR` apply there).
/// The loader fails closed: with no usable root there is no configuration, and
/// no mode accepts a certificate the store does not vouch for.
pub(crate) fn client_config() -> Result<rustls::ClientConfig, String> {
    client_config_from(rustls_native_certs::load_native_certs())
}

fn client_config_from(
    native: rustls_native_certs::CertificateResult,
) -> Result<rustls::ClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    // A store may hold an entry rustls cannot parse; it is skipped, and the
    // others still count. A store with none left is no store.
    let (added, _skipped) = roots.add_parsable_certificates(native.certs);
    if added == 0 {
        return Err(match native.errors.first() {
            Some(error) => format!("no system trust store could be read: {error}"),
            None => "the system trust store holds no usable CA certificate".into(),
        });
    }
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

/// Builds a bounded Rustls server configuration from PEM text.
pub fn server_config_from_pem(
    certificate_pem: &str,
    private_key_pem: &str,
) -> Result<rustls::ServerConfig, String> {
    if certificate_pem.len() > 64 * 1024 || private_key_pem.len() > 64 * 1024 {
        return Err("TLS material exceeds 64 KiB".into());
    }
    let cert = pem_block(certificate_pem, "CERTIFICATE")?;
    let key = pem_block(private_key_pem, "PRIVATE KEY")?;
    let certs = vec![rustls::pki_types::CertificateDer::from(cert)];
    let key =
        rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(key));
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| format!("invalid TLS certificate or key: {error}"))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

fn pem_block(input: &str, label: &str) -> Result<Vec<u8>, String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let body = input
        .split_once(&begin)
        .and_then(|(_, rest)| rest.split_once(&end).map(|(body, _)| body))
        .ok_or_else(|| format!("missing PEM {label} block"))?;
    let encoded: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    decode_base64(&encoded)
}

#[allow(clippy::cast_possible_truncation)] // each extraction is limited to one byte by the bit layout.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if input.is_empty() || !input.len().is_multiple_of(4) {
        return Err("invalid PEM base64".into());
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let bytes = input.as_bytes();
    for chunk in bytes.as_chunks::<4>().0 {
        if (chunk[0..2]).contains(&b'=') || (chunk[2] == b'=' && chunk[3] != b'=') {
            return Err("invalid PEM base64".into());
        }
        let mut value = 0u32;
        let mut padding = 0;
        for &byte in chunk {
            value <<= 6;
            if byte == b'=' {
                padding += 1;
            } else {
                let digit = match byte {
                    b'A'..=b'Z' => byte - b'A',
                    b'a'..=b'z' => byte - b'a' + 26,
                    b'0'..=b'9' => byte - b'0' + 52,
                    b'+' => 62,
                    b'/' => 63,
                    _ => return Err("invalid PEM base64".into()),
                };
                value |= u32::from(digit);
            }
        }
        if padding > 2 {
            return Err("invalid PEM base64".into());
        }
        out.push((value >> 16) as u8);
        if padding < 2 {
            out.push((value >> 8) as u8);
        }
        if padding == 0 {
            out.push(value as u8);
        }
    }
    Ok(out)
}

/// Installs the approved Rustls `ring` provider exactly once. A provider
/// that rustls already auto-installed in this process (the first TLS config
/// built before any `BNWeb` call, e.g. in tests) is accepted only if it is
/// `ring`.
pub fn install_ring_provider() -> Result<(), &'static str> {
    static RESULT: OnceLock<Result<(), &'static str>> = OnceLock::new();
    *RESULT.get_or_init(|| {
        let ring = rustls::crypto::ring::default_provider();
        let suites = ring.cipher_suites.clone();
        match ring.install_default() {
            Ok(()) => Ok(()),
            Err(installed) if installed.cipher_suites == suites => Ok(()),
            Err(_) => Err("a non-ring Rustls crypto provider is already installed"),
        }
    })
}

pub(crate) fn supports_http_alpn(protocols: &[Vec<u8>]) -> bool {
    protocols
        .iter()
        .any(|protocol| protocol.as_slice() == b"h2" || protocol.as_slice() == b"http/1.1")
}

#[cfg(test)]
mod tests {
    #[test]
    fn approved_provider_installs_idempotently() {
        assert!(super::install_ring_provider().is_ok());
        assert!(super::install_ring_provider().is_ok());
    }

    #[test]
    fn http_alpn_requires_h2_or_http11() {
        assert!(super::supports_http_alpn(&[b"h2".to_vec()]));
        assert!(super::supports_http_alpn(&[b"http/1.1".to_vec()]));
        assert!(!super::supports_http_alpn(&[b"acme/1".to_vec()]));
    }

    #[test]
    fn pem_loader_rejects_missing_or_oversized_material() {
        assert!(super::server_config_from_pem("", "").is_err());
        assert!(super::server_config_from_pem(&"x".repeat(65 * 1024), "").is_err());
        assert!(super::decode_base64("T=Q=").is_err());
        assert_eq!(super::decode_base64("TQ==").unwrap(), b"M");
    }

    #[test]
    fn client_config_follows_the_platform_trust_store() {
        // A configuration exists exactly when the platform store holds a usable
        // root: on macOS and Windows the system store, elsewhere the bundle files.
        let usable = rustls_native_certs::load_native_certs()
            .certs
            .into_iter()
            .any(|der| rustls::RootCertStore::empty().add(der).is_ok());
        assert_eq!(super::client_config().is_ok(), usable);
    }

    #[test]
    fn client_config_fails_closed_without_a_usable_root() {
        let empty = rustls_native_certs::CertificateResult::default();
        assert!(super::client_config_from(empty).is_err());
        let mut garbage = rustls_native_certs::CertificateResult::default();
        garbage
            .certs
            .push(rustls::pki_types::CertificateDer::from(vec![0_u8, 1, 2, 3]));
        assert!(super::client_config_from(garbage).is_err());
    }
}

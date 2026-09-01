#![allow(dead_code)] // ponytail: TLSConfig provider is wired when StartTLS dispatch lands.
use std::sync::OnceLock;

/// Builds a bounded Rustls server configuration from PEM text.
pub(crate) fn server_config_from_pem(
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
    for chunk in bytes.chunks_exact(4) {
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

/// Installs the approved Rustls `ring` provider exactly once.
pub(crate) fn install_ring_provider() -> Result<(), &'static str> {
    static RESULT: OnceLock<Result<(), &'static str>> = OnceLock::new();
    *RESULT.get_or_init(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .map_err(|_| "a Rustls crypto provider is already installed")
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
}

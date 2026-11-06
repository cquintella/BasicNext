//! Bounded reverse DNS for HOST.Net.Reverse.

use std::{io, net::IpAddr, time::Duration};

use super::{Address, LookupPool, Refusal};

#[derive(Debug)]
pub enum ReverseError {
    Timeout,
    NotFound,
    /// This many name lookups are already in flight, the ceiling.
    Limit(usize),
    Io(io::Error),
}

impl ReverseError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Timeout => "reverse timeout".into(),
            Self::NotFound => "reverse name not found".into(),
            Self::Limit(maximum) => {
                bn_catalog::fill("net", "resolve.limit.cause", &[("maximum", maximum)])
            }
            Self::Io(error) => error.to_string(),
        }
    }
}

/// Reverse-resolves `address` with a bounded wait.
///
/// # Errors
///
/// Returns timeout, not-found, or resolver I/O failures.
pub fn reverse_timeout(address: Address, timeout: Duration) -> Result<String, ReverseError> {
    let ip = address.as_std();
    match LookupPool::global().run(timeout, move || lookup(ip)) {
        Ok(Some(answer)) => answer,
        Ok(None) => Err(ReverseError::Timeout),
        Err(Refusal::Full { in_flight_max }) => Err(ReverseError::Limit(in_flight_max)),
        Err(Refusal::Lost) => Err(ReverseError::Io(io::Error::other(bn_catalog::text(
            "net",
            "resolve.worker_lost.cause",
        )))),
    }
}

fn lookup(ip: IpAddr) -> Result<String, ReverseError> {
    match dns_lookup::lookup_addr(&ip) {
        Ok(name) if !name.is_empty() => Ok(name),
        Ok(_) => Err(ReverseError::NotFound),
        Err(error) => {
            if error.kind() == io::ErrorKind::NotFound {
                Err(ReverseError::NotFound)
            } else {
                Err(ReverseError::Io(error))
            }
        }
    }
}

//! Bounded reverse DNS for HOST.Net.Reverse.

use std::{io, net::IpAddr, time::Duration};

use super::Address;

#[derive(Debug)]
pub enum ReverseError {
    Timeout,
    NotFound,
    Io(io::Error),
}

impl ReverseError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Timeout => "reverse timeout".into(),
            Self::NotFound => "reverse name not found".into(),
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
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let task = std::thread::spawn(move || {
        let _ = sender.send(lookup(ip));
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(name)) => {
            let _ = task.join();
            Ok(name)
        }
        Ok(Err(error)) => {
            let _ = task.join();
            Err(error)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            super::retain_resolver_task(task);
            Err(ReverseError::Timeout)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            let _ = task.join();
            Err(ReverseError::Io(io::Error::other(
                "reverse resolver stopped",
            )))
        }
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

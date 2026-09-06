//! Read-only ARP/NDP neighbor lookup for HOST.Net.Neighbor.

use super::Address;
use std::net::IpAddr;
use std::process::Command;

#[derive(Debug)]
pub enum NeighborError {
    Unsupported,
    NotFound,
}

impl NeighborError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Unsupported => "direct-neighbor lookup unsupported on this host".into(),
            Self::NotFound => "direct-neighbor entry not found".into(),
        }
    }
}

/// Looks up an existing OS ARP/NDP entry for `address` without mutating it.
///
/// # Errors
///
/// Returns `NotFound` when the platform table has no complete entry. The
/// returned address is the queried address; the language API exposes address
/// identity, not the link-layer address.
pub fn neighbor(address: Address) -> Result<Address, NeighborError> {
    let ip = address.as_std();
    if ip.is_loopback() {
        return Ok(address);
    }
    let present = match ip {
        IpAddr::V4(ip) => arp_entry_present(&ip.to_string()),
        IpAddr::V6(ip) => ndp_entry_present(&ip.to_string()),
    };
    if present {
        Ok(address)
    } else {
        Err(NeighborError::NotFound)
    }
}

fn arp_entry_present(address: &str) -> bool {
    #[cfg(target_os = "linux")]
    if let Ok(table) = std::fs::read_to_string("/proc/net/arp") {
        return table.lines().skip(1).any(|line| {
            let mut fields = line.split_whitespace();
            fields.next() == Some(address)
                && fields.nth(2).is_some_and(|mac| mac != "00:00:00:00:00:00")
        });
    }

    command_entry_present("arp", &["-n", address], address)
}

fn ndp_entry_present(address: &str) -> bool {
    command_entry_present("ndp", &["-an"], address)
}

fn command_entry_present(program: &str, arguments: &[&str], address: &str) -> bool {
    let Ok(output) = Command::new(program).args(arguments).output() else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| complete_neighbor_line(line, address))
}

fn complete_neighbor_line(line: &str, address: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains(&format!("({address})").to_ascii_lowercase())
        && normalized.contains(" at ")
        && !normalized.contains("incomplete")
        && !normalized.contains("(incomplete)")
}

#[cfg(test)]
mod tests {
    use super::complete_neighbor_line;

    #[test]
    fn parses_complete_arp_and_ndp_lines_only() {
        assert!(complete_neighbor_line(
            "? (192.0.2.1) at aa:bb:cc:dd:ee:ff on en0",
            "192.0.2.1"
        ));
        assert!(complete_neighbor_line(
            "fe80::1 (fe80::1) at aa:bb:cc:dd:ee:ff on en0",
            "fe80::1"
        ));
        assert!(!complete_neighbor_line(
            "? (192.0.2.1) at (incomplete) on en0",
            "192.0.2.1"
        ));
        assert!(!complete_neighbor_line(
            "? (192.0.2.2) at aa:bb:cc:dd:ee:ff on en0",
            "192.0.2.1"
        ));
    }
}

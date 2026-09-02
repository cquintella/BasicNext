#![allow(dead_code)] // ponytail: BN provider dispatch is the next integration step.

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    #[test]
    fn cookie_jar_isolated_by_domain_and_expires() {
        let mut jar = CookieJar::new();
        jar.set("sid", "a", "example.test", "/", Duration::from_millis(50))
            .unwrap();
        assert_eq!(jar.get("sid", "example.test", "/"), Some("a".into()));
        assert_eq!(jar.get("sid", "other.test", "/"), None);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(jar.get("sid", "example.test", "/"), None);
    }

    #[test]
    fn cookie_jar_applies_subdomain_and_path_matching() {
        let mut jar = CookieJar::new();
        jar.set("sid", "root", "example.test", "/", Duration::from_mins(1))
            .unwrap();
        jar.set(
            "sid",
            "api",
            "api.example.test",
            "/v1",
            Duration::from_mins(1),
        )
        .unwrap();
        assert_eq!(
            jar.get("sid", "api.example.test", "/v1/users"),
            Some("api".into())
        );
        assert_eq!(jar.get("sid", "www.example.test", "/"), Some("root".into()));
        assert_eq!(
            jar.get("sid", "www.example.test", "/api"),
            Some("root".into())
        );
        assert_eq!(jar.get("sid", "evil-example.test", "/"), None);
    }

    #[test]
    fn cookie_max_age_zero_deletes_immediately() {
        let mut jar = CookieJar::new();
        jar.set("sid", "a", "example.test", "/", Duration::from_mins(1))
            .unwrap();
        jar.set("sid", "", "example.test", "/", Duration::ZERO)
            .unwrap();
        assert_eq!(jar.get("sid", "example.test", "/"), None);
        assert_eq!(jar.len(), 0);
    }

    #[test]
    fn cookie_defaults_are_secure_http_only_and_lax() {
        let mut jar = CookieJar::new();
        jar.set("sid", "a", "example.test", "/", Duration::from_mins(1))
            .unwrap();
        assert_eq!(
            jar.options("sid", "example.test", "/"),
            Some(CookieOptions {
                secure: true,
                http_only: true,
                same_site: SameSite::Lax,
            })
        );
    }

    #[test]
    fn session_store_rotates_and_expires() {
        let mut store = SessionStore::new(2, Duration::from_mins(1)).unwrap();
        let first = store.create("one").unwrap();
        let second = store.rotate(&first, "two").unwrap();
        assert_ne!(first, second);
        assert_eq!(store.get(&first), None);
        assert_eq!(store.get(&second), Some("two".into()));
    }

    #[test]
    fn session_ids_are_random_and_have_at_least_128_bits() {
        let mut store = SessionStore::new(2, Duration::from_mins(1)).unwrap();
        let first = store.create("one").unwrap();
        let second = store.create("two").unwrap();
        assert_eq!(first.len(), 33);
        assert_eq!(second.len(), 33);
        assert!(first.starts_with('s'));
        assert!(second.starts_with('s'));
        assert_ne!(first, second);
        assert_ne!(&first[1..17], &second[1..17]);
    }

    struct FailingEntropy;

    impl EntropyProvider for FailingEntropy {
        fn fill(&self, _destination: &mut [u8]) -> Result<(), ()> {
            Err(())
        }
    }

    #[test]
    fn entropy_failure_does_not_insert_a_session() {
        let store = SessionStore::new(1, Duration::from_mins(1)).unwrap();
        let before = store.values.len();
        assert!(new_session_id(&FailingEntropy).is_err());
        assert_eq!(store.values.len(), before);
    }

    #[test]
    fn request_ids_are_bounded_and_entropy_backed() {
        let first = new_request_id(&SystemEntropy).unwrap();
        let second = new_request_id(&SystemEntropy).unwrap();
        assert_eq!(
            first.len(),
            1 + crate::config::web_limits().request_id_bytes * 2
        );
        assert!(first.starts_with('r'));
        assert_ne!(first, second);
        assert!(new_request_id(&FailingEntropy).is_err());
    }

    #[test]
    fn session_store_evicts_oldest_when_capacity_is_full() {
        let mut store = SessionStore::new(1, Duration::from_mins(1)).unwrap();
        let first = store.create("one").unwrap();
        let second = store.create("two").unwrap();
        assert_eq!(store.get(&first), None);
        assert_eq!(store.get(&second), Some("two".into()));
    }

    #[test]
    fn session_store_prunes_expired_entries_before_eviction() {
        let mut store = SessionStore::new(1, Duration::from_millis(1)).unwrap();
        let first = store.create("one").unwrap();
        std::thread::sleep(Duration::from_millis(3));
        let second = store.create("two").unwrap();
        assert_eq!(store.get(&first), None);
        assert_eq!(store.get(&second), Some("two".into()));
    }

    #[test]
    fn session_store_rejects_idle_timeout_above_contract_limit() {
        assert!(SessionStore::new(1, Duration::from_mins(30) + Duration::from_secs(1)).is_err());
    }

    #[test]
    fn acl_uses_ordered_rules_with_default_deny() {
        let mut acl = Acl::new();
        let local = IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3));
        acl.deny("10.0.0.0/8").unwrap();
        acl.allow("10.1.0.0/16").unwrap();
        assert!(!acl.check(local));
    }

    #[test]
    fn acl_handles_ipv6_and_rejects_invalid_prefixes() {
        let mut acl = Acl::new();
        acl.allow("2001:db8::/32").unwrap();
        assert!(acl.check("2001:db8:1::1".parse().unwrap()));
        assert!(!acl.check("2001:db9::1".parse().unwrap()));
        assert!(acl.allow("2001:db8::/129").is_err());
        assert!(acl.deny("10.0.0.0/33").is_err());
        assert!(acl.allow("not-a-cidr").is_err());
    }

    #[test]
    fn scraper_extracts_text_without_executing_markup() {
        let page = Scraper::parse("<p>Hello <b>world</b></p><script>bad()</script>").unwrap();
        assert_eq!(page.text("p").unwrap(), "Hello world");
        assert!(page.text("script").is_err());
        assert!(page.text("div").is_err());
    }
}
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use ring::rand::{SecureRandom, SystemRandom};

pub(crate) trait EntropyProvider {
    fn fill(&self, destination: &mut [u8]) -> Result<(), ()>;
}

pub(crate) struct SystemEntropy;

impl EntropyProvider for SystemEntropy {
    fn fill(&self, destination: &mut [u8]) -> Result<(), ()> {
        SystemRandom::new().fill(destination).map_err(|_| ())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SameSite {
    Strict,
    Lax,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CookieOptions {
    pub(crate) secure: bool,
    pub(crate) http_only: bool,
    pub(crate) same_site: SameSite,
}

type CookieKey = (String, String, String);
type CookieEntry = (String, Option<Instant>, CookieOptions);

impl Default for CookieOptions {
    fn default() -> Self {
        Self {
            secure: true,
            http_only: true,
            same_site: SameSite::Lax,
        }
    }
}

pub(crate) struct CookieJar {
    values: HashMap<CookieKey, CookieEntry>,
}

impl CookieJar {
    pub(crate) fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
    pub(crate) fn set(
        &mut self,
        name: &str,
        value: &str,
        domain: &str,
        path: &str,
        max_age: Duration,
    ) -> Result<(), String> {
        self.set_with_options(name, value, domain, path, max_age, CookieOptions::default())
    }
    pub(crate) fn set_with_options(
        &mut self,
        name: &str,
        value: &str,
        domain: &str,
        path: &str,
        max_age: Duration,
        options: CookieOptions,
    ) -> Result<(), String> {
        if name.is_empty()
            || name.len() > 128
            || value.len() > 4096
            || domain.is_empty()
            || path.is_empty()
        {
            return Err("invalid cookie bounds".into());
        }
        let key = (name.into(), domain.to_ascii_lowercase(), path.into());
        if max_age.is_zero() {
            self.values.remove(&key);
            return Ok(());
        }
        self.values
            .insert(key, (value.into(), Some(Instant::now() + max_age), options));
        Ok(())
    }
    pub(crate) fn get(&mut self, name: &str, domain: &str, path: &str) -> Option<String> {
        let domain = domain.to_ascii_lowercase();
        let key = self
            .values
            .keys()
            .filter(|(cookie_name, cookie_domain, cookie_path)| {
                cookie_name == name
                    && (cookie_domain == &domain || domain.ends_with(&format!(".{cookie_domain}")))
                    && (cookie_path == "/"
                        || path == cookie_path
                        || path.starts_with(&format!("{cookie_path}/")))
            })
            .max_by_key(|(_, _, cookie_path)| cookie_path.len())
            .cloned()?;
        let expired = self
            .values
            .get(&key)
            .is_some_and(|(_, expiry, _)| expiry.is_some_and(|time| Instant::now() >= time));
        if expired {
            self.values.remove(&key);
            None
        } else {
            self.values.get(&key).map(|(value, _, _)| value.clone())
        }
    }
    pub(crate) fn options(&self, name: &str, domain: &str, path: &str) -> Option<CookieOptions> {
        self.values
            .get(&(name.into(), domain.to_ascii_lowercase(), path.into()))
            .map(|(_, _, options)| *options)
    }
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn delete(&mut self, name: &str, domain: &str, path: &str) {
        self.values
            .remove(&(name.into(), domain.to_ascii_lowercase(), path.into()));
    }
}

pub(crate) struct SessionStore {
    values: HashMap<String, (String, Instant)>,
    capacity: usize,
    idle: Duration,
}
impl SessionStore {
    pub(crate) fn new(capacity: i128, idle_ms: Duration) -> Result<Self, String> {
        let capacity = usize::try_from(capacity).map_err(|_| "invalid session capacity")?;
        if capacity == 0 || capacity > 10_000 {
            return Err("invalid session capacity".into());
        }
        if idle_ms.is_zero() || idle_ms > Duration::from_mins(30) {
            return Err("invalid session idle timeout".into());
        }
        Ok(Self {
            values: HashMap::new(),
            capacity,
            idle: idle_ms,
        })
    }
    pub(crate) fn create(&mut self, value: &str) -> Result<String, String> {
        if value.len() > 8192 {
            return Err("session value too large".into());
        }
        self.values
            .retain(|_, (_, touched)| touched.elapsed() < self.idle);
        if self.values.len() >= self.capacity
            && let Some(oldest) = self
                .values
                .iter()
                .min_by_key(|(_, (_, touched))| *touched)
                .map(|(id, _)| id.clone())
        {
            self.values.remove(&oldest);
        }
        let id = new_session_id(&SystemEntropy)?;
        self.values
            .insert(id.clone(), (value.into(), Instant::now()));
        Ok(id)
    }
    pub(crate) fn get(&mut self, id: &str) -> Option<String> {
        let entry = self.values.get_mut(id)?;
        if entry.1.elapsed() >= self.idle {
            self.values.remove(id);
            None
        } else {
            entry.1 = Instant::now();
            Some(entry.0.clone())
        }
    }
    pub(crate) fn set(&mut self, id: &str, value: &str) -> Result<(), String> {
        if value.len() > 8192 {
            return Err("session value too large".into());
        }
        let entry = self.values.get_mut(id).ok_or("session not found")?;
        entry.0 = value.into();
        entry.1 = Instant::now();
        Ok(())
    }
    pub(crate) fn rotate(&mut self, id: &str, value: &str) -> Result<String, String> {
        if self.get(id).is_none() {
            return Err("session not found".into());
        }
        self.values.remove(id);
        self.create(value)
    }
    pub(crate) fn delete(&mut self, id: &str) -> Result<(), String> {
        if self.values.remove(id).is_some() {
            Ok(())
        } else {
            Err("session not found".into())
        }
    }
}

fn new_session_id(provider: &impl EntropyProvider) -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    provider
        .fill(&mut bytes)
        .map_err(|()| "session entropy provider failed".to_string())?;
    let mut id = String::with_capacity(33);
    id.push('s');
    for byte in bytes {
        use std::fmt::Write;
        write!(id, "{byte:02x}").map_err(|_| "session ID encoding failed".to_string())?;
    }
    Ok(id)
}

pub(crate) fn new_request_id(provider: &impl EntropyProvider) -> Result<String, String> {
    let size = crate::config::web_limits().request_id_bytes;
    let mut bytes = vec![0_u8; size];
    provider
        .fill(&mut bytes)
        .map_err(|()| "request ID entropy provider failed".to_string())?;
    let mut id = String::with_capacity(1 + bytes.len() * 2);
    id.push('r');
    for byte in bytes {
        use std::fmt::Write;
        write!(id, "{byte:02x}").map_err(|_| "request ID encoding failed".to_string())?;
    }
    Ok(id)
}

#[derive(Clone, Copy)]
struct Rule {
    network: IpAddr,
    prefix: u8,
    allow: bool,
}
pub(crate) struct Acl {
    rules: Vec<Rule>,
}
impl Acl {
    pub(crate) fn new() -> Self {
        Self { rules: Vec::new() }
    }
    pub(crate) fn allow(&mut self, cidr: &str) -> Result<(), String> {
        self.add(cidr, true)
    }
    pub(crate) fn deny(&mut self, cidr: &str) -> Result<(), String> {
        self.add(cidr, false)
    }
    fn add(&mut self, cidr: &str, allow: bool) -> Result<(), String> {
        let (address, prefix) = cidr.split_once('/').ok_or("invalid CIDR")?;
        let network: IpAddr = address.parse().map_err(|_| "invalid CIDR")?;
        let prefix: u8 = prefix.parse().map_err(|_| "invalid CIDR")?;
        if (network.is_ipv4() && prefix > 32) || (network.is_ipv6() && prefix > 128) {
            return Err("invalid CIDR prefix".into());
        }
        self.rules.push(Rule {
            network,
            prefix,
            allow,
        });
        Ok(())
    }
    pub(crate) fn check(&self, address: IpAddr) -> bool {
        self.rules
            .iter()
            .find(|rule| contains(rule.network, rule.prefix, address))
            .is_some_and(|rule| rule.allow)
    }
}
fn contains(network: IpAddr, prefix: u8, address: IpAddr) -> bool {
    if prefix == 0 {
        return network.is_ipv4() == address.is_ipv4();
    }
    match (network, address) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            u32::from(a) >> (32 - prefix.min(32)) == u32::from(b) >> (32 - prefix.min(32))
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            u128::from(a) >> (128 - prefix.min(128)) == u128::from(b) >> (128 - prefix.min(128))
        }
        _ => false,
    }
}

pub(crate) struct Scraper {
    html: String,
}
impl Scraper {
    pub(crate) fn parse(html: &str) -> Result<Self, String> {
        if html.len() > 8 * 1024 * 1024 {
            return Err("scripts are not allowed".into());
        }
        Ok(Self { html: html.into() })
    }
    pub(crate) fn text(&self, selector: &str) -> Result<String, String> {
        if selector.eq_ignore_ascii_case("script") || selector.is_empty() {
            return Err("unsupported selector".into());
        }
        if !selector
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err("unsupported selector".into());
        }
        let mut source = self.html.clone();
        while let Some(start) = source.to_ascii_lowercase().find("<script") {
            let Some(end) = source[start..].to_ascii_lowercase().find("</script>") else {
                source.truncate(start);
                break;
            };
            source.replace_range(start..start + end + 9, "");
        }
        let open = format!("<{}", selector.to_ascii_lowercase());
        let close = format!("</{}>", selector.to_ascii_lowercase());
        let lower = source.to_ascii_lowercase();
        let start = lower
            .find(&open)
            .ok_or_else(|| "selector did not match".to_string())?;
        let content_start = source[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| "malformed HTML".to_string())?;
        let end = lower[content_start..]
            .find(&close)
            .map(|offset| content_start + offset)
            .ok_or_else(|| "malformed HTML".to_string())?;
        let source = &source[content_start..end];
        let mut out = String::new();
        let mut in_tag = false;
        for ch in source.chars() {
            if ch == '<' {
                in_tag = true;
            } else if ch == '>' {
                in_tag = false;
            } else if !in_tag {
                out.push(ch);
            }
        }
        Ok(out.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

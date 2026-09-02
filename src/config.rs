use std::sync::OnceLock;

const LIMITS_REGISTRY: &str = include_str!("../config/0.4-bnweb-limits.toml");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WebLimits {
    pub active_connections: usize,
    pub active_connections_max: usize,
    pub backlog: usize,
    pub backlog_max: usize,
    pub pending_work: usize,
    pub pending_work_max: usize,
    pub worker_count: usize,
    pub worker_count_max: usize,
    pub socket_handles_max: usize,
    pub max_header_bytes: usize,
    pub max_header_fields: usize,
    pub max_target_bytes: usize,
    pub max_body_bytes: usize,
    pub max_response_body_bytes: usize,
    pub tls_handshake_ms: u64,
    pub tls_handshake_max_ms: u64,
    pub header_read_ms: u64,
    pub header_read_max_ms: u64,
    pub body_read_ms: u64,
    pub body_read_max_ms: u64,
    pub idle_keep_alive_ms: u64,
    pub idle_keep_alive_max_ms: u64,
    pub connection_total_ms: u64,
    pub connection_total_max_ms: u64,
    pub stop_drain_ms: u64,
    pub stop_drain_max_ms: u64,
    pub resolved_addresses_max: usize,
    pub datagram_max_bytes: usize,
    pub redirects: usize,
    pub redirects_max: usize,
    pub egress_list_max: usize,
    pub trusted_proxy: bool,
    pub rate_limit_key_capacity: usize,
    pub rate_limit_burst: usize,
    pub rate_limit_burst_max: usize,
    pub rate_limit_refill_per_second: usize,
    pub rate_limit_refill_per_second_max: usize,
    pub request_id_bytes: usize,
    pub session_id_min_bytes: usize,
    pub async_output_max_bytes: usize,
    pub concurrent_handlers: bool,
    pub dispatch: DispatchLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DispatchLimits {
    pub worker_count: usize,
    pub worker_count_max: usize,
    pub pending_tickets: usize,
    pub pending_tickets_max: usize,
    pub timeout_min_ms: i128,
    pub timeout_max_ms: i128,
    pub output_max_bytes: usize,
}

pub(crate) fn web_limits() -> &'static WebLimits {
    static LIMITS: OnceLock<WebLimits> = OnceLock::new();
    LIMITS.get_or_init(|| parse_registry(LIMITS_REGISTRY).expect("invalid embedded 0.4 limits"))
}

pub(crate) fn dispatch_limits() -> &'static DispatchLimits {
    &web_limits().dispatch
}

fn parse_registry(text: &str) -> Result<WebLimits, String> {
    let get = |section: &str, key: &str| -> Result<u64, String> {
        let mut current = "";
        for raw_line in text.lines() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current = &line[1..line.len() - 1];
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                return Err(format!("invalid registry line: {line}"));
            };
            if current == section && name.trim() == key {
                return value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| format!("invalid integer for {section}.{key}"));
            }
        }
        Err(format!("missing registry value {section}.{key}"))
    };
    let usize_value = |section: &str, key: &str| {
        usize::try_from(get(section, key)?).map_err(|_| format!("value too large: {section}.{key}"))
    };
    let dispatch = DispatchLimits {
        worker_count: usize_value("dispatch", "worker_count_default")?,
        worker_count_max: usize_value("dispatch", "worker_count_max")?,
        pending_tickets: usize_value("dispatch", "pending_tickets_default")?,
        pending_tickets_max: usize_value("dispatch", "pending_tickets_max")?,
        timeout_min_ms: i128::from(get("dispatch", "timeout_min_ms")?),
        timeout_max_ms: i128::from(get("dispatch", "timeout_max_ms")?),
        output_max_bytes: usize_value("dispatch", "output_max_bytes")?,
    };
    let limits = WebLimits {
        active_connections: usize_value("connections", "active_default")?,
        active_connections_max: usize_value("connections", "active_max")?,
        backlog: usize_value("connections", "backlog_default")?,
        backlog_max: usize_value("connections", "backlog_max")?,
        pending_work: usize_value("connections", "pending_work_default")?,
        pending_work_max: usize_value("connections", "pending_work_max")?,
        worker_count: usize_value("connections", "worker_count_default")?,
        worker_count_max: usize_value("connections", "worker_count_max")?,
        socket_handles_max: usize_value("connections", "socket_handles_max")?,
        max_header_bytes: usize_value("http", "max_header_bytes_default")?,
        max_header_fields: usize_value("http", "max_header_fields_default")?,
        max_target_bytes: usize_value("http", "max_target_bytes_default")?,
        max_body_bytes: usize_value("http", "max_body_bytes_default")?,
        max_response_body_bytes: usize_value("http", "max_response_body_bytes_default")?,
        tls_handshake_ms: get("timeouts_ms", "tls_handshake_default")?,
        tls_handshake_max_ms: get("timeouts_ms", "tls_handshake_max")?,
        header_read_ms: get("timeouts_ms", "header_read_default")?,
        header_read_max_ms: get("timeouts_ms", "header_read_max")?,
        body_read_ms: get("timeouts_ms", "body_read_default")?,
        body_read_max_ms: get("timeouts_ms", "body_read_max")?,
        idle_keep_alive_ms: get("timeouts_ms", "idle_keep_alive_default")?,
        idle_keep_alive_max_ms: get("timeouts_ms", "idle_keep_alive_max")?,
        connection_total_ms: get("timeouts_ms", "connection_total_default")?,
        connection_total_max_ms: get("timeouts_ms", "connection_total_max")?,
        stop_drain_ms: get("timeouts_ms", "stop_drain_default")?,
        stop_drain_max_ms: get("timeouts_ms", "stop_drain_max")?,
        resolved_addresses_max: usize_value("client", "resolved_addresses_max")?,
        datagram_max_bytes: usize_value("client", "datagram_max_bytes")?,
        redirects: usize_value("client", "redirects_default")?,
        redirects_max: usize_value("client", "redirects_max")?,
        egress_list_max: usize_value("client", "egress_list_max")?,
        trusted_proxy: get("client", "trusted_proxy_default")? != 0,
        rate_limit_key_capacity: usize_value("rate_limit", "key_capacity_default")?,
        rate_limit_burst: usize_value("rate_limit", "burst_default")?,
        rate_limit_burst_max: usize_value("rate_limit", "burst_max")?,
        rate_limit_refill_per_second: usize_value("rate_limit", "refill_per_second_default")?,
        rate_limit_refill_per_second_max: usize_value("rate_limit", "refill_per_second_max")?,
        request_id_bytes: usize_value("identity", "request_id_bytes")?,
        session_id_min_bytes: usize_value("identity", "session_id_min_bytes")?,
        async_output_max_bytes: dispatch.output_max_bytes,
        concurrent_handlers: get("dispatch", "concurrent_handlers_default")? != 0,
        dispatch,
    };
    if limits.active_connections > limits.active_connections_max
        || limits.backlog > limits.backlog_max
        || limits.pending_work > limits.pending_work_max
        || limits.worker_count > limits.worker_count_max
        || limits.socket_handles_max == 0
        || limits.datagram_max_bytes == 0
        || limits.max_body_bytes > limits.max_response_body_bytes
        || limits.redirects > limits.redirects_max
        || dispatch.worker_count > dispatch.worker_count_max
        || dispatch.pending_tickets > dispatch.pending_tickets_max
        || dispatch.timeout_min_ms < 1
        || dispatch.timeout_min_ms > dispatch.timeout_max_ms
        || dispatch.timeout_max_ms > 60_000
    {
        return Err("registry default exceeds maximum".into());
    }
    Ok(limits)
}

#[cfg(test)]
mod tests {
    use super::{LIMITS_REGISTRY, parse_registry, web_limits};

    #[test]
    fn embedded_registry_has_accepted_defaults() {
        let limits = web_limits();
        assert_eq!(limits.active_connections, 128);
        assert_eq!(limits.active_connections_max, 256);
        assert_eq!(limits.backlog, 128);
        assert_eq!(limits.backlog_max, 128);
        assert_eq!(limits.max_body_bytes, 8 * 1024 * 1024);
        assert_eq!(limits.session_id_min_bytes, 16);
        assert_eq!(limits.dispatch.worker_count, 8);
        assert_eq!(limits.dispatch.worker_count_max, 64);
        assert_eq!(limits.dispatch.pending_tickets_max, 1_024);
        assert_eq!(limits.dispatch.timeout_max_ms, 60_000);
        assert_eq!(limits.socket_handles_max, 256);
        assert_eq!(limits.datagram_max_bytes, 65_536);
    }

    #[test]
    fn registry_rejects_default_above_maximum() {
        let invalid = LIMITS_REGISTRY.replace("active_default = 128", "active_default = 257");
        assert!(parse_registry(&invalid).is_err());
    }
}

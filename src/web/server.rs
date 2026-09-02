use super::{Route, RouteOutcome, dispatch_route, valid_method, valid_route_pattern};

#[derive(Debug)]
struct Bucket {
    tokens: usize,
    /// Last instant accounted for refill; fractional elapsed time remains
    /// available for the next calculation.
    updated: std::time::Instant,
}

#[derive(Debug)]
struct RateLimiter {
    burst: usize,
    refill_per_second: usize,
    max_keys: usize,
    buckets: std::collections::HashMap<String, Bucket>,
}

impl RateLimiter {
    fn new(options: &ServerOptions) -> Self {
        Self {
            burst: options.rate_limit_burst,
            refill_per_second: options.rate_limit_refill_per_second,
            max_keys: options.rate_limit_key_capacity,
            buckets: std::collections::HashMap::new(),
        }
    }

    fn allow(&mut self, key: &str, now: std::time::Instant) -> u64 {
        if let Some(bucket) = self.buckets.get_mut(key) {
            let elapsed_ms = now.saturating_duration_since(bucket.updated).as_millis();
            let refill =
                usize::try_from(elapsed_ms.saturating_mul(self.refill_per_second as u128) / 1_000)
                    .unwrap_or(usize::MAX);
            bucket.tokens = bucket.tokens.saturating_add(refill).min(self.burst);
            if refill > 0 {
                let converted_ms = refill as u128 * 1_000 / self.refill_per_second as u128;
                let converted = std::time::Duration::from_millis(
                    u64::try_from(converted_ms).unwrap_or(u64::MAX),
                );
                bucket.updated = bucket.updated.checked_add(converted).unwrap_or(now);
            }
            if bucket.tokens > 0 {
                bucket.tokens -= 1;
                return 0;
            }
            return 1;
        }
        if self.buckets.len() >= self.max_keys {
            let victim = self
                .buckets
                .iter()
                .min_by(|(left_key, left), (right_key, right)| {
                    left.updated
                        .cmp(&right.updated)
                        .then_with(|| left_key.cmp(right_key))
                })
                .map(|(key, _)| key.clone());
            if let Some(victim) = victim {
                self.buckets.remove(&victim);
            }
        }
        self.buckets.insert(
            key.to_owned(),
            Bucket {
                tokens: self.burst.saturating_sub(1),
                updated: now,
            },
        );
        0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServerOptions {
    pub(crate) active_connections: usize,
    pub(crate) backlog: usize,
    pub(crate) pending_work: usize,
    pub(crate) worker_count: usize,
    pub(crate) max_header_bytes: usize,
    pub(crate) max_header_fields: usize,
    pub(crate) max_target_bytes: usize,
    pub(crate) max_body_bytes: usize,
    pub(crate) trusted_proxy: bool,
    pub(crate) tls_handshake_ms: u64,
    pub(crate) header_read_ms: u64,
    pub(crate) body_read_ms: u64,
    pub(crate) idle_keep_alive_ms: u64,
    pub(crate) connection_total_ms: u64,
    pub(crate) stop_drain_ms: u64,
    pub(crate) rate_limit_burst: usize,
    pub(crate) rate_limit_refill_per_second: usize,
    pub(crate) rate_limit_key_capacity: usize,
    pub(crate) concurrent_handlers: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServerStatus {
    Starting,
    Accepting,
    Draining,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ServerStats {
    pub(crate) accepted: u64,
    pub(crate) active: u64,
    pub(crate) rejected: u64,
    pub(crate) timed_out: u64,
    pub(crate) completed: u64,
    pub(crate) failed: u64,
    pub(crate) rate_limited: u64,
    pub(crate) duration_total_ms: u64,
    pub(crate) duration_max_ms: u64,
}

impl ServerStats {
    fn increment(value: &mut u64) {
        *value = value.saturating_add(1);
    }
}

impl Default for ServerOptions {
    fn default() -> Self {
        let limits = crate::config::web_limits();
        Self {
            active_connections: limits.active_connections,
            backlog: limits.backlog,
            pending_work: limits.pending_work,
            worker_count: limits.worker_count,
            max_header_bytes: limits.max_header_bytes,
            max_header_fields: limits.max_header_fields,
            max_target_bytes: limits.max_target_bytes,
            max_body_bytes: limits.max_body_bytes,
            trusted_proxy: limits.trusted_proxy,
            tls_handshake_ms: limits.tls_handshake_ms,
            header_read_ms: limits.header_read_ms,
            body_read_ms: limits.body_read_ms,
            idle_keep_alive_ms: limits.idle_keep_alive_ms,
            connection_total_ms: limits.connection_total_ms,
            stop_drain_ms: limits.stop_drain_ms,
            rate_limit_burst: limits.rate_limit_burst,
            rate_limit_refill_per_second: limits.rate_limit_refill_per_second,
            rate_limit_key_capacity: limits.rate_limit_key_capacity,
            concurrent_handlers: limits.concurrent_handlers,
        }
    }
}

impl ServerOptions {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let limits = crate::config::web_limits();
        if self.active_connections == 0
            || self.active_connections > limits.active_connections_max
            || self.backlog == 0
            || self.backlog > limits.backlog_max
            || self.pending_work == 0
            || self.pending_work > limits.pending_work_max
            || self.worker_count == 0
            || self.worker_count > limits.worker_count_max
            || self.max_header_bytes == 0
            || self.max_header_bytes > limits.max_header_bytes
            || self.max_header_fields == 0
            || self.max_header_fields > limits.max_header_fields
            || self.max_target_bytes == 0
            || self.max_target_bytes > limits.max_target_bytes
            || self.max_body_bytes == 0
            || self.max_body_bytes > limits.max_body_bytes
            || self.max_body_bytes > limits.max_response_body_bytes
            || self.tls_handshake_ms == 0
            || self.tls_handshake_ms > limits.tls_handshake_max_ms
            || self.header_read_ms == 0
            || self.header_read_ms > limits.header_read_max_ms
            || self.body_read_ms == 0
            || self.body_read_ms > limits.body_read_max_ms
            || self.idle_keep_alive_ms == 0
            || self.idle_keep_alive_ms > limits.idle_keep_alive_max_ms
            || self.connection_total_ms == 0
            || self.connection_total_ms > limits.connection_total_max_ms
            || self.stop_drain_ms == 0
            || self.stop_drain_ms > limits.stop_drain_max_ms
            || self.rate_limit_burst == 0
            || self.rate_limit_burst > limits.rate_limit_burst_max
            || self.rate_limit_refill_per_second == 0
            || self.rate_limit_refill_per_second > limits.rate_limit_refill_per_second_max
            || self.rate_limit_key_capacity == 0
            || self.rate_limit_key_capacity > limits.rate_limit_key_capacity
        {
            return Err("server options are outside the configured bounds");
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ServerState {
    routes: Vec<Route>,
    started: bool,
    stopping: bool,
    closed: bool,
    failed: Option<()>,
    pending: usize,
    active_connections: usize,
    active_sockets: Vec<crate::net::TcpStream>,
    listener: Option<std::thread::JoinHandle<()>>,
    connection_workers: Vec<std::thread::JoinHandle<()>>,
    work_sender: Option<std::sync::mpsc::SyncSender<ConnectionWork>>,
    http_runtime: Option<std::sync::Arc<tokio::runtime::Runtime>>,
    handler_slots: Option<std::sync::Arc<tokio::sync::Semaphore>>,
    active_handler_tasks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    options: ServerOptions,
    rate_limiter: RateLimiter,
    stats: ServerStats,
}

pub(crate) type ConnectionWork = Box<dyn FnOnce() + Send + 'static>;

impl ServerState {
    pub(crate) fn new() -> Self {
        Self {
            routes: Vec::new(),
            started: false,
            stopping: false,
            closed: false,
            failed: None,
            pending: 0,
            active_connections: 0,
            active_sockets: Vec::new(),
            listener: None,
            connection_workers: Vec::new(),
            work_sender: None,
            http_runtime: None,
            handler_slots: None,
            active_handler_tasks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            options: ServerOptions::default(),
            rate_limiter: RateLimiter::new(&ServerOptions::default()),
            stats: ServerStats::default(),
        }
    }
    pub(crate) fn add_route(
        &mut self,
        method: String,
        pattern: String,
    ) -> Result<(), &'static str> {
        if self.closed || self.stopping {
            return Err("server is stopping or closed");
        }
        if !valid_method(&method) || !valid_route_pattern(&pattern) {
            return Err("invalid route");
        }
        self.routes.push(Route {
            method,
            pattern,
            order: self.routes.len(),
        });
        Ok(())
    }
    pub(crate) fn matched_route_pattern(&self, method: &str, path: &str) -> Option<String> {
        super::route_for_request(&self.routes, method, path)
            .map(|(route, _)| route.pattern().to_owned())
    }
    pub(crate) fn start(&mut self) -> Result<(), &'static str> {
        self.start_with_options(ServerOptions::default())
    }
    pub(crate) fn start_with_options(
        &mut self,
        options: ServerOptions,
    ) -> Result<(), &'static str> {
        if self.closed || self.stopping {
            return Err("server is stopping or closed");
        }
        if self.started {
            return Err("server is already started");
        }
        options.validate()?;
        self.options = options;
        self.rate_limiter = RateLimiter::new(&self.options);
        self.started = true;
        Ok(())
    }
    pub(crate) fn install_worker_pool(&mut self) -> Result<(), &'static str> {
        if self.work_sender.is_some() {
            return Err("server worker pool is already installed");
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(self.options.worker_count)
            .enable_io()
            .enable_time()
            .build()
            .map_err(|_| "failed to start server runtime")?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(self.options.pending_work);
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
        let mut workers = Vec::with_capacity(self.options.worker_count);
        for index in 0..self.options.worker_count {
            let worker_receiver = receiver.clone();
            let worker = std::thread::Builder::new()
                .name(format!("bnweb-worker-{index}"))
                .spawn(move || {
                    loop {
                        let work = match worker_receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return,
                        };
                        match work {
                            Ok(work) => {
                                let _ =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
                            }
                            Err(_) => return,
                        }
                    }
                })
                .map_err(|_| "failed to start server worker pool")?;
            workers.push(worker);
        }
        self.work_sender = Some(sender);
        self.http_runtime = Some(std::sync::Arc::new(runtime));
        self.handler_slots = Some(std::sync::Arc::new(tokio::sync::Semaphore::new(
            self.options.worker_count,
        )));
        self.connection_workers.extend(workers);
        Ok(())
    }
    pub(crate) fn submit_connection_work(&self, work: ConnectionWork) -> Result<(), &'static str> {
        let sender = self
            .work_sender
            .as_ref()
            .ok_or("server worker pool is unavailable")?;
        sender.try_send(work).map_err(|error| match error {
            std::sync::mpsc::TrySendError::Full(_) => "server worker queue is full",
            std::sync::mpsc::TrySendError::Disconnected(_) => "server worker pool is unavailable",
        })
    }
    pub(crate) fn http_runtime(&self) -> Option<std::sync::Arc<tokio::runtime::Runtime>> {
        self.http_runtime.clone()
    }
    pub(crate) fn handler_slots(&self) -> Option<std::sync::Arc<tokio::sync::Semaphore>> {
        self.handler_slots.clone()
    }
    pub(crate) fn active_handler_tasks(&self) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        std::sync::Arc::clone(&self.active_handler_tasks)
    }
    pub(crate) fn begin_handler_task(&self) {
        self.active_handler_tasks
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
    pub(crate) fn try_begin_handler(&mut self) -> Result<(), &'static str> {
        if self.pending >= self.options.pending_work {
            return Err("server request queue is full");
        }
        self.pending += 1;
        Ok(())
    }
    pub(crate) fn finish_handler(&mut self) {
        self.pending = self.pending.saturating_sub(1);
    }
    pub(crate) fn finish_handler_task(&self) {
        self.active_handler_tasks
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
    pub(crate) fn begin_request(&mut self) -> Result<(), &'static str> {
        if !self.started || self.stopping || self.closed {
            return Err("server is not accepting requests");
        }
        if self.pending >= self.options.pending_work {
            ServerStats::increment(&mut self.stats.rejected);
            return Err("server request queue is full");
        }
        self.pending += 1;
        ServerStats::increment(&mut self.stats.accepted);
        ServerStats::increment(&mut self.stats.active);
        Ok(())
    }
    pub(crate) fn finish_request(&mut self) {
        self.pending = self.pending.saturating_sub(1);
        self.stats.active = self.stats.active.saturating_sub(1);
        ServerStats::increment(&mut self.stats.completed);
    }
    pub(crate) fn admit_connection(&mut self) -> Result<(), &'static str> {
        if !self.started || self.stopping || self.closed {
            return Err("server is not accepting connections");
        }
        if self.active_connections >= self.options.active_connections {
            return Err("server connection limit reached");
        }
        self.active_connections += 1;
        Ok(())
    }
    pub(crate) fn release_connection(&mut self) {
        self.active_connections = self.active_connections.saturating_sub(1);
        let _ = self.active_sockets.pop();
    }
    pub(crate) fn active_connections(&self) -> usize {
        self.active_connections
    }
    pub(crate) fn pending_requests(&self) -> usize {
        self.pending
    }
    pub(crate) fn stats(&self) -> ServerStats {
        self.stats
    }
    pub(crate) fn record_request_failure(&mut self, timed_out: bool, rate_limited: bool) {
        if timed_out {
            ServerStats::increment(&mut self.stats.timed_out);
        }
        if !timed_out && !rate_limited {
            ServerStats::increment(&mut self.stats.failed);
        }
    }
    pub(crate) fn record_connection_error(&mut self, timed_out: bool) {
        self.record_request_failure(timed_out, false);
    }
    pub(crate) fn record_duration(&mut self, duration: std::time::Duration) {
        let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self.stats.duration_total_ms = self.stats.duration_total_ms.saturating_add(milliseconds);
        self.stats.duration_max_ms = self.stats.duration_max_ms.max(milliseconds);
    }
    pub(crate) fn status(&self) -> ServerStatus {
        if self.failed.is_some() {
            ServerStatus::Failed
        } else if self.closed {
            ServerStatus::Stopped
        } else if self.stopping {
            ServerStatus::Draining
        } else if self.started {
            ServerStatus::Accepting
        } else {
            ServerStatus::Starting
        }
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.status() == ServerStatus::Accepting
    }
    pub(crate) fn mark_failed(&mut self) {
        self.failed = Some(());
        self.started = false;
        self.stopping = true;
    }
    pub(crate) fn options(&self) -> ServerOptions {
        self.options.clone()
    }
    pub(crate) fn track_connection_socket(&mut self, stream: &crate::net::TcpStream) -> bool {
        if let Ok(clone) = stream.try_clone() {
            self.active_sockets.push(clone);
            true
        } else {
            self.release_connection();
            false
        }
    }
    pub(crate) fn cancel_connections(&self) {
        for stream in &self.active_sockets {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
    pub(crate) fn install_listener(
        &mut self,
        listener: std::thread::JoinHandle<()>,
    ) -> Result<(), &'static str> {
        if self.listener.is_some() {
            return Err("server listener is already installed");
        }
        self.listener = Some(listener);
        Ok(())
    }
    pub(crate) fn track_connection_worker(&mut self, worker: std::thread::JoinHandle<()>) {
        self.connection_workers.push(worker);
    }
    pub(crate) fn run_connection_worker<F>(state: &std::sync::Arc<std::sync::Mutex<Self>>, work: F)
    where
        F: FnOnce(),
    {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        if let Ok(mut server) = state.lock() {
            server.release_connection();
        }
    }
    pub(crate) fn tracked_worker_count(&self) -> usize {
        self.connection_workers.len()
    }
    #[cfg(test)]
    pub(crate) fn worker_capacity_available(&self) -> bool {
        self.connection_workers.len() < self.options.worker_count
    }
    pub(crate) fn reap_finished_workers(&mut self) {
        let mut unfinished = Vec::with_capacity(self.connection_workers.len());
        for worker in self.connection_workers.drain(..) {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                unfinished.push(worker);
            }
        }
        self.connection_workers = unfinished;
    }
    pub(crate) fn workers_finished(&mut self) -> bool {
        self.reap_finished_workers();
        self.connection_workers.is_empty()
    }
    pub(crate) fn begin_stop(&mut self, timeout_ms: i128) -> Result<(), &'static str> {
        if !(1..=60_000).contains(&timeout_ms) {
            return Err("stop timeout is outside 1..60000 ms");
        }
        if self.closed {
            return Ok(());
        }
        self.stopping = true;
        self.pending = 0;
        self.work_sender.take();
        self.http_runtime.take();
        self.handler_slots.take();
        self.cancel_connections();
        self.started = false;
        Ok(())
    }
    pub(crate) fn finish_stop(&mut self) -> Result<(), &'static str> {
        if self.closed {
            return Ok(());
        }
        self.reap_finished_workers();
        if self
            .active_handler_tasks
            .load(std::sync::atomic::Ordering::Acquire)
            != 0
        {
            return Err("server drain timed out with active handlers");
        }
        if self.active_connections != 0 {
            return Err("server drain timed out with active connections");
        }
        Ok(())
    }
    pub(crate) fn take_listener(&mut self) -> Option<std::thread::JoinHandle<()>> {
        self.listener.take()
    }
    pub(crate) fn mark_closed(&mut self) {
        self.closed = true;
    }
    pub(crate) fn stop(&mut self, timeout_ms: i128) -> Result<(), &'static str> {
        self.begin_stop(timeout_ms)?;
        self.finish_stop()
    }
    pub(crate) fn close(&mut self, timeout_ms: i128) -> Result<(), &'static str> {
        self.stop(timeout_ms)?;
        self.closed = true;
        Ok(())
    }
    pub(crate) fn routes(&self) -> &[Route] {
        &self.routes
    }
    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping
    }
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }
    pub(crate) fn dispatch<F>(
        &mut self,
        method: &str,
        path: &str,
        handler: F,
    ) -> Result<(), &'static str>
    where
        F: FnOnce(RouteOutcome<'_>),
    {
        self.dispatch_with_key(method, path, path, handler)
    }
    pub(crate) fn dispatch_with_key<F>(
        &mut self,
        method: &str,
        path: &str,
        key: &str,
        handler: F,
    ) -> Result<(), &'static str>
    where
        F: FnOnce(RouteOutcome<'_>),
    {
        self.dispatch_with_key_at(method, path, key, std::time::Instant::now(), handler)
    }
    pub(crate) fn dispatch_with_key_at<F>(
        &mut self,
        method: &str,
        path: &str,
        key: &str,
        now: std::time::Instant,
        handler: F,
    ) -> Result<(), &'static str>
    where
        F: FnOnce(RouteOutcome<'_>),
    {
        self.begin_request()?;
        let allowed = self.rate_limiter.allow(key, now);
        if allowed != 0 {
            self.finish_request();
            ServerStats::increment(&mut self.stats.rejected);
            ServerStats::increment(&mut self.stats.rate_limited);
            return Err("rate limit exceeded");
        }
        let outcome = dispatch_route(&self.routes, method, path);
        let started = std::time::Instant::now();
        handler(outcome);
        self.record_duration(started.elapsed());
        self.finish_request();
        Ok(())
    }
}

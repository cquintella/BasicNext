use super::{Route, RouteOutcome, dispatch_route, valid_method, valid_route_pattern};

const MAX_PENDING_REQUESTS: usize = 128;

#[derive(Debug)]
pub(crate) struct ServerState {
    routes: Vec<Route>,
    started: bool,
    stopping: bool,
    closed: bool,
    pending: usize,
}

impl ServerState {
    pub(crate) fn new() -> Self {
        Self {
            routes: Vec::new(),
            started: false,
            stopping: false,
            closed: false,
            pending: 0,
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
    pub(crate) fn start(&mut self) -> Result<(), &'static str> {
        if self.closed || self.stopping {
            return Err("server is stopping or closed");
        }
        if self.started {
            return Err("server is already started");
        }
        self.started = true;
        Ok(())
    }
    pub(crate) fn begin_request(&mut self) -> Result<(), &'static str> {
        if !self.started || self.stopping || self.closed {
            return Err("server is not accepting requests");
        }
        if self.pending >= MAX_PENDING_REQUESTS {
            return Err("server request queue is full");
        }
        self.pending += 1;
        Ok(())
    }
    pub(crate) fn finish_request(&mut self) {
        self.pending = self.pending.saturating_sub(1);
    }
    pub(crate) fn stop(&mut self, timeout_ms: i128) -> Result<(), &'static str> {
        if !(1..=60_000).contains(&timeout_ms) {
            return Err("stop timeout is outside 1..60000 ms");
        }
        if self.closed {
            return Ok(());
        }
        self.stopping = true;
        self.pending = 0;
        self.started = false;
        Ok(())
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
        self.begin_request()?;
        let outcome = dispatch_route(&self.routes, method, path);
        handler(outcome);
        self.finish_request();
        Ok(())
    }
}

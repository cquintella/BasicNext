// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Server.UseQueue`, `Request.Session` and `BNWeb.Session` for the
//! interpreter (`bnweb.md`, "Parallel handlers and server sessions"). The
//! session state lives in `bn_core_web`; this module maps BN handles onto it.

#![allow(clippy::wildcard_imports)]
use super::*;

type ServerHandle = std::sync::Arc<std::sync::Mutex<crate::web::ServerState>>;

impl WebProvider {
    /// `Server.UseQueue`: the server submits its handlers to the core queue
    /// behind the program's `Dispatch.Queue`.
    pub(super) fn server_use_queue(
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        state: &ServerHandle,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 2, span)?;
        let Value::DispatchQueue(queue_id) = arguments[1] else {
            return Err(type_mismatch(
                "Dispatch.Queue",
                bn_catalog::text("web", "context.queue.non_queue"),
                bn_catalog::text("web", "op.server.use_queue"),
                span,
            ));
        };
        let Some(mut provider) = core.library_take(bn_lib_dispatch::NAME) else {
            return Ok(failed(
                name,
                &WebFailure::ProviderUnavailable(bn_lib_dispatch::NAME),
            ));
        };
        let queue = provider
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<bn_lib_dispatch::DispatchProvider>())
            .and_then(|dispatch| dispatch.core_queue(queue_id));
        core.library_insert(bn_lib_dispatch::NAME, provider);
        let Some(queue) = queue else {
            return Err(runtime_error(
                bn_diag::DiagId::STALE_HANDLE,
                bn_catalog::text("web", "context.queue.not_live"),
                span,
            ));
        };
        let result = state
            .lock()
            .map_err(|_| "server state unavailable")
            .and_then(|mut server| server.use_queue(queue));
        Ok(result.map_or_else(
            |message| failed(name, &WebFailure::from_message(message)),
            |()| Value::Null,
        ))
    }

    /// `Server.UseSessions`: the server keeps a session table.
    pub(super) fn server_use_sessions(
        name: &str,
        arguments: &[Value],
        state: &ServerHandle,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 3, span)?;
        let capacity = integer(&arguments[1], span)?.0;
        let idle = integer(&arguments[2], span)?.0;
        let result = state
            .lock()
            .map_err(|_| "server state unavailable")
            .and_then(|mut server| server.use_sessions(capacity, idle));
        Ok(result.map_or_else(
            |message| failed(name, &WebFailure::from_message(message)),
            |()| Value::Null,
        ))
    }

    /// Gives an in-process `Server.Dispatch` request the session its `sid`
    /// cookie names, when the server keeps sessions.
    pub(super) fn begin_request_session(&mut self, request: Handle, state: &ServerHandle) {
        let table = state.lock().ok().and_then(|server| server.sessions());
        if let (Some(table), Some(stored)) = (table, self.requests.get_mut(&request)) {
            let cookie = stored
                .cookie(crate::web::sessions::SESSION_COOKIE)
                .map(str::to_owned);
            stored.session = Some(crate::web::sessions::RequestSession::begin(
                &table,
                cookie.as_deref(),
            ));
        }
    }

    /// `Request.Session`: a `BNWeb.Session` object for the request's session.
    pub(super) fn request_session(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(name, arguments, 1, span)?;
        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(type_mismatch(
                "BNWeb.Request",
                "non-object value",
                "BNWeb.Request operation receiver",
                span,
            ));
        };
        let request = self.requests.get(handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::STALE_HANDLE,
                "BNWeb.Request handle is not live",
                span,
            )
        })?;
        let Some(session) = request.session.clone() else {
            return Ok(failed(name, &WebFailure::SessionsNotEnabled));
        };
        let value = core.allocate_object("BNWeb.Session", span)?;
        if let Value::Object { handle, .. } = &value {
            self.request_sessions.insert(*handle, session);
        }
        Ok(value)
    }

    /// Writes the `sid` cookie of an in-process `Server.Dispatch` request.
    pub(super) fn apply_request_session(&mut self, request: Handle, response: Handle) {
        let session = self
            .requests
            .get(&request)
            .and_then(|request| request.session.clone());
        if let (Some(session), Some(response)) = (session, self.responses.get_mut(&response)) {
            let _ = session.apply(response);
        }
    }

    pub(super) fn web_session_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        let Some(Value::Object { handle, .. }) = arguments.first() else {
            return Err(type_mismatch(
                "BNWeb.Session",
                "non-object value",
                bn_catalog::text("web", "context.session.receiver"),
                span,
            ));
        };
        if method == "CONSTRUCTOR" {
            // A Session comes only from `Request.Session`; one built with
            // NEW reaches no server, so every member reports it.
            return Ok(Value::Null);
        }
        let Some(session) = self.request_sessions.get(handle) else {
            return Ok(failed(name, &WebFailure::SessionsNotEnabled));
        };
        let key = |index: usize| match arguments.get(index) {
            Some(Value::String(text)) => Ok(text.clone()),
            _ => Err(type_mismatch(
                "STRING",
                "non-STRING value",
                bn_catalog::text("web", "context.session.key_or_value"),
                span,
            )),
        };
        let done = |result: Result<(), &'static str>| {
            result.map_or_else(
                |message| failed(name, &WebFailure::from_message(message)),
                |()| Value::Null,
            )
        };
        match method {
            "Id" => {
                require_arity(name, arguments, 1, span)?;
                Ok(session.id().map_or_else(
                    |message| failed(name, &WebFailure::from_message(message)),
                    |id| Value::String(shared_string(id)),
                ))
            }
            "Get" => {
                require_arity(name, arguments, 2, span)?;
                Ok(match session.get(&key(1)?) {
                    Ok(Some(value)) => Value::String(shared_string(value)),
                    Ok(None) => failed(name, &WebFailure::SessionKeyNotFound),
                    Err(message) => failed(name, &WebFailure::from_message(message)),
                })
            }
            "Set" => {
                require_arity(name, arguments, 3, span)?;
                Ok(done(session.set(&key(1)?, &key(2)?)))
            }
            "Remove" => {
                require_arity(name, arguments, 2, span)?;
                Ok(match session.remove(&key(1)?) {
                    Ok(true) => Value::Null,
                    Ok(false) => failed(name, &WebFailure::SessionKeyNotFound),
                    Err(message) => failed(name, &WebFailure::from_message(message)),
                })
            }
            "Rotate" => {
                require_arity(name, arguments, 1, span)?;
                Ok(done(session.rotate().map(|_| ())))
            }
            "End" => {
                require_arity(name, arguments, 1, span)?;
                session.end();
                Ok(Value::Null)
            }
            _ => Ok(failed(name, &WebFailure::ProviderUnavailable("BNWeb"))),
        }
    }
}

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Server sessions (`bnweb.md`, "Parallel handlers and server sessions"): one
//! table shared by every handler task, whatever thread or program copy runs
//! it, and the `sid` cookie the server writes after the handler. Both
//! backends use it.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::Response;
use crate::web_state::{CookieOptions, SystemEntropy, new_session_id};

/// The cookie that names a client's session.
pub const SESSION_COOKIE: &str = "sid";
const KEY_MAX: usize = 128;
const VALUE_MAX: usize = 4096;
const KEYS_MAX: usize = 64;

#[derive(Debug)]
struct Entry {
    values: BTreeMap<String, String>,
    touched: Instant,
}

/// Every session of one server, behind one lock.
#[derive(Debug)]
pub struct SessionTable {
    sessions: Mutex<HashMap<String, Entry>>,
    capacity: usize,
    idle: Duration,
}

impl SessionTable {
    /// A table of at most `capacity` sessions (1 to 10,000), each forgotten
    /// after `idle_ms` without a request (1 ms to 30 minutes), as
    /// `SessionStore` bounds them.
    pub fn new(capacity: i128, idle_ms: i128) -> Result<Arc<Self>, &'static str> {
        let capacity = usize::try_from(capacity)
            .ok()
            .filter(|capacity| (1..=10_000).contains(capacity))
            .ok_or("invalid session capacity")?;
        let idle = u64::try_from(idle_ms)
            .ok()
            .map(Duration::from_millis)
            .filter(|idle| !idle.is_zero() && *idle <= Duration::from_mins(30))
            .ok_or("invalid session idle timeout")?;
        Ok(Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            capacity,
            idle,
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `id` names a live session; touches it.
    fn live(&self, id: &str) -> bool {
        let mut sessions = self.lock();
        match sessions.get_mut(id) {
            Some(entry) if entry.touched.elapsed() < self.idle => {
                entry.touched = Instant::now();
                true
            }
            Some(_) => {
                sessions.remove(id);
                false
            }
            None => false,
        }
    }

    /// A new empty session; a full table forgets the session idle longest.
    fn create(&self, values: BTreeMap<String, String>) -> Result<String, &'static str> {
        let id = new_session_id(&SystemEntropy)
            .map_err(|_| bn_catalog::text("web", "session.entropy_unavailable"))?;
        let mut sessions = self.lock();
        sessions.retain(|_, entry| entry.touched.elapsed() < self.idle);
        if sessions.len() >= self.capacity
            && let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(id, _)| id.clone())
        {
            sessions.remove(&oldest);
        }
        sessions.insert(
            id.clone(),
            Entry {
                values,
                touched: Instant::now(),
            },
        );
        Ok(id)
    }

    fn with_entry<R>(
        &self,
        id: &str,
        operation: impl FnOnce(&mut Entry) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        let mut sessions = self.lock();
        let entry = sessions
            .get_mut(id)
            .filter(|entry| entry.touched.elapsed() < self.idle)
            .ok_or("session not found")?;
        entry.touched = Instant::now();
        operation(entry)
    }

    fn remove(&self, id: &str) -> Option<BTreeMap<String, String>> {
        self.lock().remove(id).map(|entry| entry.values)
    }

    /// The number of live sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        let mut sessions = self.lock();
        sessions.retain(|_, entry| entry.touched.elapsed() < self.idle);
        sessions.len()
    }

    /// Whether no session is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CookieChange {
    Keep,
    Set,
    Clear,
}

#[derive(Debug)]
struct SessionState {
    id: Option<String>,
    cookie: CookieChange,
}

/// The session one request uses: the one its cookie names, or a new one
/// created on first use. Clones share the state, so the server sees what the
/// handler did when it writes the cookie.
#[derive(Clone, Debug)]
pub struct RequestSession {
    table: Arc<SessionTable>,
    state: Arc<Mutex<SessionState>>,
}

impl RequestSession {
    /// The session for a request whose `sid` cookie is `cookie`.
    #[must_use]
    pub fn begin(table: &Arc<SessionTable>, cookie: Option<&str>) -> Self {
        let id = cookie.filter(|id| table.live(id)).map(str::to_owned);
        Self {
            table: Arc::clone(table),
            state: Arc::new(Mutex::new(SessionState {
                id,
                cookie: CookieChange::Keep,
            })),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The session identifier, creating the session on first use.
    pub fn id(&self) -> Result<String, &'static str> {
        let mut state = self.lock();
        if let Some(id) = state.id.as_ref().filter(|id| self.table.live(id)) {
            return Ok(id.clone());
        }
        let id = self.table.create(BTreeMap::new())?;
        state.id = Some(id.clone());
        state.cookie = CookieChange::Set;
        Ok(id)
    }

    /// The live session identifier, without creating a session.
    fn current(&self) -> Option<String> {
        let state = self.lock();
        state.id.clone().filter(|id| self.table.live(id))
    }

    /// The value under `key`, if any. Reading never creates a session.
    pub fn get(&self, key: &str) -> Result<Option<String>, &'static str> {
        let Some(id) = self.current() else {
            return Ok(None);
        };
        self.table
            .with_entry(&id, |entry| Ok(entry.values.get(key).cloned()))
    }

    /// Stores `value` under `key`.
    pub fn set(&self, key: &str, value: &str) -> Result<(), &'static str> {
        if key.is_empty() || key.len() > KEY_MAX || value.len() > VALUE_MAX {
            return Err(bn_catalog::text("web", "session.invalid_entry"));
        }
        let id = self.id()?;
        self.table.with_entry(&id, |entry| {
            if !entry.values.contains_key(key) && entry.values.len() >= KEYS_MAX {
                return Err(bn_catalog::text("web", "session.key_limit"));
            }
            entry.values.insert(key.to_owned(), value.to_owned());
            Ok(())
        })
    }

    /// Removes `key`; `Ok(false)` when it was absent. Never creates a
    /// session.
    pub fn remove(&self, key: &str) -> Result<bool, &'static str> {
        let Some(id) = self.current() else {
            return Ok(false);
        };
        self.table
            .with_entry(&id, |entry| Ok(entry.values.remove(key).is_some()))
    }

    /// A new identifier for the same values (call on login).
    pub fn rotate(&self) -> Result<String, &'static str> {
        let old = self.id()?;
        let values = self.table.remove(&old).unwrap_or_default();
        let id = self.table.create(values)?;
        let mut state = self.lock();
        state.id = Some(id.clone());
        state.cookie = CookieChange::Set;
        Ok(id)
    }

    /// Forgets the session (call on logout).
    pub fn end(&self) {
        let mut state = self.lock();
        if let Some(id) = state.id.take() {
            self.table.remove(&id);
        }
        state.cookie = CookieChange::Clear;
    }

    /// Writes the `sid` cookie the handler's session work needs: set for a
    /// new or rotated session, cleared for an ended one; `Secure` over TLS.
    pub fn apply(&self, response: &mut Response) -> Result<(), &'static str> {
        let state = self.lock();
        let options = CookieOptions {
            secure: response.secure,
            ..CookieOptions::default()
        };
        match (state.cookie, state.id.as_deref()) {
            (CookieChange::Set, Some(id)) => {
                response.set_cookie(SESSION_COOKIE, id, None, "/", None, options)
            }
            (CookieChange::Clear, _) => response.clear_cookie(SESSION_COOKIE, None, "/"),
            _ => Ok(()),
        }
    }
}

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Server sessions (`bnweb.md`, "Parallel handlers and server sessions";
//! bucket 0.6.5 E.S0.e).

use super::Response;
use super::sessions::{RequestSession, SESSION_COOKIE, SessionTable};
use std::time::Duration;

fn set_cookie(response: &Response) -> Option<&str> {
    response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Set-Cookie"))
        .map(|(_, value)| value.as_str())
}

/// The `sid` a response sets, if any.
fn sid(response: &Response) -> Option<String> {
    let value = set_cookie(response)?.strip_prefix("sid=")?;
    let id = value.split(';').next()?;
    (!id.is_empty()).then(|| id.to_owned())
}

#[test]
fn session_table_bounds_follow_the_spec() {
    assert_eq!(
        SessionTable::new(0, 1000).err(),
        Some("invalid session capacity")
    );
    assert_eq!(
        SessionTable::new(10_001, 1000).err(),
        Some("invalid session capacity")
    );
    assert_eq!(
        SessionTable::new(1, 0).err(),
        Some("invalid session idle timeout")
    );
    assert_eq!(
        SessionTable::new(1, 1_800_001).err(),
        Some("invalid session idle timeout")
    );
    assert!(SessionTable::new(10_000, 1_800_000).is_ok());
}

/// Reading creates nothing; writing creates the session and the cookie; the
/// next request with that cookie sees the value and sets no cookie.
#[test]
fn a_session_is_created_on_write_and_found_by_its_cookie() {
    let table = SessionTable::new(16, 60_000).unwrap();
    let anonymous = RequestSession::begin(&table, None);
    assert_eq!(anonymous.get("user"), Ok(None));
    assert_eq!(anonymous.remove("user"), Ok(false));
    let mut response = Response::new();
    anonymous.apply(&mut response).unwrap();
    assert_eq!(set_cookie(&response), None);
    assert!(table.is_empty());

    let login = RequestSession::begin(&table, None);
    login.set("user", "ana").unwrap();
    let mut response = Response::new();
    login.apply(&mut response).unwrap();
    let cookie = set_cookie(&response).unwrap();
    assert!(
        cookie.ends_with("; Path=/; HttpOnly; SameSite=Lax"),
        "{cookie}"
    );
    let id = sid(&response).unwrap();
    assert_eq!(id.len(), 33, "128-bit identifier: {id}");

    let next = RequestSession::begin(&table, Some(&id));
    assert_eq!(next.get("user"), Ok(Some("ana".into())));
    assert_eq!(next.id(), Ok(id));
    let mut response = Response::new();
    next.apply(&mut response).unwrap();
    assert_eq!(set_cookie(&response), None);

    let unknown = RequestSession::begin(&table, Some("s0123"));
    assert_eq!(unknown.get("user"), Ok(None));

    let mut tls = Response::new();
    tls.secure = true;
    let secure = RequestSession::begin(&table, None);
    secure.id().unwrap();
    secure.apply(&mut tls).unwrap();
    assert!(set_cookie(&tls).unwrap().contains("; Secure"));
}

#[test]
fn rotate_keeps_the_values_and_end_forgets_the_session() {
    let table = SessionTable::new(16, 60_000).unwrap();
    let first = RequestSession::begin(&table, None);
    first.set("user", "ana").unwrap();
    let old = first.id().unwrap();
    let new = first.rotate().unwrap();
    assert_ne!(old, new);
    assert_eq!(first.get("user"), Ok(Some("ana".into())));
    assert_eq!(
        RequestSession::begin(&table, Some(&old)).get("user"),
        Ok(None)
    );
    let mut response = Response::new();
    first.apply(&mut response).unwrap();
    assert_eq!(sid(&response), Some(new.clone()));

    let logout = RequestSession::begin(&table, Some(&new));
    logout.end();
    let mut response = Response::new();
    logout.apply(&mut response).unwrap();
    assert_eq!(
        set_cookie(&response),
        Some("sid=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax")
    );
    assert_eq!(
        RequestSession::begin(&table, Some(&new)).get("user"),
        Ok(None)
    );
    assert!(table.is_empty());
    assert_eq!(SESSION_COOKIE, "sid");
}

#[test]
fn session_entries_are_bounded() {
    let table = SessionTable::new(16, 60_000).unwrap();
    let session = RequestSession::begin(&table, None);
    assert_eq!(session.set("", "v"), Err("invalid session entry"));
    assert_eq!(
        session.set(&"k".repeat(129), "v"),
        Err("invalid session entry")
    );
    assert_eq!(
        session.set("k", &"v".repeat(4097)),
        Err("invalid session entry")
    );
    session.set(&"k".repeat(128), &"v".repeat(4096)).unwrap();
    for key in 1..64 {
        session.set(&format!("key{key}"), "v").unwrap();
    }
    assert_eq!(
        session.set("one-more", "v"),
        Err("session key limit exceeded")
    );
    session.set("key1", "replaced").unwrap();
    assert_eq!(session.remove("key1"), Ok(true));
    session.set("one-more", "v").unwrap();
}

/// A full table forgets the session idle the longest; an idle session
/// expires.
#[test]
fn a_full_table_evicts_the_least_recently_used_and_idle_sessions_expire() {
    let table = SessionTable::new(2, 60_000).unwrap();
    let ids: Vec<String> = (0..2)
        .map(|_| RequestSession::begin(&table, None).id().unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(2));
    // Touch the first, so the second is the least recently used.
    assert_eq!(
        RequestSession::begin(&table, Some(&ids[0])).id(),
        Ok(ids[0].clone())
    );
    RequestSession::begin(&table, None).id().unwrap();
    assert_eq!(table.len(), 2);
    assert_ne!(
        RequestSession::begin(&table, Some(&ids[1])).id(),
        Ok(ids[1].clone())
    );

    let short = SessionTable::new(4, 1).unwrap();
    let session = RequestSession::begin(&short, None);
    session.set("user", "ana").unwrap();
    let id = session.id().unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(
        RequestSession::begin(&short, Some(&id)).get("user"),
        Ok(None)
    );
    assert!(short.is_empty());
}

/// Handler tasks on different threads share one table.
#[test]
fn handler_threads_share_the_table() {
    let table = SessionTable::new(64, 60_000).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|worker| {
            let table = std::sync::Arc::clone(&table);
            std::thread::spawn(move || {
                let session = RequestSession::begin(&table, None);
                session.set("worker", &worker.to_string()).unwrap();
                session.id().unwrap()
            })
        })
        .collect();
    let ids: Vec<String> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(table.len(), 8);
    for (worker, id) in ids.iter().enumerate() {
        let session = RequestSession::begin(&table, Some(id));
        assert_eq!(session.get("worker"), Ok(Some(worker.to_string())));
    }
}

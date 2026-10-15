// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The resources of one module copy (`0.6.md`, "Isolation and conformance"):
//! the program's, and a fresh one for each `ASYNC` task and `BNWeb` handler
//! both backends run. Every handle table — files, sockets, loggers, SQLite
//! connections, JSON documents, … — belongs to the copy that runs on the
//! current thread, created empty on first use ([`table`]). A copy shares no
//! handle with another, so no table is shared between threads.
//!
//! The end of a task is the end of its copy's program: [`run`] drops the
//! copy's tables when the task returns, and each resource ends as its type's
//! `Drop` ends it — a file or socket closes, a connection closes (its open
//! transaction rolls back), a document is freed. The tables live here, below
//! `bn_rt` and the `bn_core_*` crates, so every one of them can keep its
//! table in the copy.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

type Tables = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

thread_local! {
    /// The tables of the copy that runs on this thread, by table type.
    static TABLES: RefCell<Tables> = RefCell::new(HashMap::new());
}

/// The table of type `T` of the copy that runs on this thread, created empty
/// on first use. Each table is its own type, so two never share an entry.
#[must_use]
pub fn table<T: Default + Send + Sync + 'static>() -> Arc<T> {
    let entry = TABLES.with_borrow_mut(|tables| {
        Arc::clone(
            tables
                .entry(TypeId::of::<T>())
                .or_insert_with(|| Arc::new(T::default())),
        )
    });
    match entry.downcast::<T>() {
        Ok(table) => table,
        // The entry was filed under `TypeId::of::<T>()` above.
        Err(_) => unreachable!(),
    }
}

/// Runs `task` as the program of a fresh copy: it starts with no table, and
/// when it returns — or unwinds — its tables are dropped and the thread's
/// own copy comes back.
pub fn run<R>(task: impl FnOnce() -> R) -> R {
    let _copy = FreshCopy {
        outer: Some(TABLES.take()),
    };
    task()
}

/// Puts the outer copy back when the task's copy ends.
struct FreshCopy {
    outer: Option<Tables>,
}

impl Drop for FreshCopy {
    fn drop(&mut self) {
        let finished = TABLES.replace(self.outer.take().unwrap_or_default());
        // Dropped after the outer copy is back: a resource whose `Drop` asks
        // for a table finds the outer copy's, never a half-dropped one.
        drop(finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counter(Mutex<u32>);

    static DROPPED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Default)]
    struct Resource;

    impl Drop for Resource {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn a_task_copy_starts_empty_and_its_tables_end_with_it() {
        *table::<Counter>().0.lock().unwrap() = 7;
        let inner = run(|| {
            let fresh = *table::<Counter>().0.lock().unwrap();
            let _ = table::<Resource>();
            *table::<Counter>().0.lock().unwrap() = 9;
            fresh
        });
        assert_eq!(inner, 0, "the task's copy starts with no table");
        assert_eq!(
            *table::<Counter>().0.lock().unwrap(),
            7,
            "the outer copy is back"
        );
        assert_eq!(
            DROPPED.load(Ordering::Relaxed),
            1,
            "the task's resource ended"
        );
    }

    #[test]
    fn each_thread_runs_its_own_copy() {
        *table::<Counter>().0.lock().unwrap() = 3;
        let other = std::thread::spawn(|| *table::<Counter>().0.lock().unwrap())
            .join()
            .unwrap();
        assert_eq!(other, 0);
    }
}

//! Snapshot and freshness boundary shared by frontend clients.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::source::{Revision, SourceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub source: SourceId,
    pub revision: Revision,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisRequest {
    pub source: SourceId,
    pub revision: Revision,
}

#[derive(Default)]
pub struct FrontendSession {
    next_source: u64,
    next_revision: BTreeMap<SourceId, u64>,
    snapshots: BTreeMap<SourceId, Snapshot>,
    dependents: BTreeMap<SourceId, BTreeSet<SourceId>>,
    stale: BTreeSet<SourceId>,
}

impl FrontendSession {
    pub fn upsert(&mut self, source: Option<SourceId>, text: String) -> Snapshot {
        let source = source.unwrap_or_else(|| {
            self.next_source = self.next_source.saturating_add(1).max(1);
            SourceId(self.next_source)
        });
        let revision = self.next_revision.entry(source).or_insert(0);
        *revision = revision.saturating_add(1).max(1);
        let snapshot = Snapshot {
            source,
            revision: Revision(*revision),
            text,
        };
        let existed = self.snapshots.insert(source, snapshot.clone()).is_some();
        self.stale.remove(&source);
        if existed {
            self.invalidate(source);
            self.stale.remove(&source);
        }
        snapshot
    }

    /// Reuses the current revision when the source text is unchanged.
    ///
    /// # Panics
    ///
    /// This function does not panic for a consistent session; a matching
    /// snapshot is returned directly and otherwise a new snapshot is created.
    pub fn upsert_if_changed(&mut self, source: Option<SourceId>, text: String) -> Snapshot {
        if let Some(source) = source
            && self
                .snapshots
                .get(&source)
                .is_some_and(|snapshot| snapshot.text == text)
            && let Some(snapshot) = self.snapshots.get(&source).cloned()
        {
            return snapshot;
        }
        self.upsert(source, text)
    }

    pub fn register_import(&mut self, importer: SourceId, imported_source: SourceId) {
        self.dependents
            .entry(imported_source)
            .or_default()
            .insert(importer);
    }

    #[must_use]
    pub fn request(&self, source: SourceId) -> Option<AnalysisRequest> {
        let snapshot = self.snapshots.get(&source)?;
        (!self.stale.contains(&source)).then_some(AnalysisRequest {
            source,
            revision: snapshot.revision,
        })
    }

    #[must_use]
    pub fn accept(&self, request: AnalysisRequest) -> Option<&Snapshot> {
        let snapshot = self.snapshots.get(&request.source)?;
        (snapshot.revision == request.revision && !self.stale.contains(&request.source))
            .then_some(snapshot)
    }

    #[must_use]
    pub fn snapshot(&self, source: SourceId) -> Option<&Snapshot> {
        self.snapshots.get(&source)
    }

    /// Discards work for the current revision when it matches the request.
    ///
    /// Returns `true` only when the request represented the current fresh
    /// snapshot and cancellation was applied.
    pub fn cancel(&mut self, request: AnalysisRequest) -> bool {
        let matches_current = self
            .snapshots
            .get(&request.source)
            .is_some_and(|snapshot| snapshot.revision == request.revision)
            && !self.stale.contains(&request.source);
        if matches_current {
            self.invalidate(request.source);
        }
        matches_current
    }

    /// Removes a source snapshot and invalidates all of its importers.
    #[must_use]
    pub fn remove(&mut self, source: SourceId) -> Option<Snapshot> {
        let snapshot = self.snapshots.remove(&source);
        if snapshot.is_some() {
            self.invalidate(source);
        }
        snapshot
    }

    fn invalidate(&mut self, source: SourceId) {
        let mut pending = VecDeque::from([source]);
        while let Some(source) = pending.pop_front() {
            if !self.stale.insert(source) {
                continue;
            }
            if let Some(dependents) = self.dependents.get(&source) {
                pending.extend(dependents.iter().copied());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrontendSession, SourceId};

    #[test]
    fn replacing_a_snapshot_increments_revision_and_discards_old_work() {
        let mut session = FrontendSession::default();
        let first = session.upsert(None, "one".into());
        let request = session.request(first.source).expect("fresh request");
        let second = session.upsert(Some(first.source), "two".into());
        assert_eq!(second.revision.0, first.revision.0 + 1);
        assert!(session.accept(request).is_none());
        assert_eq!(session.snapshot(first.source), Some(&second));
    }

    #[test]
    fn changing_an_imported_source_invalidates_transitive_dependents() {
        let mut session = FrontendSession::default();
        let a = session.upsert(Some(SourceId(1)), "a".into());
        let b = session.upsert(Some(SourceId(2)), "b".into());
        let c = session.upsert(Some(SourceId(3)), "c".into());
        session.register_import(b.source, a.source);
        session.register_import(c.source, b.source);
        let request = session.request(c.source).expect("fresh request");
        session.upsert(Some(a.source), "changed".into());
        assert!(session.accept(request).is_none());
    }

    #[test]
    fn unrelated_snapshot_remains_current() {
        let mut session = FrontendSession::default();
        let a = session.upsert(Some(SourceId(10)), "a".into());
        let b = session.upsert(Some(SourceId(11)), "b".into());
        let request = session.request(b.source).expect("fresh request");
        session.upsert(Some(a.source), "changed".into());
        assert!(session.accept(request).is_some());
    }

    #[test]
    fn replacing_a_snapshot_makes_the_new_revision_analyzable() {
        let mut session = FrontendSession::default();
        let first = session.upsert(Some(SourceId(20)), "one".into());
        session.upsert(Some(first.source), "two".into());
        assert!(session.request(first.source).is_some());
    }

    #[test]
    fn explicit_cancellation_discards_only_the_matching_request() {
        let mut session = FrontendSession::default();
        let snapshot = session.upsert(Some(SourceId(21)), "one".into());
        let request = session.request(snapshot.source).expect("fresh request");
        assert!(session.cancel(request));
        assert!(session.accept(request).is_none());
        assert!(session.request(snapshot.source).is_none());
    }

    #[test]
    fn deleted_sources_can_be_reopened_with_a_fresh_revision() {
        let mut session = FrontendSession::default();
        let first = session.upsert(Some(SourceId(22)), "one".into());
        assert!(session.remove(first.source).is_some());
        assert!(session.snapshot(first.source).is_none());
        let reopened = session.upsert(Some(first.source), "two".into());
        assert!(reopened.revision.0 > first.revision.0);
        assert!(session.request(reopened.source).is_some());
    }
}

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! REPL history controller supporting ephemeral error-entry recall.
//!
//! History retains only successful submissions permanently, while retaining
//! the most recent submission (whether successful or erroneous) as the immediate
//! first entry when recalling history backwards ([Up] key).

use std::{borrow::Cow, path::Path};

use rustyline::{
    Config, Result,
    history::{FileHistory, History, SearchDirection, SearchResult},
};

/// Custom history manager that ensures the immediate previous submission is always
/// accessible on the first history recall, while only successful submissions are
/// retained in the long-term history.
pub struct BnrHistory {
    inner: FileHistory,
    transient_last: Option<String>,
}

impl std::fmt::Debug for BnrHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BnrHistory")
            .field("inner_len", &self.inner.len())
            .field("transient_last", &self.transient_last)
            .finish_non_exhaustive()
    }
}

impl Default for BnrHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl BnrHistory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: FileHistory::new(),
            transient_last: None,
        }
    }

    #[must_use]
    pub fn with_config(config: Config) -> Self {
        Self {
            inner: FileHistory::with_config(config),
            transient_last: None,
        }
    }

    /// Records a successful command execution, clearing any transient error entry
    /// and persisting the line into the permanent history.
    ///
    /// # Errors
    /// Returns an error if the underlying history backend fails to append the entry.
    pub fn record_success(&mut self, line: &str) -> Result<bool> {
        self.transient_last = None;
        self.inner.add(line)
    }

    /// Records an erroneous command execution as the transient last entry,
    /// without appending it to the permanent history of successful commands.
    pub fn record_failure(&mut self, line: &str) {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            self.transient_last = Some(trimmed.to_string());
        }
    }

    #[must_use]
    pub fn transient_last(&self) -> Option<&str> {
        self.transient_last.as_deref()
    }

    /// Returns the total number of history entries (including any active transient error).
    #[must_use]
    pub fn len(&self) -> usize {
        History::len(self)
    }

    /// Returns `true` if history contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        History::is_empty(self)
    }
}

impl History for BnrHistory {
    fn get(&self, index: usize, dir: SearchDirection) -> Result<Option<SearchResult<'_>>> {
        let inner_len = self.inner.len();
        if index == inner_len {
            return Ok(self
                .transient_last
                .as_deref()
                .map(|transient| SearchResult {
                    entry: Cow::Borrowed(transient),
                    idx: index,
                    pos: 0,
                }));
        }
        if index < inner_len {
            self.inner.get(index, dir)
        } else {
            Ok(None)
        }
    }

    fn add(&mut self, line: &str) -> Result<bool> {
        self.record_success(line)
    }

    fn add_owned(&mut self, line: String) -> Result<bool> {
        self.transient_last = None;
        self.inner.add_owned(line)
    }

    fn len(&self) -> usize {
        self.inner.len() + usize::from(self.transient_last.is_some())
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty() && self.transient_last.is_none()
    }

    fn set_max_len(&mut self, len: usize) -> Result<()> {
        self.inner.set_max_len(len)
    }

    fn ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.inner.ignore_dups(yes)
    }

    fn ignore_space(&mut self, yes: bool) {
        self.inner.ignore_space(yes);
    }

    fn save(&mut self, path: &Path) -> Result<()> {
        self.inner.save(path)
    }

    fn append(&mut self, path: &Path) -> Result<()> {
        self.inner.append(path)
    }

    fn load(&mut self, path: &Path) -> Result<()> {
        self.transient_last = None;
        self.inner.load(path)
    }

    fn clear(&mut self) -> Result<()> {
        self.transient_last = None;
        self.inner.clear()
    }

    fn search(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        if term.is_empty() || start >= self.len() {
            return Ok(None);
        }
        let inner_len = self.inner.len();
        match dir {
            SearchDirection::Reverse => {
                let transient_pos = if start == inner_len {
                    self.transient_last.as_deref().and_then(|t| t.find(term))
                } else {
                    None
                };
                if let Some(pos) = transient_pos {
                    let transient = self.transient_last.as_deref().unwrap_or_default();
                    return Ok(Some(SearchResult {
                        entry: Cow::Borrowed(transient),
                        idx: inner_len,
                        pos,
                    }));
                }
                if inner_len > 0 {
                    let search_start = start.min(inner_len - 1);
                    self.inner
                        .search(term, search_start, SearchDirection::Reverse)
                } else {
                    Ok(None)
                }
            }
            SearchDirection::Forward => {
                if start < inner_len {
                    let res = self.inner.search(term, start, SearchDirection::Forward)?;
                    if res.is_some() {
                        return Ok(res);
                    }
                }
                if let Some(pos) = self.transient_last.as_deref().and_then(|t| t.find(term)) {
                    let transient = self.transient_last.as_deref().unwrap_or_default();
                    return Ok(Some(SearchResult {
                        entry: Cow::Borrowed(transient),
                        idx: inner_len,
                        pos,
                    }));
                }
                Ok(None)
            }
        }
    }

    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        if term.is_empty() || start >= self.len() {
            return Ok(None);
        }
        let inner_len = self.inner.len();
        match dir {
            SearchDirection::Reverse => {
                if start == inner_len
                    && self
                        .transient_last
                        .as_deref()
                        .is_some_and(|t| t.starts_with(term))
                {
                    let transient = self.transient_last.as_deref().unwrap_or_default();
                    return Ok(Some(SearchResult {
                        entry: Cow::Borrowed(transient),
                        idx: inner_len,
                        pos: term.len(),
                    }));
                }
                if inner_len > 0 {
                    let search_start = start.min(inner_len - 1);
                    self.inner
                        .starts_with(term, search_start, SearchDirection::Reverse)
                } else {
                    Ok(None)
                }
            }
            SearchDirection::Forward => {
                if start < inner_len {
                    let res = self
                        .inner
                        .starts_with(term, start, SearchDirection::Forward)?;
                    if res.is_some() {
                        return Ok(res);
                    }
                }
                if self
                    .transient_last
                    .as_deref()
                    .is_some_and(|t| t.starts_with(term))
                {
                    let transient = self.transient_last.as_deref().unwrap_or_default();
                    return Ok(Some(SearchResult {
                        entry: Cow::Borrowed(transient),
                        idx: inner_len,
                        pos: term.len(),
                    }));
                }
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_history() {
        let h = BnrHistory::new();
        assert_eq!(h.len(), 0);
        assert!(h.is_empty());
        assert_eq!(h.get(0, SearchDirection::Reverse).unwrap(), None);
    }

    #[test]
    fn test_success_entries_only() {
        let mut h = BnrHistory::new();
        assert!(h.record_success("LET a = 1").unwrap());
        assert!(h.record_success("LET b = 2").unwrap());
        assert_eq!(h.len(), 2);
        assert!(!h.is_empty());

        let entry1 = h.get(1, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(entry1.entry, "LET b = 2");
        assert_eq!(entry1.idx, 1);

        let entry0 = h.get(0, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(entry0.entry, "LET a = 1");
        assert_eq!(entry0.idx, 0);

        assert_eq!(h.get(2, SearchDirection::Reverse).unwrap(), None);
    }

    #[test]
    fn test_transient_error_is_first_recalled() {
        let mut h = BnrHistory::new();
        h.record_success("LET a = 1").unwrap();
        h.record_success("LET b = 2").unwrap();

        // An erroneous line is entered
        h.record_failure("LET c = broken_syntax");

        // Length reflects 2 successful + 1 transient error = 3
        assert_eq!(h.len(), 3);

        // First [Up] (index 2): yields the broken line!
        let first_up = h.get(2, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(first_up.entry, "LET c = broken_syntax");
        assert_eq!(first_up.idx, 2);

        // Second [Up] (index 1): yields the last successful line
        let second_up = h.get(1, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(second_up.entry, "LET b = 2");
        assert_eq!(second_up.idx, 1);

        // Third [Up] (index 0): yields the prior successful line
        let third_up = h.get(0, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(third_up.entry, "LET a = 1");
        assert_eq!(third_up.idx, 0);
    }

    #[test]
    fn test_subsequent_failure_replaces_transient_error() {
        let mut h = BnrHistory::new();
        h.record_success("LET a = 1").unwrap();

        h.record_failure("bad line 1");
        assert_eq!(h.len(), 2);
        assert_eq!(
            h.get(1, SearchDirection::Reverse).unwrap().unwrap().entry,
            "bad line 1"
        );

        // Entering another failure replaces the transient error
        h.record_failure("bad line 2");
        assert_eq!(h.len(), 2);

        // First [Up]: newest error "bad line 2"
        assert_eq!(
            h.get(1, SearchDirection::Reverse).unwrap().unwrap().entry,
            "bad line 2"
        );
        // Second [Up]: only the successful command "LET a = 1"
        assert_eq!(
            h.get(0, SearchDirection::Reverse).unwrap().unwrap().entry,
            "LET a = 1"
        );
    }

    #[test]
    fn test_subsequent_success_clears_transient_error() {
        let mut h = BnrHistory::new();
        h.record_success("LET a = 1").unwrap();
        h.record_failure("bad line 1");
        assert_eq!(h.len(), 2);

        // User fixes the line or types a new successful one
        h.record_success("LET b = 2").unwrap();
        assert_eq!(h.len(), 2);

        // First [Up] is now "LET b = 2"
        assert_eq!(
            h.get(1, SearchDirection::Reverse).unwrap().unwrap().entry,
            "LET b = 2"
        );
        // Second [Up] is "LET a = 1"
        assert_eq!(
            h.get(0, SearchDirection::Reverse).unwrap().unwrap().entry,
            "LET a = 1"
        );
    }

    #[test]
    fn test_save_excludes_transient_error() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("bnr_test_history.txt");

        let mut h1 = BnrHistory::new();
        h1.record_success("LET a = 100").unwrap();
        h1.record_failure("SYNTAX ERROR LINE");
        h1.save(&path).unwrap();

        let mut h2 = BnrHistory::new();
        h2.load(&path).unwrap();

        // Loaded history must only have the successful command
        assert_eq!(h2.len(), 1);
        assert_eq!(
            h2.get(0, SearchDirection::Reverse).unwrap().unwrap().entry,
            "LET a = 100"
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_search_and_starts_with_with_transient_error() {
        let mut h = BnrHistory::new();
        h.record_success("LET good = 10").unwrap();
        h.record_success("LET target = 20").unwrap();
        h.record_failure("LET bad_target = broken");

        // Reverse search finds transient entry if it matches
        let found_bad = h
            .search("bad", h.len() - 1, SearchDirection::Reverse)
            .unwrap()
            .unwrap();
        assert_eq!(found_bad.entry, "LET bad_target = broken");
        assert_eq!(found_bad.idx, 2);

        // Reverse search skips transient entry if searching for successful entry
        let found_good = h
            .search("good", h.len() - 1, SearchDirection::Reverse)
            .unwrap()
            .unwrap();
        assert_eq!(found_good.entry, "LET good = 10");
        assert_eq!(found_good.idx, 0);

        // starts_with finds transient entry
        let starts = h
            .starts_with("LET bad", h.len() - 1, SearchDirection::Reverse)
            .unwrap()
            .unwrap();
        assert_eq!(starts.entry, "LET bad_target = broken");
        assert_eq!(starts.idx, 2);

        // Forward search finds transient entry when reaching the end
        let forward = h
            .search("bad", 0, SearchDirection::Forward)
            .unwrap()
            .unwrap();
        assert_eq!(forward.entry, "LET bad_target = broken");
        assert_eq!(forward.idx, 2);
    }

    #[test]
    fn test_clear_resets_both_inner_and_transient() {
        let mut h = BnrHistory::new();
        h.record_success("LET x = 1").unwrap();
        h.record_failure("err");
        assert_eq!(h.len(), 2);

        h.clear().unwrap();
        assert_eq!(h.len(), 0);
        assert!(h.is_empty());
        assert_eq!(h.transient_last(), None);
    }
}

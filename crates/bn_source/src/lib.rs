// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared source identity, revision and span types.
// Identifica qual arquivo originou determinado token, erro ou trecho.

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(pub u64);

impl SourceId {
    pub const UNKNOWN: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(pub u64);

impl Revision {
    pub const UNKNOWN: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Position {
    pub source_id: SourceId,
    pub revision: Revision,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub const UNKNOWN_SOURCE: SourceId = SourceId::UNKNOWN;
    pub const UNKNOWN_REVISION: Revision = Revision::UNKNOWN;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    #[must_use]
    pub const fn source_id(self) -> SourceId {
        self.start.source_id
    }

    #[must_use]
    pub const fn revision(self) -> Revision {
        self.start.revision
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    pub source_id: SourceId,
    pub revision: Revision,
}

impl SourceFile {
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let name = name.into();
        let text = text.into();
        Self {
            source_id: SourceId(stable_hash(name.as_bytes())),
            revision: Revision(stable_hash(text.as_bytes())),
            name,
            text,
        }
    }

    #[must_use]
    pub fn line(&self, number: usize) -> &str {
        self.text
            .lines()
            .nth(number.saturating_sub(1))
            .unwrap_or("")
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        hash.wrapping_mul(0x0100_0000_01b3)
            .wrapping_add(u64::from(*byte))
    })
}

#[cfg(test)]
mod tests {
    use super::{Revision, SourceFile, SourceId};

    #[test]
    fn source_identity_is_stable_and_revision_tracks_content() {
        let first = SourceFile::new("main.bn", "PRINT 1\n");
        let same_name = SourceFile::new("main.bn", "PRINT 2\n");
        let other_name = SourceFile::new("other.bn", "PRINT 1\n");
        assert_ne!(first.source_id, SourceId::UNKNOWN);
        assert_eq!(first.source_id, same_name.source_id);
        assert_ne!(first.source_id, other_name.source_id);
        assert_ne!(first.revision, Revision::UNKNOWN);
        assert_ne!(first.revision, same_name.revision);
    }
}

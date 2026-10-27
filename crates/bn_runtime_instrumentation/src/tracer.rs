// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    fmt,
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
};

use serde_json::json;

use crate::{consumer::EventConsumer, context::EventContext, events::Event};

/// Event consumer that streams NDJSON execution traces to a file or writer.
pub struct TracerConsumer {
    writer: Box<dyn Write + Send>,
}

impl fmt::Debug for TracerConsumer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TracerConsumer").finish_non_exhaustive()
    }
}

impl TracerConsumer {
    /// Creates a tracer writing to a newly created file, emitting a metadata header.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the file cannot be created or the header written.
    pub fn to_file(path: impl AsRef<Path>, program_name: &str) -> io::Result<Self> {
        let file = File::create(path)?;
        Self::to_writer(Box::new(BufWriter::new(file)), program_name)
    }

    /// Creates a tracer writing to any `Write + Send` stream, emitting a metadata header.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the metadata header cannot be written.
    pub fn to_writer(mut writer: Box<dyn Write + Send>, program_name: &str) -> io::Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let header = json!({
            "type": "Metadata",
            "version": env!("CARGO_PKG_VERSION"),
            "program": program_name,
            "timestamp_secs": timestamp,
        });
        serde_json::to_writer(&mut writer, &header)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(Self { writer })
    }
}

impl EventConsumer for TracerConsumer {
    fn on_event(&mut self, event: &Event, _ctx: &EventContext) {
        let json_value = event.to_json();
        if serde_json::to_writer(&mut self.writer, &json_value).is_ok() {
            let _ = self.writer.write_all(b"\n");
            let _ = self.writer.flush();
        }
    }
}

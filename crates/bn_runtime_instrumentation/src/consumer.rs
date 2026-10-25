// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt::Debug;

use crate::{context::EventContext, events::Event};

/// Pluggable consumer of runtime instrumentation events.
pub trait EventConsumer: Debug + Send {
    /// Dispatches an event to this consumer.
    fn on_event(&mut self, event: &Event, ctx: &EventContext);
}

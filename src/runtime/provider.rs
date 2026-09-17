// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The seam between the language core and everything the core does not
//! implement itself (bucket 0.5.1d): HOST capabilities and `BN*` library
//! modules. The core routes a callee to a provider and hands it a
//! [`CoreContext`]; the provider owns its own resources and never sees the
//! `Executor`.

use std::{collections::HashMap, sync::Arc};

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use bn_ir::Module;

use crate::heap::Heap;
use crate::runtime::HostEnv;

/// What a provider may ask of the core while serving a call. Grows only when
/// a migrated provider needs it (`docs/architecture/interp-extraction.md`).
pub trait CoreContext {
    /// Pointer regions (`NEW T[n]`), for providers that read vectors by handle.
    fn memory(&self) -> &Heap<Value>;

    /// Mutable regions, for providers that copy results into a caller's
    /// buffer (`BNData` `Copy*Column`).
    fn memory_mut(&mut self) -> &mut Heap<Value>;

    /// Calls a BN function by its IR name (web handlers, dispatch tasks).
    ///
    /// # Errors
    ///
    /// Propagates the callee's runtime diagnostic.
    fn call_function(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic>;

    /// Calls another library through the same contract a BN program uses
    /// (`BNLog` `Logger.Log` from `BNWeb`'s access log).
    ///
    /// # Errors
    ///
    /// Propagates the library's runtime diagnostic.
    fn library_call(
        &mut self,
        library: &'static str,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic>;

    /// The program's standard output (console transports, `PRINT` parity).
    fn output(&mut self) -> &mut dyn std::io::Write;

    /// The validated IR being executed (import facts such as
    /// `filesystem_import`).
    fn module(&self) -> &Module;

    /// The host environment (policy view).
    fn host(&self) -> &HostEnv;
}

/// One HOST capability or one `BN*` library, as seen by the core.
pub trait Provider: Send {
    /// Serves `member` — the callee with its `#<module>.` prefix removed
    /// (`MEAN`, `DataFrame.RowCount`) — for this provider's module.
    ///
    /// # Errors
    ///
    /// Returns the runtime diagnostic the call produces.
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic>;

    /// `NEW <Class>()` for a class this library owns (`Json`, `DataFrame`).
    /// `None` = not a provider-owned class; the core allocates an ordinary
    /// object.
    ///
    /// # Errors
    ///
    /// Returns the runtime diagnostic the allocation produces.
    fn allocate(&mut self, _class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        None
    }

    /// Ends the binding of a handle this library owns (`RELEASE json`).
    /// `None` = not this provider's value.
    ///
    /// # Errors
    ///
    /// Returns `DOUBLE_RELEASE` when the handle was already released.
    fn release(&mut self, _value: &Value, _span: Span) -> Option<Result<(), Diagnostic>> {
        None
    }

    /// Releases every resource still held (program end, worker teardown).
    fn close_all(&mut self) {}
}

/// Builds a fresh provider for one execution. Every `Executor` (including a
/// forked dispatch worker) gets its own instance, so provider state is never
/// shared between executions.
pub type ProviderFactory = Arc<dyn Fn() -> Box<dyn Provider> + Send + Sync>;

/// Library providers registered on a `HostEnv`, keyed by standard-module
/// name (`"BNMath"`). Absent = "provider unavailable" `Error` at call time,
/// never a language error.
#[derive(Clone, Default)]
pub struct Libraries {
    factories: HashMap<&'static str, ProviderFactory>,
}

impl Libraries {
    pub fn register(&mut self, library: &'static str, factory: ProviderFactory) {
        self.factories.insert(library, factory);
    }

    #[must_use]
    pub fn instantiate(&self) -> HashMap<&'static str, Box<dyn Provider>> {
        self.factories
            .iter()
            .map(|(name, factory)| (*name, factory()))
            .collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }
}

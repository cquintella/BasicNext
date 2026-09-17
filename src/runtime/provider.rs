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

use crate::heap::{Handle, Heap};
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

    /// Allocates an ordinary object of `class` in the core heap, for
    /// libraries whose classes are core objects with provider-side state
    /// (`BNWeb`).
    ///
    /// # Errors
    ///
    /// Propagates the heap's allocation diagnostic.
    fn allocate_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic>;

    /// `NEW <class>()` on another library (`BNWeb` creating `BNLog` `Fields`
    /// for its access log). `None` = no such provider or class.
    fn library_allocate(
        &mut self,
        library: &'static str,
        class: &str,
        span: Span,
    ) -> Option<Result<Value, Diagnostic>>;

    /// `RELEASE` of a handle owned by any library. `None` = no library claims
    /// the value.
    fn library_release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>>;

    /// Removes a library's provider from the registry (`None` = absent or
    /// currently executing). Paired with [`CoreContext::library_insert`]: a
    /// provider that must stay reachable while BN code it invoked runs in this
    /// same executor (`BNWeb` `Server.Dispatch`) parks its state here first,
    /// and a library seeding an isolated executor (`BNWeb` callbacks) edits it
    /// through this pair.
    fn library_take(&mut self, library: &'static str) -> Option<Box<dyn Provider>>;

    /// Puts a provider (back) into the registry under `library`.
    fn library_insert(&mut self, library: &'static str, provider: Box<dyn Provider>);
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

    /// The core destroyed the object at `handle`; a provider keyed by object
    /// handle drops its side state here.
    fn object_destroyed(&mut self, _handle: Handle) {}

    /// Downcast hook for a library that must reach its own concrete state
    /// through [`CoreContext::library_mut`]. `None` = not offered.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// Builds a fresh provider for one execution. Every `Executor` (including a
/// forked dispatch worker) gets its own instance, so provider state is never
/// shared between executions.
pub type ProviderFactory = Arc<dyn Fn() -> Box<dyn Provider> + Send + Sync>;

/// Providers registered on a `HostEnv`: one registry for `BN*` libraries,
/// keyed by standard-module name (`"BNMath"`), and a separate one for `HOST`
/// capabilities, keyed by the segment after `HOST.` (`"Net"`). Absent =
/// "provider unavailable" `Error` at call time, never a language error.
#[derive(Clone, Default)]
pub struct Providers {
    factories: HashMap<&'static str, ProviderFactory>,
}

impl Providers {
    pub fn register(&mut self, name: &'static str, factory: ProviderFactory) {
        self.factories.insert(name, factory);
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

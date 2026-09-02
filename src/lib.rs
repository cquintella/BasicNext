// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub mod ast;
pub(crate) mod config;
pub mod dap;
pub mod dataframe;
pub mod diagnostic;
mod dispatch;
pub mod heap;
pub(crate) mod http;
pub mod ir;
pub(crate) mod json;
pub mod keyword_registry;
pub mod lexer;
pub mod llvm;
pub(crate) mod log;
pub mod lsp;
pub mod module_graph;
pub mod net;
pub mod parser;
pub mod runtime;
pub mod semantic;
pub mod source;
pub mod temporal;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod tls;
pub mod token;
pub(crate) mod web;
pub(crate) mod web_state;

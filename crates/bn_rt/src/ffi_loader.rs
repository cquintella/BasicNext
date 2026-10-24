#![allow(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dynamic library and symbol resolution for `BNFfi`.

use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use libloading::Library;

use super::ffi::{
    ERROR_INVALID_ARGUMENT, ERROR_LIBRARY_LOAD_FAILED, ERROR_LIBRARY_NOT_FOUND,
    ERROR_SYMBOL_NOT_FOUND, FfiError,
};

type LibraryCache = Arc<Mutex<HashMap<String, Arc<Library>>>>;
type SymbolCache = Arc<Mutex<HashMap<(String, String), usize>>>;

fn libraries_map() -> &'static Mutex<HashMap<String, Arc<Library>>> {
    static LIBRARIES: OnceLock<LibraryCache> = OnceLock::new();
    LIBRARIES.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn symbols_map() -> &'static Mutex<HashMap<(String, String), usize>> {
    static SYMBOLS: OnceLock<SymbolCache> = OnceLock::new();
    SYMBOLS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Resolves and returns a dynamically loaded library by name or path.
///
/// Precedence order per Decision D4:
/// 1. Direct path if name contains `/` or `\`
/// 2. Given exact filename if name contains `.`
/// 3. Search `--ffi-path` directories (if supplied)
/// 4. Search `BN_LIBRARY_PATH` directories
/// 5. System loader search path
pub fn resolve_library(name: &str, ffi_paths: &[PathBuf]) -> Result<Arc<Library>, FfiError> {
    if name.is_empty() || name.contains('\0') {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "library name cannot be empty or contain NUL",
        ));
    }

    {
        let cache = libraries_map().lock().expect("lock libraries");
        if let Some(lib) = cache.get(name) {
            return Ok(Arc::clone(lib));
        }
    }

    let is_direct_path = name.contains('/') || name.contains('\\');
    if is_direct_path {
        let path = Path::new(name);
        return load_single_path(name, path);
    }

    let filename = if name.contains('.') {
        name.to_string()
    } else {
        #[cfg(target_os = "windows")]
        {
            format!("{name}.dll")
        }
        #[cfg(target_os = "macos")]
        {
            format!("lib{name}.dylib")
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            format!("lib{name}.so")
        }
    };

    let mut tried_locations: Vec<String> = Vec::new();

    // 1. Search `--ffi-path` directories.
    for dir in ffi_paths {
        let candidate = dir.join(&filename);
        tried_locations.push(candidate.display().to_string());
        if candidate.is_file() {
            return load_single_path(name, &candidate);
        }
    }

    // 2. Search `BN_LIBRARY_PATH` directories.
    if let Ok(val) = std::env::var("BN_LIBRARY_PATH") {
        for part in std::env::split_paths(&val) {
            if part.as_os_str().is_empty() {
                continue;
            }
            let candidate = part.join(&filename);
            tried_locations.push(candidate.display().to_string());
            if candidate.is_file() {
                return load_single_path(name, &candidate);
            }
        }
    }

    // 3. System loader search path.
    tried_locations.push(format!("system loader: {filename}"));
    match load_from_system(&filename) {
        Ok(lib) => {
            let mut cache = libraries_map().lock().expect("lock libraries");
            cache.insert(name.to_string(), Arc::clone(&lib));
            Ok(lib)
        }
        Err(err) => {
            let msg = format!(
                "library '{}' not found; tried: {}",
                name,
                tried_locations.join(", ")
            );
            Err(FfiError::with_cause(
                ERROR_LIBRARY_NOT_FOUND,
                msg,
                err.cause.unwrap_or_default(),
            ))
        }
    }
}

#[allow(unsafe_code)]
fn load_single_path(name: &str, path: &Path) -> Result<Arc<Library>, FfiError> {
    if !path.exists() {
        return Err(FfiError::new(
            ERROR_LIBRARY_NOT_FOUND,
            format!("library not found at path: {}", path.display()),
        ));
    }

    // SAFETY: Library path has been verified to exist on disk. Loading dynamic library
    // runs any constructors embedded in the shared object.
    let lib_result = unsafe { Library::new(path) };

    match lib_result {
        Ok(loaded) => {
            let arc = Arc::new(loaded);
            let mut cache = libraries_map().lock().expect("lock libraries");
            cache.insert(name.to_string(), Arc::clone(&arc));
            Ok(arc)
        }
        Err(e) => Err(FfiError::with_cause(
            ERROR_LIBRARY_LOAD_FAILED,
            format!("failed to load library at {}: {e}", path.display()),
            e.to_string(),
        )),
    }
}

#[allow(unsafe_code)]
fn load_from_system(filename: &str) -> Result<Arc<Library>, FfiError> {
    #[cfg(target_os = "windows")]
    let lib_result = {
        use libloading::os::windows::Library as WinLib;
        // 0x00001000 is LOAD_LIBRARY_SEARCH_DEFAULT_DIRS
        const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x0000_1000;
        // SAFETY: Invoking LoadLibraryExW with LOAD_LIBRARY_SEARCH_DEFAULT_DIRS per D4.
        unsafe {
            WinLib::load_with_flags(filename, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS).map(Library::from)
        }
    };

    #[cfg(not(target_os = "windows"))]
    let lib_result = {
        // SAFETY: Invoking platform dlopen for system library loading per D4.
        unsafe { Library::new(filename) }
    };

    match lib_result {
        Ok(loaded) => Ok(Arc::new(loaded)),
        Err(e) => Err(FfiError::with_cause(
            ERROR_LIBRARY_LOAD_FAILED,
            format!("system loader failed to load '{filename}': {e}"),
            e.to_string(),
        )),
    }
}

/// Resolves a symbol pointer from a library, using the symbol cache.
#[allow(unsafe_code)]
pub fn resolve_symbol(
    library_name: &str,
    symbol_name: &str,
    library: &Library,
) -> Result<usize, FfiError> {
    if symbol_name.is_empty() || symbol_name.contains('\0') {
        return Err(FfiError::new(
            ERROR_INVALID_ARGUMENT,
            "symbol name cannot be empty or contain NUL",
        ));
    }

    let cache_key = (library_name.to_string(), symbol_name.to_string());
    {
        let cache = symbols_map().lock().expect("lock symbols");
        if let Some(&ptr) = cache.get(&cache_key) {
            return Ok(ptr);
        }
    }

    let c_sym = CString::new(symbol_name)
        .map_err(|_| FfiError::new(ERROR_INVALID_ARGUMENT, "invalid symbol name CString"))?;

    // SAFETY: Symbol lookup in open library using valid NUL-terminated byte string.
    let sym_result = unsafe { library.get::<*const c_void>(c_sym.as_bytes_with_nul()) };

    match sym_result {
        Ok(sym) => {
            let ptr = *sym as usize;
            let mut cache = symbols_map().lock().expect("lock symbols");
            cache.insert(cache_key, ptr);
            Ok(ptr)
        }
        Err(e) => Err(FfiError::with_cause(
            ERROR_SYMBOL_NOT_FOUND,
            format!("symbol '{symbol_name}' not found in library '{library_name}'"),
            e.to_string(),
        )),
    }
}

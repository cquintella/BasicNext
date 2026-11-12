// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The one store of `BNCrypto.Bytes`: every byte value of both backends and of
//! every library (`BNCrypto`, `BNWeb`) lives here, so a handle names the same
//! bytes whichever library made it. The C ABI of `crypto_abi` and the
//! interpreter providers read and write through these functions only.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use zeroize::Zeroize;

use crate::counted::{Counted, Release};
use crate::crypto_abi::next_handle;

/// The byte buffers of the module copy that runs on this thread
/// (`bn_core_copy`): a task's are freed when the task ends.
#[derive(Default)]
struct Buffers(Mutex<Counted<zeroize::Zeroizing<Vec<u8>>>>);

pub fn with_buffers<T>(
    operation: impl FnOnce(&mut HashMap<u64, zeroize::Zeroizing<Vec<u8>>>) -> T,
) -> T {
    let buffers = bn_core_copy::table::<Buffers>();
    operation(
        &mut buffers
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values,
    )
}

pub fn store(bytes: Vec<u8>) -> u64 {
    let handle = next_handle();
    bn_core_copy::table::<Buffers>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, zeroize::Zeroizing::new(bytes));
    handle
}

/// A copy of the bytes behind `handle`, or `None` when it is unknown or
/// released. Every `BNCrypto.Bytes` of both backends lives in this one store,
/// whichever library made it.
#[must_use]
pub fn bytes(handle: u64) -> Option<Vec<u8>> {
    with_buffers(|buffers| buffers.get(&handle).map(|bytes| bytes.to_vec()))
}

/// The length of the bytes behind `handle`, or `None` when it is unknown.
#[must_use]
pub fn bytes_len(handle: u64) -> Option<usize> {
    with_buffers(|buffers| buffers.get(&handle).map(|bytes| bytes.len()))
}

/// One more binding holds `handle` (a copy); `false` when it is unknown or
/// released. Handle 0 is the error case of `Bytes OR Error`, which holds no
/// buffer.
#[must_use]
pub fn retain_bytes(handle: u64) -> bool {
    handle == 0
        || bn_core_copy::table::<Buffers>()
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(handle)
}

/// Drops one holder of `handle`; the last one zeroizes and frees the bytes.
/// `false` when it is unknown or already released.
#[must_use]
pub fn release_bytes(handle: u64) -> bool {
    if handle == 0 {
        return true;
    }
    let released = bn_core_copy::table::<Buffers>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .release(handle);
    match released {
        Release::Last(mut buf) => {
            buf.zeroize();
            true
        }
        Release::Held => true,
        Release::Unknown => false,
    }
}

/// Overwrites the bytes behind `handle` with zeros, keeping its length;
/// `false` when it is unknown.
#[must_use]
pub fn zeroize_bytes(handle: u64) -> bool {
    with_buffers(|buffers| {
        buffers
            .get_mut(&handle)
            .map(|buf| buf.as_mut_slice().zeroize())
            .is_some()
    })
}

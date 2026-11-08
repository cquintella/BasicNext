// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Process control for `HOST.Exec` (BDFL decision, 2026-11-06): the one
//! place its operating-system calls live, so the `bn_host_exec` core stays
//! safe Rust. Both adapters hand [`OsProcessControl`] to `bn_host_exec::run`.
//! A timeout ends the child and everything it started (BDFL decision,
//! 2026-11-05).
//!
//! Unix: the child leads a new process group, and the timeout sends
//! `SIGKILL` to the group. The kernel keeps a process-group id in use while
//! any member lives, so while one does the id names this tree alone. Once the
//! child is reaped and the last member is gone, a recycled pid that leads a
//! group of its own could reuse the id before the call ends; the window is
//! at most the call's timeout, and the risk is accepted (recorded as an open
//! issue).
//!
//! Windows: the child is assigned to a job object, and the timeout
//! terminates the job.
//!
//! A descendant that leaves the tree on purpose (`setsid`, or a job breakaway)
//! is outside it; its inherited pipe ends keep the capture threads alive
//! until it exits (recorded as an open issue).

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

use std::io;
use std::process::Child;

use bn_host_exec::{ContainedTree, ProcessControl};

#[cfg(unix)]
use unix::ProcessTree;
#[cfg(windows)]
use windows::ProcessTree;

/// The result of the call that ends a tree: a tree that is already gone is
/// success.
type KillResult = io::Result<()>;

/// The operating system's process control: a process group (Unix) or a
/// job object (Windows) per call.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsProcessControl;

impl ProcessControl for OsProcessControl {
    fn contain(&self, child: &Child) -> io::Result<Box<dyn ContainedTree>> {
        Ok(Box::new(ProcessTree::adopt(child)?))
    }
}

impl ContainedTree for ProcessTree {
    fn kill(&self) -> io::Result<()> {
        self.kill_tree()
    }
}

#[cfg(unix)]
mod unix {
    use super::{Child, KillResult};
    use std::io;
    use std::sync::Once;
    use std::sync::atomic::{AtomicI32, Ordering};

    /// Process groups of `HOST.Exec` calls that can be live at once and still
    /// be ended by a signal to the program; a group past this many is ended by
    /// its timeout only.
    const LIVE_GROUPS_MAX: usize = 256;
    /// An empty slot of [`LIVE_GROUPS`].
    const NO_GROUP: i32 = 0;
    /// The signals that end a program and must end its Exec children: the
    /// terminal's interrupt and hang-up, and a termination request.
    const ENDING_SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

    /// The live groups, in slots a signal handler can read: atomics only, no
    /// lock and no allocation.
    static LIVE_GROUPS: [AtomicI32; LIVE_GROUPS_MAX] =
        [const { AtomicI32::new(NO_GROUP) }; LIVE_GROUPS_MAX];
    static INSTALL: Once = Once::new();

    /// The child leads its own group, so the terminal's Ctrl-C reaches the
    /// program and not the child. This handler ends every live group and then
    /// lets the signal end the program as it would have: default action,
    /// raised again. It calls only async-signal-safe functions.
    #[allow(unsafe_code)]
    extern "C" fn end_groups_then_reraise(signal: libc::c_int) {
        for slot in &LIVE_GROUPS {
            let group = slot.load(Ordering::Acquire);
            if group != NO_GROUP {
                // SAFETY: `kill` is on the POSIX list of async-signal-safe
                // functions and touches no memory; a negative pid names the
                // group (`killpg` is not on that list).
                unsafe { libc::kill(-group, libc::SIGKILL) };
            }
        }
        // SAFETY: `signal` and `raise` are async-signal-safe; the signal is
        // blocked while this handler runs, so it is delivered with its
        // default action as soon as the handler returns.
        unsafe { libc::signal(signal, libc::SIG_DFL) };
        // SAFETY: as above.
        unsafe { libc::raise(signal) };
    }

    /// Installs the handler for each ending signal whose disposition is still
    /// the default: a handler the program or its host installed is kept, and
    /// an ignored signal stays ignored.
    #[allow(unsafe_code)]
    fn install_handler() {
        for signal in ENDING_SIGNALS {
            // SAFETY: an all-zero `sigaction` is a valid value for the kernel
            // to overwrite.
            let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
            // SAFETY: a null new action only reads the current one into
            // `current`, a live local.
            if unsafe { libc::sigaction(signal, std::ptr::null(), &raw mut current) } != 0
                || current.sa_sigaction != libc::SIG_DFL
            {
                continue;
            }
            // SAFETY: as above.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = (end_groups_then_reraise as *const ()).expose_provenance();
            action.sa_flags = libc::SA_RESTART;
            // SAFETY: `action` is a live, fully initialised local; the
            // handler is a plain `extern "C"` function that lives forever.
            unsafe { libc::sigaction(signal, &raw const action, std::ptr::null_mut()) };
        }
    }

    /// The process group a child leads, registered while the call lives.
    pub(crate) struct ProcessTree {
        group: libc::pid_t,
        slot: Option<usize>,
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            if let Some(slot) = self.slot {
                LIVE_GROUPS[slot].store(NO_GROUP, Ordering::Release);
            }
        }
    }

    impl ProcessTree {
        /// Names the tree of a child the core started in a new process group.
        ///
        /// # Errors
        ///
        /// Never fails on Unix; the signature matches Windows, where the
        /// child must be assigned to a job.
        pub(crate) fn adopt(child: &Child) -> io::Result<Self> {
            let group = libc::pid_t::try_from(child.id())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            INSTALL.call_once(install_handler);
            let slot = LIVE_GROUPS.iter().position(|slot| {
                slot.compare_exchange(NO_GROUP, group, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            });
            Ok(Self { group, slot })
        }

        /// Sends `SIGKILL` to every member of the group.
        ///
        /// # Errors
        ///
        /// The operating-system error, except that an empty group (`ESRCH`)
        /// is success.
        #[allow(unsafe_code)]
        pub(crate) fn kill_tree(&self) -> KillResult {
            // SAFETY: `killpg` takes two integers and touches no memory of
            // ours. `group` is the id of a group this call created, which
            // the kernel does not reuse while a member lives.
            let result = unsafe { libc::killpg(self.group, libc::SIGKILL) };
            if result == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{Child, KillResult};
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    /// The exit code of a process the job terminates.
    const TERMINATED_EXIT_CODE: u32 = 1;

    /// A job object holding the child and what it starts.
    pub struct ProcessTree {
        job: HANDLE,
    }

    impl ProcessTree {
        /// Puts a spawned child in a new job. The child was created suspended
        /// (`CREATE_SUSPENDED`); after the assignment, its thread is resumed,
        /// ensuring no descendant can escape containment.
        ///
        /// # Errors
        ///
        /// The operating-system error when the job cannot be created, the
        /// child cannot be assigned to it, or the thread cannot be resumed.
        #[allow(unsafe_code)]
        pub(crate) fn adopt(child: &Child) -> io::Result<Self> {
            // SAFETY: both arguments may be null: no security attributes and
            // an unnamed job. The returned handle is checked and closed in
            // `Drop`.
            let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let tree = Self { job };
            // SAFETY: `job` is the live handle above; the process handle is
            // owned by `child`, which outlives this call.
            let assigned = unsafe {
                AssignProcessToJobObject(tree.job, child.as_raw_handle().cast::<c_void>())
            };
            if assigned == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `child.id()` is the live process ID. We resume its
            // thread after assigning it to the job object so nothing it
            // starts can escape the job.
            unsafe { resume_process(child.id()) }?;
            Ok(tree)
        }

        /// Terminates every process of the job.
        ///
        /// # Errors
        ///
        /// The operating-system error.
        #[allow(unsafe_code)]
        pub(crate) fn kill_tree(&self) -> KillResult {
            // SAFETY: `job` is a live job handle owned by `self`.
            if unsafe { TerminateJobObject(self.job, TERMINATED_EXIT_CODE) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    #[allow(unsafe_code)]
    unsafe fn resume_process(pid: u32) -> io::Result<()> {
        // SAFETY: `TH32CS_SNAPTHREAD` creates a snapshot of all threads.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let Ok(dw_size) = u32::try_from(std::mem::size_of::<THREADENTRY32>()) else {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        };
        let mut entry = THREADENTRY32 {
            dwSize: dw_size,
            cntUsage: 0,
            th32ThreadID: 0,
            th32OwnerProcessID: 0,
            tpBasePri: 0,
            tpDeltaPri: 0,
            dwFlags: 0,
        };
        // SAFETY: `snapshot` is valid; `entry` size is initialized.
        let mut success = unsafe { Thread32First(snapshot, &raw mut entry) };
        let mut resumed = false;
        while success != 0 {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: thread handle opened for resume rights.
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if !thread.is_null() {
                    // SAFETY: valid thread handle.
                    let count = unsafe { ResumeThread(thread) };
                    // SAFETY: valid thread handle.
                    unsafe { CloseHandle(thread) };
                    if count != u32::MAX {
                        resumed = true;
                    }
                }
            }
            // SAFETY: `snapshot` is valid.
            success = unsafe { Thread32Next(snapshot, &raw mut entry) };
        }
        // SAFETY: closing snapshot handle.
        unsafe { CloseHandle(snapshot) };
        if resumed {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    impl Drop for ProcessTree {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            // SAFETY: `job` is owned by `self` and closed once. Closing does
            // not end the processes: the job has no kill-on-close limit, so
            // a background process of a call that finished keeps running.
            unsafe { CloseHandle(self.job) };
        }
    }
}

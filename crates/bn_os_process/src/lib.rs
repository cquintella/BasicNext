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
//! any member lives, so the id cannot name another process even after the
//! child has been reaped.
//!
//! Windows: the child is assigned to a job object, and the timeout
//! terminates the job.
//!
//! A descendant that leaves the tree on purpose (`setsid`, or a job breakaway)
//! is outside it; its inherited pipe ends keep the capture threads alive
//! until it exits (recorded as an open issue).

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
                // SAFETY: `killpg` is async-signal-safe and touches no memory.
                unsafe { libc::killpg(group, libc::SIGKILL) };
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
    pub struct ProcessTree {
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
        pub fn adopt(child: &Child) -> io::Result<Self> {
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
        pub fn kill_tree(&self) -> KillResult {
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
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    };

    /// The exit code of a process the job terminates.
    const TERMINATED_EXIT_CODE: u32 = 1;

    /// A job object holding the child and what it starts.
    pub struct ProcessTree {
        job: HANDLE,
    }

    impl ProcessTree {
        /// Puts a spawned child in a new job. The child runs for a moment
        /// before the assignment; a process it starts in that moment is
        /// outside the job (recorded as an open issue).
        ///
        /// # Errors
        ///
        /// The operating-system error when the job cannot be created or the
        /// child cannot be assigned to it.
        #[allow(unsafe_code)]
        pub fn adopt(child: &Child) -> io::Result<Self> {
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
            Ok(tree)
        }

        /// Terminates every process of the job.
        ///
        /// # Errors
        ///
        /// The operating-system error.
        #[allow(unsafe_code)]
        pub fn kill_tree(&self) -> KillResult {
            // SAFETY: `job` is a live job handle owned by `self`.
            if unsafe { TerminateJobObject(self.job, TERMINATED_EXIT_CODE) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
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

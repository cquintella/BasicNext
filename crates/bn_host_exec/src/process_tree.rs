// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The process tree of one `HOST.Exec.Run` call (BDFL decision, 2026-11-05,
//! D-1: a timeout ends the child and everything it started).
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
use std::process::{Child, Command};

#[cfg(unix)]
pub use unix::ProcessTree;
#[cfg(windows)]
pub use windows::ProcessTree;

/// Prepares `command` so its child starts a tree of its own. Call before
/// `spawn`.
pub fn isolate(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Process group 0: a new group whose id is the child's pid.
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// The result of the call that ends a tree: a tree that is already gone is
/// success.
pub type KillResult = io::Result<()>;

#[cfg(unix)]
mod unix {
    use super::{Child, KillResult};
    use std::io;

    /// The process group a child leads.
    pub struct ProcessTree {
        group: libc::pid_t,
    }

    impl ProcessTree {
        /// Names the tree of a child spawned after [`super::isolate`].
        ///
        /// # Errors
        ///
        /// Never fails on Unix; the signature matches Windows, where the
        /// child must be assigned to a job.
        pub fn adopt(child: &Child) -> io::Result<Self> {
            let group = libc::pid_t::try_from(child.id())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            Ok(Self { group })
        }

        /// Sends `SIGKILL` to every member of the group.
        ///
        /// # Errors
        ///
        /// The operating-system error, except that an empty group (`ESRCH`)
        /// is success.
        #[allow(unsafe_code)]
        pub fn kill(&self) -> KillResult {
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
        pub fn kill(&self) -> KillResult {
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

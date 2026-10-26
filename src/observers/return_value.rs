//! Observer: Return value (rv) tracking
//!
//! This module provides `ReturnValueObserver`, a lightweight libafl `Observer`
//! which captures an i32 return value produced by the instrumented target.
//!
//! Design notes:
//! - The observer reads the return value from an optional shared memory mapping
//!   (`StdShMem`) that the target writes into. The mapping can be attached at
//!   construction time or injected later via `set_shmem` (the latter is used by
//!   the runtime in `main.rs`). If no shmem is attached, the observer will not
//!   update `last_value`.
//! - TLS-based return-value passing used to exist but is intentionally removed
//!   from the codebase; the harness now communicates via shmem in forkserver
//!   executions. See `main.rs` where `set_shmem` is used to attach the mapping.

use crate::utils::logging::debug;
use core::fmt::Debug;
use libafl::{executors::ExitKind, observers::Observer, state::HasExecutions, Error};
use libafl_bolts::shmem::StdShMem;
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// An observer that tracks the return value (i32) of harness executions
#[derive(Debug, Serialize, Deserialize)]
pub struct ReturnValueObserver {
    name: Cow<'static, str>,
    /// The last observed numeric return value (from the harness)
    last_value: Option<i32>,
    /// Also keep track of last ExitKind for convenience
    last_exit_kind: Option<ExitKind>,
    /// Optional shared memory region that the target writes its numeric return into.
    /// If present, `post_exec*` will try to read an i32 from the start of this mapping.
    #[serde(skip)]
    rv_shmem: Option<StdShMem>,
}

impl ReturnValueObserver {
    pub fn new(name: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(name),
            last_value: None,
            last_exit_kind: None,
            rv_shmem: None,
        }
    }

    /// Attach/replace the rv shmem mapping for this observer.
    pub fn set_shmem(&mut self, shmem: StdShMem) {
        self.rv_shmem = Some(shmem);
    }

    #[must_use]
    pub fn last_value(&self) -> Option<i32> {
        self.last_value
    }

    /// Snapshot the first `n` bytes of the attached shmem (if any).
    /// Returns a Vec<u8> copy of the bytes.
    pub fn snapshot_bytes(&self, n: usize) -> Option<Vec<u8>> {
        if let Some(shm) = &self.rv_shmem {
            let len = usize::min(shm.len(), n);
            let slice = &shm[..len];
            return Some(slice.to_vec());
        }
        None
    }
}

/// Whether return-value observer debug logging is enabled via env var
fn rv_obs_debug_enabled() -> bool {
    crate::driver::env_var::env_bool_cached(crate::driver::env_var::RV_OBS_DEBUG)
}

impl Named for ReturnValueObserver {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<I, S> Observer<I, S> for ReturnValueObserver
where
    S: HasExecutions,
{
    fn pre_exec(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        Ok(())
    }

    fn post_exec(&mut self, _state: &mut S, _input: &I, exit_kind: &ExitKind) -> Result<(), Error> {
        // Record ExitKind
        self.last_exit_kind = Some(exit_kind.clone());

        // Prefer shmem if present (forkserver targets will write into shmem).
        // If no shmem is present, do not update last_value (TLS is deprecated).
        if let Some(shm) = &mut self.rv_shmem {
            // read first 4 bytes as native-endian i32 if possible (safe slice-based read)
            if shm.len() >= 4 {
                // Obtain a slice of the first 4 bytes and convert to i32 according to native endianness
                let slice = &shm[..4];
                let mut arr = [0u8; 4];
                arr.copy_from_slice(slice);
                let v = i32::from_ne_bytes(arr);
                self.last_value = Some(v);
            }
        }

        // Keep debug logging for diagnostics, controlled by DIFFRAY_RV_OBS_DEBUG
        if rv_obs_debug_enabled() {
            if let Some(v) = self.last_value {
                debug(
                    "return_value",
                    &format!("observed return value = {} ({:?})", v, exit_kind),
                );
            } else {
                debug(
                    "return_value",
                    &format!("no return value observed ({:?})", exit_kind),
                );
            }
            // If we have shmem, also dump a small hex snapshot for diagnostics
            if let Some(shm) = &self.rv_shmem {
                let len = shm.len();
                let snap = self.snapshot_bytes(8).unwrap_or_default();
                let hex: String = snap
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ");
                debug(
                    "return_value",
                    &format!("shmem len={} snapshot={}", len, hex),
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libafl::executors::ExitKind;

    struct DummyState {
        executions: u64,
    }

    impl DummyState {
        fn new() -> Self {
            Self { executions: 0 }
        }
    }

    impl HasExecutions for DummyState {
        fn executions(&self) -> &u64 {
            &self.executions
        }
        fn executions_mut(&mut self) -> &mut u64 {
            &mut self.executions
        }
    }

    #[test]
    fn observer_no_shmem_keeps_none() {
        // TLS is deprecated; without shmem attached, the observer will not update last_value
        let mut obs = ReturnValueObserver::new("rv");
        let mut state = DummyState::new();
        obs.post_exec(&mut state, &(), &ExitKind::Ok).unwrap();
        assert_eq!(obs.last_value(), None);
    }

    #[test]
    fn observer_reads_from_shmem() {
        // allocate a small shared memory and write an i32 into it
        use libafl_bolts::shmem::{ShMemProvider, StdShMemProvider};

        let mut provider = StdShMemProvider::new().expect("failed to create shmem provider");
        // allocate 8 bytes, more than enough for an i32
        let mut shm = provider.new_shmem(8).expect("failed to alloc shmem");

        let value: i32 = 0x12345678;
        let bytes = value.to_ne_bytes();
        // write bytes into shmem backing
        let ptr = shm.as_mut_ptr();
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, 4) };

        // create observer and attach shmem, then post_exec
        let mut obs = ReturnValueObserver::new("rv");
        obs.set_shmem(shm);
        let mut state = DummyState::new();
        obs.post_exec(&mut state, &(), &ExitKind::Ok).unwrap();
        assert_eq!(obs.last_value(), Some(value));
    }
}

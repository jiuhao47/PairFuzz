use std::borrow::Cow;
// helper functions are kept in this module for testability

use libafl::{observers::Observer, Error};
use libafl_bolts::Named;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// Module-local debug helper. Controlled by the environment variable
// `PAIRFUZZ_COVERAGE_COMBINER_DEBUG` (defined in `src/driver/env_var.rs`).
fn combiner_debug_log(tag: &str, msg: &str) {
    if crate::driver::env_var::env_bool_cached(crate::driver::env_var::COVERAGE_COMBINER_DEBUG) {
        crate::utils::logging::debug(tag, msg);
    }
}

/// Combine two optional source slices into the backing buffer.
///
/// Layout convention: combined_backing[0..edges_per_half] <- left,
/// combined_backing[edges_per_half..2*edges_per_half] <- right.
/// The function zeros the provided `combined_backing` slice first and then copies
/// up to `edges_per_half` bytes from each source if present.
pub(crate) fn combine_shmem_into_backing(
    left: Option<&[u8]>,
    right: Option<&[u8]>,
    combined_backing: &mut [u8],
    edges_per_half: usize,
) {
    // zero the backing buffer first (only up to combined_backing.len())
    let backing_len = combined_backing.len();
    for x in combined_backing.iter_mut() {
        *x = 0;
    }
    if edges_per_half == 0 {
        return;
    }
    if let Some(left_s) = left {
        let left_len = usize::min(left_s.len(), edges_per_half);
        let dst_len = usize::min(left_len, backing_len);
        combined_backing[..dst_len].copy_from_slice(&left_s[..dst_len]);
    }
    if let Some(right_s) = right {
        let right_len = usize::min(right_s.len(), edges_per_half);
        let start = edges_per_half;
        if start < backing_len {
            let dst_len = usize::min(right_len, backing_len - start);
            combined_backing[start..start + dst_len].copy_from_slice(&right_s[..dst_len]);
        }
    }
}

// NOTE: The heap-backed `ComboCoverageBuilder` was removed from this module.
// Live combination of coverage is performed by `ShmemComboUpdater` (shared
// memory based). The hashing and shmem-copy helpers below are retained for
// testability and potential future re-introduction of a heap-backed mode.

/// ShmemComboUpdater
///
/// Copies two per-executor shared-memory coverage buffers into a single
/// combined objective backing. The layout is: first `num_edges` bytes are
/// program A, next `num_edges` bytes are program B (combined_len = 2*num_edges).
///
/// Safety: this struct stores raw pointers and `StdShMem` mappings. Callers must
/// ensure any `StdShMem` values passed into `new` live at least as long as the
/// updater (they are moved into the struct). Methods that write into shared
/// mappings perform raw pointer writes and are explicitly documented as unsafe
/// from a Rust aliasing perspective — this is acceptable because the backing
/// is shared memory, not ordinary Rust memory.
#[derive(Debug)]
pub struct ShmemComboUpdater {
    name: Cow<'static, str>,
    num_edges: usize,
    combined_ptr: *mut u8,
    combined_len: usize,
    // Own the combined backing buffer when provided so it is freed when the
    // updater is dropped. This avoids leaking memory when the updater is
    // deserialized or constructed with an owned backing buffer.
    #[allow(dead_code)]
    combined_backing: Option<Box<[u8]>>,
    shmem_a: Option<libafl_bolts::shmem::StdShMem>,
    shmem_b: Option<libafl_bolts::shmem::StdShMem>,
}

impl ShmemComboUpdater {
    /// Create an updater that will copy shmem_a / shmem_b into combined_ptr.
    /// Note: caller must ensure the StdShMem objects live at least as long as
    /// this updater (they are moved into the struct to keep them alive).
    /// Create an updater that will copy shmem_a / shmem_b into combined_ptr.
    /// If `combined_backing` is Some(boxed_slice) the updater will take
    /// ownership of the backing and compute the pointer from it. Otherwise
    /// `combined_ptr` is used as-is (caller is responsible for backing lifetime).
    pub fn new(
        name: &'static str,
        num_edges: usize,
        combined_ptr: *mut u8,
        combined_backing: Option<Box<[u8]>>,
        shmem_a: Option<libafl_bolts::shmem::StdShMem>,
        shmem_b: Option<libafl_bolts::shmem::StdShMem>,
    ) -> Self {
        let combined_len = num_edges.checked_mul(2).unwrap_or(num_edges);
        let actual_ptr = if let Some(ref backing) = combined_backing {
            backing.as_ptr() as *mut u8
        } else {
            combined_ptr
        };
        Self {
            name: Cow::from(name),
            num_edges,
            combined_ptr: actual_ptr,
            combined_len,
            combined_backing,
            shmem_a,
            shmem_b,
        }
    }
}

impl Named for ShmemComboUpdater {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

// Custom serialization: serialize only the name and num_edges. On deserialize
// re-create the updater with a fresh combined buffer and no attached shmems.
impl Serialize for ShmemComboUpdater {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.name.as_ref(), &self.num_edges).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ShmemComboUpdater {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (name, num_edges): (String, usize) = <(String, usize)>::deserialize(deserializer)?;
        // leak the name to obtain a 'static str for the updater
        let boxed: Box<str> = name.into_boxed_str();
        let static_str: &'static str = Box::leak(boxed);

        // allocate combined backing (2 * num_edges) and keep ownership in the
        // updater so it can be dropped later instead of leaking.
        let combined_len = num_edges.checked_mul(2).unwrap_or(num_edges);
        let boxed_slice = vec![0u8; combined_len].into_boxed_slice();
        let combined_ptr = boxed_slice.as_ptr() as *mut u8;

        Ok(ShmemComboUpdater {
            name: Cow::from(static_str),
            num_edges,
            combined_ptr,
            combined_len,
            combined_backing: Some(boxed_slice),
            shmem_a: None,
            shmem_b: None,
        })
    }
}

impl<OTA, OTB, I, S> libafl::observers::DifferentialObserver<OTA, OTB, I, S> for ShmemComboUpdater
where
    S: libafl::state::HasExecutions,
{
    fn post_observe_second(&mut self, _observers: &mut OTB) -> Result<(), libafl::Error> {
        unsafe {
            if self.combined_len == 0 || self.num_edges == 0 {
                return Ok(());
            }
            let combined = core::slice::from_raw_parts_mut(self.combined_ptr, self.combined_len);
            // Use helper to copy shmem slices into combined backing
            let a_slice_opt = self.shmem_a.as_ref().map(|s| &s[..]);
            let b_slice_opt = self.shmem_b.as_ref().map(|s| &s[..]);
            combine_shmem_into_backing(a_slice_opt, b_slice_opt, combined, self.num_edges);
            // Emit debug summary about copy (non-zero counts) only when debug enabled.
            if crate::driver::env_var::env_bool_cached(
                crate::driver::env_var::COVERAGE_COMBINER_DEBUG,
            ) {
                let a_nonzero =
                    a_slice_opt.map_or(0usize, |s| s.iter().filter(|&&b| b != 0).count());
                let b_nonzero =
                    b_slice_opt.map_or(0usize, |s| s.iter().filter(|&&b| b != 0).count());
                combiner_debug_log(
                    "coverage_combiner",
                    &format!(
                        "ShmemComboUpdater::post_observe_second: copied a_nonzero={} b_nonzero={} into combined_len={}",
                        a_nonzero, b_nonzero, self.combined_len
                    ),
                );
            } else {
                combiner_debug_log(
                    "coverage_combiner",
                    &format!(
                        "ShmemComboUpdater::post_observe_second: copied into combined_len={}",
                        self.combined_len
                    ),
                );
            }
            // Optionally schedule zeroing of the source shmems after observers/feedbacks
            // Previously we optionally scheduled zeroing of the source shmems here
            // (to be performed at post_exec time). That logic caused timing issues
            // with collection and is intentionally removed — callers should perform
            // any desired cleanup after collection completes.
        }
        Ok(())
    }
}

impl<_I, _S> Observer<_I, _S> for ShmemComboUpdater {
    fn pre_exec(&mut self, _state: &mut _S, _input: &_I) -> Result<(), Error> {
        // pre_exec intentionally left empty; diagnostics previously present
        // were commented out and have been removed to keep the hot path lean.
        Ok(())
    }

    fn post_exec(
        &mut self,
        _state: &mut _S,
        _input: &_I,
        _exit_kind: &libafl::executors::ExitKind,
    ) -> Result<(), Error> {
        Ok(())
    }
}
// ComboCoverageBuilder and the related helper for allocating a single
// combined backing were removed intentionally. The module retains the
// small helpers and `ShmemComboUpdater` used at runtime.

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_combine_shmem_into_backing_basic() {
        let a = vec![2u8, 3u8, 4u8];
        let b = vec![5u8, 6u8];
        let mut combined = vec![0u8; 10];
        combine_shmem_into_backing(Some(&a), Some(&b), &mut combined, 5);
        // a should occupy combined[0..3]
        assert_eq!(&combined[0..3], &a[..]);
        // b should occupy combined[5..7]
        assert_eq!(&combined[5..7], &b[..]);
        // other slots zero
        assert_eq!(combined[3], 0u8);
        assert_eq!(combined[4], 0u8);
    }
}

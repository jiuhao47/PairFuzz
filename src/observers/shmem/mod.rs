//! Shared-memory helpers and observers (observers-side).
//!
//! This module contains shmem-based observers / updaters that use `libafl_bolts::shmem::StdShMem`.
//! Driver-side allocation helpers remain in `crate::driver::shmem`.
pub mod map_shmem;
pub mod rv_shmem;

// Re-export the primary type for convenience
pub use map_shmem::ShmemComboUpdater;

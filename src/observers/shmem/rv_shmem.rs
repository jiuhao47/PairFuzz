//! Driver-like helpers for allocating and exporting shared memory used by
//! return-value observers (rv observers). These are convenience helpers moved
//! here to co-locate shmem-related code near observer implementations.
//!
//! Note: the original `src/driver/shmem.rs` is kept as a shim that re-exports
//! these helpers to preserve external callers.
use libafl_bolts::shmem::{ShMemProvider, StdShMem, StdShMemProvider};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_tmp_env(base: &str) -> String {
    let pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{:05}_{}", base, pid, now)
}

/// Try to allocate a new shared memory region of `size` bytes using the provided
/// provider and write its id/size into the given `env_name` via
/// `ShMem::write_to_env`. Returns the allocated `StdShMem` on success.
pub fn try_new_shmem_and_write_env(
    provider: &mut StdShMemProvider,
    size: usize,
    env_name: &str,
) -> Result<StdShMem, String> {
    let s = provider.new_shmem(size).map_err(|e| e.to_string())?;
    // propagate write_to_env error if it occurs
    unsafe {
        libafl_bolts::shmem::ShMem::write_to_env(&s, env_name).map_err(|e| e.to_string())?;
    }
    Ok(s)
}

/// Allocate a coverage shmem, write a temporary env (`tmp_env`) so callers can
/// extract a textual id, remove the temporary env and return the pair
/// (shmem, id_string). This mirrors the pattern used in `main.rs`.
pub fn try_alloc_cov_shmem(
    provider: &mut StdShMemProvider,
    size: usize,
    tmp_env: &str,
) -> Result<(StdShMem, String), String> {
    let s = provider.new_shmem(size).map_err(|e| e.to_string())?;
    let uniq = unique_tmp_env(tmp_env);
    unsafe {
        libafl_bolts::shmem::ShMem::write_to_env(&s, &uniq).map_err(|e| e.to_string())?;
    }
    // Read the id and ensure we remove the temporary env in all paths.
    let id_res = env::var(&uniq).map_err(|e| e.to_string());
    let _ = env::remove_var(&uniq);
    let id = id_res?;
    Ok((s, id))
}

/// Given an already-allocated `shmem`, write a temporary env (tmp_env) using
/// `ShMem::write_to_env`, read back the id and size strings, clear the temps,
/// and return a `(id_string, size_string)` tuple. If the id is a pure number
/// we prepend the `sysv:` tag to match the existing caller-side convention.
pub fn extract_id_size_and_clear(
    shmem: &StdShMem,
    tmp_env: &str,
) -> Result<(String, String), String> {
    let uniq = unique_tmp_env(tmp_env);
    unsafe {
        libafl_bolts::shmem::ShMem::write_to_env(shmem, &uniq).map_err(|e| e.to_string())?;
    }
    // Read id and size, but ensure the temporary env vars are cleared even on
    // error.
    let id_result = env::var(&uniq).map_err(|e| e.to_string());
    let size_result = env::var(format!("{}_SIZE", &uniq)).map_err(|e| e.to_string());
    // clear temps (best-effort)
    let _ = env::remove_var(&uniq);
    let _ = env::remove_var(format!("{}_SIZE", &uniq));

    let id = id_result?;
    let size = size_result?;
    // normalize numeric ids to the `sysv:` form (keeps original behavior)
    let mut id_norm = id.clone();
    if !id_norm.is_empty() && id_norm.chars().all(|c| c.is_ascii_digit()) {
        id_norm = format!("sysv:{}", id_norm);
    }
    Ok((id_norm, size))
}

/// Write an already-allocated shared memory's id into the given environment
/// variable using `ShMem::write_to_env`. This wraps the unsafe call so callers
/// don't need to duplicate the unsafe block.
pub fn write_shmem_to_env(shmem: &StdShMem, env_name: &str) -> Result<(), String> {
    unsafe { libafl_bolts::shmem::ShMem::write_to_env(shmem, env_name).map_err(|e| e.to_string()) }
}

/// Allocate a coverage shmem and return the mapping, a raw mutable pointer to
/// its backing, and the extracted id string. This bundles allocation + id
/// extraction + returning the raw pointer used by observers, which keeps the
/// caller logic minimal while preserving semantics.
pub fn try_alloc_cov_shmem_with_ptr(
    provider: &mut StdShMemProvider,
    size: usize,
    tmp_env: &str,
) -> Result<(StdShMem, *mut u8, String), String> {
    let (mut shmem, id) = try_alloc_cov_shmem(provider, size, tmp_env)?;
    let ptr = shmem.as_mut_ptr();
    Ok((shmem, ptr, id))
}

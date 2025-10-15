//! Centralized environment variable helpers for the project.
use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{OnceLock, RwLock};

/// Read a boolean-like environment variable. Accepts "1" or "true" (case-insensitive).
pub fn env_bool(key: &str) -> bool {
    match env::var(key) {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(e) => match e {
            env::VarError::NotPresent => false,
            env::VarError::NotUnicode(_) => {
                // Warn about non-UTF-8 env value; avoid pulling a logging dependency here.
                eprintln!("warning: env var '{}' contains non-unicode value", key);
                false
            }
        },
    }
}

/// Read a usize environment variable, falling back to default on parse error or missing.
#[allow(dead_code)]
pub fn env_usize_or(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

/// Read an environment variable as string, returning Option<String>
#[allow(dead_code)]
pub fn env_opt_string(key: &str) -> Option<String> {
    env::var(key).ok()
}

// Commonly used DIFFRAY_* names
pub const FEEDBACK_DEBUG: &str = "DIFFRAY_FEEDBACK_DEBUG";
pub const APPEND_METADATA_DEBUG: &str = "DIFFRAY_APPEND_METADATA_DEBUG";
pub const COLLECT_DEBUG: &str = "DIFFRAY_COLLECT_DEBUG";
pub const LOG: &str = "DIFFRAY_LOG";
pub const RV_OBS_DEBUG: &str = "DIFFRAY_RV_OBS_DEBUG";
pub const COVERAGE_COMBINER_DEBUG: &str = "DIFFRAY_COVERAGE_COMBINER_DEBUG";

// A simple global cache for boolean env checks. Values:
// 0 = unknown (not checked yet), 1 = disabled, 2 = enabled
static ENV_BOOL_CACHE: OnceLock<RwLock<HashMap<&'static str, AtomicU8>>> = OnceLock::new();

/// Read a boolean env var but cache the parsed value across calls.
/// Accepts "1" or "true" (case-insensitive) as enabled.
pub fn env_bool_cached(key: &'static str) -> bool {
    // Use RwLock to allow concurrent reads. We do a double-checked pattern:
    // 1) Acquire read lock and check for existing entry.
    // 2) If missing, drop read lock and acquire write lock to insert.
    let map = ENV_BOOL_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    {
        let guard = map.read().unwrap();
        if let Some(entry) = guard.get(key) {
            let v = entry.load(Ordering::Relaxed);
            return v == 2;
        }
    }

    // Entry missing; insert under write lock.
    let mut guard = map.write().unwrap();
    let entry = guard.entry(key).or_insert_with(|| AtomicU8::new(0));
    let v = entry.load(Ordering::Relaxed);
    if v == 0 {
        let enabled = env_bool(key);
        entry.store(if enabled { 2 } else { 1 }, Ordering::Relaxed);
        enabled
    } else {
        v == 2
    }
}

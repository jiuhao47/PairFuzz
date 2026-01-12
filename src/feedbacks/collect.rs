use crate::feedbacks::diffray_feedback::{DiffrayFeedback, Mode, ObserverInfo};
use crate::feedbacks::metadata::TwoStateStats;
use crate::observers::ReturnValueObserver;
use crate::utils::bits::BitsRef;
use libafl::observers::MapObserver;
use libafl::Error;
use libafl::HasNamedMetadata;
use libafl_bolts::tuples::Handle;
use libafl_bolts::tuples::MatchNameRef;
use std::borrow::Cow;
use std::u8;

// Module-local debug helpers (mirror metadata.rs style). Controlled by
// environment variable `DIFFRAY_COLLECT_DEBUG`.
fn collect_debug_log(tag: &str, msg: &str) {
    if crate::driver::env_var::env_bool_cached(crate::driver::env_var::COLLECT_DEBUG) {
        crate::utils::logging::debug(tag, msg);
    }
}

/// Collect-only helpers extracted from `diffray_feedback.rs` to reduce file size
/// and separate concerns. These functions mirror the previous `collect_*` methods
/// and are thin wrappers that operate on `DiffrayFeedback` references.

pub fn collect_found<C, S, OT>(
    f: &DiffrayFeedback<C>,
    idx: usize,
    handle: &Handle<C>,
    state: &S,
    observers: &OT,
) -> Result<bool, Error>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let observer = match observers.get(handle) {
        Some(h) => h.as_ref(),
        None => {
            return Err(Error::illegal_state(format!(
                "collect_found_for_observer: map observer '{}' missing at runtime",
                f.names[idx]
            )));
        }
    };

    let initial = observer.initial();
    let initial_u8: u8 = initial.into();

    let named = state.named_metadata_map();
    let stats_opt = named.get::<TwoStateStats>(&f.names[idx]);
    if stats_opt.is_none() {
        return Ok(false);
    }
    let stats = stats_opt.unwrap();
    // History map was migrated out; decision logic now uses seen_first_bits /
    // max_seen / min_seen directly.

    let mut found = false;
    let max_seen = &stats.max_seen;
    let min_seen = &stats.min_seen;
    let seen_first_bits = BitsRef::new(&stats.seen_first_bits);
    let max_len = max_seen.len();
    let min_len = min_seen.len();

    let cfg = &f.configs[idx];
    // Cache frequently-used cfg fields to reduce repeated field access in hot loop
    let cfg_check_high = cfg.check_high;
    let cfg_check_low = cfg.check_low;
    let cfg_high_delta = cfg.high_delta;
    let cfg_low_delta = cfg.low_delta;
    let initial_breach = |i: usize| -> bool { seen_first_bits.get(i) && cfg_check_low };

    for (i, item_ref) in observer.as_iter().enumerate() {
        let item = *item_ref;

        let cur: u8 = item.into();

        if cur == initial_u8 {
            if initial_breach(i) {
                found = true;
                break;
            }
            continue;
        }
        let max = if i < max_len { max_seen[i] } else { u8::MIN };
        let min = if i < min_len { min_seen[i] } else { u8::MAX };
        // Determine found status using seen_first_bits/max_seen/min_seen instead
        // Cache seen_first_bits.get(i) to avoid repeated access calls
        let prev_seen_first_bool = seen_first_bits.get(i);
        // Condition 1: never seen before (first-seen not set) and current != initial
        if !prev_seen_first_bool && cur != initial_u8 {
            found = true;
            break;
        }
        // Condition 2: high breach - current exceeds historical max
        if cfg_check_high && cur.saturating_sub(max) >= cfg_high_delta {
            found = true;
            break;
        }
        // Condition 3: low breach - current is lower than historical min, but
        // only consider if this index has been seen before (prev_seen_first_bool)
        if cfg_check_low
            && prev_seen_first_bool
            && cur < min
            && min.saturating_sub(cur) >= cfg_low_delta
        {
            found = true;
            break;
        }
    }

    Ok(found)
}

/// Generalized RV collector: returns the (name, last_value) for the RV handle
/// configured at `f.rv_refs[idx]` if present. This replaces the prior two
/// variants and centralizes RV collection logic.
pub fn collect_rv<C, OT>(
    f: &DiffrayFeedback<C>,
    idx: usize,
    observers: &OT,
) -> Result<Option<(String, Option<i32>)>, Error>
where
    OT: MatchNameRef,
{
    if let Some(opt_h) = f.rv_refs.get(idx).and_then(|r| r.as_ref()) {
        if let Some(rv_obs) = observers.get::<ReturnValueObserver>(opt_h) {
            return Ok(Some((opt_h.name().to_string(), rv_obs.last_value())));
        } else {
            return Err(Error::illegal_state(&format!(
                "collect_rv: rv handle '{}' present but ReturnValueObserver missing",
                opt_h.name()
            )));
        }
    }
    Ok(None)
}

pub fn collect_indices<C, S, OT>(
    f: &DiffrayFeedback<C>,
    _idx: usize,
    handle: &Handle<C>,
    _state: &S,
    observers: &OT,
) -> Result<smallvec::SmallVec<[(usize, u8); 16]>, Error>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let observer = match observers.get(handle) {
        Some(h) => h.as_ref(),
        None => {
            return Err(Error::illegal_state(format!(
                "collect_indices_for_observer: map observer '{}' missing at runtime",
                f.names[_idx]
            )));
        }
    };

    let initial = observer.initial();
    let initial_u8: u8 = initial.into();

    let mut indices: smallvec::SmallVec<[(usize, u8); 16]> = smallvec::SmallVec::with_capacity(16);
    for (i, item_ref) in observer.as_iter().enumerate() {
        let cur: u8 = (*item_ref).into();
        if cur != initial_u8 {
            indices.push((i, cur));
        }
    }

    // Diagnostic logging: when enabled, print initial sentinel, count and a small sample
    if !indices.is_empty() {
        // Only perform heavy formatting if debug logging is enabled to avoid
        // unnecessary allocations in the hot path.
        if crate::driver::env_var::env_bool_cached(crate::driver::env_var::COLLECT_DEBUG) {
            let sample_cap = 16usize;
            let mut s = String::with_capacity(128);
            use core::fmt::Write;
            for (i, v) in indices.iter().take(sample_cap) {
                if !s.is_empty() {
                    s.push_str(", ");
                }
                write!(s, "{}={}", i, v).ok();
            }
            collect_debug_log(
                "collect_indices",
                &format!(
                    "initial={} collected={} sample=[{}]",
                    initial_u8,
                    indices.len(),
                    s
                ),
            );
        } else {
            // cheap log with minimal formatting
            collect_debug_log(
                "collect_indices",
                &format!("initial={} collected={}", initial_u8, indices.len()),
            );
        }
    } else if crate::driver::env_var::env_bool_cached(crate::driver::env_var::COLLECT_DEBUG) {
        // only format the 0 case when debug enabled
        collect_debug_log(
            "collect_indices",
            &format!("initial={} collected=0", initial_u8),
        );
    }

    Ok(indices)
}

pub fn collect<C, S, OT>(
    f: &DiffrayFeedback<C>,
    state: &S,
    observers: &OT,
) -> Result<Vec<ObserverInfo>, Error>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let mut infos: Vec<ObserverInfo> = Vec::with_capacity(f.map_refs.len());

    match f.mode {
        Mode::Debug => {
            for (idx, handle) in f.map_refs.iter().enumerate() {
                let found = collect_found(f, idx, handle, state, observers)?;
                let indices = match collect_indices(f, idx, handle, state, observers) {
                    Ok(v) => {
                        if v.is_empty() {
                            None
                        } else {
                            Some(v)
                        }
                    }
                    Err(e) => return Err(e),
                };
                let rv = collect_rv(f, idx, observers)?;
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(f.names[idx].clone()),
                    found,
                    indices,
                    return_value: rv,
                });
            }
        }
        Mode::SingleRv => {
            for (idx, opt_h) in f.rv_refs.iter().enumerate() {
                if opt_h.is_none() {
                    continue;
                }
                let rv = collect_rv::<C, OT>(f, idx, observers)?;
                let name = opt_h
                    .as_ref()
                    .map(|h| h.name().to_string())
                    .unwrap_or_else(|| f.names.get(idx).map(|s| s.to_string()).unwrap_or_default());
                // Also collect 'found' for RV modes by consulting the map observer
                // semantics. If the corresponding map observer is present, call
                // collect_found to determine whether any notable changes were
                // observed (e.g., first-seen/high/low breaches). This allows the
                // driver to persist coverage only when 'found' is true.
                let found = if let Some(handle) = f.map_refs.get(idx) {
                    collect_found(f, idx, handle, state, observers)?
                } else {
                    false
                };
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(name),
                    found,
                    indices: None,
                    return_value: rv,
                });
            }
        }
        Mode::SingleRvCov => {
            for (idx, opt_h) in f.rv_refs.iter().enumerate() {
                if opt_h.is_none() {
                    continue;
                }
                let rv = collect_rv::<C, OT>(f, idx, observers)?;
                let name = opt_h
                    .as_ref()
                    .map(|h| h.name().to_string())
                    .unwrap_or_else(|| f.names.get(idx).map(|s| s.to_string()).unwrap_or_default());
                // derive found via corresponding map observer if present
                let found = if let Some(handle) = f.map_refs.get(idx) {
                    collect_found(f, idx, handle, state, observers)?
                } else {
                    false
                };
                // collect indices from corresponding map observer if present
                let indices = if let Some(handle) = f.map_refs.get(idx) {
                    let idxs = collect_indices(f, idx, handle, state, observers)?;
                    if idxs.is_empty() {
                        None
                    } else {
                        Some(idxs)
                    }
                } else {
                    None
                };
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(name),
                    found,
                    indices,
                    return_value: rv,
                });
            }
        }
        Mode::ComboRv => {
            for (idx, opt_h) in f.rv_refs.iter().enumerate() {
                if opt_h.is_none() {
                    continue;
                }
                let rv = collect_rv::<C, OT>(f, idx, observers)?;
                let name = opt_h
                    .as_ref()
                    .map(|h| h.name().to_string())
                    .unwrap_or_else(|| f.names.get(idx).map(|s| s.to_string()).unwrap_or_default());
                // Mirror SingleRv: derive found via the corresponding map observer
                let found = if let Some(handle) = f.map_refs.get(idx) {
                    collect_found(f, idx, handle, state, observers)?
                } else {
                    false
                };
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(name),
                    found,
                    indices: None,
                    return_value: rv,
                });
            }
        }
        Mode::SingleCov => {
            for (idx, handle) in f.map_refs.iter().enumerate() {
                let found = collect_found(f, idx, handle, state, observers)?;
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(f.names[idx].clone()),
                    found,
                    indices: None,
                    return_value: None,
                });
            }
        }
        Mode::SinglePL | Mode::ComboPL | Mode::ComboCov => {
            for (idx, handle) in f.map_refs.iter().enumerate() {
                let found = collect_found(f, idx, handle, state, observers)?;
                let idxs = collect_indices(f, idx, handle, state, observers)?;
                let indices = if idxs.is_empty() { None } else { Some(idxs) };
                infos.push(ObserverInfo {
                    idx,
                    name: Cow::from(f.names[idx].clone()),
                    found,
                    indices,
                    return_value: None,
                });
            }
        }
    }

    Ok(infos)
}

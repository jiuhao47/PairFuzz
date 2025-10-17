// Centralized metadata append helpers for DiffrayFeedback
use crate::feedbacks::hist_stats::{
    history_contains, history_insert, ComboSeenStats, HistKey, HistVal,
};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateStats {
    pub max_seen: Vec<u8>,
    pub min_seen: Vec<u8>,
    pub seen_first_bits: Vec<u64>,
    // Migrated fields from MapFeedbackMetadata
    // history_map removed: coverage persistence is now represented by
    // seen_first_bits / max_seen / min_seen. See repo discussion.
    pub num_covered_map_indexes: usize,
}

libafl_bolts::impl_serdeany!(TwoStateStats);
// local bit helpers used via BitsMutView; avoid importing Bits to prevent unused-import warning
use core::marker::PhantomData;
use libafl::corpus::Testcase;
use libafl::events::Event;
use libafl::events::EventWithStats;
use libafl::monitors::stats::{AggregatorOps, UserStats, UserStatsValue};
use libafl::state::HasExecutions;
use libafl::Error;
use libafl::HasMetadata;
use libafl::HasNamedMetadata;
use libafl_bolts::tuples::MatchNameRef;
use std::borrow::Cow;
use std::u8;

// Reuse the DiffrayFeedback type defined in diffray_feedback.rs
// local debug helpers (mirror those in diffray_feedback.rs but module-local)
fn append_metadata_debug_log(tag: &str, msg: &str) {
    if crate::driver::env_var::env_bool_cached(crate::driver::env_var::APPEND_METADATA_DEBUG) {
        crate::utils::logging::debug(tag, msg);
    }
}

// Free functions that implement the append_* behavior for DiffrayFeedback.
// These are implemented as module-level functions to avoid duplicate inherent impls
// across files while keeping the public API via the methods in `diffray_feedback.rs`.

pub fn append_single_cov<C, EM, I, OT, S>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    _manager: &mut EM,
    observers: &OT,
    testcase: &mut Testcase<I>,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata + HasExecutions,
    EM: libafl::events::EventFirer<I, S>,
    OT: MatchNameRef,
{
    // For each map observer, perform same append behavior as two_state_map::append_metadata
    for (idx, handle) in this.map_refs.iter().enumerate() {
        let name = &this.names[idx];
        if crate::driver::env_var::env_bool_cached(crate::driver::env_var::APPEND_METADATA_DEBUG) {
            append_metadata_debug_log(
                "metadata",
                &format!("enter append_metadata() for '{}'(idx={})", name, idx),
            );
        }

        // snapshot observer
        let observer = match observers.get(handle) {
            Some(h) => h.as_ref(),
            None => {
                crate::utils::logging::warn(
                    "metadata",
                    &format!(
                        "append_metadata: observer '{}' not present; skipping metadata append.",
                        name
                    ),
                );
                continue;
            }
        };

        let initial = observer.initial();
        let initial_u8: u8 = initial.into();

        // take stats vectors out (short scope) - operate on existing vectors in-place
        let len = observer.len();

        // Operate in-place on TwoStateStats to avoid replacing internal vectors
        // with temporaries that other readers might observe. Acquire a mutable
        // reference to the stats and then borrow its fields mutably in this
        // single scope so we can update max/min and bits without moving data out.
        let stats_mut = {
            let named_mut = state.named_metadata_map_mut();
            named_mut
                .get_mut::<TwoStateStats>(name)
                .ok_or_else(|| Error::illegal_argument("DiffrayFeedback: stats metadata missing"))?
        };
        let _covered = stats_mut.num_covered_map_indexes;

        let max_seen = &mut stats_mut.max_seen;
        let min_seen = &mut stats_mut.min_seen;
        let seen_first_bits = &mut stats_mut.seen_first_bits;

        if max_seen.len() < len {
            max_seen.resize(len, u8::MIN);
        }
        if min_seen.len() < len {
            min_seen.resize(len, u8::MAX);
        }
        let bits_len = (len + 63) / 64;
        if seen_first_bits.len() < bits_len {
            seen_first_bits.resize(bits_len, 0);
        }
        // No persistent per-index history buffer to resize here; we maintain
        // seen_first_bits, max_seen and min_seen which are resized as needed.

        let debug_enabled =
            crate::driver::env_var::env_bool_cached(crate::driver::env_var::APPEND_METADATA_DEBUG);
        let covered_ever_before: usize = _covered as usize;
        let mut covered_delta: usize = 0;

        #[allow(dead_code)]
        struct IndexChange {
            idx: usize,
            cur: u8,
            prev_max: u8,
            prev_min: u8,
            // whether the seen_first_bits was set BEFORE processing this sample
            seen_first_set: bool,
            reason_first: bool,
            reason_high: bool,
            reason_low: bool,
        }

        let mut changes_opt: Option<Vec<IndexChange>> = if debug_enabled {
            Some(Vec::with_capacity(16))
        } else {
            None
        };

        // Use BitsMut from utils to operate on the existing Vec<u64> backing seen_first_bits
        let mut seen_bits = crate::utils::bits::BitsMut::new(seen_first_bits);

        for (i, item_ref) in observer.as_iter().enumerate() {
            let item = *item_ref;
            let cur: u8 = item.into();

            // Determine newly_seen by comparing current value against initial
            // and the seen_first_bits indicator.
            let prev_seen_first = seen_bits.get(i);
            let newly_seen = cur != initial_u8 && !prev_seen_first;

            // bounds check: ensure vectors were resized correctly; if not, return error instead of panic
            if i >= max_seen.len() || i >= min_seen.len() {
                return Err(Error::illegal_argument(&format!(
                    "DiffrayFeedback: stats vector length mismatch for '{}' at idx {} (max_seen.len={}, min_seen.len={})",
                    name, i, max_seen.len(), min_seen.len()
                )));
            }
            let prev_max = max_seen[i];
            let prev_min = min_seen[i];
            if cur > prev_max {
                max_seen[i] = cur;
            }
            if cur < prev_min && (cur != initial_u8 || prev_seen_first) {
                min_seen[i] = cur;
            }

            let mut reason_first = false;
            let mut reason_high = false;
            let mut reason_low = false;

            if newly_seen {
                // test-and-set: if previously unset, set and count a covered delta
                let was_set = seen_bits.test_and_set(i);
                if !was_set {
                    covered_delta += 1;
                }
                if debug_enabled {
                    reason_first = true;
                }
            }

            // Detect initial_breach: current value is initial but previously
            // seen_first bit was set and low-check is enabled. This mirrors
            // the logic in collect_found_for_observer where such a case
            // counts as a found condition.
            if cur == initial_u8 {
                let cfg = &this.configs[idx];
                if prev_seen_first && cfg.check_low {
                    if debug_enabled {
                        reason_low = true;
                    }
                }
            }

            if debug_enabled {
                let prev_seen_first_bool = prev_seen_first;
                let cfg = &this.configs[idx];
                if (prev_seen_first_bool || newly_seen)
                    && cfg.check_high
                    && cur.saturating_sub(prev_max) >= cfg.high_delta
                {
                    reason_high = true;
                }
                if (prev_seen_first_bool || newly_seen)
                    && cfg.check_low
                    && cur < prev_min
                    && prev_min.saturating_sub(cur) >= cfg.low_delta
                {
                    reason_low = true;
                }

                if let Some(changes) = changes_opt.as_mut() {
                    changes.push(IndexChange {
                        idx: i,
                        cur,
                        prev_max,
                        prev_min,
                        seen_first_set: prev_seen_first,
                        reason_first,
                        reason_high,
                        reason_low,
                    });
                }
            }
        }

        let covered_ever: usize = covered_ever_before + covered_delta;

        // update TwoStateStats' coverage count (tracked via seen_first_bits)
        stats_mut.num_covered_map_indexes = covered_ever as usize;

        if debug_enabled && len > 0 {
            let ratio = (covered_ever as f64) / (len as f64);
            append_metadata_debug_log(
                "metadata",
                &format!(
                    "append_metadata: stats ratio (ever-seen) for '{}' (covered/len) = {}/{} ({:.6})",
                    name, covered_ever, len, ratio
                ),
            );

            // build detailed change logs similar to TwoStateMapFeedback
            if let Some(changes) = changes_opt.as_ref() {
                let increased: Vec<_> = changes
                    .iter()
                    // newly covered candidates are those where the seen_first bit
                    // was NOT set before processing and the current value is non-initial
                    .filter(|c| !c.seen_first_set && c.cur != initial_u8)
                    .map(|c| c.idx)
                    .collect();
                if !increased.is_empty() {
                    let delta = covered_ever as isize - covered_ever_before as isize;
                    append_metadata_debug_log(
                        "metadata",
                        &format!(
                            "append_metadata: covered delta for '{}' = {:+} ({} -> {})",
                            name, delta, covered_ever_before, covered_ever
                        ),
                    );

                    // Collect noteworthy changes (first-seen, high-breach, low-breach)
                    let mut noteworthy: Vec<String> = Vec::new();
                    let mut covered_only_count: usize = 0;
                    for c in changes.iter() {
                        let mut rs: Vec<String> = Vec::new();
                        if c.reason_first {
                            rs.push("first-seen".to_string());
                        }
                        if c.reason_high {
                            rs.push(format!("high-breach(prev_max={})", c.prev_max));
                        }
                        if c.reason_low {
                            rs.push(format!("low-breach(prev_min={})", c.prev_min));
                        }
                        if !rs.is_empty() {
                            // seen_first_set indicates whether this bit was already set
                            // before processing; if false and cur != initial then this
                            // index is a newly-seen candidate.
                            noteworthy.push(format!(
                                "idx {} cur={} reasons=[{}] seen_first_set={}",
                                c.idx,
                                c.cur,
                                rs.join(", "),
                                c.seen_first_set
                            ));
                        } else if !c.seen_first_set && c.cur != initial_u8 {
                            covered_only_count += 1;
                        }
                    }

                    if !noteworthy.is_empty() {
                        let cap: usize = 64;
                        let mut out = format!(
                            "append_metadata: noteworthy changes for '{}' (count={}):",
                            name,
                            noteworthy.len()
                        );
                        for n in noteworthy.iter().take(cap) {
                            out.push_str("\n  - ");
                            out.push_str(n);
                        }
                        if noteworthy.len() > cap {
                            let omitted = noteworthy.len() - cap;
                            out.push_str(&format!("\n  - ... (omitted {} entries)", omitted));
                        }
                        append_metadata_debug_log("metadata", &out);
                    }

                    if covered_only_count > 0 {
                        append_metadata_debug_log(
                            "metadata",
                            &format!(
                                "append_metadata: covered-only count for '{}' = {}",
                                name, covered_only_count
                            ),
                        );
                    }
                }
            }
        }

        // stats modified in-place on stats_mut; nothing to assign back

        // Attach indices metadata like MapFeedback (don't attach novelties here; handled once after match)
        if C::INDICES {
            // collect indices in a single pass without creating intermediate iterators
            let mut indices = Vec::new();
            indices.reserve(len / 8 + 1);
            for (i, v_ref) in observer.as_iter().enumerate() {
                if *v_ref != initial {
                    indices.push(i);
                }
            }
            let meta = libafl::feedbacks::MapIndexesMetadata::new(indices);
            if testcase.try_add_metadata(meta).is_err() {
                return Err(Error::key_exists(
                    "MapIndexesMetadata is already attached to this testcase. You should not have more than one observer with tracking.",
                ));
            }
        }

        // Fire UpdateUserStats using ever-seen covered value
        _manager.fire(
            state,
            EventWithStats::with_current_time(
                Event::UpdateUserStats {
                    name: name.clone(),
                    value: UserStats::new(
                        UserStatsValue::Ratio(covered_ever as u64, len as u64),
                        AggregatorOps::Avg,
                    ),
                    phantom: PhantomData,
                },
                *state.executions(),
            ),
        )?;
    }
    Ok(())
}

pub fn append_single_rv<C, S, OT>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let infos = crate::feedbacks::collect::collect::<C, S, OT>(this, state, observers)?;
    for info in infos.iter() {
        if let Some((ref name, val_opt)) = &info.return_value {
            let stats_name = format!("ReturnValueSingle({})", name);
            let key = HistKey::Single(name.clone(), HistVal::Rv(*val_opt));
            let already_seen = history_contains(state, &stats_name, &key);
            if !already_seen {
                let prev_count = state
                    .named_metadata_map()
                    .get::<crate::feedbacks::hist_stats::SingleSeenStats>(&stats_name)
                    .map(|md| md.seen.len())
                    .unwrap_or(0);
                append_metadata_debug_log(
                    "metadata",
                    &format!(
                        "append_metadata: NEW ReturnValueSingle '{}' -> val={:?} (previously seen={})",
                        stats_name, val_opt, prev_count
                    ),
                );
            }

            history_insert(state, &stats_name, &key)?;
        }
    }
    Ok(())
}

pub fn append_single_pl<C, S, OT>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let infos = crate::feedbacks::collect::collect::<C, S, OT>(this, state, observers)?;
    for info in infos.iter() {
        if let Some(idxs) = &info.indices {
            let count = idxs.len();
            let stats_name = format!("SinglePL({})", info.name);
            // Emit a debug log when this particular (observer, count) combo
            // has not been seen historically yet. This mirrors the behavior in
            // append_combo_pl_impl where newly-seen combos are logged so users
            // can inspect evolving PL coverage.
            let key = HistKey::Single(info.name.to_string(), HistVal::Pl(count));
            let already_seen = history_contains(state, &stats_name, &key);
            if !already_seen {
                // Determine previous count of seen items for the single stats, if present
                let prev_count = state
                    .named_metadata_map()
                    .get::<crate::feedbacks::hist_stats::SingleSeenStats>(&stats_name)
                    .map(|md| md.seen.len())
                    .unwrap_or(0);
                append_metadata_debug_log(
                    "metadata",
                    &format!(
                        "append_metadata: NEW SinglePL '{}' -> count={} (previously seen={})",
                        stats_name, count, prev_count
                    ),
                );
            }

            history_insert(state, &stats_name, &key)?;
        }
    }
    Ok(())
}

pub fn append_combo_cov<C, S, OT>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let infos = crate::feedbacks::collect::collect::<C, S, OT>(this, state, observers)?;
    let mut key: Vec<HistVal> = Vec::new();
    for info in infos.iter() {
        if let Some(idxs) = &info.indices {
            let indices_only: Vec<usize> = idxs.iter().map(|(i, _v)| *i).collect();
            key.push(HistVal::Cov(indices_only));
        } else {
            key.push(HistVal::Cov(Vec::new()));
        }
    }

    let stats_name = format!("ComboCov({})", this.names.join(","));
    // ensure metadata exists (ComboSeenStats)
    if state
        .named_metadata_map()
        .get::<ComboSeenStats>(&stats_name)
        .is_none()
    {
        state.add_named_metadata_checked(
            &stats_name,
            ComboSeenStats {
                seen: std::collections::HashSet::new(),
            },
        )?;
    }
    // Only emit a debug log when this exact combo has not been seen before.
    let already_seen = history_contains(state, &stats_name, &HistKey::Combo(key.clone()));
    if !already_seen {
        let prev_count = state
            .named_metadata_map()
            .get::<ComboSeenStats>(&stats_name)
            .map(|md| md.seen.len())
            .unwrap_or(0);
        let fp = crate::feedbacks::hist_stats::fingerprint(&HistKey::Combo(key.clone()));
        append_metadata_debug_log(
            "metadata",
            &format!(
                "append_metadata: NEW ComboCov '{}' -> fp=0x{:016x} (previously seen={})",
                stats_name, fp, prev_count
            ),
        );
    }
    history_insert(state, &stats_name, &HistKey::Combo(key))?;
    Ok(())
}

pub fn append_combo_rv<C, S, OT>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let mut key: Vec<HistVal> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let infos_snapshot = crate::feedbacks::collect::collect::<C, S, OT>(this, state, observers)?;
    for (idx, opt_h) in this.rv_refs.iter().enumerate() {
        if let Some(handle) = opt_h.as_ref() {
            names.push(handle.name().to_string());
            if let Some(info) = infos_snapshot.iter().find(|i| i.idx == idx) {
                key.push(HistVal::Rv(
                    info.return_value.as_ref().map(|t| t.1).unwrap_or(None),
                ));
            } else {
                key.push(HistVal::Rv(None));
            }
        }
    }
    if !names.is_empty() {
        let stats_name = format!("ReturnValueCombo({})", names.join(","));
        // Log only when this combo has not been seen before
        let already_seen = history_contains(state, &stats_name, &HistKey::Combo(key.clone()));
        if !already_seen {
            let prev_count = state
                .named_metadata_map()
                .get::<ComboSeenStats>(&stats_name)
                .map(|md| md.seen.len())
                .unwrap_or(0);
            append_metadata_debug_log(
                "metadata",
                &format!(
                    "append_metadata: NEW ComboRv '{}' -> key={:?} (previously seen={})",
                    stats_name,
                    key.clone(),
                    prev_count
                ),
            );
        }
        history_insert(state, &stats_name, &HistKey::Combo(key))?;
    }
    Ok(())
}

pub fn append_combo_pl<C, S, OT>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    S: HasNamedMetadata,
    OT: MatchNameRef,
{
    let infos = crate::feedbacks::collect::collect::<C, S, OT>(this, state, observers)?;
    let mut counts: Vec<HistVal> = Vec::new();
    for info in infos.iter() {
        if let Some(idxs) = &info.indices {
            counts.push(HistVal::Pl(idxs.len()));
        } else {
            counts.push(HistVal::Pl(0usize));
        }
    }
    let stats_name = format!("ComboPL({})", this.names.join(","));
    // ensure metadata exists (ComboHistStats)
    if state
        .named_metadata_map()
        .get::<ComboSeenStats>(&stats_name)
        .is_none()
    {
        state.add_named_metadata_checked(
            &stats_name,
            ComboSeenStats {
                seen: std::collections::HashSet::new(),
            },
        )?;
    }
    // Check whether this combo was already seen historically. If not, emit a debug
    // log showing the new combo and how many combos were seen before insertion.
    let already_seen = state
        .named_metadata_map()
        .get::<ComboSeenStats>(&stats_name)
        .map(|md| md.seen.contains(counts.as_slice()))
        .unwrap_or(false);
    if !already_seen {
        let prev_count = state
            .named_metadata_map()
            .get::<ComboSeenStats>(&stats_name)
            .map(|md| md.seen.len())
            .unwrap_or(0);
        append_metadata_debug_log(
            "metadata",
            &format!(
                "append_metadata: NEW ComboPL '{}' -> counts={:?} (previously seen combos={})",
                stats_name,
                counts.clone(),
                prev_count
            ),
        );
    }
    history_insert(state, &stats_name, &HistKey::Combo(counts))?;
    Ok(())
}

/// Persist execution-time coverage into the state's TwoStateStats.
/// This mutates `history_map` and `num_covered_map_indexes` similar to
/// `append_single_cov`, but does so directly at execution time without
/// creating a Testcase.
pub fn append_exec_cov<C, EM, I, OT, S>(
    this: &crate::feedbacks::diffray_feedback::DiffrayFeedback<C>,
    state: &mut S,
    manager: &mut EM,
    observers: &OT,
) -> Result<(), Error>
where
    C: libafl::observers::CanTrack
        + libafl::observers::MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    EM: libafl::events::EventFirer<I, S>,
    S: HasNamedMetadata + HasExecutions,
    OT: MatchNameRef,
{
    // Persist behavior is now unconditional when callers invoke this helper.
    // Caller (e.g. DiffrayFeedback) selects when to invoke based on mode/logic.
    let mut summaries: Vec<(Cow<'static, str>, usize, usize)> = Vec::new();
    for (idx, handle) in this.map_refs.iter().enumerate() {
        let name = &this.names[idx];

        let observer = match observers.get(handle) {
            Some(h) => h.as_ref(),
            None => continue,
        };

        let initial = observer.initial();
        let initial_u8: u8 = initial.into();
        let len = observer.len();

        // Acquire mutable access to map metadata. Capture executions before mutable borrows.
        let _execs_before = *state.executions();
        // Acquire mutable stats and operate on its fields in-place to avoid
        // temporarily replacing internal data which other readers might observe.
        let stats_mut = state
            .named_metadata_map_mut()
            .get_mut::<TwoStateStats>(name)
            .ok_or_else(|| Error::illegal_argument("DiffrayFeedback: stats metadata missing"))?;
        let covered_ever_before = stats_mut.num_covered_map_indexes;

        // Prepare container for newly-discovered indices; used only for debug
        // diagnostics when APPEND_METADATA_DEBUG is enabled.

        // Use the already-acquired stats_mut to access seen_first_bits only.
        let seen_first_bits = &mut stats_mut.seen_first_bits;
        let bits_len = (len + 63) / 64;
        if seen_first_bits.len() < bits_len {
            seen_first_bits.resize(bits_len, 0);
        }

        // Debug: no aggregate counters needed here; we'll report the delta and
        // the list of newly-discovered indices if any.

        let mut seen_bits = crate::utils::bits::BitsMut::new(seen_first_bits);

        let mut covered_delta: usize = 0;
        let mut new_indices: Option<Vec<usize>> = if crate::driver::env_var::env_bool_cached(
            crate::driver::env_var::APPEND_METADATA_DEBUG,
        ) {
            Some(Vec::with_capacity(64))
        } else {
            None
        };

        for (i, item_ref) in observer.as_iter().enumerate() {
            let cur: u8 = (*item_ref).into();

            // Determine newly covered by seen_first_bits and current value.
            let prev_seen_first = seen_bits.get(i);
            if cur != initial_u8 && !prev_seen_first {
                let was_set = seen_bits.test_and_set(i);
                if !was_set {
                    covered_delta += 1;
                }
                if let Some(ref mut v) = new_indices {
                    if v.len() < 1024 {
                        v.push(i);
                    }
                }
            }

            // exec-time persistence intentionally doesn't track per-index
            // historical max/min here — only the seen-first bit is used to
            // detect newly-seen entries and update covered counts.
        }

        let covered_ever = covered_ever_before + covered_delta;
        let len_now = len;
        // Update TwoStateStats with the migrated fields. TwoStateStats is
        // authoritative; we no longer write back to MapFeedbackMetadata here.
        {
            let stats_mut = state
                .named_metadata_map_mut()
                .get_mut::<TwoStateStats>(name)
                .ok_or_else(|| {
                    Error::illegal_argument("DiffrayFeedback: stats metadata missing")
                })?;
            stats_mut.num_covered_map_indexes = covered_ever as usize;
        }

        // For post-write diagnostics use the previously-captured covered_ever_before
        // as the prior written value.
        let _prev_written_local = covered_ever_before as usize;
        let _new_written_local = covered_ever as usize;

        // (post-write diagnostic logging removed)
        summaries.push((name.clone(), covered_ever, len_now));

        if crate::driver::env_var::env_bool_cached(crate::driver::env_var::APPEND_METADATA_DEBUG) {
            let delta = covered_ever as isize - covered_ever_before as isize;
            append_metadata_debug_log(
                "metadata",
                &format!(
                    "append_metadata: covered delta for '{}' = {:+} ({} -> {})",
                    name, delta, covered_ever_before, covered_ever
                ),
            );

            if let Some(idxs) = new_indices.as_ref() {
                if !idxs.is_empty() {
                    // cap output to a small sample to keep logs readable
                    let sample_cap = 16usize.min(idxs.len());
                    let mut s = String::with_capacity(sample_cap * 6);
                    use core::fmt::Write;
                    for (i, v) in idxs.iter().take(sample_cap).enumerate() {
                        if i > 0 {
                            s.push_str(",");
                        }
                        write!(s, "{}", v).ok();
                    }
                    if idxs.len() > sample_cap {
                        let omitted = idxs.len() - sample_cap;
                        append_metadata_debug_log(
                            "metadata",
                            &format!(
                                "append_exec_cov: '{}' new_indices=[{}] ... (omitted {} entries)",
                                name, s, omitted
                            ),
                        );
                    } else {
                        append_metadata_debug_log(
                            "metadata",
                            &format!("append_exec_cov: '{}' new_indices=[{}]", name, s),
                        );
                    }
                }
            }
        }
    }

    // Emit UpdateUserStats events for each persisted observer so monitors/logs show persistent coverage
    for (name, covered_ever, len_now) in summaries.into_iter() {
        manager.fire(
            state,
            EventWithStats::with_current_time(
                Event::UpdateUserStats {
                    name: name.clone(),
                    value: UserStats::new(
                        UserStatsValue::Ratio(covered_ever as u64, len_now as u64),
                        AggregatorOps::Avg,
                    ),
                    phantom: PhantomData,
                },
                *state.executions(),
            ),
        )?;
    }

    Ok(())
}

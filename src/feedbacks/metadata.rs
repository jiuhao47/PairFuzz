// Centralized metadata append helpers for DiffrayFeedback
use crate::feedbacks::hist_stats::{
    history_contains, history_insert, ComboSeenStats, HistKey, HistVal,
};
use crate::feedbacks::two_state_map::TwoStateStats;
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
        append_metadata_debug_log(
            "multi_two_state_map",
            &format!("enter append_metadata() for '{}'(idx={})", name, idx),
        );

        // snapshot observer
        let observer = match observers.get(handle) {
            Some(h) => h.as_ref(),
            None => {
                crate::utils::logging::warn(
                    "multi_two_state_map",
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
        let named = state.named_metadata_map_mut();

        // take history_map and covered count out first (owned) to avoid overlapping mutable borrows
        let (mut history_map, _covered) = {
            let map_state_mut = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(name)
                .ok_or_else(|| Error::illegal_argument("DiffrayFeedback: map metadata missing"))?;
            (
                std::mem::take(&mut map_state_mut.history_map),
                map_state_mut.num_covered_map_indexes,
            )
        };

        let stats_mut = named
            .get_mut::<TwoStateStats>(name)
            .ok_or_else(|| Error::illegal_argument("DiffrayFeedback: stats metadata missing"))?;
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
        if history_map.len() < len {
            history_map.resize(len, initial_u8);
        }

        let debug_enabled =
            crate::driver::env_var::env_bool_cached(crate::driver::env_var::APPEND_METADATA_DEBUG);
        let covered_ever_before: usize = _covered as usize;
        let mut covered_delta: usize = 0;

        #[allow(dead_code)]
        struct IndexChange {
            idx: usize,
            prev_history: u8,
            cur: u8,
            prev_max: u8,
            prev_min: u8,
            prev_seen_first: bool,
            new_covered: bool,
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

            let prev_history = history_map[i];
            let mut new_covered = false;
            if prev_history == initial_u8 && cur != initial_u8 {
                new_covered = true;
            }
            history_map[i] = cur;

            let prev_max = max_seen[i];
            let prev_min = min_seen[i];
            let prev_seen_first = seen_bits.get(i);
            if cur > max_seen[i] {
                max_seen[i] = cur;
            }
            if cur < min_seen[i] && (cur != initial_u8 || prev_seen_first) {
                min_seen[i] = cur;
            }

            let mut reason_first = false;
            let mut reason_high = false;
            let mut reason_low = false;

            let newly_seen = prev_history == initial_u8 && cur != initial_u8;
            if prev_history == initial_u8 && cur != initial_u8 && !prev_seen_first {
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
                        prev_history,
                        cur,
                        prev_max,
                        prev_min,
                        prev_seen_first,
                        new_covered,
                        reason_first,
                        reason_high,
                        reason_low,
                    });
                }
            }
        }

        let covered_ever: usize = covered_ever_before + covered_delta;

        // write back map metadata: put history_map back and update covered count
        {
            let map_state_mut = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(name)
                .ok_or_else(|| Error::illegal_argument("DiffrayFeedback: map metadata missing"))?;
            map_state_mut.history_map = history_map;
            map_state_mut.num_covered_map_indexes = covered_ever as usize;
        }

        if debug_enabled && len > 0 {
            let ratio = (covered_ever as f64) / (len as f64);
            append_metadata_debug_log(
                "multi_two_state_map",
                &format!(
                    "append_metadata: stats ratio (ever-seen) for '{}' (covered/len) = {}/{} ({:.6})",
                    name, covered_ever, len, ratio
                ),
            );

            // build detailed change logs similar to TwoStateMapFeedback
            if let Some(changes) = changes_opt.as_ref() {
                let increased: Vec<_> = changes
                    .iter()
                    .filter(|c| c.new_covered)
                    .map(|c| c.idx)
                    .collect();
                if !increased.is_empty() {
                    let delta = covered_ever as isize - covered_ever_before as isize;
                    append_metadata_debug_log(
                        "multi_two_state_map",
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
                            let newly_seen = c.prev_history == initial_u8 && c.cur != initial_u8;
                            noteworthy.push(format!(
                                "idx {} cur={} reasons=[{}] prev_history={} prev_seen_first={} newly_seen={}",
                                c.idx,
                                c.cur,
                                rs.join(", "),
                                c.prev_history,
                                c.prev_seen_first,
                                newly_seen
                            ));
                        } else if c.new_covered {
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
                        append_metadata_debug_log("multi_two_state_map", &out);
                    }

                    if covered_only_count > 0 {
                        append_metadata_debug_log(
                            "multi_two_state_map",
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
            let mut indices = Vec::new();
            indices.reserve(len / 8 + 1);
            for (i, _) in observer
                .as_iter()
                .map(|x| *x)
                .enumerate()
                .filter(|(_, v)| *v != initial)
            {
                indices.push(i);
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
                    "multi_two_state_map",
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
                    "multi_two_state_map",
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
            "multi_two_state_map",
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
                "multi_two_state_map",
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
            "multi_two_state_map",
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

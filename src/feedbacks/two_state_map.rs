// Two-state map feedback: track per-index max/min and first-seen transitions
use core::fmt::Debug;
use core::marker::PhantomData;
use libafl::corpus::Testcase;
use libafl::events::Event;
use libafl::events::EventWithStats;
use libafl::HasMetadata;
use std::u8;
// MapFeedbackMetadata used via fully-qualified path below
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::monitors::stats::{AggregatorOps, UserStats, UserStatsValue};
use libafl::observers::MapObserver;
use libafl::state::HasExecutions;
use libafl::{Error, HasNamedMetadata};
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
// hashing imports removed: we no longer log hashes for history_map or stats
use std::sync::atomic::{AtomicU8, Ordering};

/// Per-feedback metadata: track historical max/min and whether first-transition was seen
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateStats {
    pub max_seen: Vec<u8>,
    pub min_seen: Vec<u8>,
    // bit-packed flags: each bit represents seen_first for an index
    // stored as 64-bit words for faster popcount and fewer allocations
    pub seen_first_bits: Vec<u64>,
}

libafl_bolts::impl_serdeany!(TwoStateStats);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateMapFeedback<C> {
    /// new indexes observed in the last observation (if observer supports novelties)
    pub(crate) novelties: Option<Vec<usize>>,
    name: Cow<'static, str>,
    /// user-stats name (lowercased usually) used for UpdateUserStats events
    stats_name: Cow<'static, str>,
    map_ref: Handle<C>,
    #[cfg(feature = "track_hit_feedbacks")]
    last: bool,
    /// how much above historical max counts as a high-value breakthrough
    high_delta: u8,
    /// how much below historical min counts as a low-value breakthrough
    low_delta: u8,
    /// whether to check high-side breakthroughs
    check_high: bool,
    /// whether to check low-side breakthroughs
    check_low: bool,
}

impl<C> TwoStateMapFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>
        + Named,
{
    #[allow(dead_code)]
    pub fn new(map_observer: &C) -> Self {
        Self {
            novelties: if C::NOVELTIES { Some(vec![]) } else { None },
            name: map_observer.name().clone(),
            stats_name: {
                // mimic MapFeedback::create_stats_name behavior
                let nm = map_observer.name();
                if nm.chars().all(char::is_lowercase) {
                    nm.clone()
                } else {
                    nm.to_lowercase().into()
                }
            },
            map_ref: map_observer.handle(),
            #[cfg(feature = "track_hit_feedbacks")]
            last: false,
            high_delta: 1,
            low_delta: 1,
            check_high: true,
            check_low: true,
        }
    }

    /// Create with explicit threshold parameters.
    /// high_delta/low_delta are added/subtracted to historical max/min before comparison.
    pub fn new_with_params(
        map_observer: &C,
        high_delta: u8,
        low_delta: u8,
        check_high: bool,
        check_low: bool,
    ) -> Self {
        Self {
            novelties: if C::NOVELTIES { Some(vec![]) } else { None },
            name: map_observer.name().clone(),
            stats_name: {
                let nm = map_observer.name();
                if nm.chars().all(char::is_lowercase) {
                    nm.clone()
                } else {
                    nm.to_lowercase().into()
                }
            },
            map_ref: map_observer.handle(),
            #[cfg(feature = "track_hit_feedbacks")]
            last: false,
            high_delta,
            low_delta,
            check_high,
            check_low,
        }
    }

    /// Creating a new `TwoStateMapFeedback` with a specific name. See `MapFeedback::with_name`.
    #[must_use]
    #[allow(dead_code)]
    pub fn with_name(name: &'static str, map_observer: &C) -> Self {
        let name = Cow::from(name);
        Self {
            novelties: if C::NOVELTIES { Some(vec![]) } else { None },
            name,
            stats_name: create_stats_name(&map_observer.name()),
            map_ref: map_observer.handle(),
            #[cfg(feature = "track_hit_feedbacks")]
            last: false,
            high_delta: 1,
            low_delta: 1,
            check_high: true,
            check_low: true,
        }
    }
}

impl<C> Named for TwoStateMapFeedback<C> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

// helper: mimic MapFeedback::create_stats_name
#[allow(dead_code)]
fn create_stats_name(name: &Cow<'static, str>) -> Cow<'static, str> {
    if name.chars().all(char::is_lowercase) {
        name.clone()
    } else {
        name.to_lowercase().into()
    }
}

// Module-level gated debug logger for TwoStateMapFeedback
// Enable via environment variable DIFFRAY_TWOSTATE_DEBUG=1 or DIFFRAY_TWOSTATE_DEBUG=true
fn twomap_debug_log(tag: &str, msg: &str) {
    if twomap_debug_enabled() {
        let _ = crate::utils::logging::debug_log(tag, msg);
    }
}

static TWOMAP_DEBUG_FLAG: AtomicU8 = AtomicU8::new(0);

fn twomap_debug_enabled() -> bool {
    match TWOMAP_DEBUG_FLAG.load(Ordering::Relaxed) {
        0 => {
            let enabled = match std::env::var("DIFFRAY_TWOSTATE_DEBUG") {
                Ok(v) => {
                    let ve = v.to_lowercase();
                    ve == "1" || ve == "true"
                }
                Err(_) => false,
            };
            TWOMAP_DEBUG_FLAG.store(if enabled { 2 } else { 1 }, Ordering::Relaxed);
            enabled
        }
        1 => false,
        2 => true,
        _ => false,
    }
}

impl<C> libafl::feedbacks::HasObserverHandle for TwoStateMapFeedback<C> {
    type Observer = C;

    fn observer_handle(&self) -> &Handle<Self::Observer> {
        &self.map_ref
    }
}

impl<S, C> StateInitializer<S> for TwoStateMapFeedback<C>
where
    S: HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        // Preallocate metadata vectors. Allow overriding capacity with env var
        // `DIFFRAY_TWOSTATE_CAPACITY`. If not provided, default to 65536.
        let cap: usize = std::env::var("DIFFRAY_TWOSTATE_CAPACITY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(65536usize);
        let bits_words = (cap + 63) / 64;
        state.add_named_metadata_checked(
            &self.name,
            TwoStateStats {
                max_seen: vec![u8::MIN; cap],
                min_seen: vec![u8::MAX; cap],
                seen_first_bits: vec![0u64; bits_words],
            },
        )?;
        // also add a MapFeedbackMetadata<u8> so tools / monitors expecting map history (like JSON monitor)
        // can find the standard "history_map" under the same name
        state.add_named_metadata_checked(
            &self.name,
            libafl::feedbacks::MapFeedbackMetadata::<u8>::default(),
        )?;
        Ok(())
    }
}

impl<C, EM, I, OT, S> Feedback<EM, I, OT, S> for TwoStateMapFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: 'static
        + Default
        + Debug
        + Serialize
        + serde::de::DeserializeOwned
        + PartialEq
        + Copy
        + Into<u8>,
    EM: libafl::events::EventFirer<I, S>,
    OT: MatchNameRef,
    S: HasNamedMetadata + HasExecutions,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        _exit_kind: &libafl::executors::ExitKind,
    ) -> Result<bool, Error> {
        let observer = observers
            .get(&self.map_ref)
            .ok_or_else(|| Error::illegal_argument("TwoStateMapFeedback: observer not found"))?
            .as_ref();
        let initial = observer.initial();
        let initial_u8: u8 = initial.into();
        let named = state.named_metadata_map();
        let stats_opt = named.get::<TwoStateStats>(&self.name);
        if stats_opt.is_none() {
            return Ok(false);
        }
        let stats = stats_opt.unwrap();
        // try get history_map to base first-seen decisions on actual persisted history
        let map_meta_opt = named.get::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name);
        let history_map = map_meta_opt.map(|m| m.history_map.as_slice());

        // iterate and decide
        if let Some(nov_vec) = self.novelties.as_mut() {
            nov_vec.clear();
            for (i, item_ref) in observer.as_iter().enumerate() {
                let item = *item_ref;
                let cur: u8 = item.into();
                // When historical max/min are missing, use extreme defaults so
                // that high/low breach logic works correctly (max starts at u8::MIN,
                // min starts at u8::MAX).
                let max = stats.max_seen.get(i).copied().unwrap_or(u8::MIN);
                let min = stats.min_seen.get(i).copied().unwrap_or(u8::MAX);
                let first_flag =
                    (stats.seen_first_bits.get(i >> 3).copied().unwrap_or(0) >> (i & 7)) & 1;
                // first-time discovery: persisted history_map was initial and now non-initial
                let prev_hist = history_map
                    .and_then(|h| h.get(i))
                    .copied()
                    .unwrap_or(initial_u8);
                if prev_hist == initial_u8 && cur != initial_u8 && first_flag == 0 {
                    #[cfg(feature = "track_hit_feedbacks")]
                    {
                        self.last = true;
                    }
                    nov_vec.push(i);
                    return Ok(true);
                }
                // breach historical bounds with deltas and flags
                if (self.check_high && cur.saturating_sub(max) >= self.high_delta)
                    || (self.check_low && cur < min && min.saturating_sub(cur) >= self.low_delta)
                {
                    #[cfg(feature = "track_hit_feedbacks")]
                    {
                        self.last = true;
                    }
                    nov_vec.push(i);
                    return Ok(true);
                }
            }
        } else {
            for (i, item_ref) in observer.as_iter().enumerate() {
                let item = *item_ref;
                let cur: u8 = item.into();
                // When historical max/min are missing, use extreme defaults so
                // that high/low breach logic works correctly (max starts at u8::MIN,
                // min starts at u8::MAX).
                let max = stats.max_seen.get(i).copied().unwrap_or(u8::MIN);
                let min = stats.min_seen.get(i).copied().unwrap_or(u8::MAX);
                let first_flag =
                    (stats.seen_first_bits.get(i >> 3).copied().unwrap_or(0) >> (i & 7)) & 1;
                let prev_hist = history_map
                    .and_then(|h| h.get(i))
                    .copied()
                    .unwrap_or(initial_u8);
                // first-time discovery: persisted history_map was initial and now non-initial
                if prev_hist == initial_u8 && cur != initial_u8 && first_flag == 0 {
                    #[cfg(feature = "track_hit_feedbacks")]
                    {
                        self.last = true;
                    }
                    return Ok(true);
                }
                // breach historical bounds with deltas and flags
                if (self.check_high && cur.saturating_sub(max) >= self.high_delta)
                    || (self.check_low && cur < min && min.saturating_sub(cur) >= self.low_delta)
                {
                    #[cfg(feature = "track_hit_feedbacks")]
                    {
                        self.last = true;
                    }
                    return Ok(true);
                }
            }
        }

        #[cfg(feature = "track_hit_feedbacks")]
        {
            self.last = false;
        }
        Ok(false)
    }

    fn append_metadata(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        observers: &OT,
        testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        // Phase 1: snapshot observer
        let observer = observers
            .get(&self.map_ref)
            .ok_or_else(|| Error::illegal_argument("TwoStateMapFeedback: observer not found"))?
            .as_ref();
        let initial = observer.initial();
        let initial_u8: u8 = initial.into();

        // Phase 2: reduce metadata lookups by taking Vecs out once
        let len = observer.len();
        let named = state.named_metadata_map_mut();

        // take stats vectors out (single small-scope borrow)
        let (mut max_seen, mut min_seen, mut seen_first) = {
            let stats_mut = named.get_mut::<TwoStateStats>(&self.name).ok_or_else(|| {
                Error::illegal_argument("TwoStateMapFeedback: stats metadata missing")
            })?;
            (
                std::mem::take(&mut stats_mut.max_seen),
                std::mem::take(&mut stats_mut.min_seen),
                std::mem::take(&mut stats_mut.seen_first_bits),
            )
        };

        // take history_map and covered count out
        let (mut history_map, _covered) = {
            let map_state_mut = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name)
                .ok_or_else(|| {
                    Error::illegal_argument("TwoStateMapFeedback: map metadata missing")
                })?;
            (
                std::mem::take(&mut map_state_mut.history_map),
                map_state_mut.num_covered_map_indexes,
            )
        };

        // resize locals once
        if max_seen.len() < len {
            max_seen.resize(len, u8::MIN);
        }
        if min_seen.len() < len {
            min_seen.resize(len, u8::MAX);
        }
        let bits_len = (len + 7) / 8;
        if seen_first.len() < bits_len {
            seen_first.resize(bits_len, 0);
        }
        if history_map.len() < len {
            history_map.resize(len, initial_u8);
        }

        // decide if we should run debug instrumentation (cached per-process)
        let debug_enabled = twomap_debug_enabled();
        // persisted covered count before modifications (fast)
        let covered_ever_before: usize = _covered as usize;
        // we'll maintain an incremental delta for newly-seen indices
        let mut covered_delta: usize = 0;

        // iterate observer directly (no allocation)
        // collect per-index change info so we can log why covered increased
        #[allow(dead_code)]
        struct IndexChange {
            idx: usize,
            prev_history: u8,
            cur: u8,
            prev_max: u8,
            prev_min: u8,
            prev_seen_first: bool,
            became_covered: bool,
            reason_first: bool,
            reason_high: bool,
            reason_low: bool,
        }
        // Only allocate the changes vector when debug is enabled to avoid work in hot path
        let mut changes_opt: Option<Vec<IndexChange>> = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        for (i, item_ref) in observer.as_iter().enumerate() {
            let item = *item_ref;
            let cur: u8 = item.into();

            // update history_map (covered will be recomputed later)
            let prev_history = history_map[i];
            let mut became_covered = false;
            if prev_history == initial_u8 && cur != initial_u8 {
                became_covered = true;
            }
            history_map[i] = cur;

            // update stats (max/min/seen_first)
            // Keep previous values so we can detect "first seen" correctly
            let prev_max = max_seen[i];
            let prev_min = min_seen[i];
            if cur > max_seen[i] {
                max_seen[i] = cur;
            }
            // Only update min when we actually observe a non-initial value.
            // This prevents undiscovered indices (still initial) from forcing
            // the historical min down to the initial value (e.g. 0).
            if cur != initial_u8 && cur < min_seen[i] {
                min_seen[i] = cur;
            }
            // If previously both were initial and now non-initial, mark seen_first
            let prev_seen_first =
                ((seen_first.get(i >> 6).copied().unwrap_or(0) >> (i & 63)) & 1) == 1;

            // Determine and record reasons only when debugging is enabled. We still
            // perform the necessary seen_first bit update regardless of debug mode
            // because it's part of normal metadata behavior.
            let mut reason_first = false;
            let mut reason_high = false;
            let mut reason_low = false;

            if prev_history == initial_u8 && cur != initial_u8 && !prev_seen_first {
                // first seen according to persisted history -> set seen_first bit
                let word_idx = i >> 6;
                let bit_mask = 1u64 << (i & 63);
                if (seen_first[word_idx] & bit_mask) == 0 {
                    seen_first[word_idx] |= bit_mask;
                    covered_delta += 1;
                }
                if debug_enabled {
                    reason_first = true;
                }
            }

            if debug_enabled {
                if self.check_high && cur.saturating_sub(prev_max) >= self.high_delta {
                    reason_high = true;
                }
                if self.check_low
                    && cur < prev_min
                    && prev_min.saturating_sub(cur) >= self.low_delta
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
                        became_covered,
                        reason_first,
                        reason_high,
                        reason_low,
                    });
                }
            }
        }

        // compute covered based on incremental delta
        let covered_ever: usize = covered_ever_before + covered_delta;

        // write back metadata (two separate small-scope borrows)
        {
            let map_state_mut = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name)
                .ok_or_else(|| {
                    Error::illegal_argument("TwoStateMapFeedback: map metadata missing")
                })?;
            map_state_mut.history_map = history_map;
            map_state_mut.num_covered_map_indexes = covered_ever as usize;
            // debug: (history_map hash logging removed - no longer useful)
        }
        // covered_ever_after should reflect the persisted ever-seen count we just wrote.
        let covered_ever_after: usize = covered_ever;
        if debug_enabled && len > 0 {
            let ratio = (covered_ever_after as f64) / (len as f64);
            twomap_debug_log(
                "two_state_map",
                &format!(
                    "append_metadata: stats ratio (ever-seen) for '{}' (covered/len) = {}/{} ({:.6})",
                    self.stats_name, covered_ever_after, len, ratio
                ),
            );
        }
        // debug: (stats hashes logging removed - no longer useful)
        // Log which indices caused covered to increase and why (only if debug enabled)
        if debug_enabled {
            if let Some(changes) = changes_opt.as_ref() {
                let increased: Vec<_> = changes
                    .iter()
                    .filter(|c| c.became_covered)
                    .map(|c| c.idx)
                    .collect();
                if !increased.is_empty() {
                    let delta = covered_ever_after as isize - covered_ever_before as isize;
                    // Summary line: how many indices changed value and persisted delta
                    twomap_debug_log(
                    "two_state_map",
                    &format!(
                        "append_metadata: observer '{}' had {} indices transition initial->non-initial; persisted ever-seen covered delta = {} ({} -> {})",
                        self.name,
                        increased.len(),
                        delta,
                        covered_ever_before,
                        covered_ever_after,
                    ),
                );

                    // Collect noteworthy changes: first-seen (persisted change), high-breach, low-breach
                    let mut noteworthy: Vec<String> = Vec::new();
                    let mut covered_only_examples: Vec<usize> = Vec::new();
                    for c in changes.iter().filter(|c| c.became_covered) {
                        if c.reason_first || c.reason_high || c.reason_low {
                            // Build a descriptive reasons list including original values
                            let mut rs = Vec::new();
                            if c.reason_first {
                                rs.push("first-seen".to_string());
                            }
                            if c.reason_high {
                                rs.push(format!("high-breach(prev_max={})", c.prev_max));
                            }
                            if c.reason_low {
                                rs.push(format!("low-breach(prev_min={})", c.prev_min));
                            }
                            noteworthy.push(format!(
                                "idx {} cur={} reasons=[{}] prev_history={} prev_seen_first={}",
                                c.idx,
                                c.cur,
                                rs.join(", "),
                                c.prev_history,
                                c.prev_seen_first
                            ));
                        } else {
                            // covered-only; only keep a small sample for context
                            if covered_only_examples.len() < 10 {
                                covered_only_examples.push(c.idx);
                            }
                        }
                    }

                    if !noteworthy.is_empty() {
                        // format noteworthy as a multi-line, indented list for readability
                        let mut out = format!(
                            "append_metadata: noteworthy changes for '{}' (count={}):",
                            self.name,
                            noteworthy.len()
                        );
                        for n in noteworthy.iter() {
                            out.push_str("\n  - ");
                            out.push_str(n);
                        }
                        twomap_debug_log("two_state_map", &out);
                    }

                    let covered_only_count = increased.len().saturating_sub(noteworthy.len());
                    if covered_only_count > 0 {
                        let mut out = format!(
                            "append_metadata: {} covered-only indices for '{}' (not interesting):",
                            covered_only_count, self.name
                        );
                        for idx in covered_only_examples.iter() {
                            out.push_str("\n  - idx ");
                            out.push_str(&idx.to_string());
                        }
                        twomap_debug_log("two_state_map", &out);
                    }
                }
            }
        }
        {
            let stats_mut = named.get_mut::<TwoStateStats>(&self.name).ok_or_else(|| {
                Error::illegal_argument("TwoStateMapFeedback: stats metadata missing")
            })?;
            stats_mut.max_seen = max_seen;
            stats_mut.min_seen = min_seen;
            stats_mut.seen_first_bits = seen_first;
        }

        // Attach novelties / indices metadata like MapFeedback
        if let Some(novelties) = self.novelties.as_mut().map(core::mem::take) {
            let meta = libafl::feedbacks::MapNoveltiesMetadata::new(novelties);
            testcase.add_metadata(meta);
        }

        if C::INDICES {
            let mut indices = Vec::new();
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
        manager.fire(
            state,
            EventWithStats::with_current_time(
                Event::UpdateUserStats {
                    name: self.stats_name.clone(),
                    value: UserStats::new(
                        UserStatsValue::Ratio(covered_ever_after as u64, len as u64),
                        AggregatorOps::Avg,
                    ),
                    phantom: PhantomData,
                },
                *state.executions(),
            ),
        )?;

        Ok(())
    }

    fn last_result(&self) -> Result<bool, Error> {
        #[cfg(feature = "track_hit_feedbacks")]
        {
            Ok(self.last)
        }
        #[cfg(not(feature = "track_hit_feedbacks"))]
        {
            Ok(false)
        }
    }
}

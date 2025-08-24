// Two-state map feedback: track per-index max/min and first-seen transitions
use core::fmt::Debug;
use core::marker::PhantomData;
use libafl::corpus::Testcase;
use libafl::events::Event;
use libafl::events::EventWithStats;
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

/// Per-feedback metadata: track historical max/min and whether first-transition was seen
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateStats {
    pub max_seen: Vec<u8>,
    pub min_seen: Vec<u8>,
    // bit-packed flags: each bit represents seen_first for an index
    pub seen_first_bits: Vec<u8>,
}

libafl_bolts::impl_serdeany!(TwoStateStats);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateMapFeedback<C> {
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
    C: MapObserver + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
{
    #[allow(dead_code)]
    pub fn new(map_observer: &C) -> Self {
        Self {
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
}

impl<C> Named for TwoStateMapFeedback<C> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<S, C> StateInitializer<S> for TwoStateMapFeedback<C>
where
    S: HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        state.add_named_metadata_checked(
            &self.name,
            TwoStateStats {
                max_seen: Vec::new(),
                min_seen: Vec::new(),
                seen_first_bits: Vec::new(),
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

impl<C> TwoStateMapFeedback<C>
where
    C: MapObserver + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
{
    /// Ensure named metadata vectors are initialized to observer.len() with initial values.
    pub fn ensure_initialized_with_observer<S>(
        &mut self,
        state: &mut S,
        observer: &C,
    ) -> Result<(), Error>
    where
        S: HasNamedMetadata,
    {
        let len = observer.len();
        let initial: C::Entry = observer.initial();
        let initial_u8: u8 = initial.into();
        let named = state.named_metadata_map_mut();

        // init TwoStateStats if missing
        if !named.contains::<TwoStateStats>(&self.name) {
            let bits_len = (len + 7) / 8;
            named
                .try_insert::<TwoStateStats>(
                    &self.name,
                    TwoStateStats {
                        max_seen: vec![initial_u8; len],
                        min_seen: vec![initial_u8; len],
                        seen_first_bits: vec![0u8; bits_len],
                    },
                )
                .map_err(|e| {
                    Error::illegal_state(format!("failed to insert TwoStateStats: {:?}", e))
                })?;
        } else {
            let stats = named.get_mut::<TwoStateStats>(&self.name).unwrap();
            if stats.max_seen.len() < len {
                stats.max_seen.resize(len, initial_u8);
            }
            if stats.min_seen.len() < len {
                stats.min_seen.resize(len, initial_u8);
            }
            let bits_len = (len + 7) / 8;
            if stats.seen_first_bits.len() < bits_len {
                stats.seen_first_bits.resize(bits_len, 0);
            }
        }

        // init MapFeedbackMetadata<u8>
        if !named.contains::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name) {
            let m = libafl::feedbacks::MapFeedbackMetadata::<u8>::new(len);
            named
                .try_insert::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name, m)
                .map_err(|e| {
                    Error::illegal_state(format!("failed to insert MapFeedbackMetadata: {:?}", e))
                })?;
        } else {
            let map_state = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name)
                .unwrap();
            if map_state.history_map.len() < len {
                map_state.history_map.resize(len, initial_u8);
            }
        }

        Ok(())
    }
}

impl<C, EM, I, OT, S> Feedback<EM, I, OT, S> for TwoStateMapFeedback<C>
where
    C: MapObserver + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
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

        // iterate and decide
        for (i, item_ref) in observer.as_iter().enumerate() {
            let item = *item_ref;
            let cur: u8 = item.into();
            let max = stats.max_seen.get(i).copied().unwrap_or(initial_u8);
            let min = stats.min_seen.get(i).copied().unwrap_or(initial_u8);
            let first_flag =
                (stats.seen_first_bits.get(i >> 3).copied().unwrap_or(0) >> (i & 7)) & 1;
            // first-time discovery: history was initial (min==initial && max==initial) and now non-initial
            if max == initial_u8 && min == initial_u8 && cur != initial_u8 && first_flag == 0 {
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

        #[cfg(feature = "track_hit_feedbacks")]
        {
            self.last = false;
        }
        Ok(false)
    }

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        observers: &OT,
        _testcase: &mut Testcase<I>,
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
        let (mut history_map, mut covered) = {
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
            max_seen.resize(len, initial_u8);
        }
        if min_seen.len() < len {
            min_seen.resize(len, initial_u8);
        }
        let bits_len = (len + 7) / 8;
        if seen_first.len() < bits_len {
            seen_first.resize(bits_len, 0);
        }
        if history_map.len() < len {
            history_map.resize(len, initial_u8);
        }

        // iterate observer directly (no allocation)
        for (i, item_ref) in observer.as_iter().enumerate() {
            let item = *item_ref;
            let cur: u8 = item.into();

            // update covered / history_map
            if history_map[i] == initial_u8 && cur != initial_u8 {
                covered += 1;
            }
            history_map[i] = cur;

            // update stats (max/min/seen_first)
            if cur > max_seen[i] {
                max_seen[i] = cur;
            }
            if cur < min_seen[i] {
                min_seen[i] = cur;
            }
            if max_seen[i] == initial_u8 && min_seen[i] == initial_u8 && cur != initial_u8 {
                // set the seen_first bit
                let byte_idx = i >> 3;
                let bit_mask = 1u8 << (i & 7);
                if (seen_first[byte_idx] & bit_mask) == 0 {
                    seen_first[byte_idx] |= bit_mask;
                }
            }
        }

        // write back metadata (two separate small-scope borrows)
        {
            let map_state_mut = named
                .get_mut::<libafl::feedbacks::MapFeedbackMetadata<u8>>(&self.name)
                .ok_or_else(|| {
                    Error::illegal_argument("TwoStateMapFeedback: map metadata missing")
                })?;
            map_state_mut.history_map = history_map;
            map_state_mut.num_covered_map_indexes = covered;
        }
        {
            let stats_mut = named.get_mut::<TwoStateStats>(&self.name).ok_or_else(|| {
                Error::illegal_argument("TwoStateMapFeedback: stats metadata missing")
            })?;
            stats_mut.max_seen = max_seen;
            stats_mut.min_seen = min_seen;
            stats_mut.seen_first_bits = seen_first;
        }

        // Fire UpdateUserStats
        _manager.fire(
            state,
            EventWithStats::with_current_time(
                Event::UpdateUserStats {
                    name: self.stats_name.clone(),
                    value: UserStats::new(
                        UserStatsValue::Ratio(covered as u64, len as u64),
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

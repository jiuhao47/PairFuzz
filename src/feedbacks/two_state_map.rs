// Two-state map feedback: track per-index max/min and first-seen transitions
use core::fmt::Debug;
use libafl::corpus::Testcase;
use libafl::feedbacks::{Feedback, StateInitializer};
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
    pub seen_first: Vec<u8>, // 0/1 flags to avoid repeat reporting of first discovery
}

libafl_bolts::impl_serdeany!(TwoStateStats);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TwoStateMapFeedback<C> {
    name: Cow<'static, str>,
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
                seen_first: Vec::new(),
            },
        )?;
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
        let items: Vec<C::Entry> = observer.as_iter().map(|x| *x).collect();

        let named = state.named_metadata_map();
        let stats_opt = named.get::<TwoStateStats>(&self.name);
        if stats_opt.is_none() {
            return Ok(false);
        }
        let stats = stats_opt.unwrap();

        // iterate and decide
        for (i, item) in items.iter().enumerate() {
            let cur: u8 = (*item).into();
            let max = stats.max_seen.get(i).copied().unwrap_or(initial_u8);
            let min = stats.min_seen.get(i).copied().unwrap_or(initial_u8);
            let first_flag = stats.seen_first.get(i).copied().unwrap_or(0);
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
        let items: Vec<C::Entry> = observer.as_iter().map(|x| *x).collect();

        // Phase 2: single mutable borrow to update stats
        let named = state.named_metadata_map_mut();
        let stats = named.get_mut::<TwoStateStats>(&self.name).ok_or_else(|| {
            Error::illegal_argument("TwoStateMapFeedback: stats metadata missing")
        })?;

        if stats.max_seen.len() < items.len() {
            stats.max_seen.resize(items.len(), initial_u8);
        }
        if stats.min_seen.len() < items.len() {
            stats.min_seen.resize(items.len(), initial_u8);
        }
        if stats.seen_first.len() < items.len() {
            stats.seen_first.resize(items.len(), 0);
        }

        for (i, item) in items.iter().enumerate() {
            let cur: u8 = (*item).into();
            // update bounds
            if cur > stats.max_seen[i] {
                stats.max_seen[i] = cur;
            }
            if cur < stats.min_seen[i] {
                stats.min_seen[i] = cur;
            }

            // mark first-discovery if history was initial and now non-initial
            if stats.max_seen[i] == initial_u8
                && stats.min_seen[i] == initial_u8
                && cur != initial_u8
                && stats.seen_first[i] == 0
            {
                stats.seen_first[i] = 1;
            }
        }

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

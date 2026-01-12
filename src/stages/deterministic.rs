use core::fmt::Debug;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use libafl::corpus::HasCurrentCorpusId;
use libafl::executors::{Executor, HasObservers};
use libafl::fuzzer::Evaluator;
use libafl::inputs::{BytesInput, HasMutatorBytes};
use libafl::stages::{Restartable, RetryCountRestartHelper, Stage};
use libafl::state::{HasCorpus, HasCurrentTestcase, HasExecutions, MaybeHasClientPerfMonitor};
use libafl::{Error, HasMetadata, HasNamedMetadata};
use libafl_bolts::tuples::{Handle, MatchNameRef};
use libafl_bolts::Named;
use std::borrow::Cow;
use std::time::Instant;

/// A small AFL++-inspired deterministic stage for `BytesInput`.
///
/// This stage is intentionally capped to keep runtime overhead low.
/// It runs only once for each corpus entry (when `scheduled_count() == 0`).
#[derive(Debug, Clone)]
pub struct DeterministicBytesStage<E, EM, S, Z> {
    name: Cow<'static, str>,
    max_positions: usize,
    timeout_ms: u64,
    effector_enabled: bool,
    effector_min_len: usize,
    effector_max_perc: u8,
    effector_observer_name: Cow<'static, str>,
    skip_if_no_effective_bytes: bool,
    phantom: PhantomData<(E, EM, S, Z)>,
}

impl<E, EM, S, Z> Named for DeterministicBytesStage<E, EM, S, Z> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<E, EM, S, Z> DeterministicBytesStage<E, EM, S, Z> {
    #[must_use]
    pub fn new(max_positions: usize) -> Self {
        // Keep defaults conservative: deterministic is expensive in diffray.
        // Users can bump these to match AFL++ more closely.
        let timeout_ms = std::env::var("DIFFRAY_DET_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60_000);

        let effector_enabled = std::env::var("DIFFRAY_DET_EFF_ENABLE")
            .ok()
            .as_deref()
            .map(|v| v != "0")
            .unwrap_or(true);

        let effector_min_len = std::env::var("DIFFRAY_DET_EFF_MIN_LEN")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(128);

        let effector_max_perc = std::env::var("DIFFRAY_DET_EFF_MAX_PERC")
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(90);

        let effector_observer_name = std::env::var("DIFFRAY_DET_EFF_OBSERVER")
            .ok()
            .filter(|s| !s.is_empty())
            .map(Cow::from)
            .unwrap_or_else(|| Cow::from("objective-edges"));

        let skip_if_no_effective_bytes = std::env::var("DIFFRAY_DET_SKIP_NO_EFF")
            .ok()
            .as_deref()
            .map(|v| v != "0")
            .unwrap_or(true);

        Self {
            name: Cow::from("deterministic_bytes"),
            max_positions,
            timeout_ms,
            effector_enabled,
            effector_min_len,
            effector_max_perc,
            effector_observer_name,
            skip_if_no_effective_bytes,
            phantom: PhantomData,
        }
    }

    #[inline]
    fn timed_out(start: &Instant, timeout_ms: u64) -> bool {
        if timeout_ms == 0 {
            return false;
        }
        start.elapsed().as_millis() as u64 >= timeout_ms
    }

    #[inline]
    fn bitmap_read(map: &[u8], idx: usize) -> bool {
        let byte = map[idx >> 3];
        (byte & (1u8 << (idx & 7))) != 0
    }

    #[inline]
    fn bitmap_set(map: &mut [u8], idx: usize) {
        let byte = &mut map[idx >> 3];
        *byte |= 1u8 << (idx & 7);
    }

    #[inline]
    fn objective_map_hash<E2>(executor: &E2, observer_name: &Cow<'static, str>) -> Option<u64>
    where
        E2: HasObservers,
        <E2 as HasObservers>::Observers: libafl_bolts::tuples::MatchName,
    {
        // The diffray `DiffExecutor` exposes a proxy tuple over primary/secondary/differential
        // observers. We hash the `objective-edges` hitcounts map to detect path changes.
        type ObjObs = libafl::observers::HitcountsMapObserver<
            libafl::observers::StdMapObserver<'static, u8, true>,
        >;
        type ObjObsTracked = libafl::observers::map::ExplicitTracking<ObjObs, true, false>;

        let observers = executor.observers();

        let mut hasher = libafl_bolts::hasher_std();

        // Prefer the tracked observer (current diffray default), but fall back to the
        // untracked type for older configurations.
        let handle_tracked = Handle::<ObjObsTracked>::new(observer_name.clone());
        if let Some(obs) = (&*observers).get(&handle_tracked) {
            obs.as_ref().hash(&mut hasher);
            return Some(hasher.finish());
        }

        let handle = Handle::<ObjObs>::new(observer_name.clone());
        let obs = (&*observers).get(&handle)?;
        obs.hash(&mut hasher);
        Some(hasher.finish())
    }

    #[inline]
    fn iter_offsets(limit: usize, max_positions: usize) -> impl Iterator<Item = usize> {
        let positions = core::cmp::min(limit, core::cmp::max(1, max_positions));
        (0..positions).map(move |k| {
            if positions <= 1 {
                0
            } else {
                (k * (limit - 1)) / (positions - 1)
            }
        })
    }

    #[inline]
    fn interesting_byte_values() -> &'static [u8] {
        // A small subset of AFL++ "interesting" 8-bit values.
        &[0x00, 0x01, 0x10, 0x20, 0x7f, 0x80, 0xff]
    }

    #[inline]
    fn interesting_u16_values() -> &'static [u16] {
        // A compact subset of AFL++ "interesting" 16-bit values.
        &[
            0x0000, 0x0001, 0x0010, 0x0020, 0x007f, 0x0080, 0x00ff, 0x0100, 0x0200, 0x03e8, 0x0400,
            0x1000, 0x7fff, 0x8000, 0xffff,
        ]
    }

    #[inline]
    fn interesting_u32_values() -> &'static [u32] {
        // A compact subset of AFL++ "interesting" 32-bit values.
        &[
            0x0000_0000,
            0x0000_0001,
            0x0000_0010,
            0x0000_0020,
            0x0000_007f,
            0x0000_0080,
            0x0000_00ff,
            0x0000_0100,
            0x0000_0200,
            0x0000_03e8,
            0x0000_0400,
            0x0000_1000,
            0x0000_ffff,
            0x0001_0000,
            0x7fff_ffff,
            0x8000_0000,
            0xffff_ffff,
        ]
    }

    #[inline]
    fn write_u16_le(buf: &mut [u8], off: usize, v: u16) {
        let b = v.to_le_bytes();
        buf[off] = b[0];
        buf[off + 1] = b[1];
    }

    #[inline]
    fn write_u32_le(buf: &mut [u8], off: usize, v: u32) {
        let b = v.to_le_bytes();
        buf[off] = b[0];
        buf[off + 1] = b[1];
        buf[off + 2] = b[2];
        buf[off + 3] = b[3];
    }

    #[inline]
    fn run_one<Z2, E2, EM2>(
        fuzzer: &mut Z2,
        executor: &mut E2,
        state: &mut S,
        manager: &mut EM2,
        input: &BytesInput,
    ) -> Result<bool, Error>
    where
        Z2: Evaluator<E2, EM2, BytesInput, S>,
        E2: Executor<EM2, BytesInput, S, Z2>,
    {
        let (_res, new_id) = fuzzer.evaluate_filtered(state, executor, manager, input)?;
        Ok(new_id.is_some())
    }
}

impl<E, EM, S, Z> Stage<E, EM, S, Z> for DeterministicBytesStage<E, EM, S, Z>
where
    E: Executor<EM, BytesInput, S, Z> + HasObservers,
    Z: Evaluator<E, EM, BytesInput, S>,
    <E as HasObservers>::Observers: libafl_bolts::tuples::MatchName,
    S: HasCorpus<BytesInput>
        + HasMetadata
        + HasNamedMetadata
        + HasExecutions
        + HasCurrentTestcase<BytesInput>
        + HasCurrentCorpusId
        + MaybeHasClientPerfMonitor,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error> {
        let det_start = Instant::now();
        {
            let testcase = state.current_testcase()?;
            // Align with AFL++/CalibrationStage behavior: do deterministic only once.
            if testcase.scheduled_count() > 0 {
                return Ok(());
            }
        }

        let orig = state.current_input_cloned()?;
        let len = orig.mutator_bytes().len();
        if len == 0 {
            return Ok(());
        }

        let mut work = orig.clone();

        let max_positions = core::cmp::max(1, self.max_positions);

        // AFL++-like: build an effector/skip map to avoid spending deterministic effort
        // on bytes that do not affect the observed execution.
        let sampled_positions: Vec<usize> = Self::iter_offsets(len, max_positions).collect();
        let mut use_skip_map = false;
        let mut skip_map = vec![0u8; (len + 7) / 8];

        if self.effector_enabled && len >= self.effector_min_len {
            if let Some(baseline_hash) =
                Self::objective_map_hash(executor, &self.effector_observer_name)
            {
                let mut effective = 0usize;

                for &i in &sampled_positions {
                    if Self::timed_out(&det_start, self.timeout_ms) {
                        return Ok(());
                    }

                    let base = orig.mutator_bytes()[i];
                    // Deterministic, cheap probe mutation.
                    let probe = base ^ 0xff;
                    work.mutator_bytes_mut()[i] = probe;
                    let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                    work.mutator_bytes_mut()[i] = base;

                    if let Some(h) =
                        Self::objective_map_hash(executor, &self.effector_observer_name)
                    {
                        if h != baseline_hash {
                            Self::bitmap_set(&mut skip_map, i);
                            effective += 1;
                        }
                    }
                }

                if effective == 0 && self.skip_if_no_effective_bytes {
                    // AFL++-like: if no effective bytes were found, skip deterministic.
                    return Ok(());
                }

                let denom = core::cmp::max(1, sampled_positions.len());
                let perc = (effective.saturating_mul(100)) / denom;

                if perc < self.effector_max_perc as usize {
                    use_skip_map = true;
                }
            }
        }

        // 1) Single-bit flips per byte (sampled across full length)
        for &i in &sampled_positions {
            if Self::timed_out(&det_start, self.timeout_ms) {
                return Ok(());
            }
            if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                continue;
            }
            let base = orig.mutator_bytes()[i];
            for bit in 0..8 {
                if Self::timed_out(&det_start, self.timeout_ms) {
                    return Ok(());
                }
                let new_val = base ^ (1u8 << bit);
                if new_val == base {
                    continue;
                }
                work.mutator_bytes_mut()[i] = new_val;
                let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                work.mutator_bytes_mut()[i] = base;
            }
        }

        // 1b) Whole-byte flip (xor 0xff)
        for &i in &sampled_positions {
            if Self::timed_out(&det_start, self.timeout_ms) {
                return Ok(());
            }
            if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                continue;
            }
            let base = orig.mutator_bytes()[i];
            let new_val = base ^ 0xff;
            if new_val == base {
                continue;
            }
            work.mutator_bytes_mut()[i] = new_val;
            let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
            work.mutator_bytes_mut()[i] = base;
        }

        // 2) "Interesting" byte overwrites (sampled across full length)
        for &i in &sampled_positions {
            if Self::timed_out(&det_start, self.timeout_ms) {
                return Ok(());
            }
            if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                continue;
            }
            let base = orig.mutator_bytes()[i];
            for &v in Self::interesting_byte_values() {
                if Self::timed_out(&det_start, self.timeout_ms) {
                    return Ok(());
                }
                if v == base {
                    continue;
                }
                work.mutator_bytes_mut()[i] = v;
                let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                work.mutator_bytes_mut()[i] = base;
            }
        }

        // 3) Small arithmetic (+1/-1) on bytes (sampled across full length)
        for &i in &sampled_positions {
            if Self::timed_out(&det_start, self.timeout_ms) {
                return Ok(());
            }
            if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                continue;
            }
            let base = orig.mutator_bytes()[i];
            for delta in [1u8, 0xffu8] {
                if Self::timed_out(&det_start, self.timeout_ms) {
                    return Ok(());
                }
                let new_val = base.wrapping_add(delta);
                if new_val == base {
                    continue;
                }
                work.mutator_bytes_mut()[i] = new_val;
                let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                work.mutator_bytes_mut()[i] = base;
            }
        }

        // 4) 16-bit interesting values + small arithmetic (+/-1), little-endian, sliding window
        if len >= 2 {
            let limit = len - 1;
            for i in Self::iter_offsets(limit, max_positions) {
                if Self::timed_out(&det_start, self.timeout_ms) {
                    return Ok(());
                }
                if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                    continue;
                }
                let mut orig16 = [0u8; 2];
                orig16.copy_from_slice(&orig.mutator_bytes()[i..i + 2]);
                let base = u16::from_le_bytes(orig16);

                for &v in Self::interesting_u16_values() {
                    if Self::timed_out(&det_start, self.timeout_ms) {
                        return Ok(());
                    }
                    if v == base {
                        continue;
                    }
                    Self::write_u16_le(work.mutator_bytes_mut(), i, v);
                    let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                }

                for delta in [1u16, 0xffffu16] {
                    if Self::timed_out(&det_start, self.timeout_ms) {
                        return Ok(());
                    }
                    let v = base.wrapping_add(delta);
                    if v == base {
                        continue;
                    }
                    Self::write_u16_le(work.mutator_bytes_mut(), i, v);
                    let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                }

                work.mutator_bytes_mut()[i..i + 2].copy_from_slice(&orig16);
            }
        }

        // 5) 32-bit interesting values + small arithmetic (+/-1), little-endian, sliding window
        if len >= 4 {
            let limit = len - 3;
            for i in Self::iter_offsets(limit, max_positions) {
                if Self::timed_out(&det_start, self.timeout_ms) {
                    return Ok(());
                }
                if use_skip_map && !Self::bitmap_read(&skip_map, i) {
                    continue;
                }
                let mut orig32 = [0u8; 4];
                orig32.copy_from_slice(&orig.mutator_bytes()[i..i + 4]);
                let base = u32::from_le_bytes(orig32);

                for &v in Self::interesting_u32_values() {
                    if Self::timed_out(&det_start, self.timeout_ms) {
                        return Ok(());
                    }
                    if v == base {
                        continue;
                    }
                    Self::write_u32_le(work.mutator_bytes_mut(), i, v);
                    let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                }

                for delta in [1u32, 0xffff_ffffu32] {
                    if Self::timed_out(&det_start, self.timeout_ms) {
                        return Ok(());
                    }
                    let v = base.wrapping_add(delta);
                    if v == base {
                        continue;
                    }
                    Self::write_u32_le(work.mutator_bytes_mut(), i, v);
                    let _ = Self::run_one(fuzzer, executor, state, manager, &work)?;
                }

                work.mutator_bytes_mut()[i..i + 4].copy_from_slice(&orig32);
            }
        }

        Ok(())
    }
}

impl<E, EM, S, Z> Restartable<S> for DeterministicBytesStage<E, EM, S, Z>
where
    S: HasMetadata + HasNamedMetadata + HasCurrentCorpusId,
{
    fn should_restart(&mut self, state: &mut S) -> Result<bool, Error> {
        RetryCountRestartHelper::should_restart(state, &self.name, 3)
    }

    fn clear_progress(&mut self, state: &mut S) -> Result<(), Error> {
        RetryCountRestartHelper::clear_progress(state, &self.name)
    }
}

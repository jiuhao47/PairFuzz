// Multi two-state map feedback: accepts multiple map observers and combines their
// "interesting" decisions. Default combine mode is AND (all observers must be
// interesting). The structure is intentionally extensible to allow other
// combination strategies in the future.
use crate::feedbacks::hist_stats::{history_contains, HistKey, HistVal};
use crate::feedbacks::two_state_map::TwoStateStats;
use crate::observers::ReturnValueObserver;
use core::fmt::Debug;
use libafl::corpus::Testcase;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::observers::MapObserver;
use libafl::state::HasExecutions;
use libafl::HasMetadata;
use libafl::{Error, HasNamedMetadata};
use libafl_bolts::tuples::Handle;
use libafl_bolts::tuples::Handled;
use libafl_bolts::tuples::MatchNameRef;
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::u8;

/// Mode for multi-observer decision. We implement AND now and leave
/// room to add OR, MAJORITY, custom closures, etc.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// debug mode: collect all info for each observer (found, indices, rv)
    Debug,
    /// single-coverage mode: collect per-observer `found` and treat interesting if any is true
    SingleCov,
    /// only consider single ReturnValueObserver novelty (per-observer rv novelty)
    SingleRv,
    /// single-per-map (PL) mode: collect per-map found/indices like SingleCov but
    /// treat combination policy separately (here we persist map coverage like SingleCov)
    SinglePL,
    /// combo-per-map (PL) mode: treat the vector of per-map indices-counts as a combo key
    ComboPL,
    /// combo-coverage mode: consider the vector of per-observer indices (exact index sets)
    /// and check whether this per-observer index-set combo has been seen before.
    ComboCov,
    /// consider the tuple (rv1, rv2, ...) across observers and check if that combo appeared before
    ComboRv,
}

/// Per-observer configuration for DiffrayFeedback
#[derive(Clone, Debug)]
pub struct DiffrayConfig {
    pub high_delta: u8,
    pub low_delta: u8,
    pub check_high: bool,
    pub check_low: bool,
}

impl Default for DiffrayConfig {
    fn default() -> Self {
        Self {
            high_delta: 1,
            low_delta: 1,
            check_high: true,
            check_low: false,
        }
    }
}

impl DiffrayConfig {
    /// NewOnly template: disables both high/low checks so the observer only
    /// reports newly-seen indices (no delta-based checks).
    pub fn new_only() -> Self {
        Self {
            high_delta: 1,
            low_delta: 1,
            check_high: false,
            check_low: false,
        }
    }
}

/// Per-observer collected info for combiners to use later.
/// `found` mirrors the boolean interesting decision derived from the
/// two-state logic for that observer. `indices` contains non-initial
/// indices observed in this execution (useful for combo modes).
#[derive(Debug, Clone)]
pub struct ObserverInfo {
    pub idx: usize,
    pub name: Cow<'static, str>,
    pub found: bool,
    pub indices: Option<Vec<(usize, u8)>>,
    /// Optional return-value observer (name, last_value) collected for this map observer
    pub return_value: Option<(String, Option<i32>)>,
}

#[derive(Debug, Clone)]
pub struct DiffrayFeedback<C> {
    /// names and handles for each observer involved
    pub(crate) names: Vec<Cow<'static, str>>,
    pub(crate) _stats_names: Vec<Cow<'static, str>>,
    pub(crate) map_refs: Vec<Handle<C>>,
    /// optional ReturnValueObserver handles per map observer
    pub(crate) rv_refs: Vec<Option<Handle<ReturnValueObserver>>>,
    /// per-observer config objects
    pub(crate) configs: Vec<DiffrayConfig>,
    /// novelties aggregated across all observers (optional when observers support novelties)
    pub(crate) novelties: Option<Vec<usize>>,
    /// cached joined names (e.g. "a,b") to avoid repeated join/format
    pub(crate) _names_joined: String,
    /// track-hit flag compatibility with single-version implementation
    #[cfg(feature = "track_hit_feedbacks")]
    pub(crate) last: bool,
    /// (per-observer) thresholds and flags are stored in `configs`
    /// how to combine per-observer interesting results
    pub(crate) mode: Mode,
}

// Module-level gated debug logger for DiffrayFeedback
fn feedback_debug_log(tag: &str, msg: &str) {
    if crate::driver::env_var::env_bool_cached(crate::driver::env_var::FEEDBACK_DEBUG) {
        crate::utils::logging::debug(tag, msg);
    }
}

// Lazy variant: accept a closure so callers avoid constructing Strings when
// logging is disabled.
fn feedback_debug_log_lazy<F: FnOnce() -> String>(tag: &str, msg_fn: F) {
    if crate::driver::env_var::env_bool_cached(crate::driver::env_var::FEEDBACK_DEBUG) {
        crate::utils::logging::debug(tag, &msg_fn());
    }
}

/// Methods for DiffrayFeedback
impl<C> DiffrayFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
{
    /// General constructor: build a two-observer DiffrayFeedback using the
    /// provided `mode`. Optionally supply return-value observers when the
    /// mode requires them (SingleRv/ComboRv). This consolidates construction
    /// logic in one place so callers can choose mode-driven construction.
    pub fn new_with_mode(
        map_a: &C,
        map_b: &C,
        mode: Mode,
        rv_a: Option<&ReturnValueObserver>,
        rv_b: Option<&ReturnValueObserver>,
    ) -> Self
    where
        C: libafl_bolts::Named,
    {
        let names: Vec<Cow<'static, str>> = vec![map_a.name().clone(), map_b.name().clone()];
        let stats_names = names.clone();
        let map_refs: Vec<Handle<C>> = vec![map_a.handle(), map_b.handle()];

        // RV handles: populate when provided; otherwise None entries
        let rv_refs: Vec<Option<Handle<ReturnValueObserver>>> =
            vec![rv_a.map(|r| r.handle()), rv_b.map(|r| r.handle())];

        // Choose per-observer configs: for PL/RV modes prefer new_only(),
        // otherwise use default two-state deltas for coverage modes.
        let configs: Vec<DiffrayConfig> = match mode {
            Mode::SingleRv | Mode::ComboRv | Mode::SinglePL | Mode::ComboPL => {
                vec![DiffrayConfig::new_only(), DiffrayConfig::new_only()]
            }
            _ => vec![DiffrayConfig::default(), DiffrayConfig::default()],
        };

        let novelties = if C::NOVELTIES { Some(vec![]) } else { None };

        // Cache joined names to avoid repeated join()/format! calls on hot path
        let names_joined = names
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(",");

        DiffrayFeedback {
            names,
            _stats_names: stats_names,
            map_refs,
            rv_refs,
            configs,
            novelties,
            _names_joined: names_joined,
            #[cfg(feature = "track_hit_feedbacks")]
            last: false,
            mode,
        }
    }
    /// Construct a DiffrayFeedback for two map observers in SingleCov mode.
    ///
    /// This helper is convenient for the driver which commonly combines
    /// two coverage observers (A and B). It preserves observer names,
    /// map handles and default per-observer configs. RV handles are left
    /// empty since SingleCov doesn't consider return-values.
    pub fn new_single_cov(map_a: &C, map_b: &C) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::SingleCov, None, None)
    }

    /// Construct a DiffrayFeedback for two return-value observers in SingleRv mode.
    ///
    /// This helper is convenient for the driver which may select return-value
    /// novelty as the primary feedback. It stores rv handles and leaves map
    /// handles empty since SingleRv does not consider coverage maps.
    pub fn new_single_rv(
        rv_a: &ReturnValueObserver,
        rv_b: &ReturnValueObserver,
        map_a: &C,
        map_b: &C,
    ) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::SingleRv, Some(rv_a), Some(rv_b))
    }

    /// Construct a DiffrayFeedback for two map observers in SinglePL mode.
    ///
    /// Similar to SingleCov but the combination/novelty semantics are handled
    /// by the PL logic (single PL counts per map).
    pub fn new_single_pl(map_a: &C, map_b: &C) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::SinglePL, None, None)
    }

    /// Construct a DiffrayFeedback for two map observers in ComboCov mode.
    ///
    /// Similar to ComboPL but uses exact per-observer indices vectors as the
    /// novelty key (Mode::ComboCov).
    pub fn new_combo_cov(map_a: &C, map_b: &C) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::ComboCov, None, None)
    }

    /// Construct a DiffrayFeedback for two return-value observers in ComboRv mode.
    ///
    /// Similar to `new_two_single_rv` but sets the mode to `ComboRv` so the
    /// feedback considers tuples of return-values for novelty.
    pub fn new_combo_rv(
        rv_a: &ReturnValueObserver,
        rv_b: &ReturnValueObserver,
        map_a: &C,
        map_b: &C,
    ) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::ComboRv, Some(rv_a), Some(rv_b))
    }

    /// Construct a DiffrayFeedback for two map observers in ComboPL mode.
    ///
    /// This behaves like SinglePL for collection but treats the vector of per-map
    /// indices-counts as a combo novelty key persisted as ComboHistStats.
    pub fn new_combo_pl(map_a: &C, map_b: &C) -> Self
    where
        C: libafl_bolts::Named,
    {
        Self::new_with_mode(map_a, map_b, Mode::ComboPL, None, None)
    }

    // collector helpers have been moved to `src/feedbacks/collect.rs`;
    // callers now invoke the helpers in that module directly.
    /// Generalized historical-seen query for different modes.
    /// Centralizes logic used by the specific seen_in_history helpers.
    pub fn seen_history_for<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
        mode: Mode,
    ) -> bool {
        match mode {
            Mode::ComboPL => {
                let mut counts: SmallVec<[HistVal; 4]> = SmallVec::with_capacity(infos.len());
                for info in infos.iter() {
                    if let Some(idxs) = &info.indices {
                        counts.push(HistVal::Pl(idxs.len()));
                    } else {
                        counts.push(HistVal::Pl(0usize));
                    }
                }
                let stats_name = format!("ComboPL({})", self._names_joined);
                history_contains(state, &stats_name, &HistKey::Combo(counts.into_vec()))
            }
            Mode::ComboCov => {
                let mut vecs: SmallVec<[HistVal; 4]> = SmallVec::with_capacity(infos.len());
                for info in infos.iter() {
                    if let Some(idxs) = &info.indices {
                        let mut indices_only: SmallVec<[usize; 8]> =
                            SmallVec::with_capacity(idxs.len());
                        for (i, _v) in idxs.iter() {
                            indices_only.push(*i);
                        }
                        vecs.push(HistVal::Cov(indices_only.into_vec()));
                    } else {
                        vecs.push(HistVal::Cov(Vec::new()));
                    }
                }
                let stats_name = format!("ComboCov({})", self._names_joined);
                history_contains(state, &stats_name, &HistKey::Combo(vecs.into_vec()))
            }
            Mode::ComboRv => {
                let mut key: SmallVec<[HistVal; 4]> = SmallVec::new();
                let mut names: SmallVec<[String; 4]> = SmallVec::new();
                for (idx, opt_h) in self.rv_refs.iter().enumerate() {
                    if let Some(handle) = opt_h.as_ref() {
                        names.push(handle.name().to_string());
                        if let Some(info) = infos.iter().find(|i| i.idx == idx) {
                            key.push(HistVal::Rv(
                                info.return_value.as_ref().map(|t| t.1).unwrap_or(None),
                            ));
                        } else {
                            key.push(HistVal::Rv(None));
                        }
                    }
                }
                if names.is_empty() {
                    return true;
                }
                let stats_name = format!(
                    "ReturnValueCombo({})",
                    names
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                history_contains(state, &stats_name, &HistKey::Combo(key.into_vec()))
            }
            Mode::SinglePL => {
                for info in infos.iter() {
                    if let Some(idxs) = &info.indices {
                        let count = idxs.len();
                        let stats_name = format!("SinglePL({})", info.name);
                        if !history_contains(
                            state,
                            &stats_name,
                            &HistKey::Single(info.name.to_string(), HistVal::Pl(count)),
                        ) {
                            return false;
                        }
                    }
                }
                true
            }
            Mode::SingleRv => {
                for info in infos.iter() {
                    if let Some((ref name, val_opt)) = &info.return_value {
                        let stats_name = format!("ReturnValueSingle({})", name);
                        if !history_contains(
                            state,
                            &stats_name,
                            &HistKey::Single(name.clone(), HistVal::Rv(*val_opt)),
                        ) {
                            return false;
                        }
                    }
                }
                true
            }
            _ => true,
        }
    }

    /// Return true if ALL observed single RVs in `infos` have been seen historically.
    /// Vacuously true if there are no observed RVs.
    pub fn single_rv_seen_in_history<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        self.seen_history_for(state, infos, Mode::SingleRv)
    }

    /// Return true if ALL observed indices-lengths in `infos` have been seen historically.
    /// Vacuously true if there are no observed indices.
    pub fn single_pl_seen_in_history<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        self.seen_history_for(state, infos, Mode::SinglePL)
    }

    /// Check whether the vector of per-observer exact indices vectors has been seen historically
    pub fn combo_cov_seen_in_history<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        self.seen_history_for(state, infos, Mode::ComboCov)
    }

    /// Check whether the tuple of return-values for the configured rv handles
    /// (in the same order as self.rv_refs) has been seen before.
    pub fn combo_rv_seen_in_history<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        self.seen_history_for(state, infos, Mode::ComboRv)
    }

    /// Check whether the vector of per-map indices-lengths has been seen historically
    pub fn combo_pl_seen_in_history<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        self.seen_history_for(state, infos, Mode::ComboPL)
    }

    /// Combine collected ObserverInfo using the configured mode. For SingleRv mode
    /// we consider the run interesting if any observer's rv is not present in history.
    pub fn combine_observer_infos<S: HasNamedMetadata>(
        &self,
        state: &S,
        infos: &[ObserverInfo],
    ) -> bool {
        match self.mode {
            Mode::Debug => infos.iter().all(|i| i.found),
            Mode::SingleCov => infos.iter().any(|i| i.found),
            Mode::SinglePL => {
                // interesting if any observer's indices length is new (not seen historically)
                !self.single_pl_seen_in_history(state, infos)
            }
            Mode::ComboPL => {
                // build vector of counts and check if this vector has been seen
                !self.combo_pl_seen_in_history(state, infos)
            }
            Mode::ComboCov => {
                // build vector of per-observer indices and check if this exact combo has been seen
                !self.combo_cov_seen_in_history(state, infos)
            }
            Mode::SingleRv => {
                // interesting if NOT all observed single RVs have been seen historically
                !self.single_rv_seen_in_history(state, infos)
            }
            Mode::ComboRv => {
                // interesting if the combo of rv values across observers has not been seen before
                !self.combo_rv_seen_in_history(state, infos)
            }
        }
    }

    /// Helper to persist single-coverage map history (copied from two_state_map append logic)
    pub fn append_single_cov<EM, I, OT, S>(
        &self,
        state: &mut S,
        _manager: &mut EM,
        observers: &OT,
        testcase: &mut Testcase<I>,
    ) -> Result<(), Error>
    where
        S: HasNamedMetadata + HasExecutions,
        EM: libafl::events::EventFirer<I, S>,
        OT: MatchNameRef,
    {
        // implementation moved to `metadata.rs`
        crate::feedbacks::metadata::append_single_cov(self, state, _manager, observers, testcase)
    }

    /// Helper to persist single-RV history
    pub fn append_single_rv<S, OT>(&self, state: &mut S, observers: &OT) -> Result<(), Error>
    where
        S: HasNamedMetadata,
        OT: MatchNameRef,
    {
        // implementation moved to `metadata.rs`
        crate::feedbacks::metadata::append_single_rv(self, state, observers)
    }

    /// Persist indices-lengths (counts) into SinglePlStats for observers that reported indices.
    pub fn append_single_pl<S, OT>(&self, state: &mut S, observers: &OT) -> Result<(), Error>
    where
        S: HasNamedMetadata,
        OT: MatchNameRef,
    {
        // implementation moved to `metadata.rs`
        crate::feedbacks::metadata::append_single_pl(self, state, observers)
    }

    /// Persist the vector of per-observer indices (exact index sets) into ComboSeenStats
    pub fn append_combo_cov<S, OT>(&self, state: &mut S, observers: &OT) -> Result<(), Error>
    where
        S: HasNamedMetadata,
        OT: MatchNameRef,
    {
        crate::feedbacks::metadata::append_combo_cov(self, state, observers)
    }

    /// Helper to persist combo-RV history
    pub fn append_combo_rv<S, OT>(&self, state: &mut S, observers: &OT) -> Result<(), Error>
    where
        S: HasNamedMetadata,
        OT: MatchNameRef,
    {
        // implementation moved to `metadata.rs`
        crate::feedbacks::metadata::append_combo_rv(self, state, observers)
    }

    /// Persist the vector of per-map counts into ComboPlStats
    pub fn append_combo_pl<S, OT>(&self, state: &mut S, observers: &OT) -> Result<(), Error>
    where
        S: HasNamedMetadata,
        OT: MatchNameRef,
    {
        // implementation moved to `metadata.rs`
        crate::feedbacks::metadata::append_combo_pl(self, state, observers)
    }
}

impl<C> Named for DiffrayFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>
        + Named,
{
    fn name(&self) -> &Cow<'static, str> {
        // The constructor guarantees at least one observer; assert in debug
        // builds so incorrect construction is caught early during development.
        debug_assert!(
            !self.names.is_empty(),
            "DiffrayFeedback.names must be non-empty"
        );
        &self.names[0]
    }
}

impl<C> libafl::feedbacks::HasObserverHandle for DiffrayFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>
        + Named,
{
    type Observer = C;

    fn observer_handle(&self) -> &Handle<Self::Observer> {
        // Constructor guarantees at least one handle.
        debug_assert!(
            !self.map_refs.is_empty(),
            "DiffrayFeedback.map_refs must be non-empty"
        );
        &self.map_refs[0]
    }
}

impl<C, S> StateInitializer<S> for DiffrayFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>
        + Named,
    S: HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        // For each observer we add TwoStateStats and MapFeedbackMetadata under
        // the observer's name so other tools see per-observer history as before.
        let cap: usize = std::env::var("DIFFRAY_TWOSTATE_CAPACITY")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(65536usize);
        let bits_words = (cap + 63) / 64;
        for name in self.names.iter() {
            state.add_named_metadata_checked(
                name,
                TwoStateStats {
                    max_seen: vec![u8::MIN; cap],
                    min_seen: vec![u8::MAX; cap],
                    seen_first_bits: vec![0u64; bits_words],
                },
            )?;
            state.add_named_metadata_checked(
                name,
                libafl::feedbacks::MapFeedbackMetadata::<u8>::default(),
            )?;
            crate::utils::logging::debug(
                "diffray_feedback",
                &format!(
                    "Initialized TwoStateStats metadata '{}' with capacity {}",
                    name, cap
                ),
            );
        }
        Ok(())
    }
}

impl<C, EM, I, OT, S> Feedback<EM, I, OT, S> for DiffrayFeedback<C>
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
        feedback_debug_log("diffray_feedback", "enter is_interesting()");

        // Part 1: collect per-observer infos (delegated to collect module)
        let infos = crate::feedbacks::collect::collect::<C, S, OT>(self, state, observers)?;
        // NOTE: explicit clearing must happen at a caller that holds a mutable
        // reference to the observers. Clearing here (in `is_interesting`) is
        // unsafe because we only have an immutable `observers` reference and
        // it can race with the collector. The previous implementation cleared
        // shared memory from here and caused timing issues; that behavior was
        // removed. Use the mutable helper in `collect` when you are at a site
        // that owns `&mut observers` (for example, crash handlers or other
        // executor code paths) and have access to the testcase' `MapIndexesMetadata`.
        // Part 2: combine using the configured mode (separated for extensibility)
        let final_result = self.combine_observer_infos(state, &infos);
        feedback_debug_log_lazy("diffray_feedback", || {
            format!(
                "final is_interesting -> {} (infos={:?})",
                final_result,
                infos.iter().map(|i| i.found).collect::<Vec<_>>()
            )
        });
        Ok(final_result)
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

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        observers: &OT,
        testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        // Persist return-value history for SingleRv / ComboRv modes.
        match self.mode {
            Mode::Debug => {
                // Debug: do not persist additional state beyond novelties here
            }

            Mode::SingleRv => {
                // Persist single-RV history
                self.append_single_rv::<S, OT>(state, observers)?;
                // Also persist map observer coverage metadata (if any map observers configured).
                // Map coverage should be collected and appended, but not participate in the
                // 'interesting' decision for RV modes.
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
            Mode::ComboRv => {
                // Persist combo-RV history
                self.append_combo_rv::<S, OT>(state, observers)?;
                // Also persist map observer coverage metadata (if any map observers configured).
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
            Mode::SingleCov => {
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
            Mode::SinglePL => {
                // Persist SinglePL counts then coverage metadata
                self.append_single_pl::<S, OT>(state, observers)?;
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
            Mode::ComboPL => {
                // Persist combo PL vector and coverage metadata
                self.append_combo_pl::<S, OT>(state, observers)?;
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
            Mode::ComboCov => {
                // Persist combo exact indices vector and coverage metadata
                self.append_combo_cov::<S, OT>(state, observers)?;
                self.append_single_cov::<EM, I, OT, S>(state, _manager, observers, testcase)?;
            }
        }

        // Attach novelties metadata (if any), like TwoStateMapFeedback does.
        if let Some(novelties) = self.novelties.as_mut().map(core::mem::take) {
            let meta = libafl::feedbacks::MapNoveltiesMetadata::new(novelties);
            testcase.add_metadata(meta);
        }

        // Attempt to clear any temporary source shared-memory used by the
        // ShmemComboUpdater observer. We delegate to a helper so the logic
        // is centralized and easier to reuse or test.
        self.clear_shmem_combo_updater_sources(observers);

        Ok(())
    }
}

impl<C> DiffrayFeedback<C>
where
    C: libafl::observers::CanTrack
        + MapObserver
        + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
{
    /// Helper to clear ShmemComboUpdater source buffers when we only have an
    /// immutable reference to the observers. This is a best-effort operation
    /// that skips silently if no updater is present.
    fn clear_shmem_combo_updater_sources<OT>(&self, observers: &OT)
    where
        OT: MatchNameRef,
    {
        use crate::observers::shmem::ShmemComboUpdater;
        use libafl_bolts::tuples::Handle;
        use std::borrow::Cow;

        let handle: Handle<ShmemComboUpdater> = Handle::new(Cow::from("shmem_combo_updater"));
        if let Some(upd) = observers.get(&handle) {
            // Best-effort clear; skip silently if no updater is present.
            upd.clear_shmem_sources();
            feedback_debug_log(
                "diffray_feedback",
                "append_metadata: cleared shmem_combo_updater sources",
            );
        }
    }
}

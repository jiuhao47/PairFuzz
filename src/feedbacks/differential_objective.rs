use crate::{
    feedbacks::{
        objectives::{CrashInfo, ExitInfo},
        signature::{
            compute_signature, DifferentialSignature, DifferentialSignatureMetadata,
            DifferentialSignatureSetMetadata, DiscoveryElapsedMetadata, ExitClass,
        },
    },
    observers::ReturnValueObserver,
};
use libafl::{
    corpus::Testcase,
    executors::{DiffExitKind, ExitKind},
    feedbacks::{Feedback, MapFeedbackMetadata, StateInitializer},
    observers::MapObserver,
    state::HasStartTime,
    Error, HasMetadata, HasNamedMetadata,
};
use libafl_bolts::{
    current_time,
    tuples::{Handle, Handled, MatchNameRef},
    AsIter, Named,
};
use std::{borrow::Cow, collections::HashSet};

const OBJECTIVE_A: &str = "objective-a";
const OBJECTIVE_B: &str = "objective-b";

#[derive(Clone, Copy)]
struct Evaluation<'a> {
    exit_a: ExitClass,
    exit_b: ExitClass,
    rv_a: Option<i32>,
    rv_b: Option<i32>,
    map_a: &'a [u8],
    map_b: &'a [u8],
    initial_a: u8,
    initial_b: u8,
}

#[derive(Clone, Debug)]
struct CandidateRecord {
    exit_a: ExitClass,
    exit_b: ExitClass,
    rv_a: Option<i32>,
    rv_b: Option<i32>,
    signature: DifferentialSignature,
    map_a: Vec<u8>,
    map_b: Vec<u8>,
    initial_a: u8,
    initial_b: u8,
}

fn map_novelty(map: &[u8], history: &[u8]) -> bool {
    map.iter()
        .enumerate()
        .any(|(index, value)| *value > history.get(index).copied().unwrap_or(0))
}

fn direct_dual_map_novelty(map_a: &[u8], map_b: &[u8], history_a: &[u8], history_b: &[u8]) -> bool {
    let novel_a = map_novelty(map_a, history_a);
    let novel_b = map_novelty(map_b, history_b);
    novel_a || novel_b
}

#[cfg(test)]
fn concatenated_map_novelty(
    map_a: &[u8],
    map_b: &[u8],
    history_a: &[u8],
    history_b: &[u8],
) -> bool {
    map_a
        .iter()
        .chain(map_b)
        .zip(history_a.iter().chain(history_b))
        .any(|(value, history)| value > history)
}

#[derive(Default)]
struct CandidateEvaluator {
    pending: Option<CandidateRecord>,
    map_scans: usize,
}

impl CandidateEvaluator {
    fn reset(&mut self) {
        self.pending = None;
        self.map_scans = 0;
    }

    fn semantic_candidate(input: &Evaluation<'_>) -> bool {
        let exceptional = matches!(
            input.exit_a,
            ExitClass::Crash | ExitClass::Timeout | ExitClass::Oom
        ) || matches!(
            input.exit_b,
            ExitClass::Crash | ExitClass::Timeout | ExitClass::Oom
        );
        let return_difference =
            input.rv_a.map(|value| value == 0) != input.rv_b.map(|value| value == 0);
        exceptional || return_difference
    }

    fn evaluate(
        &mut self,
        input: Evaluation<'_>,
        known: &HashSet<DifferentialSignature>,
        history_a: &[u8],
        history_b: &[u8],
    ) -> bool {
        self.reset();
        if !Self::semantic_candidate(&input) {
            return false;
        }

        let signature = compute_signature(
            input.exit_a,
            input.exit_b,
            input.rv_a,
            input.rv_b,
            input.map_a,
            input.map_b,
        );
        if known.contains(&signature) {
            return false;
        }

        self.map_scans = 2;
        if !direct_dual_map_novelty(input.map_a, input.map_b, history_a, history_b) {
            return false;
        }

        self.pending = Some(CandidateRecord {
            exit_a: input.exit_a,
            exit_b: input.exit_b,
            rv_a: input.rv_a,
            rv_b: input.rv_b,
            signature,
            map_a: input.map_a.to_vec(),
            map_b: input.map_b.to_vec(),
            initial_a: input.initial_a,
            initial_b: input.initial_b,
        });
        true
    }

    #[cfg(test)]
    fn pending(&self) -> Option<&CandidateRecord> {
        self.pending.as_ref()
    }

    #[cfg(test)]
    fn map_scans(&self) -> usize {
        self.map_scans
    }
}

pub struct DifferentialObjective<C> {
    name: Cow<'static, str>,
    map_a: Handle<C>,
    map_b: Handle<C>,
    rv_a: Handle<ReturnValueObserver>,
    rv_b: Handle<ReturnValueObserver>,
    evaluator: CandidateEvaluator,
    last_result: Option<bool>,
}

impl<C> DifferentialObjective<C>
where
    C: Named,
{
    #[must_use]
    pub fn new(
        map_a: &C,
        map_b: &C,
        rv_a: &ReturnValueObserver,
        rv_b: &ReturnValueObserver,
    ) -> Self {
        Self {
            name: Cow::Borrowed("DifferentialObjective"),
            map_a: map_a.handle(),
            map_b: map_b.handle(),
            rv_a: rv_a.handle(),
            rv_b: rv_b.handle(),
            evaluator: CandidateEvaluator::default(),
            last_result: None,
        }
    }
}

impl<C> Named for DifferentialObjective<C> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<C, S> StateInitializer<S> for DifferentialObjective<C>
where
    S: HasMetadata + HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        state.add_named_metadata_checked(OBJECTIVE_A, MapFeedbackMetadata::<u8>::default())?;
        state.add_named_metadata_checked(OBJECTIVE_B, MapFeedbackMetadata::<u8>::default())?;
        if !state.has_metadata::<DifferentialSignatureSetMetadata>() {
            state.add_metadata(DifferentialSignatureSetMetadata::default());
        }
        Ok(())
    }
}

fn diff_exit_class(kind: DiffExitKind) -> ExitClass {
    match kind {
        DiffExitKind::Ok => ExitClass::Ok,
        DiffExitKind::Crash => ExitClass::Crash,
        DiffExitKind::Timeout => ExitClass::Timeout,
        DiffExitKind::Oom => ExitClass::Oom,
        DiffExitKind::Diff => ExitClass::Other,
    }
}

fn exit_classes(kind: &ExitKind) -> (ExitClass, ExitClass) {
    match kind {
        ExitKind::Diff { primary, secondary } => {
            (diff_exit_class(*primary), diff_exit_class(*secondary))
        }
        ExitKind::Ok => (ExitClass::Ok, ExitClass::Ok),
        ExitKind::Crash => (ExitClass::Crash, ExitClass::Crash),
        ExitKind::Timeout => (ExitClass::Timeout, ExitClass::Timeout),
        ExitKind::Oom => (ExitClass::Oom, ExitClass::Oom),
    }
}

fn update_history(history: &mut MapFeedbackMetadata<u8>, map: &[u8], initial: u8) {
    if history.history_map.len() < map.len() {
        history.history_map.resize(map.len(), initial);
    }
    for (old, current) in history.history_map.iter_mut().zip(map) {
        if *old == initial && *current != initial {
            history.num_covered_map_indexes += 1;
        }
        *old = (*old).max(*current);
    }
}

impl<C, EM, I, OT, S> Feedback<EM, I, OT, S> for DifferentialObjective<C>
where
    C: MapObserver<Entry = u8> + for<'it> AsIter<'it, Item = u8>,
    OT: MatchNameRef,
    S: HasMetadata + HasNamedMetadata + HasStartTime,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        self.evaluator.reset();
        let rv_a = observers
            .get(&self.rv_a)
            .ok_or_else(|| Error::illegal_state("return observer A missing"))?
            .last_value();
        let rv_b = observers
            .get(&self.rv_b)
            .ok_or_else(|| Error::illegal_state("return observer B missing"))?
            .last_value();
        let (exit_a, exit_b) = exit_classes(exit_kind);

        let semantic_probe = Evaluation {
            exit_a,
            exit_b,
            rv_a,
            rv_b,
            map_a: &[],
            map_b: &[],
            initial_a: 0,
            initial_b: 0,
        };
        if !CandidateEvaluator::semantic_candidate(&semantic_probe) {
            self.last_result = Some(false);
            return Ok(false);
        }

        let map_a = observers
            .get(&self.map_a)
            .ok_or_else(|| Error::illegal_state("objective map A missing"))?;
        let map_b = observers
            .get(&self.map_b)
            .ok_or_else(|| Error::illegal_state("objective map B missing"))?;
        let bytes_a: Vec<u8> = map_a.as_iter().map(|value| *value).collect();
        let bytes_b: Vec<u8> = map_b.as_iter().map(|value| *value).collect();

        let empty_known = HashSet::new();
        let known = state
            .metadata_map()
            .get::<DifferentialSignatureSetMetadata>()
            .map(DifferentialSignatureSetMetadata::accepted)
            .unwrap_or(&empty_known);
        let history_a = state
            .named_metadata::<MapFeedbackMetadata<u8>>(OBJECTIVE_A)?
            .history_map
            .as_slice();
        let history_b = state
            .named_metadata::<MapFeedbackMetadata<u8>>(OBJECTIVE_B)?
            .history_map
            .as_slice();
        let result = self.evaluator.evaluate(
            Evaluation {
                exit_a,
                exit_b,
                rv_a,
                rv_b,
                map_a: &bytes_a,
                map_b: &bytes_b,
                initial_a: map_a.initial(),
                initial_b: map_b.initial(),
            },
            known,
            history_a,
            history_b,
        );
        self.last_result = Some(result);
        Ok(result)
    }

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        let Some(candidate) = self.evaluator.pending.take() else {
            return Ok(());
        };
        state
            .metadata_or_insert_with(DifferentialSignatureSetMetadata::default)
            .insert(candidate.signature);
        update_history(
            state.named_metadata_mut(OBJECTIVE_A)?,
            &candidate.map_a,
            candidate.initial_a,
        );
        update_history(
            state.named_metadata_mut(OBJECTIVE_B)?,
            &candidate.map_b,
            candidate.initial_b,
        );

        let elapsed = current_time().saturating_sub(*state.start_time());
        let nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        testcase.add_metadata(DifferentialSignatureMetadata::from(candidate.signature));
        testcase.add_metadata(DiscoveryElapsedMetadata { nanos });
        let exceptional = matches!(
            candidate.exit_a,
            ExitClass::Crash | ExitClass::Timeout | ExitClass::Oom
        ) || matches!(
            candidate.exit_b,
            ExitClass::Crash | ExitClass::Timeout | ExitClass::Oom
        );
        testcase.add_metadata(ExitInfo {
            note: if exceptional {
                "differential_exit".to_owned()
            } else {
                "rv_zero_nonzero".to_owned()
            },
            rv_a: candidate.rv_a,
            rv_b: candidate.rv_b,
            crash: exceptional.then(|| CrashInfo {
                label: format!("{:?}/{:?}", candidate.exit_a, candidate.exit_b),
                primary: Some(format!("{:?}", candidate.exit_a)),
                secondary: Some(format!("{:?}", candidate.exit_b)),
            }),
        });
        Ok(())
    }

    fn last_result(&self) -> Result<bool, Error> {
        self.last_result
            .ok_or_else(|| Error::illegal_state("differential objective has not run"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        concatenated_map_novelty, direct_dual_map_novelty, CandidateEvaluator, Evaluation,
    };
    use crate::feedbacks::signature::{compute_signature, ExitClass};
    use std::collections::HashSet;

    fn evaluation<'a>(
        exit_a: ExitClass,
        exit_b: ExitClass,
        rv_a: Option<i32>,
        rv_b: Option<i32>,
        map_a: &'a [u8],
        map_b: &'a [u8],
    ) -> Evaluation<'a> {
        Evaluation {
            exit_a,
            exit_b,
            rv_a,
            rv_b,
            map_a,
            map_b,
            initial_a: 0,
            initial_b: 0,
        }
    }

    #[test]
    fn rejected_candidate_cannot_leak_metadata() {
        let mut evaluator = CandidateEvaluator::default();
        let known = HashSet::new();
        assert!(evaluator.evaluate(
            evaluation(ExitClass::Ok, ExitClass::Ok, Some(0), Some(1), &[1], &[0],),
            &known,
            &[0],
            &[0],
        ));
        assert!(evaluator.pending().is_some());

        assert!(!evaluator.evaluate(
            evaluation(ExitClass::Ok, ExitClass::Ok, Some(0), Some(0), &[1], &[0],),
            &known,
            &[0],
            &[0],
        ));
        assert!(evaluator.pending().is_none());
    }

    #[test]
    fn crash_and_timeout_exit_classes_are_preserved() {
        let mut evaluator = CandidateEvaluator::default();
        assert!(evaluator.evaluate(
            evaluation(ExitClass::Crash, ExitClass::Timeout, None, None, &[1], &[0],),
            &HashSet::new(),
            &[0],
            &[0],
        ));
        let candidate = evaluator.pending().unwrap();
        assert_eq!(candidate.exit_a, ExitClass::Crash);
        assert_eq!(candidate.exit_b, ExitClass::Timeout);
    }

    #[test]
    fn zero_nonzero_difference_is_detected() {
        let mut evaluator = CandidateEvaluator::default();
        assert!(evaluator.evaluate(
            evaluation(ExitClass::Ok, ExitClass::Ok, Some(0), Some(-7), &[0], &[1],),
            &HashSet::new(),
            &[0],
            &[0],
        ));
    }

    #[test]
    fn ordinary_input_skips_both_objective_map_scans() {
        let mut evaluator = CandidateEvaluator::default();
        assert!(!evaluator.evaluate(
            evaluation(
                ExitClass::Ok,
                ExitClass::Ok,
                Some(1),
                Some(2),
                &[9; 64],
                &[9; 64],
            ),
            &HashSet::new(),
            &[0; 64],
            &[0; 64],
        ));
        assert_eq!(evaluator.map_scans(), 0);
    }

    #[test]
    fn known_signature_is_rejected_before_objective_map_scans() {
        let input = evaluation(ExitClass::Ok, ExitClass::Ok, Some(0), Some(1), &[1], &[2]);
        let signature = compute_signature(
            input.exit_a,
            input.exit_b,
            input.rv_a,
            input.rv_b,
            input.map_a,
            input.map_b,
        );
        let mut known = HashSet::new();
        known.insert(signature);
        let mut evaluator = CandidateEvaluator::default();

        assert!(!evaluator.evaluate(input, &known, &[0], &[0]));
        assert_eq!(evaluator.map_scans(), 0);
    }

    #[test]
    fn accepted_candidate_attaches_signature_and_elapsed_metadata() {
        use crate::{
            feedbacks::{
                objectives::ExitInfo,
                signature::{DifferentialSignatureMetadata, DiscoveryElapsedMetadata},
            },
            observers::ReturnValueObserver,
        };
        use libafl::{
            corpus::{InMemoryCorpus, Testcase},
            events::NopEventManager,
            feedbacks::{ConstFeedback, Feedback},
            inputs::BytesInput,
            observers::{HitcountsMapObserver, MapObserver, Observer, StdMapObserver},
            state::StdState,
            HasMetadata,
        };
        use libafl_bolts::{
            rands::StdRand,
            shmem::{ShMemProvider, StdShMemProvider},
            tuples::tuple_list,
        };

        let mut map_a = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "first-edges",
            vec![0u8; 8],
        ));
        let map_b = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "second-edges",
            vec![0u8; 8],
        ));
        map_a.set(3, 1);
        let mut rv_a = ReturnValueObserver::new("first_return");
        let mut rv_b = ReturnValueObserver::new("second_return");
        let mut provider = StdShMemProvider::new().unwrap();
        let mut rv_shmem_a = provider.new_shmem(4).unwrap();
        let mut rv_shmem_b = provider.new_shmem(4).unwrap();
        rv_shmem_a[..4].copy_from_slice(&0_i32.to_ne_bytes());
        rv_shmem_b[..4].copy_from_slice(&1_i32.to_ne_bytes());
        rv_a.set_shmem(rv_shmem_a);
        rv_b.set_shmem(rv_shmem_b);
        let mut objective = super::DifferentialObjective::new(&map_a, &map_b, &rv_a, &rv_b);
        let mut feedback = ConstFeedback::new(false);
        let mut state = StdState::new(
            StdRand::with_seed(7),
            InMemoryCorpus::<BytesInput>::new(),
            InMemoryCorpus::<BytesInput>::new(),
            &mut feedback,
            &mut objective,
        )
        .unwrap();
        let input = BytesInput::new(vec![1]);
        rv_a.post_exec(&mut state, &input, &libafl::executors::ExitKind::Ok)
            .unwrap();
        rv_b.post_exec(&mut state, &input, &libafl::executors::ExitKind::Ok)
            .unwrap();
        let observers = tuple_list!(map_a, map_b, rv_a, rv_b);
        let mut manager = NopEventManager::new();

        assert!(objective
            .is_interesting(
                &mut state,
                &mut manager,
                &input,
                &observers,
                &libafl::executors::ExitKind::Ok,
            )
            .unwrap());
        let mut testcase = Testcase::new(input);
        objective
            .append_metadata(&mut state, &mut manager, &observers, &mut testcase)
            .unwrap();

        assert!(testcase.has_metadata::<DifferentialSignatureMetadata>());
        assert!(testcase.has_metadata::<DiscoveryElapsedMetadata>());
        let exit = testcase.metadata::<ExitInfo>().unwrap();
        assert_eq!(exit.rv_a, Some(0));
        assert_eq!(exit.rv_b, Some(1));
    }

    #[test]
    fn direct_dual_map_novelty_matches_concatenated_map_novelty() {
        let mut value = 0x51f1_5eed_u64;
        for _ in 0..512 {
            let mut next = || {
                value = value
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (value >> 56) as u8
            };
            let map_a: Vec<u8> = (0..31).map(|_| next() & 7).collect();
            let map_b: Vec<u8> = (0..37).map(|_| next() & 7).collect();
            let history_a: Vec<u8> = (0..31).map(|_| next() & 7).collect();
            let history_b: Vec<u8> = (0..37).map(|_| next() & 7).collect();
            assert_eq!(
                direct_dual_map_novelty(&map_a, &map_b, &history_a, &history_b),
                concatenated_map_novelty(&map_a, &map_b, &history_a, &history_b),
            );
        }
    }
}

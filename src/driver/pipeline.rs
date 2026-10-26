use std::path::PathBuf;
#[cfg(test)]
use std::time::Duration;

use crate::driver::args::Args;
use crate::{
    feedbacks::differential_objective::DifferentialObjective, observers::ReturnValueObserver,
};
use libafl::{
    feedback_or,
    observers::{CanTrack, MapObserver},
};
use libafl_bolts::{AsIter, Named, ToSlice};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClientIdentity {
    pub(crate) stable_index: u32,
    pub(crate) core_id: Option<usize>,
    pub(crate) rng_seed: u64,
    pub(crate) output_dir: PathBuf,
}

impl ClientIdentity {
    pub(crate) fn single_client(args: &Args) -> Self {
        Self {
            stable_index: 0,
            core_id: None,
            rng_seed: 0,
            output_dir: args.output_dir.clone(),
        }
    }

    pub(crate) fn fast_paths(&self) -> FastClientPaths {
        let client = self
            .output_dir
            .join(format!("client-{:03}", self.stable_index));
        FastClientPaths {
            queue: client.join("queue"),
            solutions: client.join("solutions"),
            stats: client.join("fuzzer_stats"),
            plot: client.join("plot_data"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FastClientPaths {
    pub(crate) queue: PathBuf,
    pub(crate) solutions: PathBuf,
    pub(crate) stats: PathBuf,
    pub(crate) plot: PathBuf,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct StatsCadence {
    interval: Duration,
    last_write: Option<Duration>,
}

#[cfg(test)]
impl StatsCadence {
    pub(crate) const fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_write: None,
        }
    }

    pub(crate) fn should_write(&mut self, elapsed: Duration) -> bool {
        if self
            .last_write
            .is_none_or(|last| elapsed.saturating_sub(last) >= self.interval)
        {
            self.last_write = Some(elapsed);
            true
        } else {
            false
        }
    }
}

pub(crate) const fn fast_client_requested(args: &Args) -> bool {
    matches!(args.pipeline, crate::driver::args::PipelineChoice::Fast)
}

#[cfg(test)]
mod fast_pipeline_tests {
    use super::ClientIdentity;
    use std::path::PathBuf;

    #[test]
    fn fast_client_uses_collision_free_queue_and_solution_paths() {
        let first = ClientIdentity {
            stable_index: 0,
            core_id: Some(3),
            rng_seed: 7,
            output_dir: PathBuf::from("out"),
        };
        let second = ClientIdentity {
            stable_index: 1,
            core_id: Some(4),
            rng_seed: 8,
            output_dir: PathBuf::from("out"),
        };
        let first_paths = first.fast_paths();
        let second_paths = second.fast_paths();
        assert_eq!(first_paths.queue, PathBuf::from("out/client-000/queue"));
        assert_eq!(
            first_paths.solutions,
            PathBuf::from("out/client-000/solutions")
        );
        assert_ne!(first_paths.queue, second_paths.queue);
        assert_ne!(first_paths.solutions, second_paths.solutions);
    }

    #[test]
    fn stats_stage_writes_only_at_the_configured_interval() {
        let mut cadence = super::StatsCadence::new(std::time::Duration::from_secs(15));
        assert!(cadence.should_write(std::time::Duration::ZERO));
        assert!(!cadence.should_write(std::time::Duration::from_secs(14)));
        assert!(cadence.should_write(std::time::Duration::from_secs(15)));
        assert!(!cadence.should_write(std::time::Duration::from_secs(29)));
        assert!(cadence.should_write(std::time::Duration::from_secs(30)));
    }

    #[test]
    fn a_novelty_adds_one_map_indexes_metadata() {
        let result = super::evaluate_fast_feedback_for_test(&[1, 0], &[0, 0]);
        assert!(result.interesting);
        assert_eq!(result.map_indexes_metadata, 1);
        assert_eq!(result.tracked_indexes, 1);
    }

    #[test]
    fn b_only_novelty_enters_corpus_without_second_indexes_metadata() {
        let result = super::evaluate_fast_feedback_for_test(&[0, 0], &[0, 1]);
        assert!(result.interesting);
        assert_eq!(result.map_indexes_metadata, 1);
        assert_eq!(result.tracked_indexes, 0);
    }
}

pub(crate) fn build_direct_objective<A, AO, B, BO>(
    map_a: &A,
    map_b: &B,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
) -> DifferentialObjective<A, AO, B, BO>
where
    A: Named + AsRef<AO>,
    B: Named + AsRef<BO>,
{
    crate::feedbacks::factory::build_fast_objective(map_a, map_b, rv_a, rv_b)
}

#[cfg(test)]
pub(crate) struct FastFeedbackTestResult {
    pub(crate) interesting: bool,
    pub(crate) map_indexes_metadata: usize,
    pub(crate) tracked_indexes: usize,
}

#[cfg(test)]
pub(crate) fn evaluate_fast_feedback_for_test(
    map_a: &[u8],
    map_b: &[u8],
) -> FastFeedbackTestResult {
    use libafl::{
        common::HasMetadata,
        corpus::{InMemoryCorpus, Testcase},
        events::NopEventManager,
        executors::ExitKind,
        feedbacks::{Feedback, MapIndexesMetadata},
        inputs::BytesInput,
        observers::{CanTrack, HitcountsMapObserver, StdMapObserver, TimeObserver},
        state::StdState,
    };
    use libafl_bolts::{rands::StdRand, tuples::tuple_list};

    let observer_a =
        HitcountsMapObserver::new(StdMapObserver::differential_owned("fast-a", map_a.to_vec()))
            .track_indices();
    let observer_b =
        HitcountsMapObserver::new(StdMapObserver::differential_owned("fast-b", map_b.to_vec()));
    let time = TimeObserver::new("time");
    let mut feedback = build_fast_feedback(&observer_a, &observer_b, &time);
    let mut objective = ();
    let mut state = StdState::new(
        StdRand::with_seed(1),
        InMemoryCorpus::<BytesInput>::new(),
        InMemoryCorpus::new(),
        &mut feedback,
        &mut objective,
    )
    .unwrap();
    let observers = tuple_list!(observer_a, observer_b, time);
    let input = BytesInput::new(vec![0]);
    let mut manager = NopEventManager::new();
    let interesting = feedback
        .is_interesting(&mut state, &mut manager, &input, &observers, &ExitKind::Ok)
        .unwrap();
    let mut testcase = Testcase::new(input);
    if interesting {
        feedback
            .append_metadata(&mut state, &mut manager, &observers, &mut testcase)
            .unwrap();
    }
    let indexes = testcase.metadata_map().get::<MapIndexesMetadata>();
    FastFeedbackTestResult {
        interesting,
        map_indexes_metadata: usize::from(indexes.is_some()),
        tracked_indexes: indexes.map_or(0, |metadata| metadata.list.len()),
    }
}

pub(crate) fn build_fast_feedback<A, AO, B, BO>(
    map_a: &A,
    map_b: &B,
    time: &libafl::observers::TimeObserver,
) -> libafl::feedbacks::EagerOrFeedback<
    libafl::feedbacks::MaxMapFeedback<A, AO>,
    libafl::feedbacks::EagerOrFeedback<
        libafl::feedbacks::MaxMapFeedback<B, BO>,
        libafl::feedbacks::TimeFeedback,
    >,
>
where
    A: CanTrack + AsRef<AO> + Named,
    AO: MapObserver<Entry = u8> + for<'a> ToSlice<'a, Entry = u8> + for<'a> AsIter<'a, Item = u8>,
    B: CanTrack + AsRef<BO> + Named,
    BO: MapObserver<Entry = u8> + for<'a> ToSlice<'a, Entry = u8> + for<'a> AsIter<'a, Item = u8>,
{
    feedback_or!(
        libafl::feedbacks::MaxMapFeedback::with_name("fast-a", map_a),
        libafl::feedbacks::MaxMapFeedback::with_name("fast-b", map_b),
        libafl::feedbacks::TimeFeedback::new(time),
    )
}

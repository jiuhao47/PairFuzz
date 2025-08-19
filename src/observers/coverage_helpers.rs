use crate::observers::coverage_combiner::ComboCoverageBuilder;
use libafl::observers::{HitcountsMapObserver, StdMapObserver};
use std::alloc::{alloc_zeroed, Layout};

/// Construct non-multimap observers and optionally a ComboCoverageBuilder when combo is requested.
/// Returns (first_map_observer, second_map_observer, coverage_map_observer, objective_map_observer, Option<ComboCoverageBuilder>)
pub fn make_map_observers(
    num_edges: usize,
    cov_combo: bool,
) -> (
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    Option<ComboCoverageBuilder>,
) {
    // allocate single EDGES backing of size 2*num_edges
    unsafe {
        let layout = Layout::from_size_align(num_edges * 2, 64).unwrap();
        // leak the allocation into a static slice used by StdMapObserver (mirrors main.rs semantics)
        let edges_ptr = alloc_zeroed(layout);
        let _edges_slice = core::slice::from_raw_parts_mut(edges_ptr, num_edges * 2);

        // create observers by pointing into the two halves
        let first = StdMapObserver::differential_from_mut_ptr("first-edges", edges_ptr, num_edges);
        let second = StdMapObserver::differential_from_mut_ptr(
            "second-edges",
            edges_ptr.add(num_edges),
            num_edges,
        );

        let first_obs = HitcountsMapObserver::new(first);
        let second_obs = HitcountsMapObserver::new(second);

        // coverage_map_observer: either a combined backing (when combo requested) or the single backing
        // No combo: coverage uses the same single backing as objective (the full single backing at edges_ptr)
        let coverage = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "combined-edges",
            edges_ptr,
            num_edges * 2,
        ));
        // objective observer uses the full backing (2 * num_edges) so it matches the
        // coverage backing used in single mode. This ensures objective sees the same
        // map layout regardless of cov_mode.
        let objective = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "objective-edges",
            edges_ptr,
            num_edges * 2,
        ));

        // If combo requested, allocate ComboCoverageBuilder and create a combined observer from it.
        // coverage will point to the builder's combined backing; objective stays single.
        if cov_combo {
            let builder = ComboCoverageBuilder::new("combined-edges", num_edges);
            let coverage_obs = builder.combined_map_observer();
            return (
                first_obs,
                second_obs,
                coverage_obs,
                objective,
                Some(builder),
            );
        }

        (first_obs, second_obs, coverage, objective, None)
    }
}

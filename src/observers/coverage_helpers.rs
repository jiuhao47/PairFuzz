use libafl::observers::{HitcountsMapObserver, StdMapObserver};
use std::alloc::{alloc_zeroed, Layout};

/// Construct non-multimap observers and optionally a ComboCoverageBuilder when combo is requested.
pub fn make_map_observers(
    num_edges: usize,
) -> (
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>, // first program half
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>, // second program half
    HitcountsMapObserver<StdMapObserver<'static, u8, true>>, // objective observer over full backing
) {
    // allocate a single EDGES backing (2 * num_edges) and leak it as static memory
    unsafe {
        let layout = Layout::from_size_align(num_edges * 2, 64).unwrap();
        let edges_ptr = alloc_zeroed(layout);
        // create observers that point into the two halves of the backing
        let first_obs = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "first-edges",
            edges_ptr,
            num_edges,
        ));
        let second_obs = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "second-edges",
            edges_ptr.add(num_edges),
            num_edges,
        ));

        // objective observer uses the full 2*num_edges backing
        let objective = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "objective-edges",
            edges_ptr,
            num_edges * 2,
        ));

        (first_obs, second_obs, objective)
    }
}

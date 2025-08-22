use crate::observers::coverage_combiner::ComboCoverageBuilder;
use crate::observers::InvertedCopyUpdater;
use libafl::observers::{HitcountsMapObserver, StdMapObserver};
use std::alloc::{alloc_zeroed, Layout};

/// Named container for the observers and helpers returned by `make_map_observers`.
pub struct MapObservers {
    pub map_observer_a: StdMapObserver<'static, u8, true>,
    pub map_observer_b: StdMapObserver<'static, u8, true>,
    pub coverage_observer_a: HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    pub coverage_observer_b: HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    /// duplicate/alias observer when we need two observers pointing to the same combined backing
    pub coverage_combined_dup: Option<HitcountsMapObserver<StdMapObserver<'static, u8, true>>>,
    pub coverage_combined: Option<HitcountsMapObserver<StdMapObserver<'static, u8, true>>>,
    /// In non-combo mode these hold the per-run combined backing variants
    pub coverage_single: Option<HitcountsMapObserver<StdMapObserver<'static, u8, true>>>,
    pub coverage_single_dup: Option<HitcountsMapObserver<StdMapObserver<'static, u8, true>>>,
    pub objective_observer: HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
    pub combo_builder: Option<ComboCoverageBuilder>,
    pub inverted_updater: InvertedCopyUpdater,
}

/// Construct non-multimap observers and optionally a ComboCoverageBuilder when combo is requested.
pub fn make_map_observers(num_edges: usize, cov_combo: bool) -> MapObservers {
    // allocate single EDGES backing of size 2*num_edges
    unsafe {
        let layout = Layout::from_size_align(num_edges * 2, 64).unwrap();
        // leak the allocation into a static slice used by StdMapObserver (mirrors main.rs semantics)
        let edges_ptr = alloc_zeroed(layout);
        let _edges_slice = core::slice::from_raw_parts_mut(edges_ptr, num_edges * 2);

        // create observers by pointing into the two halves
        let first_obs =
            StdMapObserver::differential_from_mut_ptr("first-edges", edges_ptr, num_edges);
        let second_obs = StdMapObserver::differential_from_mut_ptr(
            "second-edges",
            edges_ptr.add(num_edges),
            num_edges,
        );

        // let first_obs = HitcountsMapObserver::new(first);
        // let second_obs = HitcountsMapObserver::new(second);

        // create coverage half observers that each point to one program's half of the backing.
        // These allow constructing feedbacks that treat the two halves differently (e.g. Max vs Min).
        let coverage_first = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "coverage-first",
            edges_ptr,
            num_edges,
        ));

        // Instead of creating an inverted observer wrapper (which complicates AsSlice/AsIter
        // trait requirements), create an owned StdMapObserver for the second half and keep
        // it in sync by copying the program backing into it with inverted bytes after the
        // second execution. This owned observer satisfies the required traits for feedbacks.
        let mut coverage_second_base_owned =
            StdMapObserver::differential_owned("coverage-second-owned", vec![0u8; num_edges]);
        // get a raw pointer to the owned backing so we can create an updater that copies into it
        let coverage_second_dst_ptr = coverage_second_base_owned.map_mut().as_mut_ptr();
        let coverage_second = HitcountsMapObserver::new(coverage_second_base_owned);

        // Create the observers for combined backing (full 2*num_edges) lazily depending on cov_combo.
        // objective observer always uses the full backing so it is constructed eagerly.
        let objective = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "objective-edges",
            edges_ptr,
            num_edges * 2,
        ));

        // We'll populate either the combo-backed observers or the single-backed observers below.

        // If combo requested, allocate ComboCoverageBuilder and create a combined observer from it.
        // coverage will point to the builder's combined backing; objective stays single.
        // Create the updater that will copy inverted bytes from the program backing
        // (edges_ptr.add(num_edges)) into the owned coverage_second backing.
        let updater =
            InvertedCopyUpdater::new(edges_ptr.add(num_edges), coverage_second_dst_ptr, num_edges);

        if cov_combo {
            let debug_msg = format!("Creating combo coverage observers");
            let _ = crate::utils::logging::debug_log("coverage_helpers", &debug_msg);
            let builder = ComboCoverageBuilder::new("combined-edges", num_edges);
            let coverage_obs = builder.combined_map_observer();
            let builder_dup = ComboCoverageBuilder::new("combined-edges-dup", num_edges);
            let coverage_obs_dup = builder_dup.combined_map_observer();
            return MapObservers {
                map_observer_a: first_obs,
                map_observer_b: second_obs,
                coverage_observer_a: coverage_first,
                coverage_observer_b: coverage_second,
                coverage_combined_dup: Some(coverage_obs_dup),
                coverage_combined: Some(coverage_obs),
                coverage_single: None,
                coverage_single_dup: None,
                objective_observer: objective,
                combo_builder: Some(builder),
                inverted_updater: updater,
            };
        }

        // Non-combo path: create a full backing observer over the single allocated edges_ptr (2*num_edges)
        let coverage_full = HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
            "combined-edges",
            edges_ptr,
            num_edges * 2,
        ));
        let coverage_full_dup =
            HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
                "combined-edges-dup",
                edges_ptr,
                num_edges * 2,
            ));

        MapObservers {
            map_observer_a: first_obs,
            map_observer_b: second_obs,
            coverage_observer_a: coverage_first,
            coverage_observer_b: coverage_second,
            coverage_combined_dup: None,
            coverage_combined: None,
            coverage_single: Some(coverage_full),
            coverage_single_dup: Some(coverage_full_dup),
            objective_observer: objective,
            combo_builder: None,
            inverted_updater: updater,
        }
    }
}

impl MapObservers {
    /// Consume the MapObservers and produce the concrete parts needed by the
    /// driver to assemble executors and the differential executor. This
    /// centralizes the selection logic between combo vs non-combo coverage
    /// backings so callers don't need to handle the Option fields.
    pub fn into_diff_parts(
        self,
    ) -> (
        StdMapObserver<'static, u8, true>,
        StdMapObserver<'static, u8, true>,
        HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
        HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
        HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
        HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
        HitcountsMapObserver<StdMapObserver<'static, u8, true>>,
        Option<ComboCoverageBuilder>,
        InvertedCopyUpdater,
    ) {
        let MapObservers {
            map_observer_a,
            map_observer_b,
            coverage_observer_a,
            coverage_observer_b,
            coverage_combined,
            coverage_combined_dup,
            coverage_single,
            coverage_single_dup,
            objective_observer,
            combo_builder,
            inverted_updater,
        } = self;

        // Choose the concrete backing pair (combined or single) and return
        // them as owned observers. Panic if neither backing is present.
        let (backing0, backing1) = match (
            coverage_combined,
            coverage_combined_dup,
            coverage_single,
            coverage_single_dup,
        ) {
            (Some(c), Some(cd), _, _) => (c, cd),
            (_, _, Some(s), Some(sd)) => (s, sd),
            _ => panic!("no coverage backing produced by make_map_observers"),
        };

        (
            map_observer_a,
            map_observer_b,
            coverage_observer_a,
            coverage_observer_b,
            backing0,
            backing1,
            objective_observer,
            combo_builder,
            inverted_updater,
        )
    }
}

// InvertedCopyUpdater moved to `inverted_map.rs` to keep coverage helpers focused.

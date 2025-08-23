use std::borrow::Cow;
use std::env;
use std::time::Instant;

use libafl::{
    observers::{DifferentialObserver, HitcountsMapObserver, Observer, StdMapObserver},
    Error,
};
use libafl_bolts::Named;

use crate::utils::logging::debug_log;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A differential helper that builds a combined coverage buffer from the two per-executor
/// coverage maps. This is useful for "combo" coverage where we want to inspect the cross
/// product of edges hit by run A and run B.
#[derive(Debug)]
pub struct ComboCoverageBuilder {
    name: Cow<'static, str>,
    num_edges: usize,
    combined_ptr: *mut u8,
    combined_len: usize,
}

impl ComboCoverageBuilder {
    /// Create a new combo builder that allocates a combined buffer of size num_edges*num_edges.
    /// The buffer is leaked to get a stable pointer usable by observers created later.
    pub fn new(name: &'static str, num_edges: usize) -> Self {
        // debug: num_edges
        let combined_len = num_edges.checked_mul(4).unwrap_or(0);
        // allocate boxed slice and leak it to obtain a stable 'static pointer
        let boxed = vec![0u8; combined_len].into_boxed_slice();
        let combined_ptr = Box::into_raw(boxed) as *mut u8;
        // creation logged only when profiling enabled elsewhere
        Self {
            name: Cow::from(name),
            num_edges,
            combined_ptr,
            combined_len,
        }
    }

    /// Return a HitcountsMapObserver over the combined buffer. Caller is responsible to keep
    /// this alive for the duration of the fuzzing run.
    ///
    /// # Safety
    /// The returned observer will reference the leaked combined buffer pointer.
    pub unsafe fn combined_map_observer(
        &self,
    ) -> HitcountsMapObserver<StdMapObserver<'static, u8, true>> {
        let ptr = self.combined_ptr;
        let len = self.combined_len;
        // create a differential StdMapObserver (DIFFERENTIAL = true)
        let std_obs: StdMapObserver<'static, u8, true> =
            StdMapObserver::differential_from_mut_ptr(self.name.clone(), ptr, len);
        // observer creation is quiet by default
        HitcountsMapObserver::new(std_obs)
    }
}

// Custom serialization: only serialize the name and num_edges. On deserialization we
// re-create the builder and allocate a new combined buffer of the requested size.
impl Serialize for ComboCoverageBuilder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.name.as_ref(), &self.num_edges).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ComboCoverageBuilder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (name, num_edges): (String, usize) = <(String, usize)>::deserialize(deserializer)?;
        // leak the name to obtain a 'static str for the builder
        let boxed: Box<str> = name.into_boxed_str();
        let static_str: &'static str = Box::leak(boxed);
        Ok(ComboCoverageBuilder::new(static_str, num_edges))
    }
}

impl Named for ComboCoverageBuilder {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<_I, _S> Observer<_I, _S> for ComboCoverageBuilder {
    fn pre_exec(&mut self, _state: &mut _S, _input: &_I) -> Result<(), Error> {
        // let debug_msg = format!(
        //     "ComboCoverageBuilder: pre_exec called with {} edges",
        //     self.num_edges
        // );
        // let _ = debug_log("coverage_combiner", &debug_msg);
        Ok(())
    }

    fn post_exec(
        &mut self,
        _state: &mut _S,
        _input: &_I,
        _exit_kind: &libafl::executors::ExitKind,
    ) -> Result<(), Error> {
        // let debug_msg = format!(
        //     "ComboCoverageBuilder: post_exec called with {} edges",
        //     self.num_edges
        // );
        // let _ = debug_log("coverage_combiner", &debug_msg);
        Ok(())
    }
}

impl<OTA, OTB, I, S> DifferentialObserver<OTA, OTB, I, S> for ComboCoverageBuilder {
    /// After the second run finished, build the combined buffer by computing the outer-product
    /// of the two per-executor edge hit arrays. This reads the global edges slice exposed by
    /// the runtime (e.g. `crate::targets::EDGES` / `libafl_targets` backing) and writes into
    /// the combined buffer.
    fn post_observe_second(&mut self, _observers: &mut OTB) -> Result<(), Error> {
        // Safety: we access raw pointers that are managed by the runtime; keep operations minimal
        unsafe {
            // optional profiling enabled via DIFFRAY_PROFILE=1
            let profiling = env::var("DIFFRAY_PROFILE")
                .map(|v| v == "1")
                .unwrap_or(false);
            let prof_t0 = if profiling {
                Some(Instant::now())
            } else {
                None
            };

            // get pointer to the global edges map and its length
            let num = self.num_edges;
            if num == 0 || self.combined_len == 0 {
                return Ok(());
            }

            let edges_ptr = libafl_targets::edges_map_mut_ptr();

            let first = core::slice::from_raw_parts(edges_ptr, num);
            let second = core::slice::from_raw_parts(edges_ptr.add(num), num);
            let combined = core::slice::from_raw_parts_mut(self.combined_ptr, self.combined_len);

            // XXX: switched to cumulative mode: do not clear the combined buffer each run.
            // The combined bitmap now accumulates writes across runs to avoid the O(n^2)
            // or O(num_edges^2) per-iteration clear cost. This changes semantics: once a
            // slot is set it remains set (or its counter increases) until the process ends.
            // If you prefer per-run isolation, consider the epoch/generation approach instead.

            // Heuristic hashing approach to avoid O(n^2) outer product.
            // 1) Treat each map as a set of seen-edge indices (ignore counters >0).
            // 2) Compute unordered-set hashes h1/h2 by mixing per-index contribution.
            // 3) Combine h1/h2 into a single 64-bit value and derive a small number of
            //    sample indices into the combined buffer by re-hashing with different salts.
            // This yields O(n) runtime and constant memory, at the cost of collisions / lower
            // resolution compared to the full outer-product bitmap.

            #[inline]
            fn splitmix64(mut x: u64) -> u64 {
                x = x.wrapping_add(0x9e3779b97f4a7c15);
                x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
                x ^ (x >> 31)
            }

            // compute an order-independent hash for each set by xoring mixed idx values
            let mut h1: u64 = 0;
            let mut h2: u64 = 0;
            let mut k1: usize = 0;
            let mut k2: usize = 0;
            for i in 0..num {
                if first.get(i).copied().unwrap_or(0) != 0 {
                    h1 = h1.wrapping_add(splitmix64(i as u64 + 0x9e3779b97f4a7c15));
                    k1 += 1;
                }
                if second.get(i).copied().unwrap_or(0) != 0 {
                    h2 = h2.wrapping_add(splitmix64(i as u64 ^ 0x243f6a8885a308d3));
                    k2 += 1;
                }
            }

            if profiling {
                if let Some(t0) = prof_t0 {
                    let dur = Instant::now().duration_since(t0).as_secs_f64();
                    let msg = format!(
                        "combo_perf: num={} k1={} k2={} hash_elapsed={:.8}s",
                        num, k1, k2, dur
                    );
                    let _ = debug_log("coverage_combiner", &msg);
                }
            }

            // combine the two set-hashes into a single value and index into the bitmap
            // Use the existing combined buffer size (num_edges^2) as BitmapSize so we keep
            // the original address space.
            let combined_hash = splitmix64(h1.wrapping_mul(0x9e3779b97f4a7c15) ^ h2);
            let m = self.combined_len as u64;
            if m != 0 {
                let idx = (combined_hash % m) as usize;

                // saturating increment at the single derived index
                let write_t0 = if profiling {
                    Some(Instant::now())
                } else {
                    None
                };
                // let new = combined.get_unchecked_mut(idx).saturating_add(1);
                *combined.get_unchecked_mut(idx) = 1;
                if profiling {
                    if let Some(w0) = write_t0 {
                        let dur_w = Instant::now().duration_since(w0).as_secs_f64();
                        let msg = format!(
                            "combo_perf: write_elapsed={:.8}s idx={} combined_len={}",
                            dur_w, idx, self.combined_len
                        );
                        let _ = debug_log("coverage_combiner", &msg);
                    }
                }

                if profiling {
                    let debug_msg = format!(
                        "ComboCoverageBuilder: post_observe_second combined hash {:x} -> index {}",
                        combined_hash, idx
                    );
                    let _ = debug_log("coverage_combiner", &debug_msg);
                }
            }

            if profiling {
                if let Some(t0) = prof_t0 {
                    let tot = Instant::now().duration_since(t0).as_secs_f64();
                    let msg = format!(
                        "combo_perf: total_elapsed={:.8}s combined_len={} num_edges={}",
                        tot, self.combined_len, self.num_edges
                    );
                    let _ = debug_log("coverage_combiner", &msg);
                }
            }
        }

        Ok(())
    }
}

/// Create the non-multimap observers: allocate a single EDGES backing of size 2*num_edges,
/// and return (first_map_observer, second_map_observer, map_swapper, combined_map_observer)
/// All returned observers use leaked buffers and thus are 'static.
// (helper removed — keep ComboCoverageBuilder for future combo buffer allocation)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_combined_small() {
        let num_edges = 8;
        let b = ComboCoverageBuilder::new("combined-test", num_edges);
        unsafe {
            let obs = b.combined_map_observer();
            // ensure combined observer created without panics
            drop(obs);
        }
    }
}

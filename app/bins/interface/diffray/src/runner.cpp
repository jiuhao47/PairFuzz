#include "diffray/runner.h"

#include <cstdlib>

namespace diffray {

Runner::Runner(const Args &args) : args_(args) {
  lib_a_.configure(args_.prog_a, args_.func_a);
  lib_b_.configure(args_.prog_b, args_.func_b);

  strategy_ = make_strategy(args_.mode);
  if (!strategy_) {
    std::abort();
  }
}

void Runner::ensure_loaded() {
  lib_a_.ensure_loaded();
  lib_b_.ensure_loaded();

  // Ensure snapshots are sized once.
  snap_a_.resize(map_.size());
  snap_b_.resize(map_.size());
}

int Runner::run_one(const std::uint8_t *data, std::size_t size) {
  ensure_loaded();

  // Separate coverage for A
  map_.clear();
  const int rv_a = lib_a_.call(data, size);
  map_.snapshot_to(snap_a_);

  // Separate coverage for B
  map_.clear();
  const int rv_b = lib_b_.call(data, size);
  map_.snapshot_to(snap_b_);

  // If the oracle triggers, terminate as a crash.
  // Current oracle: zero vs non-zero return value mismatch.
  if ((rv_a == 0 && rv_b != 0) || (rv_a != 0 && rv_b == 0)) {
    __builtin_trap();
  }

  // Derive the final map for AFL++ feedback.
  strategy_->derive(snap_a_, snap_b_, rv_a, rv_b, map_);
  return 0;
}

} // namespace diffray

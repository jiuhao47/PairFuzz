#pragma once

#include <cstddef>
#include <cstdint>

#include "diffray/afl_map.h"
#include "diffray/args.h"
#include "diffray/dylib_loader.h"
#include "diffray/strategy.h"

namespace diffray {

class Runner {
public:
  explicit Runner(const Args &args);

  Runner(const Runner &) = delete;
  Runner &operator=(const Runner &) = delete;

  // Execute A then B on the same input and derive the output map.
  // Returns 0 on normal execution. If a differential condition is detected,
  // this function will terminate the process (crash/exit) as configured.
  int run_one(const std::uint8_t *data, std::size_t size);

private:
  Args args_;
  DylibLoader lib_a_;
  DylibLoader lib_b_;
  AflMap map_;
  MapSnapshot snap_a_;
  MapSnapshot snap_b_;
  Strategy *strategy_ = nullptr;

  void ensure_loaded();
};

} // namespace diffray

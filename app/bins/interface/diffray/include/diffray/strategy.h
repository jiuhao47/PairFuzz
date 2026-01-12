#pragma once

#include "diffray/afl_map.h"
#include "diffray/mode.h"

namespace diffray {

class Strategy {
public:
  virtual ~Strategy() = default;

  // Writes the derived feedback map into `out_map`.
  virtual void derive(const MapSnapshot &map_a, const MapSnapshot &map_b,
                      int rv_a, int rv_b, AflMap &out_map) = 0;
};

Strategy *make_strategy(Mode mode);

} // namespace diffray

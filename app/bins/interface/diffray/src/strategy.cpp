#include "diffray/strategy.h"

#include "diffray/hash.h"
#include "diffray/mode.h"

#include <cstdlib>
#include <cstring>

namespace diffray {

class SingleCovStrategy final : public Strategy {
public:
  void derive(const MapSnapshot &map_a, const MapSnapshot &map_b, int /*rv_a*/,
              int /*rv_b*/, AflMap &out_map) override {
    out_map.clear();
    const std::size_t n = out_map.size();
    if (!out_map.ptr() || n == 0)
      return;

    const std::uint8_t *a = map_a.bytes.data();
    const std::uint8_t *b = map_b.bytes.data();
    std::uint8_t *out = out_map.ptr();

    for (std::size_t i = 0; i < n; ++i) {
      out[i] = static_cast<std::uint8_t>(a[i] | b[i]);
    }
    out_map.finalize();
  }
};

class ComboCovStrategy final : public Strategy {
public:
  void derive(const MapSnapshot &map_a, const MapSnapshot &map_b, int /*rv_a*/,
              int /*rv_b*/, AflMap &out_map) override {
    out_map.clear_for_output();
    // Approximate the (indicesA, indicesB) combo by (hashA, hashB).
    out_map.touch_pair(map_a.hash64, map_b.hash64);
    out_map.finalize();
  }
};

class SinglePlStrategy final : public Strategy {
public:
  void derive(const MapSnapshot &map_a, const MapSnapshot &map_b, int /*rv_a*/,
              int /*rv_b*/, AflMap &out_map) override {
    out_map.clear_for_output();
    const std::uint32_t ba = bucketize_count(map_a.nonzero_count);
    const std::uint32_t bb = bucketize_count(map_b.nonzero_count);
    out_map.touch(0xA11CE000u ^ ba);
    out_map.touch(0xB11CE000u ^ bb);
    out_map.finalize();
  }
};

class ComboPlStrategy final : public Strategy {
public:
  void derive(const MapSnapshot &map_a, const MapSnapshot &map_b, int /*rv_a*/,
              int /*rv_b*/, AflMap &out_map) override {
    out_map.clear_for_output();
    const std::uint64_t ba = bucketize_count(map_a.nonzero_count);
    const std::uint64_t bb = bucketize_count(map_b.nonzero_count);
    out_map.touch_pair(0xC01A0000ULL ^ ba, 0xC01B0000ULL ^ bb);
    out_map.finalize();
  }
};

class SingleRvStrategy final : public Strategy {
public:
  void derive(const MapSnapshot & /*map_a*/, const MapSnapshot & /*map_b*/,
              int rv_a, int rv_b, AflMap &out_map) override {
    out_map.clear_for_output();
    out_map.touch(static_cast<std::uint32_t>(0x51A00000u) ^
                  static_cast<std::uint32_t>(rv_a));
    out_map.touch(static_cast<std::uint32_t>(0x51B00000u) ^
                  static_cast<std::uint32_t>(rv_b));
    out_map.finalize();
  }
};

class ComboRvStrategy final : public Strategy {
public:
  void derive(const MapSnapshot & /*map_a*/, const MapSnapshot & /*map_b*/,
              int rv_a, int rv_b, AflMap &out_map) override {
    out_map.clear_for_output();
    out_map.touch_pair(
        static_cast<std::uint64_t>(static_cast<std::uint32_t>(rv_a)),
        static_cast<std::uint64_t>(static_cast<std::uint32_t>(rv_b)));
    out_map.finalize();
  }
};

class SingleRvCovStrategy final : public Strategy {
public:
  void derive(const MapSnapshot &map_a, const MapSnapshot &map_b, int rv_a,
              int rv_b, AflMap &out_map) override {
    out_map.clear_for_output();

    const bool a_zero = (rv_a == 0);
    const bool b_zero = (rv_b == 0);
    const bool rv_state_diff = (a_zero != b_zero);

    if (!rv_state_diff) {
      // No differential rv-state, keep the map essentially empty.
      out_map.finalize();
      return;
    }

    // Approximate (rv, path) by (rv, hash(map)).
    out_map.touch_pair(0xA0000000ULL ^ static_cast<std::uint64_t>(
                                           static_cast<std::uint32_t>(rv_a)),
                       map_a.hash64);
    out_map.touch_pair(0xB0000000ULL ^ static_cast<std::uint64_t>(
                                           static_cast<std::uint32_t>(rv_b)),
                       map_b.hash64);
    out_map.finalize();
  }
};

Strategy *make_strategy(Mode mode) {
  switch (mode) {
  case Mode::SingleCov:
    return new SingleCovStrategy();
  case Mode::ComboCov:
    return new ComboCovStrategy();
  case Mode::SinglePl:
    return new SinglePlStrategy();
  case Mode::ComboPl:
    return new ComboPlStrategy();
  case Mode::SingleRv:
    return new SingleRvStrategy();
  case Mode::ComboRv:
    return new ComboRvStrategy();
  case Mode::SingleRvCov:
    return new SingleRvCovStrategy();
  }
  return nullptr;
}

} // namespace diffray

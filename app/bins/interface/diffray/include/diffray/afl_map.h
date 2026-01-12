#pragma once

#include <cstddef>
#include <cstdint>
#include <vector>

namespace diffray {

struct MapSnapshot {
  std::vector<std::uint8_t> bytes;
  std::size_t nonzero_count = 0;
  std::uint64_t hash64 = 0;

  void resize(std::size_t n);
};

class AflMap {
public:
  AflMap();

  std::uint8_t *ptr() const { return ptr_; }
  std::size_t size() const { return size_; }

  void clear();
  void snapshot_to(MapSnapshot &out) const;

  // Clear + touch [0] so the map is never totally empty.
  void clear_for_output();
  void touch(std::uint32_t key);
  void touch_pair(std::uint64_t a, std::uint64_t b);
  void finalize();

private:
  std::uint8_t *ptr_ = nullptr;
  std::size_t size_ = 0;
};

} // namespace diffray

#include "diffray/afl_map.h"

#include "diffray/hash.h"

#include <cstring>

namespace diffray {

extern "C" {
__attribute__((weak)) std::uint8_t *__afl_area_ptr;
__attribute__((weak)) std::uint32_t __afl_map_size;
}

static constexpr std::size_t kDefaultMapSize = 65536;

void MapSnapshot::resize(std::size_t n) { bytes.resize(n); }

AflMap::AflMap() {
  // NOTE: __afl_area_ptr is expected to be provided by the AFL++ runtime.
  // We guard the access so the binary can still be invoked outside AFL
  // (weak symbol may resolve to address 0).
  if (&__afl_area_ptr != nullptr) {
    ptr_ = __afl_area_ptr;
  }

  std::size_t sz = kDefaultMapSize;
  if (&__afl_map_size != nullptr && __afl_map_size != 0) {
    sz = static_cast<std::size_t>(__afl_map_size);
  }
  size_ = sz;

  if (!ptr_) {
    // Fallback: allow construction, but operations will be no-ops.
    // This lets the binary run in non-AFL contexts for debugging.
  }
}

void AflMap::clear() {
  if (!ptr_ || size_ == 0)
    return;
  std::memset(ptr_, 0, size_);
}

void AflMap::clear_for_output() {
  clear();
  finalize();
}

void AflMap::snapshot_to(MapSnapshot &out) const {
  const std::size_t n = size_;
  out.resize(n);

  if (!ptr_ || n == 0) {
    out.nonzero_count = 0;
    out.hash64 = 0;
    return;
  }

  std::memcpy(out.bytes.data(), ptr_, n);
  out.nonzero_count = count_nonzero_bytes(out.bytes.data(), n);
  out.hash64 = hash_bytes64(out.bytes.data(), n);
}

void AflMap::touch(std::uint32_t key) {
  if (!ptr_ || size_ == 0)
    return;
  const std::uint32_t idx = static_cast<std::uint32_t>(mix64(key)) %
                            static_cast<std::uint32_t>(size_);
  ptr_[idx] = static_cast<std::uint8_t>(ptr_[idx] + 1);
}

void AflMap::touch_pair(std::uint64_t a, std::uint64_t b) {
  const std::uint64_t h = mix64(a ^ (b + 0x9e3779b97f4a7c15ULL));
  touch(static_cast<std::uint32_t>(h));
  touch(static_cast<std::uint32_t>(h >> 32));
}

void AflMap::finalize() {
  if (!ptr_ || size_ == 0)
    return;
  ptr_[0] = static_cast<std::uint8_t>(ptr_[0] | 1);
}

} // namespace diffray

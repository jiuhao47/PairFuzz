#include "diffray/hash.h"

#include <cstring>

namespace diffray {

std::uint64_t hash_bytes64(const std::uint8_t *data, std::size_t len) {
  // 64-bit word-wise mixing.
  // This is intentionally small and dependency-free.
  constexpr std::uint64_t kMul = 0x9e3779b97f4a7c15ULL;
  std::uint64_t h =
      0x243f6a8885a308d3ULL ^ (static_cast<std::uint64_t>(len) * kMul);

  const std::size_t n8 = len / 8;
  const std::uint8_t *p = data;
  for (std::size_t i = 0; i < n8; ++i) {
    std::uint64_t v;
    std::memcpy(&v, p, sizeof(v));
    p += 8;
    h ^= mix64(v + (static_cast<std::uint64_t>(i) * kMul));
    h *= kMul;
  }

  // tail
  std::uint64_t tail = 0;
  const std::size_t rem = len - (n8 * 8);
  if (rem) {
    std::memcpy(&tail, p, rem);
    h ^=
        mix64(tail ^ (static_cast<std::uint64_t>(rem) * 0x27d4eb2f165667c5ULL));
    h *= kMul;
  }

  return mix64(h);
}

std::size_t count_nonzero_bytes(const std::uint8_t *data, std::size_t len) {
  std::size_t c = 0;
  for (std::size_t i = 0; i < len; ++i) {
    c += (data[i] != 0);
  }
  return c;
}

std::uint32_t bucketize_count(std::size_t count) {
  // Log-ish bucketing.
  // 0, 1, 2-3, 4-7, 8-15, ...
  if (count == 0)
    return 0;
  std::uint32_t b = 1;
  std::size_t v = count;
  while (v > 1) {
    v >>= 1;
    ++b;
  }
  return b;
}

} // namespace diffray

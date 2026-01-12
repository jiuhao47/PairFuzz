#pragma once

#include <cstddef>
#include <cstdint>

namespace diffray {

// Very small, fast-ish mixing helpers (no external deps).
inline std::uint64_t mix64(std::uint64_t x) {
  x ^= x >> 33;
  x *= 0xff51afd7ed558ccdULL;
  x ^= x >> 33;
  x *= 0xc4ceb9fe1a85ec53ULL;
  x ^= x >> 33;
  return x;
}

// Hash a byte buffer. Designed for speed over cryptographic strength.
std::uint64_t hash_bytes64(const std::uint8_t *data, std::size_t len);

// Count non-zero bytes.
std::size_t count_nonzero_bytes(const std::uint8_t *data, std::size_t len);

// Bucketize a count to reduce churn.
std::uint32_t bucketize_count(std::size_t count);

} // namespace diffray

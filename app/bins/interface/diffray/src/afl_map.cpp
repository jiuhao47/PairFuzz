#include "diffray/afl_map.h"

#include "debug.h"

#include "diffray/hash.h"

#include <cstdlib>
#include <cstring>

namespace diffray {

extern "C" {
// IMPORTANT:
// Do NOT define these symbols here. If we define them (even weak), we may
// accidentally shadow the AFL++ runtime's own globals and end up observing
// a null/incorrect map.
extern std::uint8_t *__afl_area_ptr __attribute__((weak));
extern std::uint32_t __afl_map_size __attribute__((weak));
}

static constexpr std::size_t kDefaultMapSize = 65536;

void MapSnapshot::resize(std::size_t n) { bytes.resize(n); }

AflMap::AflMap() {
  // Do not snapshot __afl_area_ptr here.
  // At construction time (before __AFL_INIT()), AFL++ may still point this to
  // a dummy/non-shared map. We'll refresh lazily on first use.
  size_ = kDefaultMapSize;
}

void AflMap::refresh_from_runtime() {
  // NOTE: these are weak externs; when not running under AFL, they may be 0.
  std::uint8_t *runtime_ptr = __afl_area_ptr;
  std::size_t runtime_size = kDefaultMapSize;
  if (__afl_map_size != 0) {
    runtime_size = static_cast<std::size_t>(__afl_map_size);
  }

  const bool changed = (runtime_ptr != ptr_) || (runtime_size != size_);
  ptr_ = runtime_ptr;
  size_ = runtime_size;

  if (!changed) {
    return;
  }

  // Diagnostics: this is the signal we care about.
  // If this fires after __AFL_INIT(), runtime_ptr should typically be SHM-backed
  // and page-aligned.
  const char *shm_id = ::getenv("__AFL_SHM_ID");
  const char *map_size_env = ::getenv("AFL_MAP_SIZE");
  const char *inst_ratio = ::getenv("AFL_INST_RATIO");
  SAYF("[diffray] afl_map(sync): ptr=%p map_size_sym=%u resolved_size=%zu\n",
       static_cast<void *>(ptr_), static_cast<unsigned>(__afl_map_size), size_);
  SAYF("[diffray] afl_map(sync): env __AFL_SHM_ID=%s AFL_MAP_SIZE=%s AFL_INST_RATIO=%s\n",
       shm_id ? shm_id : "(null)", map_size_env ? map_size_env : "(null)",
       inst_ratio ? inst_ratio : "(null)");
  if (ptr_ && size_ >= 8) {
    SAYF("[diffray] afl_map(sync): head bytes = %02x %02x %02x %02x %02x %02x %02x %02x\n",
         ptr_[0], ptr_[1], ptr_[2], ptr_[3], ptr_[4], ptr_[5], ptr_[6], ptr_[7]);
  } else {
    SAYF("[diffray] afl_map(sync): map unavailable (ptr null or size too small)\n");
  }
}

void AflMap::clear() {
  refresh_from_runtime();
  if (!ptr_ || size_ == 0)
    return;
  std::memset(ptr_, 0, size_);
}

void AflMap::clear_for_output() {
  clear();
  finalize();
}

void AflMap::snapshot_to(MapSnapshot &out) const {
  const_cast<AflMap *>(this)->refresh_from_runtime();
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
  refresh_from_runtime();
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
  refresh_from_runtime();
  if (!ptr_ || size_ == 0)
    return;
  ptr_[0] = static_cast<std::uint8_t>(ptr_[0] | 1);
}

} // namespace diffray

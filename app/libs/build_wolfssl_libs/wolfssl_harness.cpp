// Thin wrapper harness for libdeflate core implementation.
#include <stddef.h>
#include <stdint.h>

// Include the core implementation so this harness exports the expected C
// entrypoint used by diffRay.
#include "wolfssl_core.cpp"

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return verify_wolfssl(data, size);
}

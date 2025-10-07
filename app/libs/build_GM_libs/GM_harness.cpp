// Thin harness wrapper for GraphicsMagick core implementation.
#include <stddef.h>
#include <stdint.h>

// Include the core implementation so the harness is a single-file artifact
// that exports the diffRay entrypoint used by the testing harness.
#include "GM_core.cpp"

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return GM_parse(data, size);
}
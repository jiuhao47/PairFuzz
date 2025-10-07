// Wrapper for ImageMagick core implementation.
#include <stddef.h>
#include <stdint.h>

// Include the core implementation so this harness becomes a single-file
// artifact that exports the expected C entrypoint used by diffRay.
#include "IM_core.cpp"

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return imagemagick_parse(data, size);
}

// Thin wrapper harness for libpng core implementation.
#include <stddef.h>
#include <stdint.h>

extern "C" int libpng_parse(const uint8_t *data, size_t size);

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return libpng_parse(data, size);
}
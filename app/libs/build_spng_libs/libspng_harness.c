// Thin wrapper for libspng core implementation.
#include <stddef.h>
#include <stdint.h>

int libspng_parse(const uint8_t *data, size_t size);

extern int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return libspng_parse(data, size);
}
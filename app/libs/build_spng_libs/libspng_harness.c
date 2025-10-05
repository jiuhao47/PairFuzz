// Thin wrapper for libspng core implementation.
#include <stddef.h>
#include <stdint.h>

#include "libspng_core.c"

extern int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return libspng_parse(data, size);
}
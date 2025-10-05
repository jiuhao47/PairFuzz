// Wrapper for cjson core implementation.
#include <stddef.h>
#include <stdint.h>

#include "cjson_core.c"

extern int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return cjson_parse(data, size);
}
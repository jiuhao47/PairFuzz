#include <stdio.h>

// Thin harness wrapper for Expat core implementation.
#include <stddef.h>
#include <stdint.h>

// Include the core implementation so the harness exports the C entrypoint
#include "expat_core.cpp"

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return expat_xmlparse(data, size);
}
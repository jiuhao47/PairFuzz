// Lightweight wrapper for openh264_core's parse implementation.
#include <stddef.h>
#include <stdint.h>

// Declare the core parse implementation (defined in openh264_core.cpp).
extern "C" int openh264_parse(const uint8_t *data, size_t size);

extern "C" int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return openh264_parse(data, size);
}
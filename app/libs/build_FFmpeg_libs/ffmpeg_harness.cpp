// Wrapper declarations for ffmpeg_core's exported parse.
#include <stddef.h>
#include <stdint.h>

#include "ffmpeg_core.cpp"

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return ffmpeg_h264_parse(data, size);
}

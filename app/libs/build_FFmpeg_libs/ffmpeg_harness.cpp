// Wrapper declarations for ffmpeg_core's exported parse.
#include <stddef.h>
#include <stdint.h>

// ffmpeg_h264_parse is implemented in ffmpeg_core.cpp (it handles stderr
// redirection and calls h264_parse_init).
extern "C" int ffmpeg_h264_parse(const uint8_t *data, size_t size);

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return ffmpeg_h264_parse(data, size);
}

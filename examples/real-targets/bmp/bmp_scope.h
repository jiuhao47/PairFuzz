#ifndef PAIRFUZZ_BMP_SCOPE_H
#define PAIRFUZZ_BMP_SCOPE_H

#include <stddef.h>
#include <stdint.h>

enum bmp_scope_result {
  BMP_SCOPE_DECODE = 0,
  BMP_SCOPE_UNSUPPORTED = 1,
  BMP_SCOPE_REJECTED = 2,
  BMP_SCOPE_RESOURCE_LIMIT = 3
};

struct bmp_header {
  uint32_t width;
  uint32_t height;
  uint32_t pixel_offset;
  uint32_t declared_file_size;
  uint32_t dib_size;
  uint32_t compression;
  uint16_t bits_per_pixel;
  uint8_t top_down;
  uint8_t image_count;
};

enum bmp_scope_result bmp_scope_classify(const uint8_t *data, size_t size,
                                         struct bmp_header *out);

#endif

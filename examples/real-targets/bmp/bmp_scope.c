#include "bmp_scope.h"

#include <limits.h>
#include <string.h>

#define BMP_MAX_DIMENSION UINT32_C(4096)
#define BMP_MAX_PIXELS UINT64_C(4194304)

static uint16_t read_le16(const uint8_t *p) {
  return (uint16_t)p[0] | ((uint16_t)p[1] << 8);
}

static uint32_t read_le32(const uint8_t *p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) |
         ((uint32_t)p[3] << 24);
}

static int valid_depth(uint16_t depth) {
  return depth == 1 || depth == 4 || depth == 8 || depth == 16 ||
         depth == 24 || depth == 32;
}

enum bmp_scope_result bmp_scope_classify(const uint8_t *data, size_t size,
                                         struct bmp_header *out) {
  struct bmp_header header;
  uint32_t raw_width;
  uint32_t raw_height;
  uint64_t row_bits;
  uint64_t row_bytes;
  uint64_t required;

  memset(&header, 0, sizeof(header));
  if (out != NULL) {
    memset(out, 0, sizeof(*out));
  }
  if (data == NULL || size < 2) {
    return BMP_SCOPE_REJECTED;
  }
  if (data[0] == 'B' && data[1] == 'A') {
    return BMP_SCOPE_UNSUPPORTED;
  }
  if (data[0] != 'B' || data[1] != 'M' || size < 18) {
    return BMP_SCOPE_REJECTED;
  }

  header.declared_file_size = read_le32(data + 2);
  header.pixel_offset = read_le32(data + 10);
  header.dib_size = read_le32(data + 14);
  header.image_count = 1;
  if (header.dib_size == 12) {
    if (size < 26) {
      return BMP_SCOPE_REJECTED;
    }
    raw_width = read_le16(data + 18);
    raw_height = read_le16(data + 20);
    header.bits_per_pixel = read_le16(data + 24);
    if (read_le16(data + 22) != 1) {
      return BMP_SCOPE_REJECTED;
    }
  } else if (header.dib_size >= 40 && header.dib_size <= 124) {
    int32_t signed_width;
    int32_t signed_height;
    if ((uint64_t)14 + header.dib_size > size) {
      return BMP_SCOPE_REJECTED;
    }
    signed_width = (int32_t)read_le32(data + 18);
    signed_height = (int32_t)read_le32(data + 22);
    if (signed_width <= 0 || signed_height == 0 || signed_height == INT32_MIN ||
        read_le16(data + 26) != 1) {
      return BMP_SCOPE_REJECTED;
    }
    raw_width = (uint32_t)signed_width;
    header.top_down = signed_height < 0;
    raw_height = (uint32_t)(signed_height < 0 ? -signed_height : signed_height);
    header.bits_per_pixel = read_le16(data + 28);
    header.compression = read_le32(data + 30);
  } else {
    return BMP_SCOPE_REJECTED;
  }

  header.width = raw_width;
  header.height = raw_height;
  if (out != NULL) {
    *out = header;
  }
  if (raw_width == 0 || raw_height == 0) {
    return BMP_SCOPE_REJECTED;
  }
  if (raw_width > BMP_MAX_DIMENSION || raw_height > BMP_MAX_DIMENSION ||
      (uint64_t)raw_width * raw_height > BMP_MAX_PIXELS) {
    return BMP_SCOPE_RESOURCE_LIMIT;
  }
  if (header.compression == 4 || header.compression == 5) {
    return BMP_SCOPE_UNSUPPORTED;
  }
  if (!valid_depth(header.bits_per_pixel) || header.compression > 6) {
    return BMP_SCOPE_REJECTED;
  }
  if ((header.compression == 1 && header.bits_per_pixel != 8) ||
      (header.compression == 2 && header.bits_per_pixel != 4) ||
      ((header.compression == 3 || header.compression == 6) &&
       header.bits_per_pixel != 16 && header.bits_per_pixel != 32) ||
      (header.top_down && (header.compression == 1 || header.compression == 2))) {
    return BMP_SCOPE_REJECTED;
  }
  if (header.pixel_offset < 14 + header.dib_size ||
      header.declared_file_size < header.pixel_offset ||
      header.declared_file_size > size) {
    return BMP_SCOPE_REJECTED;
  }
  if (header.declared_file_size < size) {
    size_t next = header.declared_file_size;
    if (next + 2 <= size && data[next] == 'B' &&
        (data[next + 1] == 'M' || data[next + 1] == 'A')) {
      header.image_count = 2;
      if (out != NULL) {
        *out = header;
      }
      return BMP_SCOPE_UNSUPPORTED;
    }
    return BMP_SCOPE_REJECTED;
  }

  if (header.compression == 0 || header.compression == 3 ||
      header.compression == 6) {
    row_bits = (uint64_t)raw_width * header.bits_per_pixel;
    row_bytes = ((row_bits + 31) / 32) * 4;
    required = (uint64_t)header.pixel_offset + row_bytes * raw_height;
    if (required > header.declared_file_size) {
      return BMP_SCOPE_REJECTED;
    }
  } else if (header.pixel_offset == header.declared_file_size) {
    return BMP_SCOPE_REJECTED;
  }
  if (out != NULL) {
    *out = header;
  }
  return BMP_SCOPE_DECODE;
}

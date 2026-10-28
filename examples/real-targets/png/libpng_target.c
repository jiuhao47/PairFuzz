#include "operation.h"

#include <png.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>

struct reader { const uint8_t *data; size_t size; size_t offset; };

static int apng_scope(const uint8_t *data, size_t size) {
  static const uint8_t signature[8] = {137,80,78,71,13,10,26,10};
  if (size < 8 || memcmp(data, signature, 8) != 0) return 0;
  size_t offset = 8;
  while (offset <= size && size - offset >= 12) {
    uint32_t length = ((uint32_t)data[offset] << 24) | ((uint32_t)data[offset+1] << 16) | ((uint32_t)data[offset+2] << 8) | data[offset+3];
    if ((size_t)length > size - offset - 12) return 0;
    if (memcmp(data + offset + 4, "acTL", 4) == 0) return 1;
    offset += 12 + length;
  }
  return 0;
}

static void read_png(png_structp png, png_bytep output, png_size_t length) {
  struct reader *reader = png_get_io_ptr(png);
  if (length > reader->size - reader->offset) png_error(png, "truncated input");
  memcpy(output, reader->data + reader->offset, length);
  reader->offset += length;
}

static void quiet_error(png_structp png, png_const_charp message) { (void)message; longjmp(png_jmpbuf(png), 1); }
static void quiet_warning(png_structp png, png_const_charp message) { (void)png; (void)message; }

int target_init(void) { return 0; }
const char *target_operation(void) { return "image.png.decode.rgba8"; }
uint8_t target_schema(void) { return 1; }
void target_cleanup(void) {}

void target_execute(const uint8_t *input, size_t length, struct diffray_operation_result *result) {
  if (apng_scope(input, length)) { result->class_id = DIFFRAY_RESULT_UNSUPPORTED; return; }
  png_structp png = png_create_read_struct(PNG_LIBPNG_VER_STRING, NULL, quiet_error, quiet_warning);
  if (png == NULL) { result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR; return; }
  png_infop info = png_create_info_struct(png);
  uint8_t *pixels = NULL;
  png_bytep *rows = NULL;
  if (info == NULL) { png_destroy_read_struct(&png, NULL, NULL); result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR; return; }
  if (setjmp(png_jmpbuf(png))) {
    free(rows); free(pixels); png_destroy_read_struct(&png, &info, NULL);
    result->class_id = DIFFRAY_RESULT_REJECTED; return;
  }
  struct reader reader = {input, length, 0};
  png_set_read_fn(png, &reader, read_png);
  png_set_crc_action(png, PNG_CRC_ERROR_QUIT, PNG_CRC_ERROR_QUIT);
  png_read_info(png, info);
  png_uint_32 width = png_get_image_width(png, info), height = png_get_image_height(png, info);
  if (width == 0 || height == 0) png_error(png, "zero dimensions");
  if (width > 4096 || height > 4096 || (uint64_t)width * height > 4194304) {
    png_destroy_read_struct(&png, &info, NULL); result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT; return;
  }
  int color = png_get_color_type(png, info), depth = png_get_bit_depth(png, info);
  if (depth == 16) png_set_strip_16(png);
  if (color == PNG_COLOR_TYPE_PALETTE) png_set_palette_to_rgb(png);
  if (color == PNG_COLOR_TYPE_GRAY && depth < 8) png_set_expand_gray_1_2_4_to_8(png);
  int has_trns = png_get_valid(png, info, PNG_INFO_tRNS) != 0;
  if (has_trns) png_set_tRNS_to_alpha(png);
  if (color == PNG_COLOR_TYPE_GRAY || color == PNG_COLOR_TYPE_GRAY_ALPHA) png_set_gray_to_rgb(png);
  if (!(color & PNG_COLOR_MASK_ALPHA) && !has_trns) png_set_filler(png, 0xff, PNG_FILLER_AFTER);
  png_set_interlace_handling(png);
  png_read_update_info(png, info);
  size_t output_size = (size_t)width * height * 4;
  if (png_get_rowbytes(png, info) != (size_t)width * 4) png_error(png, "unexpected row size");
  pixels = malloc(output_size); rows = malloc((size_t)height * sizeof(*rows));
  if (pixels == NULL || rows == NULL) { free(rows); free(pixels); png_destroy_read_struct(&png, &info, NULL); result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR; return; }
  for (png_uint_32 y = 0; y < height; ++y) rows[y] = pixels + (size_t)y * width * 4;
  png_read_image(png, rows); png_read_end(png, info);
  free(rows); png_destroy_read_struct(&png, &info, NULL);
  result->class_id = DIFFRAY_RESULT_ACCEPTED; result->width = width; result->height = height;
  result->normalized = pixels; result->normalized_len = output_size;
}

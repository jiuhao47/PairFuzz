#include "operation.h"

#include <spng.h>
#include <stdlib.h>
#include <string.h>

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

int target_init(void) { return 0; }
const char *target_operation(void) { return "image.png.decode.rgba8"; }
uint8_t target_schema(void) { return 1; }
void target_cleanup(void) {}

void target_execute(const uint8_t *input, size_t length, struct pairfuzz_operation_result *result) {
  if (apng_scope(input, length)) { result->class_id = PAIRFUZZ_RESULT_UNSUPPORTED; return; }
  spng_ctx *ctx = spng_ctx_new(0);
  if (ctx == NULL) { result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR; return; }
  spng_set_crc_action(ctx, SPNG_CRC_ERROR, SPNG_CRC_ERROR);
  int status = spng_set_png_buffer(ctx, input, length);
  struct spng_ihdr ihdr;
  if (status == 0) status = spng_get_ihdr(ctx, &ihdr);
  if (status != 0) { spng_ctx_free(ctx); result->class_id = PAIRFUZZ_RESULT_REJECTED; return; }
  uint64_t pixels_count = (uint64_t)ihdr.width * ihdr.height;
  if (ihdr.width > 4096 || ihdr.height > 4096 || pixels_count > 4194304) {
    spng_ctx_free(ctx); result->class_id = PAIRFUZZ_RESULT_RESOURCE_LIMIT; return;
  }
  size_t output_size = (size_t)pixels_count * 4;
  uint8_t *pixels = malloc(output_size);
  if (pixels == NULL) { spng_ctx_free(ctx); result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR; return; }
  status = spng_decode_image(ctx, pixels, output_size, SPNG_FMT_RGBA8, SPNG_DECODE_TRNS);
  if (status == 0) status = spng_decode_chunks(ctx);
  spng_ctx_free(ctx);
  if (status != 0) { free(pixels); result->class_id = PAIRFUZZ_RESULT_REJECTED; return; }
  result->class_id = PAIRFUZZ_RESULT_ACCEPTED; result->width = ihdr.width; result->height = ihdr.height;
  result->normalized = pixels; result->normalized_len = output_size;
}

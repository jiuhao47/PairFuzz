#include <inttypes.h>
#include <spng.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

DIFFRAY_INIT();

int FDL_DEBUG_TMP = 0;
// extern int LIBSPNG_ERROR_CODE_DIFF_FUZZ;

const char *color_type_str(enum spng_color_type color_type) {
  switch (color_type) {
  case SPNG_COLOR_TYPE_GRAYSCALE:
    return "grayscale";
  case SPNG_COLOR_TYPE_TRUECOLOR:
    return "truecolor";
  case SPNG_COLOR_TYPE_INDEXED:
    return "indexed color";
  case SPNG_COLOR_TYPE_GRAYSCALE_ALPHA:
    return "grayscale with alpha";
  case SPNG_COLOR_TYPE_TRUECOLOR_ALPHA:
    return "truecolor with alpha";
  default:
    return "(invalid)";
  }
}

int libspng_parse(const uint8_t *data, size_t size) {
  int ret = 0;
  spng_ctx *ctx = NULL;
  unsigned char *image = NULL;

  ctx = spng_ctx_new(0);

  if (ctx == NULL) {
    if (FDL_DEBUG_TMP)
      printf("spng_ctx_new() failed\n");
    ret = 1;
    goto error;
  }

  /* Ignore and don't calculate chunk CRC's */
  spng_set_crc_action(ctx, SPNG_CRC_USE, SPNG_CRC_USE);

  /* Set memory usage limits for storing standard and unknown chunks,
     this is important when reading untrusted files! */
  // size_t limit = 1024 * 1024 * 64;
  // spng_set_chunk_limits(ctx, limit, limit);

  /* Set source PNG */
  // spng_set_png_file(ctx, png); /* or _buffer(), _stream() */
  spng_set_png_buffer(ctx, (void *)data, size); // newly update

  struct spng_ihdr ihdr;
  ret = spng_get_ihdr(ctx, &ihdr);

  if (ret) {
    // __sanitizer_cov_trace_error(ret%1000);
    if (FDL_DEBUG_TMP)
      printf("spng_get_ihdr() error: %s\n", spng_strerror(ret));
    goto error;
  }

  const char *color_name = color_type_str(ihdr.color_type);

  if (FDL_DEBUG_TMP) {
    printf("width: %u\n"
           "height: %u\n"
           "bit depth: %u\n"
           "color type: %u - %s\n",
           ihdr.width, ihdr.height, ihdr.bit_depth, ihdr.color_type,
           color_name);
  }

  if (ihdr.width >= 400 || ihdr.height >= 400) {
    // LIBSPNG_ERROR_CODE_DIFF_FUZZ = 1;
    ret = 5;
    goto error;
  }

  size_t image_size, image_width;

  /* Output format, does not depend on source PNG format except for
     SPNG_FMT_PNG, which is the PNG's format in host-endian or
     big-endian for SPNG_FMT_RAW.
     Note that for these two formats <8-bit images are left byte-packed */
  // int fmt = SPNG_FMT_PNG;
  int fmt = SPNG_FMT_RGBA8;

  ret = spng_decoded_image_size(ctx, fmt, &image_size);

  if (ret) {
    // __sanitizer_cov_trace_error(ret % 1000);
    goto error;
  }

  image = malloc(image_size);

  if (image == NULL)
    goto error;

  /* Decode the image in one go */
  ret = spng_decode_image(ctx, image, image_size, SPNG_FMT_RGBA8, 0);

  if (ret) {
    if (FDL_DEBUG_TMP)
      printf("progressive spng_decode_image() error: %s\n", spng_strerror(ret));
    goto error;
  }

error:

  spng_ctx_free(ctx);

  free(image);

  return ret;
}

/* select_rv_shm_env is provided by app/libs/rv_shm/rv_shm.c */

int main(int argc, char **argv) {
  DIFFRAY_RUN(libspng_parse);
  return 0;
}
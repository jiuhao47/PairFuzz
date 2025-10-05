#include <inttypes.h>
#include <spng.h>
#include <stdio.h>
#include <time.h>

int libspng_parse(const uint8_t *data, size_t size) {
  int ret = 0;
  spng_ctx *ctx = NULL;
  unsigned char *image = NULL;

  ctx = spng_ctx_new(0);
  if (ctx == NULL) {
    ret = 1;
    goto error;
  }

  spng_set_crc_action(ctx, SPNG_CRC_USE, SPNG_CRC_USE);
  spng_set_png_buffer(ctx, (void *)data, size);

  struct spng_ihdr ihdr;
  ret = spng_get_ihdr(ctx, &ihdr);
  if (ret)
    goto error;

  if (ihdr.width >= 400 || ihdr.height >= 400) {
    ret = 5;
    goto error;
  }

  size_t image_size;
  int fmt = SPNG_FMT_RGBA8;
  ret = spng_decoded_image_size(ctx, fmt, &image_size);
  if (ret)
    goto error;

  image = malloc(image_size);
  if (image == NULL)
    goto error;

  ret = spng_decode_image(ctx, image, image_size, SPNG_FMT_RGBA8, 0);
  if (ret)
    goto error;

error:
  spng_ctx_free(ctx);
  free(image);
  return ret;
}

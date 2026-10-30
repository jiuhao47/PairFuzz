#include "bmp_scope.h"
#include "operation.h"

#include <magick/api.h>
#include <stdlib.h>
#include <string.h>

#ifndef PAIRFUZZ_MAGICK_CONFIG_PATH
#define PAIRFUZZ_MAGICK_CONFIG_PATH ""
#endif

int target_init(void) {
  if (setenv("MAGICK_CONFIGURE_PATH", PAIRFUZZ_MAGICK_CONFIG_PATH, 1) != 0 ||
      setenv("MAGICK_CODER_MODULE_PATH", "/nonexistent", 1) != 0 ||
      setenv("HOME", PAIRFUZZ_MAGICK_CONFIG_PATH, 1) != 0 ||
      setenv("XDG_CONFIG_HOME", PAIRFUZZ_MAGICK_CONFIG_PATH, 1) != 0) {
    return -1;
  }
  InitializeMagick(NULL);
  if (SetMagickResourceLimit(ThreadsResource, 1) == MagickFail ||
      SetMagickResourceLimit(WidthResource, 4096) == MagickFail ||
      SetMagickResourceLimit(HeightResource, 4096) == MagickFail ||
      SetMagickResourceLimit(PixelsResource, 4194304) == MagickFail ||
      SetMagickResourceLimit(MemoryResource, 64) == MagickFail ||
      SetMagickResourceLimit(MapResource, 64) == MagickFail) {
    DestroyMagick();
    return -1;
  }
  return 0;
}

const char *target_operation(void) { return "image.bmp.decode.rgba8"; }
uint8_t target_schema(void) { return 1; }
void target_cleanup(void) { DestroyMagick(); }

static enum pairfuzz_result_class exception_class(
    const ExceptionInfo *exception) {
  if (exception != NULL &&
      (exception->severity == ResourceLimitError ||
       exception->severity == ResourceLimitFatalError)) {
    return PAIRFUZZ_RESULT_RESOURCE_LIMIT;
  }
  return PAIRFUZZ_RESULT_REJECTED;
}

static uint32_t read_le32(const uint8_t *bytes) {
  return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
         ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static unsigned int mask_width(uint32_t mask) {
  unsigned int width = 0;
  while (mask != 0) {
    width += mask & 1u;
    mask >>= 1;
  }
  return width;
}

static uint8_t expand_channel(uint8_t value, unsigned int width) {
  if (width == 0 || width >= 8) return value;
  unsigned int maximum = (1u << width) - 1u;
  unsigned int sample = value >> (8u - width);
  return (uint8_t)((sample * 255u + maximum / 2u) / maximum);
}

static void normalize_16bit_channels(const uint8_t *input, size_t length,
                                     const struct bmp_header *header,
                                     uint8_t *pixels, size_t output_length) {
  unsigned int widths[3] = {5, 5, 5};
  if (header->compression == 3 && length >= 14u + 52u) {
    widths[0] = mask_width(read_le32(input + 14u + 40u));
    widths[1] = mask_width(read_le32(input + 14u + 44u));
    widths[2] = mask_width(read_le32(input + 14u + 48u));
  }
  for (size_t offset = 0; offset < output_length; offset += 4) {
    pixels[offset] = expand_channel(pixels[offset], widths[0]);
    pixels[offset + 1] = expand_channel(pixels[offset + 1], widths[1]);
    pixels[offset + 2] = expand_channel(pixels[offset + 2], widths[2]);
  }
}

void target_execute(const uint8_t *input, size_t length,
                    struct pairfuzz_operation_result *result) {
  struct bmp_header header;
  switch (bmp_scope_classify(input, length, &header)) {
    case BMP_SCOPE_UNSUPPORTED:
      result->class_id = PAIRFUZZ_RESULT_UNSUPPORTED;
      return;
    case BMP_SCOPE_RESOURCE_LIMIT:
      result->class_id = PAIRFUZZ_RESULT_RESOURCE_LIMIT;
      return;
    case BMP_SCOPE_REJECTED:
      result->class_id = PAIRFUZZ_RESULT_REJECTED;
      return;
    case BMP_SCOPE_DECODE:
      break;
  }

  ExceptionInfo exception;
  GetExceptionInfo(&exception);
  ImageInfo *image_info = CloneImageInfo(NULL);
  if (image_info == NULL) {
    DestroyExceptionInfo(&exception);
    result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "GraphicsMagick per-call setup failed");
    return;
  }
  (void)strncpy(image_info->magick, "BMP", MaxTextExtent - 1);
  image_info->magick[MaxTextExtent - 1] = '\0';
  (void)strncpy(image_info->filename, "BMP:", MaxTextExtent - 1);
  image_info->filename[MaxTextExtent - 1] = '\0';
  Image *image = BlobToImage(image_info, input, length, &exception);
  if (image == NULL) {
    result->class_id = exception_class(&exception);
    DestroyImageInfo(image_info);
    DestroyExceptionInfo(&exception);
    return;
  }
  if (GetImageListLength(image) != 1 || image->columns != header.width ||
      image->rows != header.height) {
    result->class_id = PAIRFUZZ_RESULT_REJECTED;
    DestroyImageList(image);
    DestroyImageInfo(image_info);
    DestroyExceptionInfo(&exception);
    return;
  }

  size_t output_length = (size_t)header.width * header.height * 4u;
  uint8_t *pixels = malloc(output_length);
  if (pixels == NULL) {
    result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "GraphicsMagick output allocation failed");
    DestroyImageList(image);
    DestroyImageInfo(image_info);
    DestroyExceptionInfo(&exception);
    return;
  }
  if (DispatchImage(image, 0, 0, header.width, header.height, "RGBA",
                    CharPixel, pixels, &exception) == MagickFail) {
    free(pixels);
    result->class_id = exception_class(&exception);
    if (result->class_id == PAIRFUZZ_RESULT_REJECTED) {
      result->class_id = PAIRFUZZ_RESULT_ADAPTER_ERROR;
      strcpy(result->diagnostic, "GraphicsMagick RGBA export failed");
    }
    DestroyImageList(image);
    DestroyImageInfo(image_info);
    DestroyExceptionInfo(&exception);
    return;
  }
  if (header.bits_per_pixel == 32 && header.compression == 0) {
    for (size_t offset = 3; offset < output_length; offset += 4) {
      pixels[offset] = 255;
    }
  } else if (header.bits_per_pixel == 16) {
    normalize_16bit_channels(input, length, &header, pixels, output_length);
  }
  result->class_id = PAIRFUZZ_RESULT_ACCEPTED;
  result->width = header.width;
  result->height = header.height;
  result->normalized = pixels;
  result->normalized_len = output_length;
  DestroyImageList(image);
  DestroyImageInfo(image_info);
  DestroyExceptionInfo(&exception);
}

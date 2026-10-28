#include "bmp_scope.h"
#include "operation.h"

#include <MagickCore/MagickCore.h>
#include <stdlib.h>
#include <string.h>

#ifndef DIFFRAY_MAGICK_CONFIG_PATH
#define DIFFRAY_MAGICK_CONFIG_PATH ""
#endif

int target_init(void) {
  if (setenv("MAGICK_CONFIGURE_PATH", DIFFRAY_MAGICK_CONFIG_PATH, 1) != 0 ||
      setenv("MAGICK_CODER_MODULE_PATH", "/nonexistent", 1) != 0 ||
      setenv("MAGICK_CODER_FILTER_PATH", "/nonexistent", 1) != 0 ||
      setenv("HOME", DIFFRAY_MAGICK_CONFIG_PATH, 1) != 0 ||
      setenv("XDG_CONFIG_HOME", DIFFRAY_MAGICK_CONFIG_PATH, 1) != 0) {
    return -1;
  }
  MagickCoreGenesis(NULL, MagickFalse);
  if (SetMagickResourceLimit(ThreadResource, 1) == MagickFalse ||
      SetMagickResourceLimit(WidthResource, 4096) == MagickFalse ||
      SetMagickResourceLimit(HeightResource, 4096) == MagickFalse ||
      SetMagickResourceLimit(AreaResource, 4194304) == MagickFalse ||
      SetMagickResourceLimit(MemoryResource, 64u * 1024u * 1024u) ==
          MagickFalse ||
      SetMagickResourceLimit(MapResource, 64u * 1024u * 1024u) ==
          MagickFalse) {
    MagickCoreTerminus();
    return -1;
  }
  return 0;
}

const char *target_operation(void) { return "image.bmp.decode.rgba8"; }
uint8_t target_schema(void) { return 1; }
void target_cleanup(void) { MagickCoreTerminus(); }

static enum diffray_result_class exception_class(
    const ExceptionInfo *exception) {
  if (exception != NULL &&
      (exception->severity == ResourceLimitError ||
       exception->severity == ResourceLimitFatalError)) {
    return DIFFRAY_RESULT_RESOURCE_LIMIT;
  }
  return DIFFRAY_RESULT_REJECTED;
}

void target_execute(const uint8_t *input, size_t length,
                    struct diffray_operation_result *result) {
  struct bmp_header header;
  switch (bmp_scope_classify(input, length, &header)) {
    case BMP_SCOPE_UNSUPPORTED:
      result->class_id = DIFFRAY_RESULT_UNSUPPORTED;
      return;
    case BMP_SCOPE_RESOURCE_LIMIT:
      result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT;
      return;
    case BMP_SCOPE_REJECTED:
      result->class_id = DIFFRAY_RESULT_REJECTED;
      return;
    case BMP_SCOPE_DECODE:
      break;
  }

  ExceptionInfo *exception = AcquireExceptionInfo();
  ImageInfo *image_info = CloneImageInfo(NULL);
  if (exception == NULL || image_info == NULL) {
    if (image_info != NULL) (void)DestroyImageInfo(image_info);
    if (exception != NULL) (void)DestroyExceptionInfo(exception);
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "ImageMagick per-call setup failed");
    return;
  }
  (void)CopyMagickString(image_info->magick, "BMP", MagickPathExtent);
  (void)CopyMagickString(image_info->filename, "BMP:", MagickPathExtent);
  Image *image = BlobToImage(image_info, input, length, exception);
  if (image == NULL) {
    result->class_id = exception_class(exception);
    (void)DestroyImageInfo(image_info);
    (void)DestroyExceptionInfo(exception);
    return;
  }
  if (GetImageListLength(image) != 1 || image->columns != header.width ||
      image->rows != header.height) {
    result->class_id = DIFFRAY_RESULT_REJECTED;
    image = DestroyImageList(image);
    (void)DestroyImageInfo(image_info);
    (void)DestroyExceptionInfo(exception);
    return;
  }

  size_t output_length = (size_t)header.width * header.height * 4u;
  uint8_t *pixels = malloc(output_length);
  if (pixels == NULL) {
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "ImageMagick output allocation failed");
    image = DestroyImageList(image);
    (void)DestroyImageInfo(image_info);
    (void)DestroyExceptionInfo(exception);
    return;
  }
  if (ExportImagePixels(image, 0, 0, header.width, header.height, "RGBA",
                        CharPixel, pixels, exception) == MagickFalse) {
    free(pixels);
    result->class_id = exception_class(exception);
    if (result->class_id == DIFFRAY_RESULT_REJECTED) {
      result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
      strcpy(result->diagnostic, "ImageMagick RGBA export failed");
    }
    image = DestroyImageList(image);
    (void)DestroyImageInfo(image_info);
    (void)DestroyExceptionInfo(exception);
    return;
  }
  if (header.bits_per_pixel == 32 && header.compression == 0) {
    for (size_t offset = 3; offset < output_length; offset += 4) {
      pixels[offset] = 255;
    }
  }
  result->class_id = DIFFRAY_RESULT_ACCEPTED;
  result->width = header.width;
  result->height = header.height;
  result->normalized = pixels;
  result->normalized_len = output_length;
  image = DestroyImageList(image);
  (void)DestroyImageInfo(image_info);
  (void)DestroyExceptionInfo(exception);
}

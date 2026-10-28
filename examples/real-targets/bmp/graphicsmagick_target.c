#include "bmp_scope.h"
#include "operation.h"

#include <magick/api.h>
#include <stdlib.h>

#ifndef DIFFRAY_MAGICK_CONFIG_PATH
#define DIFFRAY_MAGICK_CONFIG_PATH ""
#endif

int target_init(void) {
  if (setenv("MAGICK_CONFIGURE_PATH", DIFFRAY_MAGICK_CONFIG_PATH, 1) != 0 ||
      setenv("MAGICK_CODER_MODULE_PATH", "/nonexistent", 1) != 0) {
    return -1;
  }
  InitializeMagick(NULL);
  return 0;
}

const char *target_operation(void) { return "image.bmp.decode.rgba8"; }
uint8_t target_schema(void) { return 1; }
void target_cleanup(void) { DestroyMagick(); }

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
      /* Task 2 replaces this scope-only checkpoint with normalized decoding. */
      result->class_id = DIFFRAY_RESULT_REJECTED;
      return;
  }
}

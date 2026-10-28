#include "operation.h"

#include <stdio.h>
#include <stdlib.h>

static int read_file(const char *path, uint8_t **bytes, size_t *length) {
  FILE *input = fopen(path, "rb");
  if (input == NULL || fseek(input, 0, SEEK_END) != 0) return -1;
  long size = ftell(input);
  if (size < 0 || (unsigned long)size > DIFFRAY_INPUT_LIMIT ||
      fseek(input, 0, SEEK_SET) != 0) {
    fclose(input);
    return -1;
  }
  uint8_t *buffer = malloc(size == 0 ? 1u : (size_t)size);
  if (buffer == NULL ||
      (size != 0 && fread(buffer, 1, (size_t)size, input) != (size_t)size)) {
    free(buffer);
    fclose(input);
    return -1;
  }
  fclose(input);
  *bytes = buffer;
  *length = (size_t)size;
  return 0;
}

int main(int argc, char **argv) {
  if (argc != 4 || target_init() != 0) return 2;
  for (int index = 1; index < argc; ++index) {
    uint8_t *input = NULL;
    size_t length = 0;
    if (read_file(argv[index], &input, &length) != 0) return 3;
    struct diffray_operation_result result;
    diffray_operation_result_init(&result);
    target_execute(input, length, &result);
    free(input);
    printf("%u %016llx\n", (unsigned int)result.class_id,
           (unsigned long long)diffray_semantic_code(&result, target_schema()));
    diffray_result_release(&result);
  }
  target_cleanup();
  return 0;
}
